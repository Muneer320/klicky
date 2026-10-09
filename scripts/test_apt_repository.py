import importlib.util
import tempfile
import unittest
from html.parser import HTMLParser
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
        self.assertRegex(page, r"prefers-color-scheme:\s*dark")
        self.assertIn('href="#main"', page)
        self.assertNotIn("<script", page)
        self.assertNotIn("https://fonts.", page)

    def test_product_page_preserves_navigation_packs_and_archive_paths(self):
        class Elements(HTMLParser):
            def __init__(self):
                super().__init__()
                self.ids = []
                self.links = []

            def handle_starttag(self, tag, attrs):
                attrs = dict(attrs)
                if "id" in attrs:
                    self.ids.append(attrs["id"])
                if tag == "a":
                    self.links.append(attrs["href"])

        page = apt_repository.render_index(
            "https://example.com/klicky", ["0.3.0-1"], "A" * 40
        )
        elements = Elements()
        elements.feed(page)
        self.assertEqual(len(elements.ids), len(set(elements.ids)))
        for link in elements.links:
            if link.startswith("#"):
                self.assertIn(link[1:], elements.ids)
            elif not link.startswith("https://"):
                self.assertIn(link, {
                    "klicky.sources", "klicky-archive-keyring.gpg",
                    "klicky-archive-keyring.asc", "dists/stable/InRelease",
                })
        for pack in (MODULE_PATH.parent.parent / "sounds").iterdir():
            if pack.is_dir():
                self.assertIn(pack.name, page)
        self.assertNotIn("{{", page)
        self.assertIn("not physical key-to-speaker latency", page)
        self.assertRegex(page, r"prefers-reduced-motion:\s*reduce")
        self.assertIn("Silent, conceptual illustration", page)

    def test_index_escapes_dynamic_archive_metadata(self):
        page = apt_repository.render_index(
            "https://example.com/klicky", ['<img src=x onerror="bad">'], "A" * 40
        )
        self.assertNotIn("<img", page)
        self.assertIn("&lt;img", page)


if __name__ == "__main__":
    unittest.main()
