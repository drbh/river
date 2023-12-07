#!/bin/bash -l

echo "🐟 running river 🐟"

# Run river-zmq-proxy
river-zmq-proxy &

sleep 1
echo "  3 seconds..."
sleep 1
echo "  2 seconds..."
sleep 1
echo "  1 seconds..."
sleep 1

# Run river-serve
river-serve