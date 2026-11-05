# Build-time inventory, consumed by build-windows-portable.ps1. No network fetches.
function Get-KlickyLicenseFiles {
    param($Metadata, [string]$Repository, [string]$Sysroot)
    $lockedChecksums = @{}
    $lock = Get-Content -LiteralPath (Join-Path $Repository 'Cargo.lock') -Raw
    foreach ($block in [regex]::Matches($lock, '(?ms)^\[\[package\]\]\r?\n(.*?)(?=^\[\[package\]\]|\z)')) {
        $name = [regex]::Match($block.Value, '(?m)^name = "([^"]+)"\r?$')
        $version = [regex]::Match($block.Value, '(?m)^version = "([^"]+)"\r?$')
        $checksum = [regex]::Match($block.Value, '(?m)^checksum = "([0-9a-f]{64})"\r?$')
        if ($name.Success -and $version.Success -and $checksum.Success) {
            $key = "$($name.Groups[1].Value)@$($version.Groups[1].Value)"
            if ($lockedChecksums.ContainsKey($key)) { throw "Ambiguous locked dependency: $key" }
            $lockedChecksums[$key] = $checksum.Groups[1].Value
        }
    }
    $packages = @{}
    foreach ($package in $Metadata.packages) { $packages[$package.id] = $package }
    $nodes = @{}
    foreach ($node in $Metadata.resolve.nodes) { $nodes[$node.id] = $node }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $pending = [Collections.Generic.Stack[string]]::new()
    $pending.Push($Metadata.resolve.root)
    $inventory = [Collections.Generic.List[object]]::new()
    # Exact declarations in the reviewed runtime graph. New terms require review.
    $reviewedLicenses = @(
        '(Apache-2.0 OR MIT) AND BSD-3-Clause', '(MIT OR Apache-2.0) AND Unicode-3.0',
        'Apache-2.0', 'Apache-2.0 OR MIT', 'MIT', 'MIT OR Apache-2.0', 'MIT/Apache-2.0',
        'MPL-2.0', 'Unlicense OR MIT', 'Zlib OR Apache-2.0 OR MIT'
    )
    while ($pending.Count) {
        $id = $pending.Pop()
        if (-not $seen.Add($id)) { continue }
        $package = $packages[$id]
        foreach ($edge in $nodes[$id].deps) {
            if (@($edge.dep_kinds | Where-Object { $null -eq $_.kind }).Count) { $pending.Push($edge.pkg) }
        }
        if ($id -eq $Metadata.resolve.root) { continue }
        if ($package.source -ne 'registry+https://github.com/rust-lang/crates.io-index') {
            throw "Review non-registry runtime dependency before packaging: $id"
        }
        if ($reviewedLicenses -cnotcontains $package.license) { throw "Review runtime license declaration before packaging: $id ($($package.license))" }
        $root = Split-Path $package.manifest_path
        $key = "$($package.name)-$($package.version)"
        $notices = @(Get-ChildItem -LiteralPath $root -File | Where-Object {
            $_.Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)([._-]|$)'
        })
        if ($package.license_file) {
            $declared = [IO.Path]::GetFullPath((Join-Path $root $package.license_file))
            if (-not $declared.StartsWith($root + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
                throw "License file leaves crate directory: $key"
            }
            $notices += Get-Item -LiteralPath $declared
        }
        if (-not $notices.Count) {
            if ($package.name -match '^symphonia($|-)' -and $package.version -eq '0.5.5' -and $package.license -eq 'MPL-2.0') {
                $fallback = 'symphonia-MPL-2.0.txt'
                $expected = 'c76f740d1521b9bed9ca7a04ad526c310493c62621b1341d623b431736533b30'
            } elseif ($key -eq 'dasp_sample-0.11.0' -and $package.license -eq 'MIT OR Apache-2.0') {
                $fallback = 'dasp_sample-MIT.txt'
                $expected = 'b1d6df41ed3aa96806e74c729444d7c121d90e6660a6aed01d298e03fde475a0'
            } else { throw "Missing runtime license notice: $key ($($package.license))" }
            $notice = Get-Item -LiteralPath (Join-Path $Repository "packaging/windows/licenses/$fallback")
            if ((Get-FileHash -LiteralPath $notice.FullName -Algorithm SHA256).Hash -ne $expected) {
                throw "Changed pinned upstream license: $fallback"
            }
            $notices = @($notice)
        }
        foreach ($notice in ($notices | Sort-Object FullName -Unique)) {
            Add-BundleFile $notice.FullName "licenses/crates/$key/$($notice.Name)"
        }
        $archiveName = $null
        if ($package.license -eq 'MPL-2.0') {
            $index = Split-Path $root
            $registry = Split-Path (Split-Path $index)
            $archive = Join-Path $registry "cache/$([IO.Path]::GetFileName($index))/$key.crate"
            $checksum = $lockedChecksums["$($package.name)@$($package.version)"]
            if (-not $checksum -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $checksum) {
                throw "MPL source archive does not match Cargo checksum: $key"
            }
            $archiveName = "third-party-sources/$key.crate"
            Add-BundleFile $archive $archiveName
        }
        $inventory.Add([ordered]@{
            name = $package.name; version = $package.version; license = $package.license
            source = "https://crates.io/crates/$($package.name)/$($package.version)"
            included_source = $archiveName
        })
    }
    $rustNotices = Join-Path $Sysroot 'share/doc/rust'
    Add-BundleFile (Join-Path $rustNotices 'COPYRIGHT-library.html') 'licenses/rust/COPYRIGHT-library.html'
    foreach ($file in (Get-ChildItem -LiteralPath (Join-Path $rustNotices 'licenses') -File)) {
        Add-BundleFile $file.FullName "licenses/rust/licenses/$($file.Name)"
    }
    return @($inventory | Sort-Object name, version)
}
