param(
  [ValidateSet("clippy","test","build","clean","all")]
  [string]$Cmd = "clippy",
  [switch]$NoDeps,
  [switch]$Release,
  [string]$TargetDir,
  [switch]$NoDebugInfo,
  [switch]$NoIncremental
)

$ErrorActionPreference = "Stop"

$pkgs = @("kdapp-merchant","kdapp-customer","kdapp-guardian")
$pkgArgs = @()
foreach ($p in $pkgs) { $pkgArgs += @("-p", $p) }

$origTargetDir = $env:CARGO_TARGET_DIR
$origRustFlags = $env:RUSTFLAGS
$origIncremental = $env:CARGO_INCREMENTAL

try {
  if ($TargetDir) { $env:CARGO_TARGET_DIR = $TargetDir }
  if ($NoDebugInfo) {
    if ($env:RUSTFLAGS) { $env:RUSTFLAGS = "$($env:RUSTFLAGS) -C debuginfo=0" }
    else { $env:RUSTFLAGS = "-C debuginfo=0" }
  }
  if ($NoIncremental) { $env:CARGO_INCREMENTAL = "0" }

  switch ($Cmd) {
    "clippy" {
      $argsList = @("clippy") + $pkgArgs + @("--all-targets")
      if ($NoDeps) { $argsList += "--no-deps" }
      if ($Release) { $argsList += "--release" }
      $argsList += @("--","-D","warnings")
      Write-Host "Running: cargo $($argsList -join ' ')" -ForegroundColor Cyan
      & cargo @argsList
    }
    "test" {
      $argsList = @("test") + $pkgArgs
      if ($Release) { $argsList += "--release" }
      Write-Host "Running: cargo $($argsList -join ' ')" -ForegroundColor Cyan
      & cargo @argsList
    }
    "build" {
      $argsList = @("build") + $pkgArgs
      if ($Release) { $argsList += "--release" }
      Write-Host "Running: cargo $($argsList -join ' ')" -ForegroundColor Cyan
      & cargo @argsList
    }
    "clean" {
      $argsList = @("clean") + $pkgArgs
      Write-Host "Running: cargo $($argsList -join ' ')" -ForegroundColor Cyan
      & cargo @argsList
    }
    "all" {
      # clippy then test
      $argsClippy = @("clippy") + $pkgArgs + @("--all-targets")
      if ($NoDeps) { $argsClippy += "--no-deps" }
      if ($Release) { $argsClippy += "--release" }
      $argsClippy += @("--","-D","warnings")
      Write-Host "Running: cargo $($argsClippy -join ' ')" -ForegroundColor Cyan
      & cargo @argsClippy

      $argsTest = @("test") + $pkgArgs
      if ($Release) { $argsTest += "--release" }
      Write-Host "Running: cargo $($argsTest -join ' ')" -ForegroundColor Cyan
      & cargo @argsTest
    }
  }
}
finally {
  $env:CARGO_TARGET_DIR = $origTargetDir
  $env:RUSTFLAGS = $origRustFlags
  $env:CARGO_INCREMENTAL = $origIncremental
}

