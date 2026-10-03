#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKIP_PARTS = {".git", "target"}
MARKDOWN_LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")


def repository_files(pattern: str):
    for path in ROOT.rglob(pattern):
        if SKIP_PARTS.intersection(path.parts) or path.name == "AGENTS.md":
            continue
        yield path


def check_em_dashes(errors: list[str]) -> None:
    for path in repository_files("*"):
        if not path.is_file():
            continue
        try:
            text = path.read_text()
        except UnicodeDecodeError:
            continue
        if chr(0x2014) in text:
            errors.append(f"em dash: {path.relative_to(ROOT)}")


def check_markdown_links(errors: list[str]) -> None:
    for path in repository_files("*.md"):
        text = path.read_text()
        for raw_target in MARKDOWN_LINK.findall(text):
            target = raw_target.split()[0].strip("<>")
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            target = target.split("#", 1)[0]
            if target and not (path.parent / target).resolve().exists():
                errors.append(f"broken link: {path.relative_to(ROOT)} -> {target}")


def check_svg(errors: list[str]) -> None:
    for path in repository_files("*.svg"):
        try:
            ET.parse(path)
        except ET.ParseError as error:
            errors.append(f"invalid SVG: {path.relative_to(ROOT)}: {error}")


def main() -> int:
    errors: list[str] = []
    check_em_dashes(errors)
    check_markdown_links(errors)
    check_svg(errors)

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print("Documentation checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
