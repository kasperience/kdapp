# onchain_demo.ps1 — run an on-chain demo end-to-end (Windows PowerShell)
param(
  [switch]$NoStartEngine,
  [int]$EpisodeId = 20,
  [string]$Key,                          # optional: env/file fallback used if omitted
  [UInt64]$Amount = 100000000,           # 1 KAS (atoms)
  [int[]]$Numbers = @(1,2,3,4,5),        # demo numbers
  [int]$WaitDrawSeconds = 16,
  [switch]$Mainnet,                      # default: testnet-10
  [string]$WrpcUrl,                      # optional; defaults via resolver or WRPC_URL
  [switch]$UseBin                        # use built binary instead of cargo run
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Resolve paths
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir '..\..')

# Resolve dev key: CLI -> env KASPA_PRIVATE_KEY -> env KAS_DRAW_DEV_SK -> dev.key files
function Resolve-DevKey {
  param([string]$Cli)
  if ($Cli) { return $Cli }
  if ($env:KASPA_PRIVATE_KEY) { return $env:KASPA_PRIVATE_KEY }
  if ($env:KAS_DRAW_DEV_SK) { return $env:KAS_DRAW_DEV_SK }
  $candidates = @(
    (Join-Path $scriptDir 'dev.key'),
    (Join-Path $repoRoot 'dev.key'),
    (Join-Path $repoRoot '.dev.key')
  )
  foreach ($p in $candidates) { if (Test-Path $p) { $txt = (Get-Content $p -Raw).Trim(); if ($txt) { return $txt } } }
  return $null
}

function Engine-Cmd {
  param([string]$WrpcUrl,[switch]$Mainnet)
  $cargo = 'cargo'
  $args = @('run','-p','kas-draw','--','engine')
  if ($Mainnet) { $args += '--mainnet' }
  if ($WrpcUrl) { $args += @('--wrpc-url', $WrpcUrl) }
  return $cargo, $args
}

function Submit-Cmd {
  param([ValidateSet('new','buy','draw','claim')]$Kind,[int]$EpisodeId,[string]$Key,[UInt64]$Amount,[int[]]$Numbers,[string]$Entropy,[switch]$UseBin,[string]$WrpcUrl,[switch]$Mainnet)
  $cmd = 'cargo'
  $args = @('run','-q','-p','kas-draw','--')
  if ($UseBin) { $cmd = (Join-Path $repoRoot 'target\debug\kas-draw.exe'); $args = @() }
  switch ($Kind) {
    'new'  { $args += @('submit-new',  '--episode-id',"$EpisodeId", '--kaspa-private-key', $Key) }
    'buy'  { $args += @('submit-buy',  '--episode-id',"$EpisodeId", '--kaspa-private-key', $Key, '--amount',"$Amount") + ($Numbers | % { "$_" }) }
    'draw' { $args += @('submit-draw', '--episode-id',"$EpisodeId", '--kaspa-private-key', $Key, '--entropy','demo') }
    default { throw "Unsupported kind: $Kind" }
  }
  if ($Mainnet) { $args += '--mainnet' }
  if ($WrpcUrl) { $args += @('--wrpc-url', $WrpcUrl) }
  Write-Host ("Submitting {0}..." -f $Kind.ToUpper())
  & $cmd @args
  if ($LASTEXITCODE -ne 0) { throw "submit-$Kind failed" }
}

# Start engine (separate window) if requested
if (-not $NoStartEngine) {
  $wrpc = if ($WrpcUrl) { $WrpcUrl } elseif ($env:WRPC_URL) { $env:WRPC_URL } else { $null }
  $cmd,$args = Engine-Cmd -WrpcUrl $wrpc -Mainnet:$Mainnet
  $netLabel = if ($Mainnet) { 'mainnet' } else { 'testnet-10' }
  Write-Host ("Starting engine (new window) with {0}..." -f $netLabel)
  $engineWork = Join-Path $scriptDir '.engine-run-l1'
  if (-not (Test-Path $engineWork)) { New-Item -ItemType Directory -Path $engineWork | Out-Null }
  $engineTarget = Join-Path $engineWork 'target'
  $cmdLine = 'set "CARGO_TARGET_DIR={0}" && {1} {2}' -f $engineTarget, $cmd, ($args -join ' ')
  Start-Process -FilePath 'cmd.exe' -ArgumentList '/k', $cmdLine -WorkingDirectory $engineWork -WindowStyle Normal
  Start-Sleep -Seconds 3
}

# Resolve key and WRPC URL
$resolvedKey = Resolve-DevKey -Cli $Key
if (-not $resolvedKey) { throw "No private key provided. Pass -Key, set KASPA_PRIVATE_KEY/KAS_DRAW_DEV_SK, or create examples/kas-draw/dev.key" }
$wrpc = if ($WrpcUrl) { $WrpcUrl } elseif ($env:WRPC_URL) { $env:WRPC_URL } else { $null }

# Submit sequence: NEW -> BUY -> wait -> DRAW
Submit-Cmd -Kind new  -EpisodeId $EpisodeId -Key $resolvedKey -Amount $Amount -Numbers $Numbers -UseBin:$UseBin -WrpcUrl $wrpc -Mainnet:$Mainnet
Start-Sleep -Seconds 3
Submit-Cmd -Kind buy  -EpisodeId $EpisodeId -Key $resolvedKey -Amount $Amount -Numbers $Numbers -UseBin:$UseBin -WrpcUrl $wrpc -Mainnet:$Mainnet
Write-Host ("Waiting ~{0}s for draw window..." -f $WaitDrawSeconds)
Start-Sleep -Seconds $WaitDrawSeconds
Submit-Cmd -Kind draw -EpisodeId $EpisodeId -Key $resolvedKey -Amount $Amount -Numbers $Numbers -UseBin:$UseBin -WrpcUrl $wrpc -Mainnet:$Mainnet

Write-Host "Done. Check the engine window for NEW/BUY/DRAW and real txids." -ForegroundColor Green
