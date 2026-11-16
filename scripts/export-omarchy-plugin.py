#!/usr/bin/env python3
"""Export the curated Klicky Bar publication tree into a new directory."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


FILES = ("manifest.json", "BarWidget.qml", "ControlsPanel.qml", "Controller.qml", "PanelText.qml", "i18n/en.json", "README.md")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--source-commit", help="Full source commit when exporting an archive")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    commit = args.source_commit
    if commit is None:
        commit = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
        if subprocess.check_output(["git", "-C", str(root), "status", "--porcelain"], text=True).strip():
            parser.error("Commit the source tree first, or explicitly supply --source-commit for a development export")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        parser.error("--source-commit must be a full Git commit hash")
    source = root / "packaging" / "omarchy-bar"
    contents = {}
    for name in FILES:
        path = source / name
        if path.is_symlink() or not path.is_file():
            parser.error(f"Expected regular source file: {path}")
        contents[name] = path.read_bytes().replace(b"\r\n", b"\n")
    contents["LICENSE"] = (root / "LICENSE").read_bytes().replace(b"\r\n", b"\n")
    manifest = json.loads(contents["manifest.json"])
    if manifest.get("schemaVersion") != 1 or manifest.get("id") != "io.github.muneer320.klicky-bar":
        parser.error("Unexpected plugin manifest identity or schema")
    for entry in manifest["entryPoints"].values():
        if entry not in contents:
            parser.error(f"Entrypoint is outside the curated export: {entry}")
    provenance = {
        "repository": "https://github.com/Muneer320/klicky",
        "commit": commit,
        "version": manifest["version"],
        "files": {name: hashlib.sha256(data).hexdigest() for name, data in sorted(contents.items())},
    }
    args.output.mkdir(parents=True, exist_ok=False)
    for name, data in contents.items():
        destination = args.output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    (args.output / "SOURCE.json").write_bytes((json.dumps(provenance, indent=2) + "\n").encode("utf-8"))
    print(f"Exported Klicky Bar {manifest['version']} to {args.output}")


if __name__ == "__main__":
    main()
