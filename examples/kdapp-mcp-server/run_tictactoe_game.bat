@echo off
REM TicTacToe AI Game Runner for Windows
REM This script sets up and runs a TicTacToe game between two AI agents

echo === TicTacToe AI Game Setup ===

REM Check if required commands are available
where cargo >nul 2>&1
if %errorlevel% neq 0 (
    echo Error: cargo is not installed
    exit /b 1
)

where python >nul 2>&1
if %errorlevel% neq 0 (
    echo Error: python is not installed
    exit /b 1
)

where ollama >nul 2>&1
if %errorlevel% neq 0 (
    echo Error: ollama is not installed
    exit /b 1
)

REM Check if Ollama has the gemma3 model
ollama list | findstr gemma3 >nul 2>&1
if %errorlevel% neq 0 (
    echo Error: gemma3 model not found in Ollama
    echo Please run: ollama pull gemma3
    exit /b 1
)

REM Check if the HTTP agent is running
curl -s http://127.0.0.1:1234/v1/models >nul 2>&1
if %errorlevel% neq 0 (
    echo Warning: HTTP AI agent not detected at http://127.0.0.1:1234
    set /p choice="Continue anyway? (y/N): "
    if /i not "%choice%"=="y" (
        exit /b 1
    )
)

echo All prerequisites checked!

REM Install Python requirements if needed
python -c "import requests" >nul 2>&1
if %errorlevel% neq 0 (
    echo Installing Python requirements...
    pip install requests
)

REM Build the kdapp MCP server
echo Building kdapp MCP server...
cargo build --release

if %errorlevel% neq 0 (
    echo Error: Failed to build kdapp MCP server
    exit /b 1
)

echo === Starting TicTacToe Game ===
echo The game will start shortly...
echo Press Ctrl+C to stop the game

REM Run the game coordinator
python tictactoe_coordinator.py