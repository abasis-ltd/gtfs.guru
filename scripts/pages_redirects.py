#!/usr/bin/env python3
"""Write redirect stubs into the built docs site for pages that moved to gtfs.guru.

Before the website moved to https://gtfs.guru, GitHub Pages served website/
itself at https://abasis-ltd.github.io/gtfs.guru/. Search engines still rank
those addresses (the GTFS guides especially), and since the docs site took over
the Pages slot they return 404. GitHub Pages cannot send a 301, so each old
path gets a small page with a canonical link and an immediate meta refresh to
the same path on gtfs.guru, which search engines treat as a permanent redirect.

Paths the docs build already produced are left alone: the docs site owns them.

Usage: pages_redirects.py <website_dir> <site_dir>
"""

from __future__ import annotations

import html
import sys
from pathlib import Path

TARGET = "https://gtfs.guru/"

STUB = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Moved to gtfs.guru</title>
<link rel="canonical" href="{url}">
<meta http-equiv="refresh" content="0; url={url}">
<meta name="viewport" content="width=device-width, initial-scale=1">
</head>
<body>
<p>This page moved to <a href="{url}">{url}</a>.</p>
</body>
</html>
"""


def moved_pages(website: Path) -> list[str]:
    """Site-relative paths of every HTML page website/ publishes."""
    pages = []
    for path in sorted(website.rglob("*.html")):
        relative = path.relative_to(website)
        if any(part.startswith(".") for part in relative.parts):
            continue
        if relative.parts[0] in {"pkg", "pkg-mt", "demo"}:
            continue
        pages.append(relative.as_posix())
    return pages


def target_url(relative: str) -> str:
    """gtfs.guru serves directory indexes at their trailing-slash URL."""
    if relative == "index.html":
        return TARGET
    if relative.endswith("/index.html"):
        return TARGET + relative[: -len("index.html")]
    return TARGET + relative


def write_stubs(website: Path, site: Path) -> list[str]:
    written = []
    for relative in moved_pages(website):
        destination = site / relative
        if destination.exists():
            continue
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(
            STUB.format(url=html.escape(target_url(relative), quote=True)),
            encoding="utf-8",
        )
        written.append(relative)
    return written


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    website, site = Path(argv[1]), Path(argv[2])
    if not (site / "index.html").exists():
        print(f"{site} has no index.html; build the docs first", file=sys.stderr)
        return 1
    written = write_stubs(website, site)
    print(f"wrote {len(written)} redirect stubs to {site}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
