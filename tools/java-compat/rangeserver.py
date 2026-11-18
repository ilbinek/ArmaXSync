#!/usr/bin/env python3
"""Minimal static HTTP server with single-range support (test harness only).

Mirrors what the legacy client needs: clamped ranges (end may exceed EOF),
206 + exact Content-Length, '//' collapsing, percent-decoding.
Usage: rangeserver.py ROOT PORT
"""
import os
import re
import sys
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = os.path.abspath(sys.argv[1])
PORT = int(sys.argv[2])
LOG = os.environ.get("RANGESERVER_LOG")


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        if LOG:
            with open(LOG, "a", encoding="utf-8") as f:
                f.write("%s %s\n" % (self.command, fmt % args))

    def resolve(self):
        # The legacy client sends raw UTF-8 bytes in the request line (only
        # spaces etc. are percent-encoded); http.server decoded them as latin-1.
        raw = self.path.split("?", 1)[0].encode("latin-1", "replace")
        path = urllib.parse.unquote_to_bytes(raw).decode("utf-8", "replace")
        parts = [p for p in path.split("/") if p not in ("", ".")]
        if ".." in parts:
            return None
        return os.path.join(ROOT, *parts)

    def do_HEAD(self):
        self.serve(head=True)

    def do_GET(self):
        self.serve(head=False)

    def serve(self, head):
        p = self.resolve()
        if p is None or not os.path.isfile(p):
            self.send_response(404)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        size = os.path.getsize(p)
        start, end, status = 0, size - 1, 200
        rng = self.headers.get("Range")
        if rng:
            m = re.match(r"bytes=(\d*)-(\d*)$", rng.strip())
            if m and (m.group(1) or m.group(2)):
                if m.group(1):
                    start = int(m.group(1))
                    end = min(int(m.group(2)), size - 1) if m.group(2) else size - 1
                else:
                    start = max(0, size - int(m.group(2)))
                if start >= size:
                    self.send_response(416)
                    self.send_header("Content-Range", "bytes */%d" % size)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                status = 206
        length = max(0, end - start + 1)
        self.send_response(status)
        self.send_header("Accept-Ranges", "bytes")
        self.send_header("Content-Length", str(length))
        if status == 206:
            self.send_header("Content-Range", "bytes %d-%d/%d" % (start, end, size))
        self.end_headers()
        if head:
            return
        with open(p, "rb") as f:
            f.seek(start)
            left = length
            while left > 0:
                chunk = f.read(min(1 << 20, left))
                if not chunk:
                    break
                self.wfile.write(chunk)
                left -= len(chunk)


ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
