#requires -Version 7.0
param(
    [string]$Executable = 'dist\RoomPlanner-v0.1.1.exe',
    [string]$OutputDirectory = '.test-output\startup'
)
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
$binary = [System.IO.Path]::GetFullPath((Join-Path $repository $Executable))
$output = [System.IO.Path]::GetFullPath((Join-Path $repository $OutputDirectory))
if (-not $output.StartsWith($repository + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Diagnostics must stay inside this repository' }
if (-not $binary.StartsWith($repository + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Only this repository executable may be launched' }
New-Item -ItemType Directory -Path $output -Force | Out-Null
$startInfo = [System.Diagnostics.ProcessStartInfo]::new($binary)
$startInfo.WorkingDirectory = $repository
$startInfo.UseShellExecute = $false
$startInfo.CreateNoWindow = $true
$startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $true
$startInfo.ArgumentList.Add('--diagnose')
$startInfo.ArgumentList.Add($output)
$startInfo.Environment['LOCALAPPDATA'] = Join-Path $output 'profile'
$process = [System.Diagnostics.Process]::Start($startInfo)
$stdout = $process.StandardOutput.ReadToEndAsync()
$stderr = $process.StandardError.ReadToEndAsync()
$samples = [System.Collections.Generic.List[object]]::new()
$timer = [System.Diagnostics.Stopwatch]::StartNew()
$deadline = [DateTime]::UtcNow.AddSeconds(30)
$lastTime = 0.0
$lastCpu = 0.0
while (-not $process.HasExited -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 500
    if ($process.HasExited) { break }
    $process.Refresh()
    $seconds = $timer.Elapsed.TotalSeconds
    $cpu = $process.TotalProcessorTime.TotalSeconds
    $samples.Add([PSCustomObject]@{elapsed_seconds=[math]::Round($seconds,3);cpu_one_core_percent=[math]::Round(100*($cpu-$lastCpu)/($seconds-$lastTime),3);working_set_bytes=$process.WorkingSet64;window_handle=$process.MainWindowHandle.ToInt64()})
    $lastTime = $seconds
    $lastCpu = $cpu
}
if (-not $process.HasExited) {
    [void]$process.CloseMainWindow()
    if (-not $process.WaitForExit(2000)) { $process.Kill(); $process.WaitForExit() }
    throw 'The isolated synthetic diagnostic exceeded its 30-second limit'
}
[System.IO.File]::WriteAllText((Join-Path $output 'stdout.log'), $stdout.GetAwaiter().GetResult())
[System.IO.File]::WriteAllText((Join-Path $output 'stderr.log'), $stderr.GetAwaiter().GetResult())
[System.IO.File]::WriteAllText((Join-Path $output 'process.json'), ($samples | ConvertTo-Json -Depth 4))
if ($process.ExitCode -ne 0) { throw "RoomPlanner exited with code $($process.ExitCode). See $output" }
Get-Content -LiteralPath (Join-Path $output 'runtime.json')
