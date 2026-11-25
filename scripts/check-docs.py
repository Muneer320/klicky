#!/usr/bin/env python3
from __future__ import annotations

import fnmatch
import os
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent
SKIP_PARTS = {".git", "target", "node_modules"}
MARKDOWN_LINK = re.compile(r"\[[^\]]*\]\((<[^>\n]*>[^)]*|[^)]+)\)")


def repository_files(pattern: str):
    for folder, directories, filenames in os.walk(ROOT):
        directories[:] = sorted(name for name in directories if name not in SKIP_PARTS)
        for name in sorted(filenames):
            if name != "AGENTS.md" and fnmatch.fnmatch(name, pattern):
                yield Path(folder) / name


def check_em_dashes(errors: list[str]) -> None:
    for path in repository_files("*"):
        if not path.is_file():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        if chr(0x2014) in text:
            errors.append(f"em dash: {path.relative_to(ROOT)}")


def link_destination(raw_target: str) -> str:
    """Return an inline link's destination without its optional title.

    An angle-enclosed destination may contain spaces (CommonMark), so it ends at the closing ``>``.
    """
    raw_target = raw_target.strip()
    if raw_target.startswith("<"):
        end = raw_target.find(">")
        if end != -1:
            return raw_target[1:end]
    return raw_target.split()[0].strip("<>")


def check_markdown_links(errors: list[str]) -> None:
    for path in repository_files("*.md"):
        text = path.read_text(encoding="utf-8")
        for raw_target in MARKDOWN_LINK.findall(text):
            target = link_destination(raw_target)
            if target.startswith(("http://", "https://", "mailto:", "#")):
                continue
            target = target.split("#", 1)[0]
            if target and not (path.parent / unquote(target)).resolve().exists():
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
