use axum::{
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use shared::{GarageClient, ServiceRegistration};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{self, Duration};
use tracing::{error, info};
use uuid::Uuid;

mod data_writer;
use data_writer::{DataWriter, MessageRecord};

#[derive(Clone)]
struct AppState {
    garage_client: Arc<GarageClient>,
    service_id: String,
    host: String,
    port: u16,
    data_writer: Arc<DataWriter>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let _host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port_str = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let port: u16 = port_str.parse()?;

    let s3_endpoint =
        std::env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let s3_bucket = std::env::var("S3_BUCKET").unwrap_or_else(|_| "service-registry".to_string());
    let s3_region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());

    let service_id = Uuid::new_v4().to_string();
    info!("Starting Receiver Service ID: {}", service_id);

    let garage_client = Arc::new(GarageClient::new(s3_endpoint, s3_bucket, s3_region).await);

    if let Err(e) = garage_client.ensure_bucket_exists().await {
        error!("Failed to ensure bucket exists: {}", e);
    }

    let data_writer = Arc::new(DataWriter::new(garage_client.clone()));

    let host = if let Ok(h) = std::env::var("ADVERTISED_HOST") {
        h
    } else if let Ok(hostname) = std::env::var("HOSTNAME") {
        if std::path::Path::new("/.dockerenv").exists() {
            hostname
        } else {
            "127.0.0.1".to_string()
        }
    } else {
        "127.0.0.1".to_string()
    };

    let state = AppState {
        garage_client: garage_client.clone(),
        service_id: service_id.clone(),
        host: host.clone(),
        port,
        data_writer,
    };

    let state_clone = state.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(5));
        loop {
            let registration = ServiceRegistration {
                id: state_clone.service_id.clone(),
                host: state_clone.host.clone(),
                port: state_clone.port,
                last_seen: Utc::now(),
            };

            if let Err(e) = state_clone
                .garage_client
                .register_service(&registration)
                .await
            {
                error!("Failed to refresh registration: {}", e);
            } else {
                info!(
                    "Refreshed registration for {} at {}:{}",
                    state_clone.service_id, state_clone.host, state_clone.port
                );
            }
            interval.tick().await;
        }
    });

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/messages", post(receive_message))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Deserialize, Debug)]
struct Message {
    content: String,
    sender_id: Option<String>,
}

async fn receive_message(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(payload): Json<Message>,
) -> &'static str {
    info!(
        "Received message from {:?}: {}",
        payload.sender_id, payload.content
    );

    let record = MessageRecord {
        id: Uuid::new_v4().to_string(),
        content: payload.content,
        sender_id: payload.sender_id,
        timestamp: Utc::now(),
    };

    state.data_writer.push(record).await;

    "Message received"
}
