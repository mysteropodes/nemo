#!/usr/bin/env python3
# Dev-only static file server (drop-in replacement for `python3 -m
# http.server`, same CLI usage: `python3 dev_server.py <port> -d src`) that
# ALSO handles a few /__feedback/* endpoints so feedback-bridge.js's browser
# fallback can write/read real files on disk instead of localStorage — see
# CLAUDE.md §6. Point of this file: browser localStorage isn't reachable by
# Claude between conversation turns (it only exists inside a live browser
# tab), but a real JSON file on disk is — this makes the debug-feedback
# system actually usable during ordinary localhost dev-preview testing,
# not just in the built Tauri app (which already writes real files via its
# own fs plugin, untouched by this script).
#
# Entries land in <repo>/.feedback/<projectKey>/<id>.json — sibling to (NOT
# inside) src/, so they're never served as static files and never bundled
# into the shipped app by accident.
import http.server
import json
import os
import re
import sys
from pathlib import Path

# Tests may point the browser-preview bridge at the same app-data feedback
# folder used by Tauri. The default remains the repo-local, unbundled
# `.feedback` directory so ordinary development never touches app data.
FEEDBACK_ROOT = Path(
    os.environ.get(
        "STROKEMOTION_FEEDBACK_ROOT",
        str(Path(__file__).resolve().parent.parent / ".feedback"),
    )
).expanduser()


def safe_segment(s):
    s = (s or "unknown").strip()
    s = re.sub(r"[^a-zA-Z0-9_\-]", "_", s)
    return s[:80] or "unknown"


class Handler(http.server.SimpleHTTPRequestHandler):
    # SimpleHTTPRequestHandler sends no Cache-Control at all, so the OS
    # webview (WKWebView in the Tauri app, but also plain Chrome) applies
    # its own heuristic freshness and can keep serving an old JS/CSS file
    # after an edit even through an explicit user-triggered reload (only
    # reloadFromOrigin()/hard-reload bypasses that, and Tauri's native
    # right-click "Reload" does a plain reload()) — see CLAUDE.md's
    # documented stale-cache gotcha, this is the same root cause hitting a
    # different reload path. Every response gets no-store so every dev
    # reload always reflects the file on disk.
    def end_headers(self):
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate")
        http.server.SimpleHTTPRequestHandler.end_headers(self)

    def send_head(self):
        # SimpleHTTPRequestHandler still answers 304 to an old
        # If-Modified-Since request even when the response says no-store.
        # During rapid shader/UI iterations the filesystem timestamp can
        # share the same one-second value, making WebKit reuse stale bytes.
        # A dev server should always return the source currently on disk.
        if "If-Modified-Since" in self.headers:
            del self.headers["If-Modified-Since"]
        return super().send_head()

    def _json(self, status, payload):
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _project_dir(self, project_key):
        d = FEEDBACK_ROOT / safe_segment(project_key)
        d.mkdir(parents=True, exist_ok=True)
        return d

    def do_GET(self):
        if self.path.startswith("/__feedback/ping"):
            self._json(200, {"ok": True})
            return
        if self.path.startswith("/__feedback/list"):
            query = self.path.split("?", 1)[1] if "?" in self.path else ""
            params = dict(p.split("=", 1) for p in query.split("&") if "=" in p)
            from urllib.parse import unquote
            project_key = unquote(params.get("projectKey", "untitled-autosave"))
            d = self._project_dir(project_key)
            entries = []
            for f in sorted(d.glob("*.json")):
                try:
                    entries.append(json.loads(f.read_text(encoding="utf-8")))
                except Exception:
                    pass
            entries.sort(key=lambda e: e.get("createdAt", 0), reverse=True)
            self._json(200, entries)
            return
        super().do_GET()

    def do_POST(self):
        if self.path == "/__feedback/save":
            length = int(self.headers.get("Content-Length", 0))
            try:
                data = json.loads(self.rfile.read(length).decode("utf-8"))
                entry_id = safe_segment(data.get("id"))
                d = self._project_dir(data.get("projectKey"))
                (d / (entry_id + ".json")).write_text(
                    json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8"
                )
                self._json(200, {"ok": True})
            except Exception as e:
                self._json(500, {"ok": False, "error": str(e)})
            return
        if self.path == "/__feedback/delete":
            length = int(self.headers.get("Content-Length", 0))
            try:
                data = json.loads(self.rfile.read(length).decode("utf-8"))
                entry_id = safe_segment(data.get("id"))
                d = self._project_dir(data.get("projectKey"))
                f = d / (entry_id + ".json")
                if f.exists():
                    f.unlink()
                self._json(200, {"ok": True})
            except Exception as e:
                self._json(500, {"ok": False, "error": str(e)})
            return
        self._json(404, {"ok": False, "error": "unknown endpoint"})

    # Quiets the default per-request stderr logging for the noisy /__feedback
    # polling calls the settings panel makes; static-file requests still log
    # normally via the parent implementation.
    def log_message(self, fmt, *args):
        if "/__feedback/" in (self.path or ""):
            return
        super().log_message(fmt, *args)


def main():
    port = 8000
    directory = "."
    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == "-d" and i + 1 < len(args):
            directory = args[i + 1]
            i += 2
        else:
            try:
                port = int(args[i])
            except ValueError:
                pass
            i += 1

    handler = lambda *a, **kw: Handler(*a, directory=directory, **kw)
    with http.server.ThreadingHTTPServer(("", port), handler) as httpd:
        print(f"Serving {directory} on port {port} (feedback endpoints under /__feedback/*)")
        httpd.serve_forever()


if __name__ == "__main__":
    main()
