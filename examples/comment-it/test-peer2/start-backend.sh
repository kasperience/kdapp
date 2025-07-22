#!/bin/bash
# Test-peer2 Backend Startup Script
# This script starts the comment-it backend with test-peer2's separate wallet

echo "🚀 Starting test-peer2 backend with separate wallet..."
echo "📁 Using wallet directory: test-peer2/.kaspa-auth/"

# Set environment variable to use test-peer2's wallet directory
export KASPA_AUTH_WALLET_DIR="test-peer2/.kaspa-auth"

# Change to main project directory to run the backend
cd ..

# Start the backend on a different port to avoid conflicts
cargo run --bin comment-it -- http-peer --port 8081

echo "✅ Test-peer2 backend started on port 8081"
echo "🔑 Using wallet: test-peer2/.kaspa-auth/participant-peer-wallet.key"