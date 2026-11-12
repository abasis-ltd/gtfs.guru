#!/usr/bin/env python3
"""Offline tests for pages_redirects.py."""

import tempfile
import unittest
from pathlib import Path

import pages_redirects


class PagesRedirectsTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.website = root / "website"
        self.site = root / "site"
        for relative in [
            "index.html",
            "how-to-create-gtfs-file.html",
            "notices/index.html",
            "notices/expired_calendar/index.html",
            "pkg/report.html",
            ".hidden/page.html",
        ]:
            path = self.website / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("<html></html>")
        (self.site).mkdir()
        (self.site / "index.html").write_text("docs home")

    def tearDown(self):
        self.tmp.cleanup()

    def test_moved_pages_point_at_the_same_path_on_gtfs_guru(self):
        written = pages_redirects.write_stubs(self.website, self.site)

        self.assertEqual(
            written,
            [
                "how-to-create-gtfs-file.html",
                "notices/expired_calendar/index.html",
                "notices/index.html",
            ],
        )
        stub = (self.site / "how-to-create-gtfs-file.html").read_text()
        self.assertIn(
            '<link rel="canonical" href="https://gtfs.guru/how-to-create-gtfs-file.html">',
            stub,
        )
        self.assertIn('url=https://gtfs.guru/how-to-create-gtfs-file.html"', stub)
        notice = (self.site / "notices/expired_calendar/index.html").read_text()
        self.assertIn('href="https://gtfs.guru/notices/expired_calendar/"', notice)

    def test_pages_the_docs_build_produced_are_kept(self):
        pages_redirects.write_stubs(self.website, self.site)
        self.assertEqual((self.site / "index.html").read_text(), "docs home")

    def test_the_wasm_bundle_and_dotfiles_are_not_pages(self):
        pages_redirects.write_stubs(self.website, self.site)
        self.assertFalse((self.site / "pkg/report.html").exists())
        self.assertFalse((self.site / ".hidden/page.html").exists())


if __name__ == "__main__":
    unittest.main()
