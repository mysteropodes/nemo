#!/usr/bin/env python3
"""Regenerate THIRD_PARTY_NOTICES.md from the current dependency trees.

Aggregates:
  - npm deps (via `npx license-checker --json`, project root)
  - Rust crates for src-tauri AND geometry-wasm (via `cargo license --json`,
    one crate tree each — they're independent Cargo projects, not a
    workspace)
  - Vendored, non-package-manager-tracked files (opentype.min.js,
    mp4box.all.min.js) and the bundled ffmpeg binary — these can't be seen
    by either scanner since they're plain files/external binaries, not
    npm/cargo dependencies, so they're hardcoded below. If you vendor
    another third-party file this way, add it to VENDORED_FILES.

Run after adding/upgrading any dependency, and before any build meant for
distribution — a new transitive dependency can introduce a new license
family (this project has already hit MPL-2.0 via a few well-known
transitive crates; the next one might need actual review, not just a
mention here). Requires `cargo install cargo-license` once (see below).

Usage:
    python3 scripts/generate-third-party-notices.py
"""
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT_PATH = ROOT / "THIRD_PARTY_NOTICES.md"

# Files vendored directly (not through npm/cargo) — can't be auto-discovered,
# so tracked here by hand. Keep in sync with the attribution header actually
# present at the top of each file.
VENDORED_FILES = [
    {
        "name": "opentype.js",
        "path": "src/js/opentype.min.js",
        "license": "MIT",
        "copyright": "Copyright (c) 2020 Frederik De Bleser",
        "source": "https://github.com/opentypejs/opentype.js",
        "note": "Minified, unmodified. Font loading/glyph metrics (vector-text-bridge.js).",
    },
    {
        "name": "MP4Box.js",
        "path": "src/js/mp4box.all.min.js",
        "license": "BSD-3-Clause",
        "copyright": "Copyright (c) 2012. Telecom ParisTech/TSI/MM/GPAC Cyril Concolato",
        "source": "https://github.com/gpac/mp4box.js",
        "note": "v0.5.2, minified, unmodified. MP4/MOV demuxing (native-video-bridge.js).",
    },
]

# Bundled external binary — not a library dependency, but its license terms
# apply to distribution the same way. See CLAUDE.md §7 for the full
# reasoning already worked out with the project owner.
BUNDLED_BINARIES = [
    {
        "name": "ffmpeg",
        "license": "GPL (built with --enable-gpl --enable-libx264 --enable-libx265)",
        "note": (
            "Piped as an external subprocess (src-tauri/binaries/, invoked via "
            "std::process::Command) — never linked into the Rust binary. This is "
            "\"simple aggregation\", not linking, which is the standard, safer "
            "pattern for shipping ffmpeg with a commercial app. It does NOT make "
            "the GPL dependency disappear, though: before selling this app, this "
            "binary should be replaced with a custom LGPL-only, decode-only ffmpeg "
            "build to be fully clean of GPL obligations."
        ),
    },
]


def run_json(cmd, cwd=None):
    result = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=180)
    if result.returncode != 0:
        print(f"WARNING: {' '.join(cmd)} exited {result.returncode}: {result.stderr[:400]}", file=sys.stderr)
    # license-checker/cargo-license both print pure JSON to stdout even on
    # partial failure (unknown-license warnings go to stderr) — parse
    # optimistically, only raise if stdout truly isn't JSON.
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        print(f"ERROR: could not parse JSON from {' '.join(cmd)}", file=sys.stderr)
        raise


def get_npm_licenses():
    data = run_json(["npx", "--yes", "license-checker", "--json"], cwd=ROOT)
    # dict keyed "name@version" -> {licenses, repository, ...}
    entries = []
    for key, info in data.items():
        if key.startswith("nemo@"):
            continue  # this project's own package.json entry, not a dependency
        entries.append({
            "name": key,
            "license": info.get("licenses", "UNKNOWN"),
            "repository": info.get("repository", ""),
        })
    return sorted(entries, key=lambda e: e["name"].lower())


def get_cargo_licenses(subdir):
    data = run_json(["cargo", "license", "--json"], cwd=ROOT / subdir)
    entries = []
    for pkg in data:
        if pkg["name"] in ("nemo_lib", "geometry-wasm"):
            continue  # this project's own crate, not a dependency
        entries.append({
            "name": f"{pkg['name']}@{pkg['version']}",
            "license": pkg.get("license") or "UNKNOWN",
            "repository": pkg.get("repository", ""),
        })
    return sorted(entries, key=lambda e: e["name"].lower())


def group_by_license(entries):
    grouped = defaultdict(list)
    for e in entries:
        grouped[e["license"]].append(e)
    return dict(sorted(grouped.items(), key=lambda kv: kv[0].lower()))


def render_group(grouped, heading):
    lines = [f"### {heading}\n"]
    for license_name, entries in grouped.items():
        lines.append(f"<details><summary><strong>{license_name}</strong> ({len(entries)})</summary>\n")
        for e in entries:
            repo = f" — {e['repository']}" if e.get("repository") else ""
            lines.append(f"- `{e['name']}`{repo}")
        lines.append("\n</details>\n")
    return "\n".join(lines)


def main():
    print("Scanning npm dependencies...")
    npm_entries = get_npm_licenses()
    print(f"  {len(npm_entries)} npm package(s)")

    print("Scanning src-tauri Rust crates...")
    tauri_entries = get_cargo_licenses("src-tauri")
    print(f"  {len(tauri_entries)} crate(s)")

    print("Scanning geometry-wasm Rust crates...")
    wasm_entries = get_cargo_licenses("geometry-wasm")
    print(f"  {len(wasm_entries)} crate(s)")

    from datetime import datetime, timezone
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    parts = [
        "# Third-Party Notices\n",
        (
            f"Auto-generated on {generated_at} by `scripts/generate-third-party-notices.py` "
            f"— do not edit by hand, re-run the script instead (requires "
            f"`cargo install cargo-license` once; `npx license-checker` needs no separate install).\n"
        ),
        (
            "This file lists every third-party dependency this project pulls in "
            "(npm packages, Rust crates for both `src-tauri` and `geometry-wasm`), "
            "plus files vendored directly (not through a package manager) and "
            "external binaries bundled at build time. Re-run this script whenever "
            "a dependency is added, removed, or upgraded, and before any build "
            "meant for distribution — a newly-added dependency can bring in a "
            "license family not yet reviewed here.\n"
        ),
        "## Vendored files (not tracked by npm/cargo)\n",
    ]
    for f in VENDORED_FILES:
        parts.append(
            f"- **{f['name']}** (`{f['path']}`) — **{f['license']}**\n"
            f"  {f['copyright']} — {f['source']}\n"
            f"  {f['note']}\n"
        )

    parts.append("## Bundled external binaries\n")
    for b in BUNDLED_BINARIES:
        parts.append(f"- **{b['name']}** — **{b['license']}**\n  {b['note']}\n")

    parts.append(f"## npm dependencies ({len(npm_entries)})\n")
    parts.append(render_group(group_by_license(npm_entries), "By license"))

    parts.append(f"\n## Rust crates — src-tauri ({len(tauri_entries)})\n")
    parts.append(render_group(group_by_license(tauri_entries), "By license"))

    parts.append(f"\n## Rust crates — geometry-wasm ({len(wasm_entries)})\n")
    parts.append(render_group(group_by_license(wasm_entries), "By license"))

    OUT_PATH.write_text("\n".join(parts) + "\n", encoding="utf-8")
    print(f"Wrote {OUT_PATH}")


if __name__ == "__main__":
    main()
