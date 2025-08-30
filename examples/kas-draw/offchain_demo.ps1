# offchain_demo.ps1 — run an off-chain demo end-to-end (Windows PowerShell)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# --- Config ---
$EPISODE_ID = 10
$KEY = '<put_testnet_private_key_hex_here>'  # DO NOT commit real keys
$CARGO = 'cargo'

# --- Resolve repo root ---
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
# This script lives in examples/kas-draw; repo root is two levels up
$repoRoot = Resolve-Path (Join-Path $scriptDir '..\..')
Set-Location $repoRoot

# --- Reset local auto-seq stores (both possible locations) ---
$seqRoot = Join-Path $repoRoot 'target\kas_draw_offchain_seq.txt'
if (Test-Path $seqRoot) { Remove-Item $seqRoot -Force }
$seqEx = Join-Path $scriptDir 'target\kas_draw_offchain_seq.txt'
if (Test-Path $seqEx) { Remove-Item $seqEx -Force }

Write-Host "Starting engine (new window)..."
Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', "$CARGO run -p kas-draw -- offchain-engine" -WorkingDirectory $repoRoot -WindowStyle Normal
Start-Sleep -Seconds 3

function Run-Cargo {
  param([string[]]$Args)
  & $CARGO @Args
  if ($LASTEXITCODE -ne 0) { throw "cargo run failed: $Args" }
}

# New (seq 0) — include participant so Buys are authorized
Write-Host "Sending NEW (seq 0)..."
Run-Cargo @('run','-p','kas-draw','--','offchain-send','--type','new','--episode-id',"$EPISODE_ID",'--kaspa-private-key',"$KEY",'--force-seq','0')

# Buy (seq 1) — signed
Start-Sleep -Seconds 1
Write-Host "Sending BUY (seq 1)..."
Run-Cargo @('run','-p','kas-draw','--','offchain-send','--type','cmd','--episode-id',"$EPISODE_ID",'--kaspa-private-key',"$KEY",'--amount','100000000','1','2','3','4','5','--force-seq','1')

# Draw (seq 2) — wait ~15s window
Write-Host "Waiting ~16s for draw window..."; Start-Sleep -Seconds 16
Write-Host "Sending DRAW (seq 2)..."
Run-Cargo @('run','-p','kas-draw','--','offchain-send','--type','cmd','--episode-id',"$EPISODE_ID",'--entropy','demo','--force-seq','2')

# Close (seq 3)
Start-Sleep -Seconds 1
Write-Host "Sending CLOSE (seq 3)..."
Run-Cargo @('run','-p','kas-draw','--','offchain-send','--type','close','--episode-id',"$EPISODE_ID",'--force-seq','3')

Write-Host "Done. Check the engine window for BUY/DRAW/CLOSE and Finalized." -ForegroundColor Green

