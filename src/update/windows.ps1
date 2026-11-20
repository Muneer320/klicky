param(
    [Parameter(Mandatory)][ValidateSet('Standalone', 'Scoop')][string]$Channel,
    [string]$Cli,
    [int]$CallerPid = 0
)
$ErrorActionPreference = 'Stop'
try {
    if ($CallerPid) {
        $caller = Get-Process -Id $CallerPid -ErrorAction SilentlyContinue
        if ($caller -and -not $caller.WaitForExit(30000)) {
            throw 'The Klicky update command is still running; nothing was replaced.'
        }
    }
    if ($Channel -eq 'Standalone') {
        Invoke-RestMethod 'https://klicky.muneer320.tech/install.ps1' | Invoke-Expression
    }
    else {
        $status = (& $Cli status)
        if ($LASTEXITCODE -ne 0) { throw 'Cannot read playback state' }
        $running = ($status -join "`n") -notmatch 'Klicky is not running'
        if ($running) {
            $state = (& $Cli diagnostics | ConvertFrom-Json)
            if ($LASTEXITCODE -ne 0) { throw 'Cannot preserve playback state; update with Scoop manually.' }
            & $Cli service stop
            if ($LASTEXITCODE -ne 0) { throw 'Klicky did not stop' }
        }
        try {
            scoop update
            if ($LASTEXITCODE -ne 0) { throw 'Scoop refresh failed' }
            scoop update klicky
            if ($LASTEXITCODE -ne 0) { throw 'Scoop update failed' }
        }
        finally {
            if ($running) {
                if ($state.snapshot.state -eq 'paused') { & $Cli service start --paused }
                else { & $Cli service start }
                if ($LASTEXITCODE -ne 0) { Write-Warning 'Klicky did not restart; use its Start shortcut.' }
            }
        }
    }
}
catch {
    Write-Error $_ -ErrorAction Continue
    Read-Host 'Update failed. Press Enter to close'
    exit 1
}
