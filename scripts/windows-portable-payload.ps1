function Expand-KlickyPortable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Archive,
        [Parameter(Mandatory)][string]$Destination,
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][string]$Revision,
        [Parameter(Mandatory)][string]$Tree
    )
    $name = "klicky-$Version-windows-x64"
    if ([IO.Path]::GetFileName($Archive) -cne "$name.zip") { throw 'Unexpected portable archive name.' }
    New-Item -ItemType Directory -Path $Destination -ErrorAction Stop | Out-Null
    $stream = [IO.File]::OpenRead($Archive)
    try {
        $hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream)).ToLowerInvariant()
        if ((Get-Content -LiteralPath "$Archive.sha256" -Raw).Trim() -cne "$hash  $name.zip") {
            throw 'Portable archive checksum differs from its sidecar.'
        }
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
                if (($entry.ExternalAttributes -shr 16 -band 0xF000) -eq 0xA000) { throw "Archive symlink: $relative" }
                $totalBytes += $entry.Length
                if ($entry.Length -gt 268435456 -or $totalBytes -gt 536870912 -or $seen.Count -gt 4096) { throw 'Portable archive exceeds staging limits.' }
                $target = Join-Path $Destination $relative
                New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($target)) -Force | Out-Null
                $reader = $entry.Open()
                try {
                    $writer = [IO.File]::Open($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
                    try { $reader.CopyTo($writer) } finally { $writer.Dispose() }
                } finally { $reader.Dispose() }
            }
        } finally { $zip.Dispose() }
    } finally { $stream.Dispose() }
    $identity = Get-Content -LiteralPath (Join-Path $Destination 'BUILD-INFO.json') -Raw | ConvertFrom-Json
    if ($identity.product -ne 'Klicky' -or $identity.source_revision -ne $Revision -or $identity.source_tree -ne $Tree -or $identity.version -ne $Version -or $identity.target -ne 'x86_64-pc-windows-msvc' -or $identity.static_crt -ne $true -or $identity.signing -ne 'unsigned') {
        throw 'Portable build identity does not match the committed source.'
    }
    $inventoried = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($file in $identity.files) {
        if (-not $inventoried.Add($file.path) -or -not $seen.Contains($file.path) -or (Get-FileHash -LiteralPath (Join-Path $Destination $file.path) -Algorithm SHA256).Hash -ine $file.sha256) { throw "Portable payload verification failed: $($file.path)" }
    }
    if ($seen.Count -ne $inventoried.Count + 2 -or -not $seen.Contains('BUILD-INFO.json') -or -not $seen.Contains('THIRD-PARTY.txt')) { throw 'Unexpected un-inventoried payload files.' }
    return $identity
}
