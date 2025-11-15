#requires -Version 7.0
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    & (Join-Path $PSScriptRoot 'Collect-Licenses.ps1')
    cargo build --release --locked --offline -j 2
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $manifest = Get-Content -LiteralPath Cargo.toml -Raw
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    if (-not $version) { throw 'Missing package version' }
    $directory = Join-Path (Get-Location) 'dist'
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $name = "RoomPlanner-v$version.exe"
    $executable = Join-Path $directory $name
    Copy-Item -LiteralPath 'target\release\room-planner.exe' -Destination $executable
    $hash = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText("$executable.sha256", "$hash  $name`n", [System.Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath LICENSE -Destination (Join-Path $directory 'LICENSE')
    Copy-Item -LiteralPath THIRD_PARTY_LICENSES.txt -Destination (Join-Path $directory 'THIRD_PARTY_LICENSES.txt')
    python (Join-Path $PSScriptRoot 'verify_package.py') $executable
    if ($LASTEXITCODE -ne 0) { throw 'Package verification failed' }
    Write-Output $executable
} finally { Pop-Location }
