[CmdletBinding()]
param(
    [string]$OutputDirectory = '',
    [string]$TargetDirectory = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'Build the initial Windows bundle on Windows x64 with PowerShell 7.'
}
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$toolchain = '1.87.0-x86_64-pc-windows-msvc'
$target = 'x86_64-pc-windows-msvc'
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repository 'target/windows-dist' }
if (-not $TargetDirectory) { $TargetDirectory = Join-Path $repository 'target/windows-package-build' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$TargetDirectory = [IO.Path]::GetFullPath($TargetDirectory)

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    $result = & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
    return $result
}
function Find-BuildTool([string]$Name) {
    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    if ($Name -eq 'dumpbin.exe') {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
        $installation = Invoke-Checked $vswhere @('-latest', '-prerelease', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath')
        $version = (Get-Content -LiteralPath (Join-Path $installation 'VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt') -Raw).Trim()
        $candidate = Join-Path $installation "VC/Tools/MSVC/$version/bin/Hostx64/x64/$Name"
        if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    } else {
        foreach ($key in @('HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows Kits\Installed Roots')) {
            $kits = Get-ItemProperty -LiteralPath $key -Name KitsRoot10 -ErrorAction SilentlyContinue
            if (-not $kits) { continue }
            $candidate = Get-ChildItem -LiteralPath (Join-Path $kits.KitsRoot10 'bin') -Directory |
                Where-Object { $_.Name -match '^10\.0\.\d+\.\d+$' } |
                Sort-Object { [version]$_.Name } -Descending |
                ForEach-Object { Join-Path $_.FullName "x64/$Name" } |
                Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
            if ($candidate) { return $candidate }
        }
    }
    throw "Install Visual Studio C++ tools and a Windows SDK, or put $Name on PATH."
}

$files = [Collections.Generic.List[object]]::new()
$names = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
function Add-BundleFile([string]$Path, [string]$Name) {
    $item = Get-Item -LiteralPath $Path
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Not a regular bundle file: $Path" }
    if (-not $names.Add($Name)) { throw "Duplicate bundle file: $Name" }
    $files.Add([pscustomobject]@{ Path = $item.FullName; Name = $Name })
}

Push-Location $repository
try {
    if (Invoke-Checked git @('status', '--porcelain=v1')) { throw 'Commit source changes before building an identified test bundle.' }
    if ($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS) { throw 'Clear custom Rust flags before building the static-CRT test bundle.' }
    $revision = Invoke-Checked git @('rev-parse', 'HEAD')
    $tree = Invoke-Checked git @('rev-parse', 'HEAD^{tree}')
    $dumpbin = Find-BuildTool 'dumpbin.exe'
    $mt = Find-BuildTool 'mt.exe'
    $env:CARGO_ENCODED_RUSTFLAGS = '-C' + [char]31 + 'target-feature=+crt-static'
    try {
        Invoke-Checked cargo @("+$toolchain", 'build', '--locked', '--release', '--target', $target, '--target-dir', $TargetDirectory, '--bin', 'klicky', '--bin', 'klicky-desktop') | Out-Host
        $metadata = (Invoke-Checked cargo @("+$toolchain", 'metadata', '--locked', '--format-version', '1', '--filter-platform', $target)) | ConvertFrom-Json
    } finally { Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue }
    $rootPackage = $metadata.packages | Where-Object { $_.id -eq $metadata.resolve.root }
    $version = $rootPackage.version
    $bundleName = "klicky-$version-windows-x64"
    $binaries = Join-Path $TargetDirectory "$target/release"
    $imports = [ordered]@{}
    $systemDlls = @('advapi32.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'combase.dll', 'gdi32.dll', 'kernel32.dll', 'mmdevapi.dll', 'ntdll.dll', 'ole32.dll', 'oleaut32.dll', 'shell32.dll', 'shlwapi.dll', 'user32.dll', 'userenv.dll', 'wtsapi32.dll')
    foreach ($binary in @('klicky.exe', 'klicky-desktop.exe')) {
        $path = Join-Path $binaries $binary
        $data = [IO.File]::ReadAllBytes($path)
        $pe = [BitConverter]::ToInt32($data, 60)
        if ([BitConverter]::ToUInt32($data, $pe) -ne 0x4550 -or [BitConverter]::ToUInt16($data, $pe + 4) -ne 0x8664) { throw "Not an x64 PE: $binary" }
        $subsystem = [BitConverter]::ToUInt16($data, $pe + 24 + 68)
        if ($subsystem -ne $(if ($binary -eq 'klicky.exe') { 3 } else { 2 })) { throw "Wrong executable subsystem: $binary" }
        $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($path)
        if ($info.OriginalFilename -ne $binary -or $info.ProductVersion -ne $version -or $info.IsPreRelease) { throw "Unexpected version metadata: $binary" }
        if ((Get-AuthenticodeSignature -LiteralPath $path).Status -ne 'NotSigned') { throw "Expected an unsigned test executable: $binary" }
        $dependencies = @(Invoke-Checked $dumpbin @('/DEPENDENTS', $path) | ForEach-Object {
            if ($_ -match '^\s+([\w.-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() }
        } | Sort-Object -Unique)
        if (-not $dependencies.Count) { throw "Could not inspect DLL dependencies: $binary" }
        foreach ($dll in $dependencies) {
            if ($dll -notin $systemDlls -and $dll -notmatch '^(api-ms-win-|ext-ms-win-)[\w.-]+\.dll$') { throw "Review unexpected runtime DLL dependency $dll in $binary" }
        }
        $imports[$binary] = $dependencies
        $manifestFile = Join-Path ([IO.Path]::GetTempPath()) ("klicky-manifest-" + [guid]::NewGuid().ToString('N') + '.xml')
        try {
            Invoke-Checked $mt @("-inputresource:$path;#1", "-out:$manifestFile") | Out-Host
            [xml]$manifest = Get-Content -LiteralPath $manifestFile -Raw
            $privilege = $manifest.SelectSingleNode('//*[local-name()="requestedExecutionLevel"]')
            if ($privilege.level -ne 'asInvoker' -or $privilege.uiAccess -ne 'false' -or $manifest.SelectSingleNode('//*[local-name()="dpiAwareness"]').InnerText -ne 'PerMonitorV2' -or $manifest.SelectSingleNode('//*[local-name()="longPathAware"]').InnerText -ne 'true') { throw "Unexpected application manifest: $binary" }
        } finally { if (Test-Path -LiteralPath $manifestFile) { Remove-Item -LiteralPath $manifestFile } }
        Add-BundleFile $path $binary
    }
    foreach ($entry in @(@('LICENSE', 'LICENSE'), @('ACKNOWLEDGEMENTS.md', 'ACKNOWLEDGEMENTS.md'), @('docs/WINDOWS.md', 'WINDOWS.md'))) {
        Add-BundleFile (Join-Path $repository $entry[0]) $entry[1]
    }
    foreach ($item in (Get-ChildItem -LiteralPath (Join-Path $repository 'sounds') -Recurse)) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point in bundled sounds: $($item.FullName)" }
        if (-not $item.PSIsContainer) { Add-BundleFile $item.FullName ([IO.Path]::GetRelativePath($repository, $item.FullName).Replace('\', '/')) }
    }
    if (@($files | Where-Object { $_.Name -match '^sounds/[^/]+/sound\.ogg$' }).Count -ne 10) { throw 'Expected all ten bundled sound recordings.' }
    . (Join-Path $PSScriptRoot 'windows-license-files.ps1')
    $sysroot = Invoke-Checked rustc @("+$toolchain", '--print', 'sysroot')
    $licenses = Get-KlickyLicenseFiles $metadata $repository $sysroot
    $inventory = @($files | Sort-Object Name | ForEach-Object {
        [ordered]@{ path = $_.Name; sha256 = (Get-FileHash -LiteralPath $_.Path -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    $identity = [ordered]@{
        product = 'Klicky'; version = $version; source_revision = $revision; source_tree = $tree
        target = $target; rustc = @(Invoke-Checked rustc @("+$toolchain", '-Vv'))
        static_crt = $true; signing = 'unsigned'
        direct_dll_imports = $imports; runtime_crates = $licenses; files = $inventory
    }
    $notice = @'
This Klicky Windows build is unsigned. Source identity is recorded in BUILD-INFO.json.
Klicky and its sound recordings: see LICENSE and sounds/LICENSE.md.
Third-party crate notices: licenses/crates. Exact versions and source URLs: BUILD-INFO.json.
Unmodified MPL-2.0 source archives are included in third-party-sources and are made
available under MPL-2.0; each archive retains its original source notices.
Rust library notices come from the exact build toolchain in licenses/rust.
They cover Rust's supported targets; not every listed component is linked here.
'@
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $archivePath = Join-Path $OutputDirectory "$bundleName.zip"
    if ((Test-Path -LiteralPath $archivePath) -or (Test-Path -LiteralPath "$archivePath.sha256")) { throw 'The identified bundle already exists; choose a new output directory.' }
    $partialPath = "$archivePath.$([guid]::NewGuid().ToString('N')).partial"
    $stream = [IO.File]::Open($partialPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $zip = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
        try {
            $seconds = [long](Invoke-Checked git @('show', '-s', '--format=%ct', 'HEAD'))
            $stamp = [DateTimeOffset]::FromUnixTimeSeconds([Math]::Max(315532800, [Math]::Min(4354819198, $seconds)))
            foreach ($file in ($files | Sort-Object Name)) {
                $entry = $zip.CreateEntry("$bundleName/$($file.Name)", [IO.Compression.CompressionLevel]::Optimal)
                $entry.LastWriteTime = $stamp
                $writer = $entry.Open()
                try {
                    $reader = [IO.File]::OpenRead($file.Path)
                    try {
                        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($reader)).ToLowerInvariant()
                        if ($hash -ne ($inventory | Where-Object { $_.path -eq $file.Name }).sha256) { throw "Bundle input changed: $($file.Name)" }
                        $reader.Position = 0
                        $reader.CopyTo($writer)
                    } finally { $reader.Dispose() }
                } finally { $writer.Dispose() }
            }
            foreach ($extra in @(@('BUILD-INFO.json', ($identity | ConvertTo-Json -Depth 12)), @('THIRD-PARTY.txt', $notice))) {
                $entry = $zip.CreateEntry("$bundleName/$($extra[0])"); $entry.LastWriteTime = $stamp
                $writer = [IO.StreamWriter]::new($entry.Open(), [Text.UTF8Encoding]::new($false))
                try { $writer.Write($extra[1]) } finally { $writer.Dispose() }
            }
        } finally { $zip.Dispose() }
    } finally { $stream.Dispose() }
    if ((Invoke-Checked git @('rev-parse', 'HEAD')) -ne $revision -or (Invoke-Checked git @('status', '--porcelain=v1'))) { throw 'Source identity changed during packaging; partial archive retained for inspection.' }
    $sha = (Get-FileHash -LiteralPath $partialPath -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::Move($partialPath, $archivePath)
    $checksum = [IO.File]::Open("$archivePath.sha256", [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $bytes = [Text.UTF8Encoding]::new($false).GetBytes("$sha  $bundleName.zip`n"); $checksum.Write($bytes, 0, $bytes.Length) } finally { $checksum.Dispose() }
    Write-Output "Saved unsigned Windows bundle: $archivePath"
    Write-Output "SHA-256: $sha"
} finally { Pop-Location }
