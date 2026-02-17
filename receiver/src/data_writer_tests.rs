#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn test_concurrent_message_buffering() {
        let garage_client = Arc::new(GarageClient::new_test().await);
        let writer = Arc::new(DataWriter::new(garage_client));

        let mut set = JoinSet::new();
        for i in 0..50 {
            let writer = writer.clone();
            set.spawn(async move {
                let record = MessageRecord {
                    id: format!("msg-{}", i),
                    content: format!("Content {}", i),
                    sender_id: Some("test-sender".to_string()),
                    timestamp: Utc::now(),
                };
                writer.push(record).await;
            });
        }
        while set.join_next().await.is_some() {}
        writer.flush().await.unwrap();
        let buffer = writer.buffer.lock().await;
        assert_eq!(buffer.len(), 0);
    }

    #[tokio::test]
    async fn test_race_condition_on_flush_boundary() {
        let garage_client = Arc::new(GarageClient::new_test().await);
        let writer = Arc::new(DataWriter::new(garage_client));

        let mut handles = vec![];
        for i in 0..10 {
            let writer = writer.clone();
            handles.push(tokio::spawn(async move {
                let record = MessageRecord {
                    id: format!("msg-{}", i),
                    content: format!("Content {}", i),
                    sender_id: Some("test-sender".to_string()),
                    timestamp: Utc::now(),
                };
                writer.push(record).await;
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }
        let buffer = writer.buffer.lock().await;
        assert_eq!(buffer.len(), 0, "Buffer should be empty after auto-flush");
    }
}
