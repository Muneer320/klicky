#!/usr/bin/env python3
"""Compile and test the menu's real subprocess client without installing it."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="klicky-menu-tests-") as directory:
    main = Path(directory) / "main.swift"
    main.write_text(
        (root / "packaging/macos/KlickyMenu.swift").read_text()
        + "\n"
        + (root / "packaging/macos/tests/KlickyMenuTests.swift").read_text()
    )
    binary = Path(directory) / "menu-tests"
    subprocess.run(["swiftc", "-D", "MENU_TEST", str(main), "-o", str(binary)], check=True)
    subprocess.run([str(binary)], check=True, timeout=15)
