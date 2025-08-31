# offchain_demo.ps1 — run an off-chain demo end-to-end (Windows PowerShell)
param(
  [switch]$NoStartEngine,
  [int]$EpisodeId = 10,
  [string]$Key = '<put_testnet_private_key_hex_here>',
  [string]$Bind = '127.0.0.1:18181',
  [UInt64]$Amount = 100000000,
  [int[]]$Numbers = @(1,2,3,4,5),
  [int]$WaitDrawSeconds = 16,
  [switch]$UseBin,
  [switch]$NoAck
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# --- Config ---
$EPISODE_ID = $EpisodeId
$KEY = $Key  # DO NOT commit real keys
$CARGO = 'cargo'

# --- Resolve paths ---
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
# This script lives in examples/kas-draw; repo root is two levels up
$repoRoot = Resolve-Path (Join-Path $scriptDir '..\..')
# Run sender commands from the script folder (per Windows cargo concurrency quirks)
Set-Location $scriptDir

# --- Reset local auto-seq stores (both possible locations) ---
$seqRoot = Join-Path $repoRoot 'target\kas_draw_offchain_seq.txt'
if (Test-Path $seqRoot) { Remove-Item $seqRoot -Force }
$seqEx = Join-Path $scriptDir 'target\kas_draw_offchain_seq.txt'
if (Test-Path $seqEx) { Remove-Item $seqEx -Force }

function Wait-EngineReady {
  param(
    [string]$Endpoint,
    [int]$TimeoutSec = 30
  )
  $parts = $Endpoint.Split(':');
  if ($parts.Count -ne 2) { return $false }
  $port = [int]$parts[1]
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    try {
      $ep = Get-NetUDPEndpoint -LocalPort $port -ErrorAction SilentlyContinue
      if ($ep) { return $true }
    } catch {}
    Start-Sleep -Milliseconds 250
  }
  return $false
}

if (-not $NoStartEngine) {
  # If the port is already in use, assume an engine is running and do not start another instance.
  $inUse = $false
  try {
    $parts = $Bind.Split(':'); if ($parts.Count -eq 2) { $p=[int]$parts[1]; $inUse = [bool](Get-NetUDPEndpoint -LocalPort $p -ErrorAction SilentlyContinue) }
  } catch {}
  if ($inUse) {
    Write-Host "Port $Bind already in use; assuming engine is running there. Skipping start." -ForegroundColor Yellow
  } else {
    Write-Host "Starting engine (new window) on $Bind..."
    # Use a separate working dir and target dir to avoid cargo target lock contention and keep TUI isolated
    $engineWork = Join-Path $scriptDir '.engine-run'
    if (-not (Test-Path $engineWork)) { New-Item -ItemType Directory -Path $engineWork | Out-Null }
    $engineTarget = Join-Path $engineWork 'target'
    # Build the cmd line using format operator to avoid PowerShell quoting issues
    $cmdLine = 'set "CARGO_TARGET_DIR={0}" && {1} run -p kas-draw -- offchain-engine --bind {2}' -f $engineTarget, $CARGO, $Bind
    # /k keeps the window open even if the process exits, to inspect errors
    Start-Process -FilePath 'cmd.exe' -ArgumentList '/k', $cmdLine -WorkingDirectory $engineWork -WindowStyle Normal
    if (-not (Wait-EngineReady -Endpoint $Bind -TimeoutSec 30)) {
      Write-Host "Engine did not bind to $Bind within timeout; sending anyway (may fail)." -ForegroundColor Yellow
    }
  }
} else {
  Write-Host "Skipping engine start (NoStartEngine). Ensure offchain-engine runs on $Bind." -ForegroundColor Yellow
}

function Run-Cargo {
  param([string[]]$Args)
  & $CARGO @Args
  if ($LASTEXITCODE -ne 0) { throw "cargo run failed: $Args" }
}

# Send a single offchain message and verify ACK unless -NoAck is set
function Invoke-Send {
  param(
    [Parameter(Mandatory=$true)][ValidateSet('new','cmd','close')] [string]$Type,
    [Parameter(Mandatory=$true)] [int]$EpisodeId,
    [Parameter(Mandatory=$true)] [string]$Router,
    [Parameter(Mandatory=$true)] [int]$Seq,
    [string]$Key,
    [UInt64]$Amount = 0,
    [int[]]$Numbers = @(),
    [string]$Entropy,
    [switch]$UseBin,
    [switch]$NoAck
  )
  $cmd = $CARGO
  $args = @('run','-q','--','offchain-send','--type', $Type, '--episode-id', "$EpisodeId", '--router', "$Router", '--force-seq', "$Seq")
  if ($UseBin) {
    $cmd = (Join-Path $repoRoot 'target\debug\kas-draw.exe')
    $args = @('offchain-send','--type', $Type, '--episode-id', "$EpisodeId", '--router', "$Router", '--force-seq', "$Seq")
  }
  if ($NoAck) { $args += '--no-ack' }
  if ($Type -eq 'new' -and $Key) { $args += @('--kaspa-private-key', "$Key") }
  if ($Type -eq 'cmd') {
    if ($Key) { $args += @('--kaspa-private-key', "$Key") }
    if ($Entropy) { $args += @('--entropy', "$Entropy") }
    if ($Amount -gt 0 -and $Numbers.Length -eq 5) { $args += @('--amount', "$Amount") + ($Numbers | ForEach-Object { "$_" }) }
  }
  Write-Host ("Sending {0} (seq {1})..." -f $Type.ToUpper(), $Seq)
  $out = & $cmd @args 2>&1
  $out | ForEach-Object { Write-Host $_ }
  if (-not $NoAck) {
    $joined = ($out -join "`n")
    if ($joined -notmatch "ack received.*seq $Seq") { throw "No ACK received for seq $Seq" }
  }
}

# New (seq 0) — include participant so Buys are authorized
Invoke-Send -Type 'new' -EpisodeId $EPISODE_ID -Router $Bind -Seq 0 -Key $KEY -UseBin:$UseBin -NoAck:$NoAck

# Buy (seq 1) — signed
Start-Sleep -Seconds 1
Invoke-Send -Type 'cmd' -EpisodeId $EPISODE_ID -Router $Bind -Seq 1 -Key $KEY -Amount $Amount -Numbers $Numbers -UseBin:$UseBin -NoAck:$NoAck

# Draw (seq 2) — wait ~15s window
Write-Host ("Waiting ~{0}s for draw window..." -f $WaitDrawSeconds)
Start-Sleep -Seconds $WaitDrawSeconds
Invoke-Send -Type 'cmd' -EpisodeId $EPISODE_ID -Router $Bind -Seq 2 -Entropy 'demo' -UseBin:$UseBin -NoAck:$NoAck

# Close (seq 3)
Start-Sleep -Seconds 1
Invoke-Send -Type 'close' -EpisodeId $EPISODE_ID -Router $Bind -Seq 3 -UseBin:$UseBin -NoAck:$NoAck

Write-Host "Done. Check the engine window for BUY/DRAW/CLOSE and Finalized." -ForegroundColor Green
