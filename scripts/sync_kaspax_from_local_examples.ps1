param(
    [string]$KaspaXRoot = "examples/kaspa-linux/kaspax",
    [string]$ExamplesRoot,
    [string[]]$Examples,
    [switch]$DryRun,
    [switch]$IncludeCore
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Info($msg) { Write-Host "[info] $msg" -ForegroundColor Cyan }
function Write-Warn($msg) { Write-Host "[warn] $msg" -ForegroundColor Yellow }
function Write-Err($msg)  { Write-Host "[err ] $msg" -ForegroundColor Red }

if (-not (Get-Command robocopy.exe -ErrorAction SilentlyContinue)) {
    Write-Err "robocopy is required (Windows). On Linux/macOS use rsync manually."
    exit 1
}

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $ExamplesRoot) { $ExamplesRoot = Join-Path $RepoRoot "examples" }

$KaspaXRoot = (Resolve-Path $KaspaXRoot).Path
$ExamplesRoot = (Resolve-Path $ExamplesRoot).Path

$sourceNames = Get-ChildItem -Directory $ExamplesRoot | Select-Object -ExpandProperty Name
$destRoot    = Join-Path $KaspaXRoot "applications/kdapps"
if (-not (Test-Path $destRoot)) { Write-Err "KaspaX kdapps dir missing: $destRoot"; exit 1 }
$destNames   = Get-ChildItem -Directory $destRoot | Select-Object -ExpandProperty Name

if ($Examples -and $Examples.Count -gt 0) {
    $targets = @()
    foreach ($e in $Examples) {
        if ($sourceNames -contains $e) { $targets += $e } else { Write-Warn "No such example locally: $e" }
    }
} else {
    $targets = $sourceNames | Where-Object { $destNames -contains $_ }
}

if ($targets.Count -eq 0) { Write-Warn "No targets to sync"; exit 0 }

Write-Info ("Syncing: " + ($targets -join ', '))

# Common excludes
$xd = @('.git','target','target-join','node_modules','dist','build','.next','.pytest_cache','.mypy_cache','.venv','.cargo')
$xf = @('Thumbs.db','.DS_Store')

foreach ($ex in $targets) {
    $src = Join-Path $ExamplesRoot $ex
    $dst = Join-Path $destRoot $ex
    if (-not (Test-Path $dst)) { New-Item -ItemType Directory -Path $dst | Out-Null }
    $args = @(
        '"' + $src + '"',
        '"' + $dst + '"',
        '/MIR','/R:1','/W:1','/NFL','/NDL','/NJH','/NJS','/NP'
    )
    foreach ($d in $xd) { $args += '/XD'; $args += $d }
    foreach ($f in $xf) { $args += '/XF'; $args += $f }
    if ($DryRun) { $args += '/L' }

    Write-Info "robocopy $ex -> kdapps/$ex"
    $argList = $args -join ' '
    $proc = Start-Process -FilePath robocopy.exe -ArgumentList $argList -NoNewWindow -PassThru -Wait
    $code = $proc.ExitCode
    # robocopy uses bit-coded exit codes; treat >=8 as failure
    if ($code -ge 8) {
        Write-Err "robocopy failed for $ex (code $code)"
        exit $code
    }
}

# Optionally sync the core kdapp crate to keep APIs aligned
if ($IncludeCore) {
    $coreSrc = Join-Path $RepoRoot 'kdapp'
    $coreDst = Join-Path $destRoot 'kdapp'
    if (-not (Test-Path $coreDst)) { New-Item -ItemType Directory -Path $coreDst | Out-Null }
    $argsCore = @(
        '"' + $coreSrc + '"',
        '"' + $coreDst + '"',
        '/MIR','/R:1','/W:1','/NFL','/NDL','/NJH','/NJS','/NP'
    )
    foreach ($d in $xd) { $argsCore += '/XD'; $argsCore += $d }
    foreach ($f in $xf) { $argsCore += '/XF'; $argsCore += $f }
    Write-Info "robocopy kdapp core -> kdapps/kdapp"
    $procC = Start-Process -FilePath robocopy.exe -ArgumentList ($argsCore -join ' ') -NoNewWindow -PassThru -Wait
    if ($procC.ExitCode -ge 8) { Write-Err "robocopy for kdapp core failed (code $($procC.ExitCode))"; exit $procC.ExitCode }
}

Write-Info "Done. Review changes: git -C $KaspaXRoot status -s"
