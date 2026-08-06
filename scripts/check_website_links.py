#!/usr/bin/env python3
"""Fail when a static website page references a missing local asset."""

from __future__ import annotations

import argparse
import html.parser
import pathlib
import sys
import urllib.parse


class LinkParser(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.references: list[tuple[str, str]] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        for name, value in attrs:
            if value and name in {"href", "src"}:
                self.references.append((name, value))


def resolve_local_reference(
    website_root: pathlib.Path, page: pathlib.Path, reference: str
) -> pathlib.Path | None:
    parsed = urllib.parse.urlsplit(reference)
    if parsed.scheme or parsed.netloc or not parsed.path:
        return None

    decoded_path = urllib.parse.unquote(parsed.path)
    if decoded_path.startswith("/"):
        target = website_root / decoded_path.lstrip("/")
    else:
        target = page.parent / decoded_path

    if decoded_path.endswith("/") or target.is_dir():
        target /= "index.html"
    return target.resolve()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("website_root", nargs="?", default="website")
    args = parser.parse_args()

    website_root = pathlib.Path(args.website_root).resolve()
    failures: list[str] = []
    for page in sorted(website_root.rglob("*.html")):
        link_parser = LinkParser()
        link_parser.feed(page.read_text(encoding="utf-8"))
        for attribute, reference in link_parser.references:
            target = resolve_local_reference(website_root, page, reference)
            if target is None:
                continue
            if website_root not in target.parents and target != website_root:
                failures.append(
                    f"{page.relative_to(website_root)}: {attribute}={reference!r} "
                    "escapes the website root"
                )
            elif not target.is_file():
                failures.append(
                    f"{page.relative_to(website_root)}: {attribute}={reference!r} "
                    f"does not resolve to a file ({target.relative_to(website_root)})"
                )

    for failure in failures:
        print(f"::error::{failure}")
    if failures:
        print(f"Found {len(failures)} broken local website reference(s).")
        return 1
    print("All local website links and assets resolve.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
