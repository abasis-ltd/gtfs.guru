#!/usr/bin/env python3
"""Serve website/ locally with the headers production sends.

`python3 -m http.server` omits COOP/COEP, so `crossOriginIsolated` is false and
the local site silently never exercises the multithreaded worker tier that real
visitors get. Bugs that only live in that tier stay invisible until deploy.

    python3 scripts/serve_website.py [port]
"""

from __future__ import annotations

import functools
import http.server
import pathlib
import socketserver
import sys


class IsolatedHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self) -> None:
        # Required for SharedArrayBuffer, and so for wasm threads. Mirrors
        # website/nginx.conf and website/_headers.
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        # Local iteration should never be served a cached worker or wasm blob.
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, fmt: str, *args) -> None:
        if not self.path.startswith("/notices/"):
            super().log_message(fmt, *args)


def main() -> int:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8901
    root = pathlib.Path(__file__).resolve().parent.parent / "website"
    socketserver.TCPServer.allow_reuse_address = True
    handler = functools.partial(IsolatedHandler, directory=str(root))
    with socketserver.TCPServer(("127.0.0.1", port), handler) as httpd:
        print(f"Serving {root} at http://localhost:{port} (cross-origin isolated)", flush=True)
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
