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
        $installer = Invoke-RestMethod 'https://klicky.muneer320.tech/install.ps1' -TimeoutSec 30
        & ([scriptblock]::Create($installer)) -Update
    }
    else {
        $receipt = @(& $Cli service prepare-update)
        if ($LASTEXITCODE -ne 0 -or $receipt.Count -ne 1 -or $receipt[0] -notmatch '^KLICKY_UPDATE_V1 ([01]) ([01])$') {
            throw 'Klicky did not confirm update preparation; update with Scoop manually.'
        }
        $running = $Matches[1] -eq '1'
        $paused = $Matches[2] -eq '1'
        try {
            scoop update
            if ($LASTEXITCODE -ne 0) { throw 'Scoop refresh failed' }
            scoop update klicky
            if ($LASTEXITCODE -ne 0) { throw 'Scoop update failed' }
        }
        finally {
            if ($running) {
                if ($paused) { & $Cli service start --paused }
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
