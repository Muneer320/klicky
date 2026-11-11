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
    $name = "klicky-$version-windows-x64"
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
    New-Item -ItemType Directory -Path $compiled -Force | Out-Null
    . (Join-Path $PSScriptRoot 'windows-portable-payload.ps1')
    $identity = Expand-KlickyPortable -Archive $PortableArchive -Destination $payload -Version $version -Revision $revision -Tree $tree
    $cliHash = (Get-FileHash -LiteralPath (Join-Path $payload 'klicky.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    $receipt = "[Klicky]`nAppId={B8E55779-4EDB-4D8F-962D-10460EC7C230}`nVersion=$version`nSourceRevision=$revision`nCliSHA256=$cliHash`nRecoveryVersion=1`n"
    [IO.File]::WriteAllText((Join-Path $payload 'INSTALLER-IDENTITY.ini'), $receipt, [Text.UTF8Encoding]::new($false))
    Invoke-Checked $CompilerPath @('--no-ide-signtools', '--no-signing', "--output-dir=$compiled", "--define=PayloadDirectory=$payload", "--define=AppVersion=$version", "--define=SourceRevision=$revision", (Join-Path $repository 'packaging/windows/installer.iss')) | Out-Host
    $installer = Join-Path $compiled "$name-setup.exe"
    if ((Get-AuthenticodeSignature -LiteralPath $installer).Status -ne 'NotSigned') { throw 'Expected an unsigned Windows installer.' }
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
    Write-Output "Saved unsigned Windows installer: $finalPath"
    Write-Output "SHA-256: $hash"
    Write-Output 'Compilation is not installation, upgrade, rollback or physical-latency qualification.'
} finally { Pop-Location }
