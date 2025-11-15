#requires -Version 7.0
param([string]$OutputPath = 'THIRD_PARTY_LICENSES.txt')
$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $metadataText = cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw 'Dependency metadata failed' }
    $metadata = $metadataText | ConvertFrom-Json
    $overrides=Get-Content -LiteralPath 'licenses\overrides.json' -Raw | ConvertFrom-Json -AsHashtable
    $reachable = [System.Collections.Generic.HashSet[string]]::new()
    $queue = [System.Collections.Generic.Queue[string]]::new()
    $queue.Enqueue($metadata.resolve.root)
    while ($queue.Count -gt 0) {
        $id = $queue.Dequeue()
        if (-not $reachable.Add($id)) { continue }
        $node = $metadata.resolve.nodes | Where-Object { $_.id -eq $id }
        foreach ($dependency in $node.deps) {
            if (@($dependency.dep_kinds | Where-Object { $_.kind -ne 'dev' }).Count -gt 0) { $queue.Enqueue($dependency.pkg) }
        }
    }
    $notices = [System.Text.StringBuilder]::new()
    [void]$notices.AppendLine('RoomPlanner third-party license notices')
    foreach ($package in ($metadata.packages | Where-Object { $reachable.Contains($_.id) -and $_.id -ne $metadata.resolve.root } | Sort-Object name,version)) {
        $directory = Split-Path $package.manifest_path -Parent
        $files = @(Get-ChildItem -LiteralPath $directory -Recurse -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)([._-].*)?$|^(OFL|UFL)\.txt$' } | Sort-Object FullName)
        [void]$notices.AppendLine("`n================================================================")
        [void]$notices.AppendLine("$($package.name) $($package.version) ($($package.license))")
        if ($package.repository) { [void]$notices.AppendLine($package.repository) }
        if ($files.Count -eq 0) {
            if (-not $overrides.ContainsKey($package.name)) { throw "No license text found for $($package.name) $($package.version)" }
            $override=$overrides[$package.name]
            [void]$notices.AppendLine($override.source)
            [void]$notices.AppendLine([System.IO.File]::ReadAllText((Join-Path (Get-Location) (Join-Path 'licenses' $override.file))))
        }
        foreach ($file in $files) {
            [void]$notices.AppendLine("`n--- $([System.IO.Path]::GetRelativePath($directory, $file.FullName)) ---")
            [void]$notices.AppendLine([System.IO.File]::ReadAllText($file.FullName))
        }
    }
    [System.IO.File]::WriteAllText((Join-Path (Get-Location) $OutputPath), $notices.ToString(), [System.Text.UTF8Encoding]::new($false))
    Write-Output "Collected notices in $OutputPath"
} finally { Pop-Location }
