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

mod circuit_breaker;
use circuit_breaker::CircuitBreaker;

#[derive(Clone)]
struct AppState {
    receivers: Arc<RwLock<Vec<ServiceRegistration>>>,
    http_client: HttpClient,
    circuit_breaker: Arc<CircuitBreaker>,
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

    use tokio_stream::StreamExt;
    
    let receivers_clone = receivers.clone();
    let garage_clone = garage_client.clone();
    tokio::spawn(async move {
        let mut stream = std::pin::pin!(garage_clone.monitor_services(Duration::from_secs(10)));
        while let Some(services) = stream.next().await {
            info!("Discovered {} receivers", services.len());
            if let Ok(mut writer) = receivers_clone.write() {
                *writer = services;
            }
        }
    });

    let circuit_breaker = CircuitBreaker::new(3, Duration::from_secs(10));

    let state = AppState {
        receivers,
        http_client: HttpClient::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        circuit_breaker: Arc::new(circuit_breaker),
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

        let result = state.circuit_breaker.call(|| async {
            state.http_client.post(&url).json(&msg).send().await
        }).await;

        match result {
            Ok(_response) => Json(SendResponse {
                status: "sent".to_string(),
                receiver_id: Some(receiver.id),
            }),
            Err(e) => match e {
                 circuit_breaker::Error::CircuitOpen => {
                     warn!("Circuit breaker open for {}", receiver.id);
                     Json(SendResponse {
                        status: "circuit_open".to_string(),
                        receiver_id: Some(receiver.id),
                    })
                 },
                 circuit_breaker::Error::Inner(inner_e) => {
                    error!("Failed to send to {}: {}", receiver.id, inner_e);
                    Json(SendResponse {
                        status: "failed".to_string(),
                        receiver_id: Some(receiver.id),
                    })
                 }
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
