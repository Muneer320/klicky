# Downloads a published Windows installer; never uses expiring CI artifacts.
& {
    Set-StrictMode -Version Latest
    $ErrorActionPreference = 'Stop'
    if ($env:OS -ne 'Windows_NT') { throw 'This installer is for Windows 11 x64.' }
    $architecture = [Environment]::GetEnvironmentVariable('PROCESSOR_ARCHITECTURE', 'Machine')
    if ($architecture -ne 'AMD64') { throw 'The Windows download currently supports x64 PCs.' }
    $build = [int][Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'CurrentBuildNumber', '0')
    if ($build -lt 22000) { throw 'Windows 11 or newer is required.' }

    $headers = @{ Accept = 'application/vnd.github+json'; 'X-GitHub-Api-Version' = '2022-11-28' }
    $releases = Invoke-RestMethod -Uri 'https://api.github.com/repos/Muneer320/klicky/releases?per_page=30' -Headers $headers -TimeoutSec 30
    $installer = $null
    $checksum = $null
    foreach ($release in $releases) {
        if ($release.draft -or $release.prerelease -or $release.tag_name -cnotmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { continue }
        $version = $release.tag_name.Substring(1)
        $candidates = @($release.assets | Where-Object { $_.name -ceq "klicky-$version-windows-x64-setup.exe" })
        if ($candidates.Count -ne 1) { continue }
        $checksums = @($release.assets | Where-Object { $_.name -ceq ($candidates[0].name + '.sha256') })
        if ($checksums.Count -ne 1) { continue }
        $installer = $candidates[0]
        $checksum = $checksums[0]
        break
    }
    if ($null -eq $installer) {
        throw 'A public Windows installer is not available yet. See https://github.com/Muneer320/klicky/blob/master/docs/WINDOWS.md for current download options.'
    }
    foreach ($asset in @($installer, $checksum)) {
        $uri = [uri]$asset.browser_download_url
        if ($uri.Scheme -ne 'https' -or $uri.Host -ne 'github.com' -or -not $uri.AbsolutePath.StartsWith('/Muneer320/klicky/releases/download/', [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Unexpected release download location.'
        }
    }
    if ($installer.size -le 0 -or $installer.size -gt 268435456 -or $checksum.size -le 0 -or $checksum.size -gt 4096) {
        throw 'The release download exceeds the expected size limits.'
    }

    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    $working = Join-Path $temporaryRoot ('klicky-setup-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $working | Out-Null
    $setup = Join-Path $working $installer.name
    $sidecar = Join-Path $working $checksum.name
    try {
        Write-Output 'Downloading Klicky from its GitHub release.'
        Invoke-WebRequest -UseBasicParsing -Uri $installer.browser_download_url -OutFile $setup -TimeoutSec 180
        Invoke-WebRequest -UseBasicParsing -Uri $checksum.browser_download_url -OutFile $sidecar -TimeoutSec 30
        $expected = (Get-Content -LiteralPath $sidecar -Raw).Trim()
        $pattern = '^([0-9a-fA-F]{64})  ' + [regex]::Escape($installer.name) + '$'
        if ($expected -cnotmatch $pattern) { throw 'The release checksum file is invalid.' }
        $digest = $Matches[1]
        if ((Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash -ine $digest) {
            throw 'The installer checksum does not match. Installation was not started.'
        }
        $logDirectory = Join-Path $env:LOCALAPPDATA 'KlickyInstaller/logs'
        New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
        $log = Join-Path $logDirectory ('install-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff') + '.log')
        Write-Output 'Opening the Klicky installer. Choose your startup preferences in the wizard.'
        $process = Start-Process -FilePath $setup -ArgumentList @('/NORESTART', ('/LOG="' + $log + '"')) -PassThru
        $process.WaitForExit()
        if ($process.ExitCode -ne 0) { throw "Installer finished with code $($process.ExitCode). Details: $log" }
        $cli = Join-Path $env:LOCALAPPDATA 'Programs/Klicky/klicky.exe'
        if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) { throw "The installed CLI is missing: $cli" }
        $launch = Start-Process -FilePath $cli -ArgumentList @('service', 'start') -WindowStyle Hidden -PassThru
        if (-not $launch.WaitForExit(30000)) { throw 'Klicky startup was not confirmed within 30 seconds. Open it from Start for details.' }
        if ($launch.ExitCode -ne 0) { throw 'Klicky installed, but startup reported an error. Open it from Start for details.' }
        Write-Output 'Klicky is installed and running. Its tray menu changes sounds and volume. Choose Resume if you selected a paused launch.'
        Write-Output 'A little sound for every keystroke.'
    }
    finally {
        # Delete only the known downloads, not a recursively computed directory.
        foreach ($file in @($setup, $sidecar)) {
            if (Test-Path -LiteralPath $file -PathType Leaf) { Remove-Item -LiteralPath $file -Force }
        }
        if ((Test-Path -LiteralPath $working -PathType Container) -and @(Get-ChildItem -LiteralPath $working -Force).Count -eq 0) {
            [IO.Directory]::Delete($working)
        }
    }
}
