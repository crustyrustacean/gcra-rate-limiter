#!/bin/bash

echo "Testing GCRA Rate Limiter"
echo "========================="
echo ""

# Test rapid requests
echo "Sending 20 rapid requests..."
for i in {1..20}
do
    response=$(curl -s -w "%{http_code}" http://localhost:8000)
    status_code=${response: -3}
    body=${response%???}
    
    if [ "$status_code" = "200" ]; then
        echo "✅ Request $i: SUCCESS (HTTP $status_code)"
    elif [ "$status_code" = "429" ]; then
        echo "❌ Request $i: RATE LIMITED (HTTP $status_code)"
    else
        echo "⚠️  Request $i: UNEXPECTED (HTTP $status_code)"
    fi
    
    # Small delay to see the output clearly
    sleep 0.1
done

echo ""
echo "Waiting 3 seconds..."
sleep 3

echo ""
echo "Testing after cooldown period..."
for i in {1..5}
do
    response=$(curl -s -w "%{http_code}" http://localhost:8000)
    status_code=${response: -3}
    
    if [ "$status_code" = "200" ]; then
        echo "✅ Cooldown Request $i: SUCCESS (HTTP $status_code)"
    else
        echo "❌ Cooldown Request $i: FAILED (HTTP $status_code)"
    fi
    
    sleep 0.5
done

echo ""
echo "Test complete!"