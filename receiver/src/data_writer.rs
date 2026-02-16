use anyhow::{Context as _, Result};
use arrow::array::{StringArray, TimestampNanosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Utc};
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use shared::GarageClient;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub id: String,
    pub content: String,
    pub sender_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

pub struct DataWriter {
    garage_client: Arc<GarageClient>,
    buffer: Mutex<Vec<MessageRecord>>,
}

impl DataWriter {
    pub fn new(garage_client: Arc<GarageClient>) -> Self {
        Self {
            garage_client,
            buffer: Mutex::new(Vec::new()),
        }
    }

    pub async fn push(&self, msg: MessageRecord) {
        let mut buffer = self.buffer.lock().await;
        buffer.push(msg);
        if buffer.len() >= 10 {
            drop(buffer);
            if let Err(e) = self.flush().await {
                tracing::error!("Failed to flush data: {}", e);
            }
        }
    }

    pub async fn flush(&self) -> Result<()> {
        let mut buffer = self.buffer.lock().await;
        if buffer.is_empty() {
            return Ok(());
        }

        let records = std::mem::take(&mut *buffer);
        drop(buffer);

        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let contents: Vec<String> = records.iter().map(|r| r.content.clone()).collect();
        let senders: Vec<Option<String>> = records.iter().map(|r| r.sender_id.clone()).collect();
        let timestamps: Vec<i64> = records
            .iter()
            .map(|r| r.timestamp.timestamp_nanos_opt().unwrap_or(0))
            .collect();

        let id_array = StringArray::from(ids);
        let content_array = StringArray::from(contents);
        let sender_array = StringArray::from(senders);
        let timestamp_array = TimestampNanosecondArray::from(timestamps);

        let schema = Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("sender_id", DataType::Utf8, true),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                false,
            ),
        ]);

        let batch = RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(id_array),
                Arc::new(content_array),
                Arc::new(sender_array),
                Arc::new(timestamp_array),
            ],
        )?;

        let props = WriterProperties::builder().build();
        let mut cursor = Vec::new();
        let mut writer = ArrowWriter::try_new(&mut cursor, batch.schema(), Some(props))?;
        writer.write(&batch)?;
        writer.close()?;

        let now = Utc::now();
        let key = format!(
            "data/messages/year={}/month={:02}/day={:02}/{}.parquet",
            now.format("%Y"),
            now.format("%m"),
            now.format("%d"),
            Uuid::new_v4()
        );

        self.garage_client.upload_data(&key, cursor).await?;

        tracing::info!(
            "Flushed {} records to s3://{}/{}",
            records.len(),
            "bucket",
            key
        );

        Ok(())
    }
}
