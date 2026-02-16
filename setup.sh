#!/bin/sh
set -e

echo "Setting up Garage..."
GARAGE=/garage
CFG=/etc/garage.toml

until $GARAGE -c $CFG status >/dev/null 2>&1; do
  echo "Waiting for Garage..."
  sleep 1
done

STATUS=$($GARAGE -c $CFG status 2>/dev/null)
if echo "$STATUS" | grep -q "NO ROLE ASSIGNED"; then
  echo "Configuring layout..."
  NODE_ID=$(echo "$STATUS" | grep "NO ROLE ASSIGNED" | awk '{print $1}' | head -1)
  $GARAGE -c $CFG layout assign "$NODE_ID" -z dc1 -c 1G
  
  LAYOUT_VERSION=$($GARAGE -c $CFG layout show 2>/dev/null | grep -o 'version: [0-9]*' | awk '{print $2}' || echo "1")
  if [ -z "$LAYOUT_VERSION" ] || [ "$LAYOUT_VERSION" = "0" ]; then
    LAYOUT_VERSION=1
  fi
  echo "Applying layout version $LAYOUT_VERSION..."
  $GARAGE -c $CFG layout apply --version "$LAYOUT_VERSION" || $GARAGE -c $CFG layout apply
  
  echo "Waiting for layout to be applied..."
  for i in $(seq 1 30); do
    NEW_STATUS=$($GARAGE -c $CFG status 2>/dev/null || true)
    if ! echo "$NEW_STATUS" | grep -q "NO ROLE ASSIGNED"; then
      echo "Layout applied successfully!"
      break
    fi
    echo "Still waiting for layout... ($i)"
    sleep 1
  done
fi

echo "Creating bucket ${S3_BUCKET}..."
$GARAGE -c $CFG bucket create "${S3_BUCKET}" 2>/dev/null || true

echo "Importing key..."
$GARAGE -c $CFG key import \
  --yes \
  "${AWS_ACCESS_KEY_ID}" \
  "${AWS_SECRET_ACCESS_KEY}" \
  -n app-key 2>/dev/null || true

echo "Setting bucket permissions..."
$GARAGE -c $CFG bucket allow --read --write "${S3_BUCKET}" --key "${AWS_ACCESS_KEY_ID}" 2>/dev/null || true

echo "Garage setup complete!"
