#!/usr/bin/env python3
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: test_apt_repository_integration.py PACKAGE.deb")

    project = Path(__file__).resolve().parent.parent
    package = Path(sys.argv[1]).resolve()

    with tempfile.TemporaryDirectory(prefix="klicky-apt-test-") as directory:
        root = Path(directory)
        root.chmod(0o755)
        gnupg = root / "gnupg"
        repository = root / "repository"
        apt_state = root / "apt-state"
        apt_cache = root / "apt-cache"
        sources = root / "klicky.sources"
        gnupg.mkdir(mode=0o700)
        (apt_state / "lists" / "partial").mkdir(parents=True)
        (apt_cache / "archives" / "partial").mkdir(parents=True)

        environment = os.environ.copy()
        environment["GNUPGHOME"] = str(gnupg)
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

        subprocess.run(
            [
                sys.executable,
                str(project / "scripts" / "build-apt-repository.py"),
                "--output",
                str(repository),
                "--fingerprint",
                fingerprint,
                "--public-url",
                "https://muneer320.github.io/klicky",
                str(package),
            ],
            cwd=project,
            env=environment,
            check=True,
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
        policy = subprocess.check_output(
            ["apt-cache", *apt_options, "policy", "klicky"], text=True
        )
        if "0.3.0-1" not in policy:
            raise RuntimeError(policy)

        print(f"fingerprint={fingerprint}")
        print("signature=verified")
        print("apt_update=passed")
        print("package_version=0.3.0-1")


if __name__ == "__main__":
    main()
