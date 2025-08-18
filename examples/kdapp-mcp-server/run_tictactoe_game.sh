#!/bin/bash

# TicTacToe AI Game Runner
# This script sets up and runs a TicTacToe game between two AI agents

echo "=== TicTacToe AI Game Setup ==="

# Check if required commands are available
if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is not installed"
    exit 1
fi

if ! command -v python3 &> /dev/null; then
    echo "Error: python3 is not installed"
    exit 1
fi

if ! command -v ollama &> /dev/null; then
    echo "Error: ollama is not installed"
    exit 1
fi

# Check if Ollama has the gemma3 model
if ! ollama list | grep -q gemma3; then
    echo "Error: gemma3 model not found in Ollama"
    echo "Please run: ollama pull gemma3"
    exit 1
fi

# Check if the HTTP agent is running
if ! curl -s http://127.0.0.1:1234/v1/models &> /dev/null; then
    echo "Warning: HTTP AI agent not detected at http://127.0.0.1:1234"
    echo "Please start your HTTP AI agent before continuing"
    read -p "Continue anyway? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

echo "All prerequisites checked!"

# Install Python requirements if needed
if ! python3 -c "import requests" &> /dev/null; then
    echo "Installing Python requirements..."
    pip install requests
fi

# Build the kdapp MCP server
echo "Building kdapp MCP server..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "Error: Failed to build kdapp MCP server"
    exit 1
fi

echo "=== Starting TicTacToe Game ==="
echo "The game will start shortly..."
echo "Press Ctrl+C to stop the game"

# Run the game coordinator
python3 tictactoe_coordinator.py