#!/usr/bin/env python3
from __future__ import annotations

import argparse
import html
import re
import shutil
import subprocess
import tempfile
from functools import cmp_to_key
from pathlib import Path
from urllib.parse import urlsplit


FINGERPRINT = re.compile(r"[0-9A-Fa-f]{40}")
PRESERVED_OUTPUTS = {".git", "CNAME"}


def validate_fingerprint(value: str) -> str:
    value = value.strip()
    if not FINGERPRINT.fullmatch(value):
        raise ValueError("fingerprint must contain exactly 40 hexadecimal characters")
    return value.upper()


def validate_public_url(value: str) -> str:
    value = value.rstrip("/")
    parsed = urlsplit(value)
    if parsed.scheme != "https" or not parsed.netloc or parsed.query or parsed.fragment:
        raise ValueError("public repository URL must be an HTTPS URL without a query or fragment")
    return value


def collect_package_sources(
    existing_root: Path | None, incoming: list[Path]
) -> dict[str, Path]:
    packages: dict[str, Path] = {}
    if existing_root and existing_root.exists():
        for package in sorted(existing_root.rglob("*.deb")):
            packages[package.name] = package
    for package in incoming:
        packages[package.name] = package
    return packages


def render_sources(public_url: str) -> str:
    public_url = validate_public_url(public_url)
    return (
        "Types: deb\n"
        f"URIs: {public_url}\n"
        "Suites: stable\n"
        "Components: main\n"
        "Architectures: amd64\n"
        "Signed-By: /etc/apt/keyrings/klicky-archive-keyring.gpg\n"
    )


def render_distributions(fingerprint: str) -> str:
    fingerprint = validate_fingerprint(fingerprint)
    return (
        "Origin: Klicky\n"
        "Label: Klicky\n"
        "Codename: stable\n"
        "Suite: stable\n"
        "Architectures: amd64\n"
        "Components: main\n"
        "Description: Klicky stable APT repository\n"
        f"SignWith: {fingerprint}\n"
    )


def command_output(command: list[str]) -> str:
    return subprocess.check_output(command, text=True).strip()


def validate_package(path: Path) -> str:
    if not path.is_file():
        raise ValueError(f"package not found: {path}")
    package = command_output(["dpkg-deb", "--field", str(path), "Package"])
    architecture = command_output(
        ["dpkg-deb", "--field", str(path), "Architecture"]
    )
    version = command_output(["dpkg-deb", "--field", str(path), "Version"])
    if package != "klicky":
        raise ValueError(f"unexpected package name in {path}: {package}")
    if architecture != "amd64":
        raise ValueError(f"unexpected architecture in {path}: {architecture}")
    return version


def compare_packages(left: tuple[Path, str], right: tuple[Path, str]) -> int:
    left_version = left[1]
    right_version = right[1]
    if subprocess.run(
        ["dpkg", "--compare-versions", left_version, "lt", right_version]
    ).returncode == 0:
        return -1
    if subprocess.run(
        ["dpkg", "--compare-versions", left_version, "gt", right_version]
    ).returncode == 0:
        return 1
    return 0


def write_index(path: Path, public_url: str, versions: list[str]) -> None:
    items = "".join(f"<li>Klicky {html.escape(version)}</li>" for version in versions)
    path.write_text(
        "<!doctype html>\n"
        '<html lang="en"><meta charset="utf-8">\n'
        "<title>Klicky APT repository</title>\n"
        "<h1>Klicky APT repository</h1>\n"
        "<p>Signed stable packages for Debian-compatible amd64 systems.</p>\n"
        f"<p>Repository: <code>{html.escape(public_url)}</code></p>\n"
        f"<ul>{items}</ul>\n"
        '<p><a href="klicky.sources">Deb822 source</a> | '
        '<a href="klicky-archive-keyring.gpg">Signing key</a></p>\n'
    )


def build_repository(
    output: Path,
    existing_root: Path | None,
    incoming: list[Path],
    fingerprint: str,
    public_url: str,
) -> None:
    fingerprint = validate_fingerprint(fingerprint)
    public_url = validate_public_url(public_url)
    sources = collect_package_sources(existing_root, incoming)
    if not sources:
        raise ValueError("no Debian packages were provided")

    packages_with_versions = [
        (path, validate_package(path)) for path in sources.values()
    ]
    packages_with_versions.sort(key=cmp_to_key(compare_packages))

    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="klicky-apt-", dir=output.parent) as temp:
        temp_root = Path(temp)
        repository = temp_root / "repository"
        publish = temp_root / "publish"
        package_dir = temp_root / "packages"
        (repository / "conf").mkdir(parents=True)
        package_dir.mkdir()
        publish.mkdir()

        (repository / "conf" / "distributions").write_text(
            render_distributions(fingerprint)
        )

        for package, _ in packages_with_versions:
            staged = package_dir / package.name
            shutil.copy2(package, staged)
            subprocess.run(
                ["reprepro", "--basedir", str(repository), "includedeb", "stable", str(staged)],
                check=True,
            )

        subprocess.run(
            ["reprepro", "--basedir", str(repository), "check", "stable"],
            check=True,
        )

        required = [
            repository / "dists" / "stable" / "InRelease",
            repository / "dists" / "stable" / "Release",
            repository / "dists" / "stable" / "Release.gpg",
            repository / "dists" / "stable" / "main" / "binary-amd64" / "Packages",
            repository / "dists" / "stable" / "main" / "binary-amd64" / "Packages.gz",
        ]
        missing = [str(path) for path in required if not path.is_file()]
        if missing:
            raise RuntimeError(f"repository export is incomplete: {missing}")

        shutil.copytree(repository / "dists", publish / "dists")
        shutil.copytree(repository / "pool", publish / "pool")
        (publish / ".nojekyll").write_text("")
        (publish / "klicky.sources").write_text(render_sources(public_url))

        subprocess.run(
            [
                "gpg",
                "--batch",
                "--yes",
                "--output",
                str(publish / "klicky-archive-keyring.gpg"),
                "--export",
                fingerprint,
            ],
            check=True,
        )
        subprocess.run(
            [
                "gpg",
                "--batch",
                "--yes",
                "--armor",
                "--output",
                str(publish / "klicky-archive-keyring.asc"),
                "--export",
                fingerprint,
            ],
            check=True,
        )
        subprocess.run(
            [
                "gpgv",
                "--keyring",
                str(publish / "klicky-archive-keyring.gpg"),
                str(publish / "dists" / "stable" / "InRelease"),
            ],
            check=True,
        )

        versions = [version for _, version in packages_with_versions]
        write_index(publish / "index.html", public_url, versions)

        output.mkdir(parents=True, exist_ok=True)
        for path in output.iterdir():
            if path.name in PRESERVED_OUTPUTS:
                continue
            if path.is_dir():
                shutil.rmtree(path)
            else:
                path.unlink()
        for path in publish.iterdir():
            shutil.move(str(path), output / path.name)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build the signed Klicky APT repository")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--existing-root", type=Path)
    parser.add_argument("--fingerprint", required=True)
    parser.add_argument("--public-url", required=True)
    parser.add_argument("deb", type=Path, nargs="+")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    build_repository(
        output=args.output,
        existing_root=args.existing_root,
        incoming=args.deb,
        fingerprint=args.fingerprint,
        public_url=args.public_url,
    )


if __name__ == "__main__":
    main()
