$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $RepoRoot

cargo build -p zl-ffi

$Dest = Join-Path $RepoRoot "bindings\python\src\zerolink\native"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

$Candidates = @(
  (Join-Path $RepoRoot "target\debug\zl_ffi.dll"),
  (Join-Path $RepoRoot "target\debug\libzl_ffi.so"),
  (Join-Path $RepoRoot "target\debug\libzl_ffi.dylib")
)

foreach ($Path in $Candidates) {
  if (Test-Path $Path) {
    Copy-Item -Force $Path $Dest
  }
}

Write-Host "native library copied to $Dest"
