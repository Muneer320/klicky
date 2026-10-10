#!/usr/bin/env python3
from __future__ import annotations

import argparse
import gzip
import hashlib
import importlib.util
import os
import subprocess
import sys
import tempfile
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("build-apt-repository.py")
spec = importlib.util.spec_from_file_location("apt_repository", MODULE_PATH)
if spec is None or spec.loader is None:
    raise RuntimeError(f"Unable to load {MODULE_PATH}")
apt_repository = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apt_repository)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--existing-root", type=Path)
    parser.add_argument("--exercise-history-retention", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--gnupg-home", type=Path)
    parser.add_argument("--fingerprint")
    parser.add_argument("packages", nargs="+", type=Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    project = Path(__file__).resolve().parent.parent
    packages = [package.resolve() for package in args.packages]

    with tempfile.TemporaryDirectory(prefix="klicky-apt-test-") as directory:
        root = Path(directory)
        root.chmod(0o755)
        gnupg = root / "gnupg"
        apt_state = root / "apt-state"
        apt_cache = root / "apt-cache"
        sources = root / "klicky.sources"
        repository = args.output.resolve() if args.output else root / "repository"
        existing_root = args.existing_root.resolve() if args.existing_root else None
        if args.exercise_history_retention:
            if existing_root:
                raise ValueError(
                    "--exercise-history-retention cannot be combined with --existing-root"
                )
            existing_root = root / "existing-packages"
            package_root = root / "historical-package"
            (package_root / "DEBIAN").mkdir(parents=True)
            (package_root / "usr/share/doc/klicky").mkdir(parents=True)
            (package_root / "DEBIAN/control").write_text(
                "Package: klicky\n"
                "Version: 0.0.1-1\n"
                "Architecture: amd64\n"
                "Section: utils\n"
                "Priority: optional\n"
                "Maintainer: Klicky CI <ci@example.invalid>\n"
                "Description: Historical package fixture\n"
            )
            (package_root / "usr/share/doc/klicky/README").write_text("fixture\n")
            existing_root.mkdir()
            subprocess.run(
                [
                    "dpkg-deb",
                    "--build",
                    "--root-owner-group",
                    str(package_root),
                    str(existing_root / "klicky_0.0.1-1_amd64.deb"),
                ],
                check=True,
            )
        package_sources = apt_repository.collect_package_sources(
            existing_root, packages)
        expected_versions = {
            apt_repository.validate_package(package)
            for package in package_sources.values()
        }
        if bool(args.gnupg_home) != bool(args.fingerprint):
            raise ValueError(
                "--gnupg-home and --fingerprint must be provided together"
            )
        if args.gnupg_home:
            gnupg = args.gnupg_home.resolve()
            fingerprint = apt_repository.validate_fingerprint(args.fingerprint)
        else:
            gnupg.mkdir(mode=0o700)
            fingerprint = ""
        (apt_state / "lists" / "partial").mkdir(parents=True)
        (apt_cache / "archives" / "partial").mkdir(parents=True)

        environment = os.environ.copy()
        environment["GNUPGHOME"] = str(gnupg)
        environment["LC_ALL"] = "C"
        if not args.gnupg_home:
            subprocess.run(
                [
                    "gpg",
                    "--batch",
                    "--passphrase",
                    "",
                    "--quick-generate-key",
                    "Klicky CI Repository <ci@example.invalid>",
                    "rsa2048",
                    "sign",
                    "1d",
                ],
                env=environment,
                check=True,
            )
            listing = subprocess.check_output(
                ["gpg", "--batch", "--with-colons", "--list-secret-keys"],
                env=environment,
                text=True,
            )
            fingerprint = next(
                line.split(":")[9]
                for line in listing.splitlines()
                if line.startswith("fpr:")
            )

        command = [
            sys.executable,
            str(project / "scripts" / "build-apt-repository.py"),
            "--output",
            str(repository),
            "--fingerprint",
            fingerprint,
            "--public-url",
            "https://muneer320.github.io/klicky",
        ]
        if existing_root:
            command.extend(["--existing-root", str(existing_root)])
        command.extend(str(package) for package in packages)
        subprocess.run(command, cwd=project, env=environment, check=True)

        required_files = [
            "index.html",
            "klicky.sources",
            "klicky-archive-keyring.gpg",
            "klicky-archive-keyring.asc",
            "dists/stable/InRelease",
            "dists/stable/Release",
            "dists/stable/Release.gpg",
            "dists/stable/main/binary-amd64/Packages",
            "dists/stable/main/binary-amd64/Packages.gz",
        ]
        missing = [
            name for name in required_files if not (repository / name).is_file()
        ]
        if missing:
            raise RuntimeError(
                f"Pages artifact is missing required files: {missing}")

        page = (repository / "index.html").read_text()
        grouped_fingerprint = " ".join(
            fingerprint[index: index + 4] for index in range(0, len(fingerprint), 4)
        )
        for content in (
            "<title>Klicky / Give your keys a voice</title>",
            "sudo apt install klicky",
            grouped_fingerprint,
        ):
            if content not in page:
                raise RuntimeError(
                    f"Pages homepage is missing expected content: {content}"
                )
        if "{{" in page:
            raise RuntimeError(
                "Pages homepage contains an unresolved template marker")

        index = gzip.decompress(
            (repository / "dists/stable/main/binary-amd64/Packages.gz").read_bytes()
        ).decode("utf-8")
        indexed_versions = set()
        for paragraph in index.strip().split("\n\n"):
            fields = {}
            for line in paragraph.splitlines():
                if line and not line[0].isspace() and ": " in line:
                    name, value = line.split(": ", 1)
                    fields[name] = value
            if (
                fields.get("Package") != "klicky"
                or fields.get("Architecture") != "amd64"
            ):
                raise RuntimeError(
                    f"Unexpected package entry in generated index: {fields}")
            filename = Path(fields["Filename"])
            if filename.is_absolute() or ".." in filename.parts:
                raise RuntimeError(
                    f"Unsafe package path in generated index: {filename}")
            package_path = repository / filename
            if not package_path.is_file():
                raise RuntimeError(
                    f"Indexed package file is missing: {filename}")
            package_bytes = package_path.read_bytes()
            if len(package_bytes) != int(fields["Size"]):
                raise RuntimeError(
                    f"Indexed package size does not match: {filename}")
            if hashlib.sha256(package_bytes).hexdigest() != fields["SHA256"]:
                raise RuntimeError(
                    f"Indexed package checksum does not match: {filename}")
            indexed_versions.add(fields["Version"])
        if indexed_versions != expected_versions:
            raise RuntimeError(
                "Generated repository package history differs from its inputs: "
                f"expected={sorted(expected_versions)}, actual={sorted(indexed_versions)}"
            )

        sources.write_text(
            "Types: deb\n"
            f"URIs: file:{repository}\n"
            "Suites: stable\n"
            "Components: main\n"
            "Architectures: amd64\n"
            f"Signed-By: {repository / 'klicky-archive-keyring.gpg'}\n"
        )

        apt_options = [
            "-o",
            f"Dir::Etc::sourcelist={sources}",
            "-o",
            "Dir::Etc::sourceparts=-",
            "-o",
            f"Dir::State={apt_state}",
            "-o",
            f"Dir::Cache={apt_cache}",
            "-o",
            "APT::Get::List-Cleanup=0",
            "-o",
            "APT::Sandbox::User=root",
        ]
        subprocess.run(["apt-get", *apt_options, "update"], check=True)
        madison = subprocess.check_output(
            ["apt-cache", *apt_options, "madison", "klicky"],
            env=environment,
            text=True,
        )
        apt_versions = set()
        for line in madison.splitlines():
            columns = line.split("|")
            if len(columns) >= 2 and columns[0].strip() == "klicky":
                apt_versions.add(columns[1].strip())
        if apt_versions != expected_versions:
            raise RuntimeError(
                "APT does not discover every indexed version: "
                f"expected={sorted(expected_versions)}, actual={sorted(apt_versions)}"
            )
        policy = subprocess.check_output(
            ["apt-cache", *apt_options, "policy", "klicky"],
            env=environment,
            text=True,
        )
        latest_version = next(iter(apt_versions))
        for version in apt_versions:
            if apt_repository.compare_packages(
                (Path(""), latest_version), (Path(""), version)
            ) < 0:
                latest_version = version
        if f"Candidate: {latest_version}" not in {
            line.strip() for line in policy.splitlines()
        }:
            raise RuntimeError(policy)

        print(f"fingerprint={fingerprint}")
        print("signature=verified")
        print("apt_update=passed")
        print(f"package_versions={','.join(sorted(expected_versions))}")


if __name__ == "__main__":
    main()
