#!/usr/bin/env python3
from __future__ import annotations

import argparse
import gzip
import html
import os
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
        raise ValueError(
            "fingerprint must contain exactly 40 hexadecimal characters")
    return value.upper()


def validate_public_url(value: str) -> str:
    value = value.rstrip("/")
    parsed = urlsplit(value)
    if parsed.scheme != "https" or not parsed.netloc or parsed.query or parsed.fragment:
        raise ValueError(
            "public repository URL must be an HTTPS URL without a query or fragment")
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


def render_index(public_url: str, versions: list[str], fingerprint: str) -> str:
    public_url = validate_public_url(public_url)
    fingerprint = validate_fingerprint(fingerprint)
    if not versions:
        raise ValueError("at least one package version is required")

    template_path = Path(__file__).resolve(
    ).parent.parent / "packaging/apt/index.html"
    template = template_path.read_text()
    items = "".join(
        f"<li>klicky {html.escape(version)}</li>" for version in versions
    )
    grouped_fingerprint = " ".join(
        fingerprint[index: index + 4] for index in range(0, len(fingerprint), 4)
    )
    replacements = {
        "{{PUBLIC_URL}}": html.escape(public_url, quote=True),
        "{{VERSIONS}}": items,
        "{{LATEST_VERSION}}": html.escape(versions[-1]),
        "{{FINGERPRINT}}": grouped_fingerprint,
    }
    for marker, value in replacements.items():
        template = template.replace(marker, value)
    return template


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
                [
                    "reprepro",
                    "--keepunreferencedfiles",
                    "--basedir",
                    str(repository),
                    "includedeb",
                    "stable",
                    str(staged),
                ],
                check=True,
            )

        subprocess.run(
            ["reprepro", "--basedir", str(repository), "check", "stable"],
            check=True,
        )

        package_index = (
            repository / "dists/stable/main/binary-amd64/Packages"
        )
        package_index_gzip = package_index.with_name("Packages.gz")
        packages_output = subprocess.check_output(
            ["dpkg-scanpackages", "--multiversion",
                "--arch", "amd64", "pool", "/dev/null"],
            cwd=repository,
            text=True,
        )
        package_index.write_text(packages_output)
        package_index_gzip.write_bytes(
            gzip.compress(packages_output.encode("utf-8"), mtime=0)
        )

        release_path = repository / "dists/stable/Release"
        release_options = [
            "-o",
            "APT::FTPArchive::Release::Origin=Klicky",
            "-o",
            "APT::FTPArchive::Release::Label=Klicky",
            "-o",
            "APT::FTPArchive::Release::Codename=stable",
            "-o",
            "APT::FTPArchive::Release::Suite=stable",
            "-o",
            "APT::FTPArchive::Release::Architectures=amd64",
            "-o",
            "APT::FTPArchive::Release::Components=main",
            "-o",
            "APT::FTPArchive::Release::Description=Klicky stable APT repository",
        ]
        with release_path.open("w", encoding="utf-8", newline="\n") as release:
            subprocess.run(
                ["apt-ftparchive", *release_options, "release", "dists/stable"],
                cwd=repository,
                stdout=release,
                check=True,
            )

        passphrase = os.environ.get("APT_GPG_PASSPHRASE", "") + "\n"
        signature_args = [
            "gpg",
            "--batch",
            "--yes",
            "--pinentry-mode",
            "loopback",
            "--passphrase-fd",
            "0",
            "--local-user",
            fingerprint,
            "--digest-algo",
            "SHA256",
        ]
        for signature_path, signature_mode in (
            (repository / "dists/stable/InRelease", "--clearsign"),
            (repository / "dists/stable/Release.gpg", "--detach-sign"),
        ):
            subprocess.run(
                [
                    *signature_args,
                    "--output",
                    str(signature_path),
                    signature_mode,
                    str(release_path),
                ],
                input=passphrase,
                text=True,
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
        (publish / "index.html").write_text(render_index(public_url, versions, fingerprint))

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
    parser = argparse.ArgumentParser(
        description="Build the signed Klicky APT repository")
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
