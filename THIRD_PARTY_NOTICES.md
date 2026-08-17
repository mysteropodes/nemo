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

## Needs a decision before open-sourcing — ffmpeg (Tauri build only)

**This is the one real issue.** `src-tauri/binaries/ffmpeg-aarch64-apple-darwin` is
a prebuilt ffmpeg 8.1 binary, sidecar-invoked as a subprocess for every video/GIF/
TIFF/ProRes export (`src-tauri/src/lib.rs`'s `run_ffmpeg` command, called from
`src/js/export.js`). Its own `-version` output:

```
--enable-gpl --enable-libx264 --enable-libx265 --enable-libvvenc --enable-libvpx
--enable-libwebp --enable-libass --enable-libfreetype ... --enable-libsvtav1 ...
```

Two **separate** legal questions, both real:

1. **GPL (copyright).** `--enable-gpl` plus `libx264`/`libx265` makes this a GPL
   binary. Per CLAUDE.md §7's existing analysis, invoking it as a subprocess
   (pipe stdout, never linked into the Rust binary — confirmed via `otool -L`,
   zero Homebrew/dylib dependencies) is the standard "mere aggregation" pattern
   the FSF's own GPL FAQ treats as NOT requiring the rest of the app to be GPL.
   That reasoning holds for a **closed-source commercial** build. It does NOT
   remove the GPL's own conditions on the ffmpeg binary itself: distributing it
   at all requires (a) including the GPL license text, and (b) making the exact
   corresponding source available (or a written offer to provide it) for
   *that specific build* — this repo currently does neither. Trivial to fix
   once the following decision is made.
2. **Software patents (H.264/H.265), separate from copyright.** `libx264`/
   `libx265` implement patent-encumbered codecs. Distributing a binary that
   encodes H.264/H.265 for public use can trigger royalty obligations under the
   MPEG-LA/Access Advance patent pools, independent of the GPL question above —
   browsers and OSes typically have their own paid patent licenses covering
   *their* encoders (which is exactly why `exportVideoBrowser` in `export.js`,
   the new browser-native MediaRecorder path, has ZERO exposure here: it asks
   the user's own browser to encode, using a license the browser vendor already
   holds — Nemo never bundles or ships an H.264 encoder itself in that path).

**Recommended fix (requires a real ffmpeg rebuild — not something achievable in
this coding session, no internet-fetchable prebuilt satisfies this cleanly):**
compile a custom ffmpeg with `--disable-gpl` and without `libx264`/`libx265`,
keeping only royalty-free / non-restrictive encoders — `libvpx` (VP8/VP9),
`libaom`/`libsvtav1` (AV1), `libopus`, `libtheora`/`libvorbis`. This changes the
MP4 export's default codec (H.264 won't be available from that binary anymore)
— WebM (VP9) becomes the desktop app's own default too, which also makes the
Tauri and browser export paths converge on the same codec family instead of
diverging (MP4/H.264 via ffmpeg vs. WebM/VP9 via MediaRecorder). ProRes
(`prores_ks`) is a separate concern not addressed here — Apple's own patent/
licensing position on ProRes is murkier and worth a dedicated look if that
export format matters for the open-source release.

**Until that rebuild happens:** do not publish the repository with this
binary included, or at minimum ship the GPL license text + a source-code offer
alongside it (`ffmpeg-aarch64-apple-darwin`'s LICENSE.txt equivalent) and be
aware the patent question isn't resolved by that alone.

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
