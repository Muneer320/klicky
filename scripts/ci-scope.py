#!/usr/bin/env python3
"""Select expensive PR checks from NUL-delimited git diff paths on stdin.

Only known documentation and website paths get a reduced check set. Unknown
paths, workflow changes and an empty diff retain full native/package coverage.
"""
import sys


def scope(paths):
    paths = list(paths)
    native = packages = not paths
    for path in paths:
        if path.endswith(".md"):
            continue
        packages = True
        if not path.startswith(("assets/website/", "packaging/apt/")):
            native = True
    return {"native": native, "packages": packages}


if __name__ == "__main__":
    result = scope(p.decode("utf-8", errors="surrogateescape")
                   for p in sys.stdin.buffer.read().split(b"\0") if p)
    for name, enabled in result.items():
        print(f"{name}={str(enabled).lower()}")
