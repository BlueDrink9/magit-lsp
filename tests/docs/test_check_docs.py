"""Focused tests for local documentation link checks."""

import tempfile
import unittest
from pathlib import Path

from scripts.check_docs import check_document


class CheckDocsTests(unittest.TestCase):
    """Check valid and broken local links and heading fragments."""

    def test_valid_relative_path_and_heading(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "target.md").write_text("# Useful Heading\n", encoding="utf-8")
            source = root / "source.md"
            source.write_text("[ok](target.md#useful-heading)\n", encoding="utf-8")
            self.assertEqual([], check_document(source))

    def test_missing_path_and_heading_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.md"
            source.write_text("[path](absent.md) [anchor](#absent)\n", encoding="utf-8")
            errors = check_document(source)
            self.assertEqual(2, len(errors))
            self.assertTrue(any("missing path" in error for error in errors))
            self.assertTrue(any("missing heading" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
