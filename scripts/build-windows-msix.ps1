[CmdletBinding(DefaultParameterSetName = 'Store')]
param(
    [Parameter(Mandatory)][string]$PortableArchive,
    [Parameter(Mandatory, ParameterSetName = 'Store')][string]$IdentityName,
    [Parameter(Mandatory, ParameterSetName = 'Store')][string]$Publisher,
    [Parameter(Mandatory, ParameterSetName = 'Store')][string]$PublisherDisplayName,
    [Parameter(Mandatory, ParameterSetName = 'Development')][switch]$Development,
    [string]$PackageVersion = '',
    [string]$OutputDirectory = '',
    [string]$MakeAppxPath = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'Build MSIX on Windows x64 with PowerShell 7 and the Windows SDK.'
}
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    $result = & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
    return $result
}
if ($Development) {
    $IdentityName = 'Klicky.Development'
    $Publisher = 'CN=Klicky Development'
    $PublisherDisplayName = 'Klicky Development'
}
if ($IdentityName -notmatch '^[a-zA-Z0-9.-]{3,50}$' -or -not $Publisher.StartsWith('CN=') -or [string]::IsNullOrWhiteSpace($PublisherDisplayName)) {
    throw 'Use the exact package identity, publisher and publisher display name assigned in Partner Center.'
}
if (-not $MakeAppxPath) {
    $kits = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -Name KitsRoot10
    $MakeAppxPath = Get-ChildItem -LiteralPath (Join-Path $kits.KitsRoot10 'bin') -Directory |
        Where-Object { $_.Name -match '^10\.0\.\d+\.\d+$' } | Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64/makeappx.exe' } |
        Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $MakeAppxPath) { throw 'Install the Windows SDK or provide -MakeAppxPath.' }
}
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repository 'target/windows-msix' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
Push-Location $repository
try {
    if (Invoke-Checked git @('status', '--porcelain=v1')) { throw 'Commit source changes before building an identified package.' }
    $revision = Invoke-Checked git @('rev-parse', 'HEAD')
    $tree = Invoke-Checked git @('rev-parse', 'HEAD^{tree}')
    $versionLine = Get-Content -LiteralPath Cargo.toml | Where-Object { $_ -match '^version = "\d+\.\d+\.\d+"$' } | Select-Object -First 1
    if (-not $versionLine) { throw 'Missing numeric package version.' }
    $version = $versionLine.Split('"')[1]
    if (-not $PackageVersion) { $PackageVersion = "$version.0" }
    if ($PackageVersion -notmatch '^\d+\.\d+\.\d+\.0$' -or @($PackageVersion.Split('.') | Where-Object { [long]$_ -gt 65535 }).Count) {
        throw 'MSIX versions require four components from 0 to 65535 and a final zero reserved for the Store.'
    }
    $staging = Join-Path $repository ('target/windows-msix-staging/' + [guid]::NewGuid().ToString('N'))
    $payload = Join-Path $staging 'payload'
    . (Join-Path $PSScriptRoot 'windows-portable-payload.ps1')
    $identity = Expand-KlickyPortable -Archive ([IO.Path]::GetFullPath($PortableArchive)) -Destination $payload -Version $version -Revision $revision -Tree $tree
    # MSIX payloads are immutable. The existing reader lease can open this marker,
    # but must never try to create it in WindowsApps.
    [IO.File]::WriteAllBytes((Join-Path $payload '.klicky.bundle.lock'), [byte[]]@())
    $assets = Join-Path $payload 'Assets'
    New-Item -ItemType Directory -Path $assets | Out-Null
    Add-Type -AssemblyName System.Drawing
    $source = [Drawing.Image]::FromFile((Join-Path $repository 'assets/branding/klicky.png'))
    try {
        foreach ($asset in @(@('StoreLogo', 50), @('Square44x44Logo', 44), @('Square150x150Logo', 150))) {
            $image = [Drawing.Bitmap]::new($asset[1], $asset[1])
            $graphics = [Drawing.Graphics]::FromImage($image)
            try {
                $graphics.Clear([Drawing.Color]::Transparent)
                $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.DrawImage($source, 0, 0, $asset[1], $asset[1])
                $image.Save((Join-Path $assets "$($asset[0]).png"), [Drawing.Imaging.ImageFormat]::Png)
            } finally { $graphics.Dispose(); $image.Dispose() }
        }
    } finally { $source.Dispose() }
    $manifest = Get-Content -LiteralPath packaging/windows/msix/AppxManifest.xml.in -Raw
    $replacements = @{
        IDENTITY_NAME = $IdentityName; PUBLISHER = $Publisher; VERSION = $PackageVersion
        DISPLAY_NAME = $(if ($Development) { 'Klicky Development' } else { 'Klicky' })
        PUBLISHER_DISPLAY_NAME = $PublisherDisplayName
        CLI_ALIAS = $(if ($Development) { 'klicky-dev.exe' } else { 'klicky.exe' })
    }
    foreach ($key in $replacements.Keys) { $manifest = $manifest.Replace("@$key@", [Security.SecurityElement]::Escape($replacements[$key])) }
    [IO.File]::WriteAllText((Join-Path $payload 'AppxManifest.xml'), $manifest, [Text.UTF8Encoding]::new($false))
    $provenance = [ordered]@{
        package_identity = $IdentityName; publisher = $Publisher; package_version = $PackageVersion
        development = [bool]$Development; source_revision = $revision; source_tree = $tree
        portable_sha256 = (Get-FileHash -LiteralPath $PortableArchive -Algorithm SHA256).Hash.ToLowerInvariant()
        signing = 'unsigned; Store signing occurs only after certification'
    }
    [IO.File]::WriteAllText((Join-Path $payload 'MSIX-INFO.json'), ($provenance | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $name = "klicky-$PackageVersion-windows-x64$(if ($Development) { '-development' }).msix"
    $package = Join-Path $OutputDirectory $name
    if ((Test-Path -LiteralPath $package) -or (Test-Path -LiteralPath "$package.sha256")) { throw 'Package output already exists; choose a new output directory.' }
    $partial = Join-Path $staging $name
    Invoke-Checked $MakeAppxPath @('pack', '/d', $payload, '/p', $partial, '/no') | Out-Host
    # Never use /nv: MakeAppx must perform its manifest and payload validation.
    $unpacked = Join-Path $staging 'verified'
    Invoke-Checked $MakeAppxPath @('unpack', '/p', $partial, '/d', $unpacked, '/no') | Out-Host
    foreach ($file in Get-ChildItem -LiteralPath $payload -Recurse -File -Force) {
        $relative = [IO.Path]::GetRelativePath($payload, $file.FullName)
        if ((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath (Join-Path $unpacked $relative) -Algorithm SHA256).Hash) { throw "MSIX round-trip mismatch: $relative" }
    }
    if ((Invoke-Checked git @('rev-parse', 'HEAD')) -ne $revision -or (Invoke-Checked git @('status', '--porcelain=v1'))) { throw 'Source changed while packaging.' }
    $hash = (Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::Move($partial, $package)
    [IO.File]::WriteAllText("$package.sha256", "$hash  $name`n", [Text.UTF8Encoding]::new($false))
    Write-Output "Saved unsigned MSIX: $package"
    Write-Output "Verified layout: $unpacked"
    Write-Output 'No certificate was installed, no package was registered, and nothing was published.'
} finally { Pop-Location }
