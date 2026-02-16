use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use rand::seq::SliceRandom;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use shared::{GarageClient, ServiceRegistration};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::time::{self, Duration};
use tracing::{error, info, warn};

#[derive(Clone)]
struct AppState {
    receivers: Arc<RwLock<Vec<ServiceRegistration>>>,
    http_client: HttpClient,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let port_str = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let port: u16 = port_str.parse()?;

    let s3_endpoint =
        std::env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_bucket = std::env::var("S3_BUCKET").unwrap_or_else(|_| "service-registry".to_string());
    let s3_region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());

    let garage_client = Arc::new(GarageClient::new(s3_endpoint, s3_bucket, s3_region).await);

    let receivers = Arc::new(RwLock::new(Vec::new()));

    let receivers_clone = receivers.clone();
    let garage_clone = garage_client.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            match garage_clone.discover_services().await {
                Ok(services) => {
                    info!("Discovered {} receivers", services.len());
                    if let Ok(mut writer) = receivers_clone.write() {
                        *writer = services;
                    }
                }
                Err(e) => error!("Failed to discover services: {}", e),
            }
        }
    });

    let state = AppState {
        receivers,
        http_client: HttpClient::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
    };

    let app = Router::new()
        .route("/receivers", get(list_receivers))
        .route("/send", post(send_message))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Sender listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn list_receivers(State(state): State<AppState>) -> Json<Vec<ServiceRegistration>> {
    let receivers = state.receivers.read().unwrap().clone();
    Json(receivers)
}

#[derive(Deserialize, Debug)]
struct SendRequest {
    content: String,
}

#[derive(Serialize)]
struct SendResponse {
    status: String,
    receiver_id: Option<String>,
}

async fn send_message(
    State(state): State<AppState>,
    Json(payload): Json<SendRequest>,
) -> Json<SendResponse> {
    let receiver = {
        let receivers = state.receivers.read().unwrap();
        receivers.choose(&mut rand::thread_rng()).cloned()
    };

    if let Some(receiver) = receiver {
        let url = format!("http://{}:{}/messages", receiver.host, receiver.port);
        info!("Attempting to send message to: {}", url);
        let msg = serde_json::json!({
            "content": payload.content,
            "sender_id": "sender-1",
        });

        match state.http_client.post(&url).json(&msg).send().await {
            Ok(_) => Json(SendResponse {
                status: "sent".to_string(),
                receiver_id: Some(receiver.id),
            }),
            Err(e) => {
                error!("Failed to send to {}: {}", receiver.id, e);
                Json(SendResponse {
                    status: "failed".to_string(),
                    receiver_id: Some(receiver.id),
                })
            }
        }
    } else {
        warn!("No receivers available");
        Json(SendResponse {
            status: "no_receivers".to_string(),
            receiver_id: None,
        })
    }
}
