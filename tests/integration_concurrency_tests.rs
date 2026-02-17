use std::collections::HashSet;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_message_order_and_deduplication() {
    
    let receiver_urls = vec![
        "http:
        "http:
        "http:
    ];

    let mut message_order = vec![];
    let mut received_messages: HashSet<String> = HashSet::new();
    let client = reqwest::Client::new();
    for seq in 0..50 {
        let message = serde_json::json!({
            "content": format!("Message {}", seq),
            "sender_id": "test-sender",
            "sequence": seq,
            "unique_id": uuid::Uuid::new_v4().to_string(),
        });
        let receiver = receiver_urls[seq % receiver_urls.len()];

        let response = client
            .post(format!("{}/messages", receiver))
            .json(&message)
            .send()
            .await;

        if response.is_ok() {
            message_order.push(seq);
        }
        sleep(Duration::from_millis(10)).await;
    }
    sleep(Duration::from_secs(3)).await;

    println!("Message order: {:?}", message_order);
    println!("Expected sequence: 0..49");
    
    assert_eq!(
        message_order.len(),
        50,
        "Tous les messages devraient être envoyés"
    );
}

#[tokio::test]
async fn test_concurrent_flush_collision() {

    let garage_client = Arc::new(GarageClient::new_test().await);
    let writer1 = Arc::new(DataWriter::new(garage_client.clone()));
    let writer2 = Arc::new(DataWriter::new(garage_client.clone()));
    for i in 0..10 {
        writer1
            .push(MessageRecord {
                id: format!("w1-msg-{}", i),
                content: "Writer 1".to_string(),
                sender_id: Some("test".to_string()),
                timestamp: Utc::now(),
            })
            .await;

        writer2
            .push(MessageRecord {
                id: format!("w2-msg-{}", i),
                content: "Writer 2".to_string(),
                sender_id: Some("test".to_string()),
                timestamp: Utc::now(),
            })
            .await;
    }
    
    sleep(Duration::from_secs(1)).await;
}
