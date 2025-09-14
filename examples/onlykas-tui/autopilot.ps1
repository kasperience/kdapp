param(
  [string]$WrpcUrl = "",
  [switch]$Mainnet,
  [int]$EpisodeId = 42,
  [int]$MerchantPort = 3000,
  [int]$WebhookPort = 9655,
  [int]$WatcherPort = 9591,
  [int]$GuardianPort = 9650,
  [switch]$Debug,
  [switch]$Stop
)

$procNames = @('kdapp-merchant','onlykas-tui','guardian-service')
if ($Stop) {
  foreach ($n in $procNames) {
    Get-Process $n -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  }
  $jobs = Get-Job -ErrorAction SilentlyContinue
  if ($jobs) { $jobs | Stop-Job -ErrorAction SilentlyContinue; $jobs | Remove-Job -ErrorAction SilentlyContinue }
  Write-Host "Stopped onlykas processes" -ForegroundColor Yellow
  exit 0
}

function New-Hex {
  param([int]$Bytes)
  $buf = New-Object byte[] $Bytes
  [System.Security.Cryptography.RandomNumberGenerator]::Fill($buf)
  ($buf | ForEach-Object { $_.ToString('x2') }) -join ''
}

function Read-EnvFile {
  param([string]$Path)
  $map = @{}
  if (Test-Path $Path) {
    Get-Content $Path | ForEach-Object {
      $line = $_.Trim()
      if (-not $line -or $line.StartsWith('#')) { return }
      $kv = $line -split '=',2
      if ($kv.Length -eq 2) { $map[$kv[0]] = $kv[1] }
    }
  }
  return $map
}

function Append-EnvLine {
  param([string]$Path,[string]$Key,[string]$Value)
  if (-not (Test-Path $Path)) { New-Item -ItemType File -Path $Path | Out-Null }
  if (-not (Select-String -Path $Path -Pattern "^$Key=" -Quiet)) {
    Add-Content -Path $Path -Value "$Key=$Value"
  }
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
$EnvPath = Join-Path $ScriptDir ".env"
$ExamplePath = Join-Path $ScriptDir ".env.example"

if (-not (Test-Path $EnvPath) -and (Test-Path $ExamplePath)) {
  Copy-Item $ExamplePath $EnvPath
  Write-Host "Created .env from template" -ForegroundColor DarkGray
}

$envmap = Read-EnvFile -Path $EnvPath

if (-not $WrpcUrl -and $envmap.ContainsKey('WRPC_URL')) { $WrpcUrl = $envmap['WRPC_URL'] }
if (-not $EpisodeId -and $envmap.ContainsKey('EPISODE_ID')) { $EpisodeId = [int]$envmap['EPISODE_ID'] }
if (-not $MerchantPort -and $envmap.ContainsKey('MERCHANT_PORT')) { $MerchantPort = [int]$envmap['MERCHANT_PORT'] }
if (-not $WebhookPort -and $envmap.ContainsKey('WEBHOOK_PORT')) { $WebhookPort = [int]$envmap['WEBHOOK_PORT'] }
if (-not $GuardianPort -and $envmap.ContainsKey('GUARDIAN_PORT')) { $GuardianPort = [int]$envmap['GUARDIAN_PORT'] }
if (-not $Mainnet -and $envmap.ContainsKey('MAINNET')) { if ($envmap['MAINNET'] -eq '1') { $Mainnet = $true } }


if ($envmap.ContainsKey('API_KEY')) { $ApiKey = $envmap['API_KEY'] } else { $ApiKey = New-Hex -Bytes 16; Append-EnvLine -Path $EnvPath -Key 'API_KEY' -Value $ApiKey }
if ($envmap.ContainsKey('WEBHOOK_SECRET')) { $WebhookSecret = $envmap['WEBHOOK_SECRET'] } else { $WebhookSecret = New-Hex -Bytes 32; Append-EnvLine -Path $EnvPath -Key 'WEBHOOK_SECRET' -Value $WebhookSecret }
if ($envmap.ContainsKey('MERCHANT_SK')) { $MerchantSk = $envmap['MERCHANT_SK'] } else { $MerchantSk = New-Hex -Bytes 32; Append-EnvLine -Path $EnvPath -Key 'MERCHANT_SK' -Value $MerchantSk }
if ($envmap.ContainsKey('KASPA_SK')) { $KaspaSk = $envmap['KASPA_SK'] } else { $KaspaSk = New-Hex -Bytes 32; Append-EnvLine -Path $EnvPath -Key 'KASPA_SK' -Value $KaspaSk }

# Defensive trim to avoid hidden CR/LF or spaces from environment/files
$ApiKey = ($ApiKey | ForEach-Object { $_.Trim() })
$WebhookSecret = ($WebhookSecret | ForEach-Object { $_.Trim() })
$MerchantSk = ($MerchantSk | ForEach-Object { $_.Trim() })
$KaspaSk = ($KaspaSk | ForEach-Object { $_.Trim() })

if ($envmap.ContainsKey('MERCHANT_DB_PATH')) { $env:MERCHANT_DB_PATH = $envmap['MERCHANT_DB_PATH'] } else { $env:MERCHANT_DB_PATH = "merchant-live.db"; Append-EnvLine -Path $EnvPath -Key 'MERCHANT_DB_PATH' -Value $env:MERCHANT_DB_PATH }
if ($WrpcUrl -and $WrpcUrl -ne 'wss://node:port') {
  Append-EnvLine -Path $EnvPath -Key 'WRPC_URL' -Value $WrpcUrl
}
Append-EnvLine -Path $EnvPath -Key 'MAINNET' -Value ($Mainnet.IsPresent ? '1' : '0')
Append-EnvLine -Path $EnvPath -Key 'EPISODE_ID' -Value $EpisodeId
Append-EnvLine -Path $EnvPath -Key 'MERCHANT_PORT' -Value $MerchantPort
Append-EnvLine -Path $EnvPath -Key 'WEBHOOK_PORT' -Value $WebhookPort
Append-EnvLine -Path $EnvPath -Key 'GUARDIAN_PORT' -Value $GuardianPort

$netArgs = @()
if ($Mainnet) { $netArgs += "--mainnet" }

if (-not $env:RUST_LOG) { $env:RUST_LOG = 'info,kdapp=info,kdapp_merchant=info' }

# Start merchant server + proxy in one process (shared engine)
$merchantArgs = @("run","-p","kdapp-merchant","--","serve-proxy",
  "--bind","127.0.0.1:$MerchantPort",
  "--episode-id","$EpisodeId",
  "--api-key","$ApiKey",
  "--merchant-private-key","$MerchantSk",
  "--webhook-url","http://127.0.0.1:$WebhookPort/hook",
  "--webhook-secret","$WebhookSecret"
)
if ($WrpcUrl -and $WrpcUrl -ne 'wss://node:port') { $merchantArgs += @("--wrpc-url", $WrpcUrl) }
if ($Mainnet) { $merchantArgs += "--mainnet" }
Start-Process -FilePath cargo -ArgumentList $merchantArgs -NoNewWindow -RedirectStandardOutput merchant-serve.out -RedirectStandardError merchant-serve.err

Start-Sleep -Seconds 2

# Start watcher (UDP listener + optional HTTP metrics)
$watcherArgs = @("run","-p","kdapp-merchant","--","watcher",
  "--bind","127.0.0.1:9590",
  "--kaspa-private-key",$KaspaSk,
  "--http-port",$WatcherPort
)
if ($WrpcUrl -and $WrpcUrl -ne 'wss://node:port') { $watcherArgs += @("--wrpc-url", $WrpcUrl) }
if ($Mainnet) { $watcherArgs += "--mainnet" }
Start-Process -FilePath cargo -ArgumentList $watcherArgs -NoNewWindow -RedirectStandardOutput watcher.out -RedirectStandardError watcher.err

# Start guardian
$guardianArgs = @("run","-p","kdapp-guardian","--bin","guardian-service","--",
  "--listen-addr","127.0.0.1:$GuardianPort"
)
Start-Process -FilePath cargo -ArgumentList $guardianArgs -NoNewWindow -RedirectStandardOutput guardian.out -RedirectStandardError guardian.err

Start-Sleep -Seconds 2

# Optionally start a guardian demo (no-op metrics)
# $guardianArgs = @("run","-p","kdapp-guardian","--bin","guardian-service","--","--listen-addr","127.0.0.1:$GuardianPort")
# Start-Process -FilePath cargo -ArgumentList $guardianArgs -NoNewWindow -RedirectStandardOutput guardian.out -RedirectStandardError guardian.err

Write-Host "API key:        $ApiKey" -ForegroundColor Cyan
Write-Host "Webhook secret: $WebhookSecret" -ForegroundColor Cyan
Write-Host "Merchant SK:    $MerchantSk" -ForegroundColor DarkGray
Write-Host "Kaspa SK:       $KaspaSk" -ForegroundColor DarkGray
Write-Host "Episode ID:     $EpisodeId" -ForegroundColor Cyan
Write-Host "Merchant URL:   http://127.0.0.1:$MerchantPort" -ForegroundColor Cyan
if ($Debug) {
  Write-Host -NoNewline "Kaspa Address:  "
  & cargo run -p kdapp-merchant -- kaspa-addr --kaspa-private-key $KaspaSk @($netArgs)
}

Write-Host "Starting onlykas-tui ..." -ForegroundColor Green
$tuiArgs = @("run","-p","onlykas-tui","--",
  "--merchant-url","http://127.0.0.1:$MerchantPort",
  "--guardian-url","http://127.0.0.1:$GuardianPort",
  "--webhook-secret","$WebhookSecret",
  "--api-key","$ApiKey",
  "--webhook-port","$WebhookPort"
)
if ($Debug) {
  Start-Job -ScriptBlock { Get-Content -Path 'merchant-serve.out' -Wait } | Out-Null
  Start-Job -ScriptBlock { Get-Content -Path 'merchant-serve.err' -Wait } | Out-Null
  Start-Job -ScriptBlock { Get-Content -Path 'watcher.out' -Wait } | Out-Null
  Start-Job -ScriptBlock { Get-Content -Path 'watcher.err' -Wait } | Out-Null
  Start-Job -ScriptBlock { Get-Content -Path 'guardian.out' -Wait } | Out-Null
  Start-Job -ScriptBlock { Get-Content -Path 'guardian.err' -Wait } | Out-Null
}
# Run TUI in the same window so keypresses and environment apply here
& cargo @tuiArgs
