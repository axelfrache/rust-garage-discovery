#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_multiple_senders_race_condition() {
        
        let receiver_url = "http:
        let sender_count = 5;
        let messages_per_sender = 100;

        let mut handles = vec![];

        for sender_id in 0..sender_count {
            let handle = tokio::spawn(async move {
                let client = reqwest::Client::new();
                let mut sent_ids = vec![];

                for msg_id in 0..messages_per_sender {
                    let payload = serde_json::json!({
                        "content": format!("Sender {} - Message {}", sender_id, msg_id),
                        "sender_id": format!("sender-{}", sender_id),
                        "message_uuid": uuid::Uuid::new_v4().to_string(), 
                    });

                    let response = client
                        .post(receiver_url)
                        .json(&payload)
                        .timeout(Duration::from_secs(5))
                        .send()
                        .await;

                    if let Ok(resp) = response {
                        if resp.status() == StatusCode::OK {
                            sent_ids.push(msg_id);
                        }
                    }
                }

                sent_ids.len()
            });

            handles.push(handle);
        }

        let total_sent: usize = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .sum();

        println!("Total messages sent: {}", total_sent);
        sleep(Duration::from_secs(2)).await;
        
        assert_eq!(total_sent, sender_count * messages_per_sender);
    }

    #[tokio::test]
    async fn test_circuit_breaker_under_load() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(1));
        let failed_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let mut handles = vec![];
        for _ in 0..10 {
            let cb = cb.clone();
            let failed = failed_calls.clone();

            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let result = cb
                        .call(|| async {
                            
                            Err::<(), reqwest::Error>(reqwest::Error::from(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "test error",
                            )))
                        })
                        .await;

                    if let Err(circuit_breaker::Error::CircuitOpen) = result {
                        failed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }
        let failures = failed_calls.load(std::sync::atomic::Ordering::SeqCst);
        assert!(failures > 0, "Circuit breaker should have opened");
    }
}
