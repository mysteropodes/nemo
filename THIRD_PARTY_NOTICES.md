# Third-Party Notices

Audit performed 2026-08-17 (Cyril: "faire en sorte que les 2 versions n'ait pas de
problème de droits quand ça sortira en open source" — the browser preview and the
Tauri desktop build). This catalogs everything bundled or vendored in this
repository that isn't original code written for this project, its license, and
what (if anything) needs to happen before public/open-source distribution.

No LICENSE file exists for Nemo's own code yet — that's a choice only Cyril can
make (MIT/Apache/GPL/source-available/etc. all have different implications for a
commercial product going open source), so this document doesn't presume one. It
only covers what's *bundled inside* the app.

## Clean — no action needed

| Component | Where | License | Notes |
|---|---|---|---|
| Paper.js | `src/paper-full.min.js` | MIT | Vendored minified build. The local copy carries no embedded license header (minification stripped it) — MIT requires the notice travel with copies of the software, so this repo's own README/NOTICES (this file) is what satisfies that now. |
| opentype.js | `src/js/opentype.min.js` | MIT | Vendored minified build. Unlike Paper.js, this one already carries its full MIT header inline (verified) — no action needed beyond keeping this table's reference current. |
| Roboto (Regular + Bold) | `src/fonts/Roboto-*.ttf` | Apache License 2.0 (Google) | Used for vector text rendering (`vector-text-bridge.js`). Apache 2.0 is permissive and font-distribution-friendly — no action needed. |

## Removed during this audit

| Component | Where | Why removed |
|---|---|---|
| `icons-subset.ttf` | `src/fonts/icons-subset.ttf` | 30KB font file, **zero references** anywhere in the codebase (JS, CSS, or Tauri bundle config) — confirmed via full-repo grep. Undocumented origin (no comment, no license, unclear if it was ever actually wired up or is leftover from an earlier icon-font approach before the app switched to inline SVG icons — see `ICO_3D`'s own comment in `timeline.js` referencing "this project's embedded font is a subset..." with no such reference actually existing). An unused binary asset of unknown provenance is pure downside before going open source: removed rather than left in with a license nobody can currently verify. |

## Resolved (2026-08-18) — ffmpeg (Tauri build only)

`src-tauri/binaries/ffmpeg-aarch64-apple-darwin` was a prebuilt ffmpeg 8.1
binary built with `--enable-gpl --enable-libx264 --enable-libx265
--enable-libvvenc --enable-libkvazaar --enable-libvidstab` — GPL (copyright)
*and* H.264/H.265/H.266 patent exposure (two separate legal questions, see
git history of this file for the original analysis of both).

**Rebuilt from FFmpeg n8.1 source** (`scripts/rebuild-ffmpeg-lgpl.sh`,
reproducible) without `--enable-gpl` and without any of the five
patent/GPL-encumbered components above. `ffmpeg -version`'s own configure
line now shows **no** `--enable-gpl`, and ffmpeg's build system itself
reports **License: LGPL version 2.1 or later** for this exact flag
combination — confirmed directly against this build's `configure` output,
not inferred. Kept (all permissive or royalty-free): `libvmaf` (BSD-2),
`libopenjpeg` (BSD-2), `libopus` (BSD, royalty-free), `libmp3lame` (LGPL —
fine, MP3's own patents expired 2017 and LGPL only obligates the library
itself, not what invokes it, unlike GPL), `libvpx` (BSD, VP8/VP9,
royalty-free), `libwebp` (BSD), `libass` (ISC), `libfreetype` (FTL/BSD-style),
`fontconfig` (MIT-style), `libtheora`/`libvorbis` (BSD, patent pool
disbanded), `libsnappy` (BSD), `libaom` (BSD-2 + AOM patent grant, AV1,
royalty-free), `libzimg` (BSD-2), `libsvtav1` (BSD-3 + patent grant, AV1),
`libharfbuzz` (MIT). MP4/H.264 export is gone from this binary — WebM (VP9)
is the new desktop default, converging with the browser build's own
MediaRecorder/WebM path (`exportVideoBrowser` below) onto the same codec
family instead of diverging. AV1 (via libaom/libsvtav1) is available as a
higher-efficiency alternative in both.

Result: **now linked dynamically against Homebrew dylibs** (the old build
was fully static — trading that for the license fix). `scripts/
bundle-ffmpeg-dylibs.py` (previously a no-op once the old binary went
static, see CLAUDE.md §7) makes a packaged `.app` self-contained again by
bundling those dylibs into `Contents/Frameworks/` at build time — this
means the **build machine** needs the Homebrew formulas listed in
`rebuild-ffmpeg-lgpl.sh`'s header comment installed; the *distributed* app
does not need Homebrew.

**Not addressed here, unchanged from before:** ProRes (`prores_ks`) is
ffmpeg's own native encoder (not from an external GPL/patent-flagged lib),
kept as-is — Apple's licensing/patent position on ProRes is murkier and
still worth a dedicated look if that export format matters for a fully
buttoned-up public release, but it was never part of the GPL/x264/x265
issue this rebuild resolves.

## Browser export path — no bundled-codec exposure (2026-08-17, new)

`exportVideoBrowser`/`exportGifBrowser` (`src/js/export.js`) ship ZERO
third-party or patent-encumbered code:
- MP4/WebM: `MediaRecorder` + `canvas.captureStream()` — encoding happens
  inside the user's own browser, using whatever codec license the browser
  vendor already holds. Nemo's code never touches compressed video bytes.
- GIF: hand-written GIF89a encoder (median-cut quantization + LZW), original
  code written for this project from the public GIF89a specification, not
  derived from or copied out of any existing encoder.

This means the **browser build has no equivalent of the ffmpeg problem above**
— it was already open-source-safe before this audit, and stays that way.
