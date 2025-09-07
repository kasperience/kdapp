param(
    [Parameter(Mandatory=$true)]
    [string]$KaspaXRoot,

    [string]$OutputDir = "PATCHES",

    # Optional: limit to specific example names (must match directory names)
    [string[]]$Examples
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Info($msg) { Write-Host "[info] $msg" -ForegroundColor Cyan }
function Write-Warn($msg) { Write-Host "[warn] $msg" -ForegroundColor Yellow }
function Write-Err($msg)  { Write-Host "[err ] $msg" -ForegroundColor Red }

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Err "git not found in PATH"
    exit 1
}

$KaspaXRoot = (Resolve-Path $KaspaXRoot).Path
$RepoRoot   = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

$kxKdapps   = Join-Path $KaspaXRoot "applications/kdapps"
$localExamplesRoot = Join-Path $RepoRoot "examples"
$outputPath = Join-Path $RepoRoot $OutputDir

if (-not (Test-Path $kxKdapps)) {
    Write-Err "KaspaX kdapps dir not found: $kxKdapps"
    exit 1
}
if (-not (Test-Path $localExamplesRoot)) {
    Write-Err "Local examples dir not found: $localExamplesRoot"
    exit 1
}
if (-not (Test-Path $outputPath)) {
    New-Item -ItemType Directory -Path $outputPath | Out-Null
}

$local = Get-ChildItem -Directory $localExamplesRoot | Select-Object -ExpandProperty Name
$kx    = Get-ChildItem -Directory $kxKdapps         | Select-Object -ExpandProperty Name

# Determine targets: either provided, or intersection of local examples and KaspaX kdapps
$targets = @()
if ($Examples -and $Examples.Count -gt 0) {
    foreach ($e in $Examples) {
        if ($local -contains $e -and $kx -contains $e) { $targets += $e }
        else { Write-Warn "Skipping '$e' (not found in both repos)" }
    }
} else {
    $targets = $local | Where-Object { $kx -contains $_ }
}

if ($targets.Count -eq 0) {
    Write-Warn "No matching examples found between repos. Nothing to do."
    exit 0
}

Write-Info ("Generating patches for: " + ($targets -join ', '))

Push-Location $KaspaXRoot
try {
    foreach ($ex in $targets) {
        $left  = Join-Path "applications/kdapps" $ex
        $rightLocal = Join-Path $RepoRoot (Join-Path "examples" $ex)

        if (-not (Test-Path $left))  { Write-Warn "Missing in KaspaX: $left";  continue }
        if (-not (Test-Path $rightLocal)) { Write-Warn "Missing locally: $rightLocal";   continue }

        $patch = Join-Path $outputPath ("kaspax_" + $ex + ".patch")
        Write-Info "Diffing $left <= local examples/$ex"

        # Mirror the local example into a temp folder inside KaspaX to avoid absolute paths in the patch
        $tmpRightRoot = ".kdp_right"
        $tmpLeftRoot  = ".kdp_left"
        $tmpRight = Join-Path $tmpRightRoot $ex
        $tmpLeft  = Join-Path $tmpLeftRoot  $ex
        foreach ($t in @($tmpRight,$tmpLeft)) { if (Test-Path $t) { Remove-Item $t -Recurse -Force }; New-Item -ItemType Directory -Path $t | Out-Null }

        # Use robocopy to mirror content, excluding build/system folders
        $xd = @('.git','target','target-join','node_modules','dist','build','.next','.pytest_cache','.mypy_cache','.venv','.cargo')
        $xf = @('Thumbs.db','.DS_Store')
        $argsRight = @(
            '"' + $rightLocal + '"',
            '"' + (Join-Path (Get-Location).Path $tmpRight) + '"',
            '/MIR','/R:1','/W:1','/NFL','/NDL','/NJH','/NJS','/NP'
        )
        foreach ($d in $xd) { $argsRight += '/XD'; $argsRight += $d }
        foreach ($f in $xf) { $argsRight += '/XF'; $argsRight += $f }
        $proc = Start-Process -FilePath robocopy.exe -ArgumentList ($argsRight -join ' ') -NoNewWindow -PassThru -Wait
        if ($proc.ExitCode -ge 8) { Write-Err "robocopy to mirror $ex failed (code $($proc.ExitCode))"; continue }

        # Mirror kdapps/<ex> into a clean left temp as well, so build artifacts are excluded from diff
        $argsLeft = @(
            '"' + (Join-Path (Get-Location).Path $left) + '"',
            '"' + (Join-Path (Get-Location).Path $tmpLeft) + '"',
            '/MIR','/R:1','/W:1','/NFL','/NDL','/NJH','/NJS','/NP'
        )
        foreach ($d in $xd) { $argsLeft += '/XD'; $argsLeft += $d }
        foreach ($f in $xf) { $argsLeft += '/XF'; $argsLeft += $f }
        $procL = Start-Process -FilePath robocopy.exe -ArgumentList ($argsLeft -join ' ') -NoNewWindow -PassThru -Wait
        if ($procL.ExitCode -ge 8) { Write-Err "robocopy to prepare left side for $ex failed (code $($procL.ExitCode))"; continue }

        # Now produce a diff between kdapps/<ex> and temp mirror
        $raw = & git diff --no-index -- $tmpLeft $tmpRight
        if ($LASTEXITCODE -gt 1) {
            Write-Err "git diff failed for $ex (exit $LASTEXITCODE)"
            continue
        }
        if (-not $raw) {
            Write-Info "No differences for $ex; skipping patch write"
            if (Test-Path $patch) { Remove-Item $patch -Force }
            continue
        }

        # Sanitize patch paths: rewrite b/.kdp_mirror/<ex>/... -> b/applications/kdapps/<ex>/...
        $patternBR = [regex]::Escape("b/" + $tmpRight + "/")
        $patternAR = [regex]::Escape("a/" + $tmpRight + "/")
        $patternBL = [regex]::Escape("b/" + $tmpLeft  + "/")
        $patternAL = [regex]::Escape("a/" + $tmpLeft  + "/")
        $targetB = "b/" + (Join-Path "applications/kdapps" $ex).Replace("\", "/") + "/"
        $targetA = "a/" + (Join-Path "applications/kdapps" $ex).Replace("\", "/") + "/"
        $fixed = $raw -replace $patternBR, $targetB
        $fixed = $fixed -replace $patternAR, $targetA
        $fixed = $fixed -replace $patternBL, $targetB
        $fixed = $fixed -replace $patternAL, $targetA

        $fixed | Out-File -FilePath $patch -Encoding utf8
        Write-Host "[made] $patch" -ForegroundColor Green
    }
}
finally {
    Pop-Location
}

Write-Info "Done. Review patches under $outputPath."
