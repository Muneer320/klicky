# Explicit, local-only microphone recording. Run interactively for a controlled test.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Microphone,
    [string]$OutputDirectory,
    [string]$Executable,
    [string]$Pack,
    [ValidateRange(1, 300)][int]$Seconds = 30,
    [ValidateRange(0.01, 1.0)][double]$Volume = 0.8,
    [ValidateSet('baseline', 'device-default')][string]$Buffer = 'baseline',
    [string]$OutputRoute = 'built-in-speakers'
)
$ErrorActionPreference = 'Stop'
if ($PSVersionTable.PSVersion.Major -lt 7) { throw 'Run this helper with PowerShell 7 (pwsh).' }
if (-not $IsWindows -or [Environment]::Is64BitProcess -eq $false -or [Environment]::OSVersion.Version.Build -lt 22000) {
    throw 'This capture helper targets Windows 11 x64.'
}
$taskRepo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
if (-not $Executable) { $Executable = Join-Path $taskRepo 'target/windows-qualification/release/klicky-qualify.exe' }
if (-not $Pack) { $Pack = Join-Path $taskRepo 'sounds/eg-oreo' }
$taskBinary = (Get-Item -LiteralPath $Executable).FullName
$taskBuild = & $taskBinary info | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot read qualification executable identity.' }
$taskPack = (Get-Item -LiteralPath $Pack).FullName
$taskFfmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
$taskPython = (Get-Command python -ErrorAction Stop).Source
if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $env:LOCALAPPDATA ('KlickyResearch/qualification-sessions/' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
}
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
$taskRepoPrefix = $taskRepo.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
$taskIgnoredPrefix = (Join-Path $taskRepo 'target').TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
if ($taskOutput.StartsWith($taskRepoPrefix, [StringComparison]::OrdinalIgnoreCase) -and
    -not $taskOutput.StartsWith($taskIgnoredPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Store recordings outside the repository or beneath ignored target/.'
}
if (Test-Path -LiteralPath $taskOutput) { throw 'Output directory already exists; refusing to replace a previous session.' }
New-Item -ItemType Directory -Path $taskOutput | Out-Null
$taskVolumeText = $Volume.ToString([Globalization.CultureInfo]::InvariantCulture)
& $taskBinary reference --pack $taskPack --volume $taskVolumeText --output (Join-Path $taskOutput 'reference.wav')
if ($LASTEXITCODE -ne 0) { throw 'Reference export failed; recording has not started.' }
$taskRevision = (& git -C $taskRepo rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the tested Git revision.' }
$taskDirty = [bool](& git -C $taskRepo status --porcelain=v1)
$taskMetadata = [ordered]@{
    platform = 'windows-11-x64'
    source_revision = $taskBuild.source_revision
    checkout_revision = $taskRevision
    source_tree_dirty = ($taskDirty -or $taskBuild.source_tree_dirty -or $taskBuild.source_revision -ne $taskRevision)
    executable_sha256 = (Get-FileHash -LiteralPath $taskBinary -Algorithm SHA256).Hash.ToLowerInvariant()
    comparison_id = 'replace-with-matched-physical-hardware-and-acquisition-id'
    output_route = $OutputRoute
    pack_sha256 = (Get-FileHash -LiteralPath (Join-Path $taskPack 'sound.ogg') -Algorithm SHA256).Hash.ToLowerInvariant()
    pack_config_sha256 = (Get-FileHash -LiteralPath (Join-Path $taskPack 'config.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    volume = $Volume
    reference_kind = 'acoustic-impact'
    audio_processing = 'uncharacterized'
    calibration = $null
    expected_event_count = $null
    buffer_policy = $Buffer
    os_version = [Environment]::OSVersion.Version.ToString()
    note = 'Built-in microphone comparison only. No electrical-actuation qualification is implied.'
}
$taskMetadata | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskOutput 'metadata.json') -Encoding utf8
'event_id,reference_frame,audible_frame,accepted,reason' | Set-Content -LiteralPath (Join-Path $taskOutput 'annotations.csv') -Encoding utf8
$taskStart = [Diagnostics.ProcessStartInfo]::new($taskFfmpeg)
$taskStart.UseShellExecute = $false
$taskStart.CreateNoWindow = $true
$taskStart.RedirectStandardInput = $true
$taskStart.RedirectStandardOutput = $true
$taskStart.RedirectStandardError = $true
foreach ($taskArgument in @('-hide_banner', '-f', 'dshow', '-i', "audio=$Microphone", '-t', ($Seconds + 5).ToString(), '-ac', '1', '-ar', '48000', '-c:a', 'pcm_s16le', '-n', (Join-Path $taskOutput 'recording.wav'))) {
    $taskStart.ArgumentList.Add($taskArgument)
}
$taskRecorder = [Diagnostics.Process]::new()
$taskRecorder.StartInfo = $taskStart
$taskRecorderStarted = $false
$taskRecorderError = $null
$taskRecorderOutput = $null
Write-Host "Starting the explicitly requested microphone recording for at most $($Seconds + 5) seconds."
Write-Host 'Use the selected speakers, keep the room quiet, and press Space about once per second after READY.'
Write-Host 'Press Escape to end playback early. Recording will stop after the short sound tail.'
try {
    if (-not $taskRecorder.Start()) { throw 'Cannot start recorder.' }
    $taskRecorderStarted = $true
    $taskRecorderError = $taskRecorder.StandardError.ReadToEndAsync()
    $taskRecorderOutput = $taskRecorder.StandardOutput.ReadToEndAsync()
    Start-Sleep -Seconds 2
    if ($taskRecorder.HasExited) { throw ('Recorder failed: ' + $taskRecorderError.GetAwaiter().GetResult()) }
    & $taskBinary capture --pack $taskPack --seconds $Seconds --volume $taskVolumeText --buffer $Buffer --output (Join-Path $taskOutput 'dispatch.json')
    $taskCaptureExit = $LASTEXITCODE
    Start-Sleep -Milliseconds 500
    if (-not $taskRecorder.HasExited) {
        $taskRecorder.StandardInput.WriteLine('q')
        $taskRecorder.StandardInput.Flush()
    }
    if (-not $taskRecorder.WaitForExit(5000)) { throw 'Recorder did not finish its WAV file within five seconds.' }
    $taskRecorderError.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $taskOutput 'recorder.log') -Encoding utf8
    $null = $taskRecorderOutput.GetAwaiter().GetResult()
    if ($taskRecorder.ExitCode -ne 0 -or $taskCaptureExit -ne 0) { throw 'Capture failed; preserve the session for diagnosis, not a qualification pass.' }
    & $taskPython (Join-Path $PSScriptRoot 'analyze.py') --inspect-recording (Join-Path $taskOutput 'recording.wav') --output (Join-Path $taskOutput 'acquisition.json')
    if ($LASTEXITCODE -ne 0) { throw 'Recorded WAV validation failed.' }
    $taskAcquisition = Get-Content -LiteralPath (Join-Path $taskOutput 'acquisition.json') -Raw | ConvertFrom-Json
    if (-not $taskAcquisition.audio.signal_above_quantization_dither) {
        throw "Recording is silent or contains only dither. Check microphone mute/input; no latency evidence was collected. Files remain at $taskOutput"
    }
    Write-Host "Saved locally: $taskOutput"
    Write-Host 'Annotate the key impact and generated onset in the WAV; acoustic impact remains an uncalibrated reference.'
} finally {
    if ($taskRecorderStarted -and -not $taskRecorder.HasExited) {
        try { $taskRecorder.StandardInput.WriteLine('q'); $taskRecorder.StandardInput.Flush() } catch { }
        if (-not $taskRecorder.WaitForExit(3000)) { $taskRecorder.Kill(); $taskRecorder.WaitForExit() }
    }
    if ($taskRecorderStarted -and $null -ne $taskRecorderError) {
        $taskRecorderError.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $taskOutput 'recorder.log') -Encoding utf8
    }
    $taskRecorder.Dispose()
}
