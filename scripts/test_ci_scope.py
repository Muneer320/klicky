"""Protect scope selection: native changes must never take the prose shortcut."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("ci_scope", Path(__file__).with_name("ci-scope.py"))
ci_scope = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci_scope)


class ScopeTests(unittest.TestCase):
    def test_prose_only(self):
        self.assertEqual(ci_scope.scope(["README.md", "docs/WINDOWS.md"]),
                         {"native": False, "packages": False})

    def test_website_keeps_package_and_repository_checks(self):
        self.assertEqual(ci_scope.scope(["assets/website/site.js", "packaging/apt/index.html"]),
                         {"native": False, "packages": True})

    def test_native_and_unknown_changes_keep_full_checks(self):
        for path in ("src/main.rs", "Cargo.lock", "sounds/eg-oreo/sound.ogg",
                     "scripts/install-macos.sh", "packaging/macos/KlickyMenu.swift",
                     ".github/workflows/ci.yml", ".github/workflows/ci-scope.yml",
                     "some-new-build-input", "scripts/a file\nwith newline.py"):
            with self.subTest(path=path):
                self.assertEqual(ci_scope.scope(["README.md", path]),
                                 {"native": True, "packages": True})

    def test_empty_diff_keeps_full_checks(self):
        self.assertEqual(ci_scope.scope([]), {"native": True, "packages": True})
