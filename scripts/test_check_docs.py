import importlib.util
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check-docs.py")
spec = importlib.util.spec_from_file_location("check_docs", MODULE_PATH)
if spec is None or spec.loader is None:
    raise RuntimeError(f"Unable to load {MODULE_PATH}")
check_docs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_docs)


class MarkdownLinkTests(unittest.TestCase):
    def setUp(self):
        self._root = check_docs.ROOT
        self._directory = tempfile.TemporaryDirectory()
        check_docs.ROOT = Path(self._directory.name)

    def tearDown(self):
        check_docs.ROOT = self._root
        self._directory.cleanup()

    def write(self, name, text=""):
        path = check_docs.ROOT / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def errors_for(self, markdown):
        self.write("README.md", markdown)
        errors = []
        check_docs.check_markdown_links(errors)
        return errors

    def test_percent_encoded_space_resolves(self):
        self.write("guide with spaces.md")
        errors = self.errors_for("[Guide](guide%20with%20spaces.md#usage)")
        self.assertEqual(errors, [])

    def test_percent_encoded_utf8_name_resolves(self):
        self.write("über.md")
        errors = self.errors_for("[Guide](%C3%BCber.md)")
        self.assertEqual(errors, [])

    def test_literal_plus_is_not_a_space(self):
        self.write("notes+cache.md")
        self.assertEqual(self.errors_for("[Notes](notes+cache.md)"), [])
        errors = self.errors_for("[Notes](notes%20cache.md)")
        self.assertEqual(len(errors), 1)

    def test_encoded_hash_stays_in_the_filename(self):
        self.write("issue#42.md")
        self.assertEqual(self.errors_for("[Issue](issue%2342.md)"), [])
        self.assertEqual(self.errors_for("[Issue](issue%2342.md#details)"), [])

    def test_fragment_and_external_links_are_unchanged(self):
        errors = self.errors_for(
            "[Section](#usage)\n[Site](https://example.com/missing.md)")
        self.assertEqual(errors, [])

    def test_missing_file_is_still_reported(self):
        errors = self.errors_for("[Gone](gone.md)")
        self.assertEqual(len(errors), 1)
        self.assertIn("gone.md", errors[0])


if __name__ == "__main__":
    unittest.main()
