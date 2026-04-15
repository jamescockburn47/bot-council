#!/usr/bin/env bash
set -euo pipefail

BASE_URL="http://localhost:3100"
TOKEN="test-token"

echo "=== Bot Council Phase 1 Smoke Test ==="

# Register 5 bots
for i in 1 2 3 4 5; do
  PORT=$((3199 + i))
  echo "Registering bot-${i}..."
  curl -s -X POST "${BASE_URL}/bots" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"smoke-bot-${i}\",\"endpoint_url\":\"http://localhost:${PORT}/debate\",\"token\":\"bot-token-${i}\"}" | jq .
done

echo ""
echo "Listing bots..."
BOTS=$(curl -s "${BASE_URL}/bots" -H "Authorization: Bearer ${TOKEN}")
echo "${BOTS}" | jq '.[] | {id, name}'

echo ""
echo "Creating multi-round debate..."
DEBATE=$(curl -s -X POST "${BASE_URL}/debates" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"topic":"Should AI systems be required to explain their reasoning?"}')
echo "${DEBATE}" | jq .
DEBATE_ID=$(echo "${DEBATE}" | jq -r '.id')

echo ""
echo "Waiting for debate to complete (5-round protocol)..."
for i in $(seq 1 60); do
  STATUS=$(curl -s "${BASE_URL}/debates/${DEBATE_ID}" -H "Authorization: Bearer ${TOKEN}" | jq -r '.status')
  echo "  Status: ${STATUS}"
  if [ "${STATUS}" = "complete" ] || [ "${STATUS}" = "failed" ]; then
    break
  fi
  sleep 5
done

echo ""
echo "Fetching transcript..."
curl -s "${BASE_URL}/debates/${DEBATE_ID}/transcript" -H "Authorization: Bearer ${TOKEN}" | jq '.rounds | length'

echo ""
echo "Fetching synthesis..."
curl -s "${BASE_URL}/debates/${DEBATE_ID}/synthesis" -H "Authorization: Bearer ${TOKEN}" | jq .

echo ""
echo "=== Smoke test complete ==="
