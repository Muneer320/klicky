#!/usr/bin/env python3
"""Verify the complete candidate set before generating public release checksums."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import tarfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath


def assemble(directory: Path, version: str, revision: str, tree: str) -> None:
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("Expected a numeric semantic version")
    if not all(re.fullmatch(r"[0-9a-f]{40}", value) for value in (revision, tree)):
        raise ValueError("Expected full source revision and tree hashes")
    windows = f"klicky-{version}-windows-x64"
    payloads = [
        f"klicky_{version}-1_amd64.deb",
        f"klicky-{version}-1-x86_64.pkg.tar.zst",
        f"klicky-{version}-source.tar.gz",
        f"{windows}.zip",
        f"{windows}-setup.exe",
    ]
    sidecars = [f"{windows}.zip.sha256", f"{windows}-setup.exe.sha256"]
    expected = set(payloads + sidecars)
    if {path.name for path in directory.iterdir()} != expected:
        raise ValueError("Candidate must contain exactly the expected platform assets and sidecars")
    hashes = {}
    for name in sorted(expected):
        path = directory / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"Not a nonempty ordinary asset: {name}")
        with path.open("rb") as source:
            hashes[name] = hashlib.file_digest(source, "sha256").hexdigest()
    for name in sidecars:
        payload = name.removesuffix(".sha256")
        if (directory / name).read_text().strip() != f"{hashes[payload]}  {payload}":
            raise ValueError(f"Checksum mismatch: {payload}")
    with zipfile.ZipFile(directory / f"{windows}.zip") as archive:
        identity = json.loads(archive.read(f"{windows}/BUILD-INFO.json"))
        if (identity["version"], identity["source_revision"], identity["source_tree"]) != (
            version, revision, tree
        ):
            raise ValueError("Windows bundle source identity differs from the release candidate")
        if identity["signing"] != "unsigned" or identity["target"] != "x86_64-pc-windows-msvc":
            raise ValueError("Unexpected Windows signing or target identity")
        names = [f"{windows}/BUILD-INFO.json", f"{windows}/THIRD-PARTY.txt"]
        for item in identity["files"]:
            path = PurePosixPath(item["path"])
            if path.is_absolute() or ".." in path.parts or "\\" in item["path"]:
                raise ValueError("Invalid Windows inventory path")
            names.append(f"{windows}/{item['path']}")
            if hashlib.sha256(archive.read(f"{windows}/{item['path']}")).hexdigest() != item["sha256"]:
                raise ValueError(f"Windows payload mismatch: {item['path']}")
        if len(names) != len(set(names)) or sorted(archive.namelist()) != sorted(names):
            raise ValueError("Unexpected or duplicate Windows ZIP entries")
    with tarfile.open(directory / f"klicky-{version}-source.tar.gz") as archive:
        if archive.pax_headers.get("comment") != revision:
            raise ValueError("Source archive is not from the candidate commit")
        with archive.extractfile(f"klicky-{version}/Cargo.toml") as source:
            if tomllib.loads(source.read().decode())["package"]["version"] != version:
                raise ValueError("Source archive version mismatch")
        for name in ("scripts/install.sh", "scripts/install-macos.sh", "scripts/uninstall.sh"):
            if archive.getmember(f"klicky-{version}/{name}").mode & 0o111 != 0o111:
                raise ValueError(f"Source archive lost executable mode: {name}")
    manifest = {
        "product": "Klicky", "version": version,
        "source_revision": revision, "source_tree": tree,
        "windows_signing": "unsigned", "macos_distribution": "source",
        "assets": [{"name": name, "sha256": hashes[name], "bytes": (directory / name).stat().st_size}
                   for name in sorted(expected)],
    }
    (directory / "RELEASE-MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
    hashes["RELEASE-MANIFEST.json"] = hashlib.sha256((directory / "RELEASE-MANIFEST.json").read_bytes()).hexdigest()
    (directory / "SHA256SUMS").write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(hashes.items())))
    print(f"Verified {len(payloads)} release payloads from {revision}; no release published.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--tree", required=True)
    args = parser.parse_args()
    assemble(args.directory, args.version, args.revision, args.tree)
