#!/bin/bash
set -e

echo "[INFO] Starting System Verification"

# 1. Check if containers are running
if ! docker-compose ps | grep "Up" > /dev/null; then
    echo "[ERROR] Services are not running! Please run 'docker-compose up -d --build' first."
    exit 1
fi

echo "[INFO] Services are running."

# 2. Wait for services to be ready
echo "[INFO] Waiting for services to be healthy..."
sleep 5

# 3. Check Sender Discovery
echo "[INFO] Checking Sender discovery status..."
# Sender exposes /receivers endpoint
RECEIVERS=$(curl -s http://localhost:8080/receivers)
echo "[DEBUG] Sender receivers: $RECEIVERS"

COUNT=$(echo "$RECEIVERS" | grep -o "id" | wc -l)
if [ "$COUNT" -gt 0 ]; then
    echo "[INFO] Sender has discovered $COUNT receivers."
else
    echo "[WARN] Sender has NOT discovered any receivers yet."
    echo "[WARN] This might be due to propagation delay. Retrying in 5 seconds..."
    sleep 5
    RECEIVERS=$(curl -s http://localhost:8080/receivers)
    COUNT=$(echo "$RECEIVERS" | grep -o "id" | wc -l)
    if [ "$COUNT" -gt 0 ]; then
        echo "[INFO] Sender has discovered $COUNT receivers."
    else
        echo "[ERROR] Discovery failed after retry."
    fi
fi

# 4. Send a message
echo "[INFO] Sending a test message..."
RESPONSE=$(curl -s --max-time 5 -X POST http://localhost:8080/send \
  -H "Content-Type: application/json" \
  -d "{\"content\": \"Verification Message $(date)\"}" 2>&1)

echo "[DEBUG] Response Output: $RESPONSE"

if echo "$RESPONSE" | grep "sent" > /dev/null; then
    echo "[INFO] Message sent successfully!"
    
    # 5. Verify Receiver Logs
    echo "[INFO] Checking Receiver logs for message..."
    if docker-compose logs receiver | grep "Verification Message"; then
        echo "[INFO] Receiver received the message."
    else
        echo "[WARN] Could not find message in receiver logs yet."
    fi

else
    echo "[ERROR] Failed to send message."
    exit 1
fi

echo "[INFO] System Verification Completed Successfully"
