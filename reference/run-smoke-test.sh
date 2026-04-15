#!/bin/bash
set -e

# Kill any existing bots
pkill -f "debate-endpoint-node" 2>/dev/null || true
sleep 1

# Start 3 reference bots
nohup node ~/bot-council/reference/debate-endpoint-node.js 9001 > /tmp/bot1.log 2>&1 &
nohup node ~/bot-council/reference/debate-endpoint-node.js 9002 > /tmp/bot2.log 2>&1 &
nohup node ~/bot-council/reference/debate-endpoint-node.js 9003 > /tmp/bot3.log 2>&1 &
sleep 2

# Verify bots respond
echo "=== Testing bot connectivity ==="
curl -sf -X POST http://localhost:9001/debate -H "Content-Type: application/json" -d '{"round":0,"prompt":"test","session_id":"t"}' && echo " [9001 OK]" || echo " [9001 FAIL]"
curl -sf -X POST http://localhost:9002/debate -H "Content-Type: application/json" -d '{"round":0,"prompt":"test","session_id":"t"}' && echo " [9002 OK]" || echo " [9002 FAIL]"
curl -sf -X POST http://localhost:9003/debate -H "Content-Type: application/json" -d '{"round":0,"prompt":"test","session_id":"t"}' && echo " [9003 OK]" || echo " [9003 FAIL]"

# Check harness health
echo ""
echo "=== Harness health ==="
curl -s http://localhost:3100/health
echo ""

# Create debate
echo ""
echo "=== Creating debate ==="
RESULT=$(curl -s -X POST http://localhost:3100/debates -H "Content-Type: application/json" -d '{"topic":"Should AI-generated evidence be admissible in court?"}')
echo "$RESULT" | python3 -m json.tool
DEBATE_ID=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

# Wait and poll
echo ""
echo "=== Waiting for completion ==="
for i in 1 2 3 4 5 6 7 8 9 10; do
    sleep 2
    STATUS=$(curl -s "http://localhost:3100/debates/$DEBATE_ID" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])")
    echo "Poll $i: status=$STATUS"
    if [ "$STATUS" = "complete" ] || [ "$STATUS" = "failed" ]; then
        break
    fi
done

# Fetch final results
echo ""
echo "=== Final results ==="
curl -s "http://localhost:3100/debates/$DEBATE_ID" | python3 -m json.tool
