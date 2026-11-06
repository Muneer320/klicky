[CmdletBinding(SupportsShouldProcess)]
param([switch]$Purge)

& {
    [CmdletBinding(SupportsShouldProcess)]
    param([switch]$Purge)
    Set-StrictMode -Version Latest
    $ErrorActionPreference = 'Stop'
    if ($env:OS -ne 'Windows_NT') { throw 'Use uninstall.sh on Linux or macOS.' }
    $local = [IO.Path]::GetFullPath($env:LOCALAPPDATA)
    $app = Join-Path $local 'Programs/Klicky'
    $registrationPath = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{B8E55779-4EDB-4D8F-962D-10460EC7C230}_is1'
    function Get-KlickyUninstallRegistration {
        # Read the actual current-user hive, independent of the caller's PSDrives.
        # Access errors must propagate, rather than masquerading as a missing key.
        $entry = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($registrationPath)
        if ($null -eq $entry) { return $null }
        try {
            [pscustomobject]@{ UninstallString = $entry.GetValue('UninstallString', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
        }
        finally { $entry.Dispose() }
    }
    $registration = Get-KlickyUninstallRegistration
    if (-not $PSCmdlet.ShouldProcess('Klicky for the current user', 'Uninstall app and desktop controls')) { return }
    if ($null -ne $registration) {
        $command = [string]$registration.UninstallString
        if ($command -notmatch '^"([^"]+\.exe)"$') { throw 'The registered uninstall command is unexpected; no program was started.' }
        $uninstaller = [IO.Path]::GetFullPath($Matches[1])
        if (-not [IO.Path]::GetDirectoryName($uninstaller).Equals($app, [StringComparison]::OrdinalIgnoreCase) -or
            [IO.Path]::GetFileName($uninstaller) -notmatch '^unins[0-9]{3}\.exe$') {
            throw 'The registered uninstaller is outside the Klicky installation; no program was started.'
        }
        if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) { throw 'The registered uninstaller is missing. Repair the installation before removing it.' }
        if (-not (Test-Path -LiteralPath (Join-Path $app 'klicky.exe') -PathType Leaf)) {
            throw 'The installed Klicky control executable is missing, so its uninstaller cannot safely stop the app. Check Windows Security Protection history. If it was quarantined, resolve the detection before repair; do not disable protection. See https://github.com/Muneer320/klicky/blob/master/docs/TROUBLESHOOTING.md#windows'
        }
        $logDirectory = Join-Path $local 'KlickyInstaller/logs'
        New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
        $log = Join-Path $logDirectory ('uninstall-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff') + '.log')
        $process = Start-Process -FilePath $uninstaller -ArgumentList @('/NORESTART', ('/LOG="' + $log + '"')) -PassThru
        if (-not $process.WaitForExit(600000)) { throw 'The uninstaller is still open. Finish its wizard before removing settings.' }
        if ($process.ExitCode -ne 0) { throw "Uninstall did not finish successfully. Details: $log" }
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while ((($null -ne (Get-KlickyUninstallRegistration)) -or (Test-Path -LiteralPath (Join-Path $app 'klicky-desktop.exe'))) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
        if (($null -ne (Get-KlickyUninstallRegistration)) -or (Test-Path -LiteralPath (Join-Path $app 'klicky-desktop.exe'))) {
            throw 'The installed app remains registered or present. Removal may have been cancelled; settings were kept.'
        }
    }
    elseif (Test-Path -LiteralPath (Join-Path $app 'klicky-desktop.exe')) {
        throw 'Klicky files exist without an uninstall registration. Repair with its installer before removing it.'
    }
    if (@(Get-Process -Name klicky,klicky-desktop -ErrorAction SilentlyContinue).Count -ne 0) {
        throw 'A Klicky process remains open. Close remaining portable copies or other sessions before completing removal. See the portable removal guide.'
    }
    if ($Purge) {
        $purgePaths = @()
        foreach ($name in @('Klicky', 'KlickyInstaller')) {
            $data = [IO.Path]::GetFullPath((Join-Path $local $name))
            if (-not [IO.Path]::GetDirectoryName($data).Equals($local, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected data path; settings were kept.' }
            if (Test-Path -LiteralPath $data) {
                $item = Get-Item -LiteralPath $data -Force
                if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                    throw "A link exists in $data. Its target was left untouched; inspect it before purging."
                }
                $links = @(Get-ChildItem -LiteralPath $data -Force -Recurse | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint })
                if ($links.Count -ne 0) { throw "A link exists in $data. Its target was left untouched; inspect it before purging." }
                $purgePaths += $data
            }
        }
        foreach ($data in $purgePaths) {
            if ($PSCmdlet.ShouldProcess($data, 'Delete settings, custom sounds and installer data')) {
                Remove-Item -LiteralPath $data -Recurse -Force
            }
        }
        Write-Output 'Klicky, its installed desktop controls and personal data have been removed.'
    }
    else { Write-Output 'Klicky and its installed desktop controls have been removed. Settings and custom sounds were kept.' }
} -Purge:$Purge
