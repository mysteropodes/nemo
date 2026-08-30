# Third-Party Notices

Audit performed 2026-08-17 (Cyril: "faire en sorte que les 2 versions n'ait pas de
problème de droits quand ça sortira en open source" — the browser preview and the
Tauri desktop build), Rust dependencies added 2026-08-30. This catalogs everything
bundled or vendored in this repository that isn't original code written for this
project, its license, and what (if anything) needs to happen for distribution.

**Nemo's own code is [GPL-3.0-or-later](LICENSE)**, and the repository is public
under that license. This document covers only what is *bundled inside* the app, so
the question it answers is whether each of those components may be redistributed
inside a GPL-3.0-or-later work. (An earlier revision of this file said no license
had been chosen yet — that was true when it was written and is not any more.)

## Clean — no action needed

| Component | Where | License | Notes |
|---|---|---|---|
| Paper.js | `src/paper-full.min.js` | MIT | Vendored minified build. The local copy carries no embedded license header (minification stripped it) — MIT requires the notice travel with copies of the software, so this repo's own README/NOTICES (this file) is what satisfies that now. |
| opentype.js | `src/js/opentype.min.js` | MIT | Vendored minified build. Unlike Paper.js, this one already carries its full MIT header inline (verified) — no action needed beyond keeping this table's reference current. |
| Roboto (Regular + Bold) | `src/fonts/Roboto-*.ttf` | Apache License 2.0 (Google) | Used for vector text rendering (`vector-text-bridge.js`). Apache 2.0 is permissive and font-distribution-friendly — no action needed. |
| Delaunator | `src/js/delaunator.vendor.js` | ISC (Mapbox) | v5.0.1, upstream UMD build, unmodified apart from a prepended license header. Delaunay triangulation for the image mesh (`image-mesh.js`). ISC is permissive and GPL-3.0-compatible. Upstream's own rollup build inlines **robust-predicates** 3.0.2 (Unlicense / public domain, Mourner) — that is the `epsilon`/`splitter`/`orient2d` code at the top of the bundle; public domain imposes no conditions, and it is called out here rather than left implicit because it isn't visible as a separate file. |

## Rust dependencies (2026-08-30)

`cargo license --avoid-dev-deps` run against both Rust crates. Full raw output is
committed beside this file so the claim can be checked without re-running anything:
[THIRD_PARTY_LICENSES_RUST_TAURI.txt](THIRD_PARTY_LICENSES_RUST_TAURI.txt) (511 crate
instances) and [THIRD_PARTY_LICENSES_RUST_WASM.txt](THIRD_PARTY_LICENSES_RUST_WASM.txt)
(159). A name can appear more than once at different pinned versions, hence
"instances" rather than "crates".

**Result: clean for GPL-3.0-or-later.** The two findings worth stating explicitly,
because they are the only ones that aren't plainly permissive:

| Finding | Crates | Why it's fine |
|---|---|---|
| `MPL-2.0` | cssparser, cssparser-macros, dtoa-short, option-ext, selectors | Weak copyleft, and explicitly GPL-compatible: MPL 2.0 §3.3 permits distributing the covered files as part of a Larger Work under a Secondary License, which it defines as the GPL family. File-level copyleft, so it does not reach Nemo's own sources. |
| `Apache-2.0 OR LGPL-2.1-or-later OR MIT` | r-efi (×2) | A triple license — take the MIT or Apache-2.0 arm and the LGPL question never arises. Even the LGPL arm is "or later", so it is compatible anyway. |

**No GPL-2.0-only anywhere**, which is the one incompatibility that would actually
bite: GPL-2.0-*only* cannot be combined with GPL-3.0 code, while GPL-2.0-*or-later*
can (you take the v3 option). Everything else resolves to MIT, Apache-2.0, BSD, ISC,
Zlib, Unlicense, CC0, 0BSD, BSL-1.0, Unicode-3.0 or CDLA-Permissive-2.0 — all
permissive and all redistributable inside a GPL-3.0-or-later work.

Both `Cargo.toml` files declare `license = "GPL-3.0-or-later"` so tooling reports
Nemo's own crates correctly instead of `N/A`.

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

**H.264/H.265 export restored via VideoToolbox (2026-08-18, same day).**
`--enable-videotoolbox` (an Apple system framework, not a Homebrew dep, not
GPL) exposes `h264_videotoolbox`/`hevc_videotoolbox`/`prores_videotoolbox` —
Apple's own OS-provided hardware encoders, called through their API rather
than bundled as a library. Apple already holds whatever patent license its
own encoder needs to ship in macOS; this app never redistributes an H.264/
H.265 implementation of its own, exactly the same principle already
applied to `exportVideoBrowser`'s browser-native MediaRecorder path. No
Via LA (AVC) or Access Advance (HEVC) registration needed for this path.
`exportMP4ToPath` (`src/js/export.js`) now uses `h264_videotoolbox` instead
of the removed `libx264` (quality via `-q:v`, VideoToolbox has no `-crf`
equivalent). `prores_videotoolbox` also exists as a cleaner alternative to
the still-present native `prores_ks` reimplementation, not yet wired in —
worth switching to if the ProRes question below ever gets revisited, since
it sidesteps that question entirely (Apple's own encoder, not a third-party
reimplementation of their format).

**Not addressed, still open:** ProRes export currently still runs through
ffmpeg's native `prores_ks` (not VideoToolbox) in one code path
(`src/js/export.js`, ProRes 4444/422 branch) — a clean-room reimplementation
of the now-SMPTE-standardized RDD-36 spec, not Apple's own code. No public
patent pool exists for ProRes (unlike AVC/HEVC), and the wider ecosystem
(DaVinci Resolve off-Mac, Adobe, ffmpeg everywhere) has shipped this for
years without a known enforcement precedent — lower-profile risk than the
H.264/H.265 issue this rebuild resolved, but still technically unresolved
if a fully buttoned-up release ever demands it. Switching that one call to
`prores_videotoolbox` would close even this residual question.

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
