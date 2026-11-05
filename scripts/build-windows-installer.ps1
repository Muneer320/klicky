[CmdletBinding()]
param(
    [string]$CompilerPath = '',
    [string]$PortableArchive = '',
    [string]$OutputDirectory = '',
    [string]$TargetDirectory = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'Build the initial installer on Windows x64 with PowerShell 7.'
}
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $CompilerPath) {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { $CompilerPath = $command.Source }
    else { throw 'Provide -CompilerPath for the official Inno Setup 7.1.1 x64 compiler.' }
}
$CompilerPath = [IO.Path]::GetFullPath($CompilerPath)
$compilerVersion = & $CompilerPath --version
if ($LASTEXITCODE -ne 0 -or $compilerVersion -ne '7.1.1') { throw 'Inno Setup 7.1.1 is required.' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repository 'target/windows-dist' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    $result = & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
    return $result
}
Push-Location $repository
try {
    if (Invoke-Checked git @('status', '--porcelain=v1')) { throw 'Commit source changes before building an identified installer.' }
    $revision = Invoke-Checked git @('rev-parse', 'HEAD')
    $tree = Invoke-Checked git @('rev-parse', 'HEAD^{tree}')
    $versionLine = Get-Content -LiteralPath (Join-Path $repository 'Cargo.toml') | Where-Object { $_ -match '^version = "\d+\.\d+\.\d+"$' } | Select-Object -First 1
    if (-not $versionLine) { throw 'Missing numeric package version.' }
    $version = $versionLine.Split('"')[1]
    $name = "klicky-$version-windows-x64-preview-$($revision.Substring(0, 12))"
    if (-not $PortableArchive) {
        $arguments = @{ OutputDirectory = $OutputDirectory }
        if ($TargetDirectory) { $arguments.TargetDirectory = $TargetDirectory }
        & (Join-Path $PSScriptRoot 'build-windows-portable.ps1') @arguments | Out-Host
        $PortableArchive = Join-Path $OutputDirectory "$name.zip"
    }
    $PortableArchive = [IO.Path]::GetFullPath($PortableArchive)
    if ([IO.Path]::GetFileName($PortableArchive) -ne "$name.zip") { throw 'The portable archive does not identify the current committed source.' }
    $staging = Join-Path $repository ('target/windows-installer-staging/' + [guid]::NewGuid().ToString('N'))
    $payload = Join-Path $staging 'payload'
    $compiled = Join-Path $staging 'compiled'
    New-Item -ItemType Directory -Path $payload, $compiled -Force | Out-Null
    $stream = [IO.File]::OpenRead($PortableArchive)
    try {
        $archiveHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()
        $checksum = (Get-Content -LiteralPath "$PortableArchive.sha256" -Raw).Trim()
        if ($checksum -cne "$archiveHash  $name.zip") { throw 'Portable archive checksum differs from its sidecar.' }
        $stream.Position = 0
        $zip = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Read, $true)
        try {
            $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
            $totalBytes = [long]0
            foreach ($entry in $zip.Entries) {
                $prefix = "$name/"
                if (-not $entry.FullName.StartsWith($prefix, [StringComparison]::Ordinal)) { throw 'Unexpected archive root.' }
                $relative = $entry.FullName.Substring($prefix.Length)
                if ($relative -notmatch '^[a-zA-Z0-9_./+-]+$' -or $relative.Split('/') -contains '..' -or $relative.Split('/') -contains '.' -or $relative.Contains('//') -or -not $seen.Add($relative)) { throw "Unsafe or duplicate archive entry: $relative" }
                $totalBytes += $entry.Length
                if ($entry.Length -gt 268435456 -or $totalBytes -gt 536870912 -or $seen.Count -gt 4096) { throw 'Portable archive exceeds staging limits.' }
                $destination = Join-Path $payload $relative
                New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($destination)) -Force | Out-Null
                $reader = $entry.Open()
                try {
                    $writer = [IO.File]::Open($destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
                    try { $reader.CopyTo($writer) } finally { $writer.Dispose() }
                } finally { $reader.Dispose() }
            }
        } finally { $zip.Dispose() }
    } finally { $stream.Dispose() }
    $identity = Get-Content -LiteralPath (Join-Path $payload 'BUILD-INFO.json') -Raw | ConvertFrom-Json
    if ($identity.source_revision -ne $revision -or $identity.source_tree -ne $tree -or $identity.version -ne $version -or $identity.target -ne 'x86_64-pc-windows-msvc' -or $identity.static_crt -ne $true -or $identity.signing -ne 'unsigned' -or $identity.qualification -ne 'development-unqualified') { throw 'Portable build identity does not match this unsigned installer source.' }
    $inventoried = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $identity.files) {
        if (-not $inventoried.Add($file.path) -or -not $seen.Contains($file.path) -or (Get-FileHash -LiteralPath (Join-Path $payload $file.path) -Algorithm SHA256).Hash -ine $file.sha256) { throw "Portable payload verification failed: $($file.path)" }
    }
    if ($seen.Count -ne $inventoried.Count + 2 -or -not $seen.Contains('BUILD-INFO.json') -or -not $seen.Contains('THIRD-PARTY.txt')) { throw 'Unexpected un-inventoried payload files.' }
    $cliHash = (Get-FileHash -LiteralPath (Join-Path $payload 'klicky.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    $receipt = "[Klicky]`nAppId={B8E55779-4EDB-4D8F-962D-10460EC7C230}`nVersion=$version`nSourceRevision=$revision`nCliSHA256=$cliHash`n"
    [IO.File]::WriteAllText((Join-Path $payload 'INSTALLER-IDENTITY.ini'), $receipt, [Text.UTF8Encoding]::new($false))
    Invoke-Checked $CompilerPath @('--no-ide-signtools', '--no-signing', "--output-dir=$compiled", "--define=PayloadDirectory=$payload", "--define=AppVersion=$version", "--define=SourceRevision=$revision", (Join-Path $repository 'packaging/windows/installer.iss')) | Out-Host
    $installer = Join-Path $compiled "$name-setup.exe"
    if ((Get-AuthenticodeSignature -LiteralPath $installer).Status -ne 'NotSigned') { throw 'Expected an unsigned development installer.' }
    if ((Invoke-Checked git @('rev-parse', 'HEAD')) -ne $revision -or (Invoke-Checked git @('status', '--porcelain=v1'))) { throw 'Source identity changed during packaging; inspect the staged installer.' }
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $finalPath = Join-Path $OutputDirectory "$name-setup.exe"
    if ((Test-Path -LiteralPath $finalPath) -or (Test-Path -LiteralPath "$finalPath.sha256")) { throw 'The identified installer already exists; choose a new output directory.' }
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::Move($installer, $finalPath)
    $sidecar = [IO.File]::Open("$finalPath.sha256", [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes("$hash  $name-setup.exe`n")
        $sidecar.Write($bytes, 0, $bytes.Length)
    } finally { $sidecar.Dispose() }
    Write-Output "Saved unsigned development installer: $finalPath"
    Write-Output "SHA-256: $hash"
    Write-Output 'Compilation is not installation, upgrade, rollback or physical-latency qualification.'
} finally { Pop-Location }
