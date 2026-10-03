import importlib.util
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("build-apt-repository.py")
spec = importlib.util.spec_from_file_location("apt_repository", MODULE_PATH)
if spec is None or spec.loader is None:
    raise RuntimeError(f"Unable to load {MODULE_PATH}")
apt_repository = importlib.util.module_from_spec(spec)
spec.loader.exec_module(apt_repository)


class AptRepositoryTests(unittest.TestCase):
    def test_fingerprint_is_normalized(self):
        value = "0123456789abcdef0123456789abcdef01234567"
        self.assertEqual(apt_repository.validate_fingerprint(value), value.upper())

    def test_invalid_fingerprint_is_rejected(self):
        for value in ("short", "g" * 40, "0" * 39, "0" * 41):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    apt_repository.validate_fingerprint(value)

    def test_repository_url_requires_https(self):
        self.assertEqual(
            apt_repository.validate_public_url("https://example.com/klicky/"),
            "https://example.com/klicky",
        )
        with self.assertRaises(ValueError):
            apt_repository.validate_public_url("http://example.com/klicky")

    def test_incoming_package_overrides_existing_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            existing = root / "existing" / "pool" / "main" / "k" / "klicky"
            incoming = root / "incoming"
            existing.mkdir(parents=True)
            incoming.mkdir()
            old = existing / "klicky_0.3.0-1_amd64.deb"
            new = incoming / old.name
            old.write_bytes(b"old")
            new.write_bytes(b"new")

            packages = apt_repository.collect_package_sources(root / "existing", [new])

            self.assertEqual(packages, {old.name: new})

    def test_deb822_source_uses_repository_keyring(self):
        source = apt_repository.render_sources("https://muneer320.github.io/klicky")
        self.assertIn("Types: deb", source)
        self.assertIn("Suites: stable", source)
        self.assertIn("Components: main", source)
        self.assertIn("Architectures: amd64", source)
        self.assertIn(
            "Signed-By: /etc/apt/keyrings/klicky-archive-keyring.gpg", source
        )

    def test_repository_index_is_static_accessible_and_complete(self):
        page = apt_repository.render_index(
            "https://muneer320.github.io/klicky",
            ["0.3.0-1"],
            "DBB67AE478D2FFCEC6637B559899E554D3580D8F",
        )

        self.assertIn("Klicky APT Repository", page)
        self.assertIn("0.3.0-1", page)
        self.assertIn("sudo apt install klicky", page)
        self.assertIn("DBB6 7AE4 78D2 FFCE C663 7B55 9899 E554 D358 0D8F", page)
        self.assertIn("prefers-color-scheme: dark", page)
        self.assertIn('href="#main"', page)
        self.assertNotIn("<script", page)
        self.assertNotIn("https://fonts.", page)


if __name__ == "__main__":
    unittest.main()
