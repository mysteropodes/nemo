# Nemo fixture corpus (R03)

Twelve deterministic Nemo documents, each with **independent expectations**, that the
test harnesses and the bench characterize the application against. Everything under
`tests/fixtures/<id>/` and `manifest.json` itself is **generated** by `generate.cjs` from
`lib/corpus.cjs`; nothing is edited by hand, and every committed file is pinned by
SHA-256 in the manifest.

```
npm run fixtures                             # regenerate (node tests/fixtures/generate.cjs)
node tests/fixtures/generate.cjs --check     # exit 1 when a committed file differs from the generator's output
node --test tests/fixtures/fixtures.test.cjs # verify (also reached by npm test through tests/nemo-fixtures.test.cjs)
```

## Generator runtime

Generator version 1 pins **zlib builds**, because `lib/png.cjs` uses the host
`deflateSync` with level 9 and filter-0 scanlines. Node's version alone does not
identify the compressor: a distributor can link a system zlib. Use the official
Node **20.19.4** distribution (`process.versions.zlib` =
`1.3.0.1-motley-82a5fec`) as the reference generation runtime. The following
available builds also reproduced all 41 committed files byte-for-byte:

| Node version(s) | `process.versions.zlib` |
|---|---|
| 20.19.0, 20.19.4, 22.13.0, 22.13.1 | `1.3.0.1-motley-82a5fec` |
| 22.5.0 | `1.3.0.1-motley-209717d` |
| 22.12.0 | `1.3.0.1-motley-71660e1` |
| 22.17.1 | `1.3.0.1-motley-780819f` |
| 24.18.0 | `1.3.1-e00f703` |
| 24.20.0 | `1.3.2.1-motley-42c2f19` |

`render()`, generation (including `--out`), and `--check` reject unverified zlib
builds before generating or writing anything. The CLI exits **2** with the actual
Node/zlib identity and a compatible runtime; stale artifacts still exit **1** on
a supported runtime. Fixture verification and consumers can read the committed
assets on other runtimes: the generation pin does not disable their hash, pixel,
structure, or production-behavior checks. Before admitting a new zlib build, use
an isolated candidate to extend the list and verify **every byte** with `--check`;
do not regenerate the corpus to make a runtime qualify.

The original mismatch was reproduced with Homebrew Node 25.9.0 and 24.19.0,
both reporting zlib `1.2.12`: all 12 export PNGs differed, and consequently their
manifest hashes differed. All PNG chunk CRCs were valid; IHDR/IEND, dimensions
(320 × 240), inflated filter-0 scanlines, and every decoded RGBA pixel matched.
Only the IDAT compression bytes and corresponding CRCs differed. The three
smaller media/mesh PNGs and every other generated file matched. For example,
frame 1 was 743 bytes in both builds but had SHA-256 `7f0a02dc964504fdbb202cb9bb710e51c24b5b268cfdc15ad29770172f0b10fd`
in the committed/bundled-zlib output versus `abff1715a2fc3f4ca0814d1798578d902c25371169e99f1a5abbcde09a25b1bb`
with zlib 1.2.12. This is compression drift, not a reason to replace reference
pixels or weaken exact asset/manifest comparisons.

## Layout

| Path | Content |
|---|---|
| `<id>/project.json` | the document, in the shape `exportJSON` writes (format version 13; the migration fixture is version 12) |
| `<id>/expected.json` | `{schema, fixture, reference, checks}`: the independent expectations, one check per line, each with `kind`, `verify` and the values to compare |
| `<id>/assets/…`, `<id>/gesture.json` | PNG assets (embedded image bytes, an export frame sequence) and the recorded pointer script |
| `manifest.json` | one entry per fixture (below), the bench `workloads` (generated on demand, pinned by hash, never committed) and the `checkKinds` glossary |
| `lib/corpus.cjs` | one builder per fixture, plus `benchDocument()` for the workloads |
| `lib/reference.cjs` | the independent expectations: plain restatements of documented rules; imports no application code |
| `lib/sandbox.cjs` | loads **production** modules into a Node `vm` sandbox: `motion.js` whole, the pure document helpers lifted from `app.js`, `image-mesh.js` with the vendored Delaunator |
| `lib/png.cjs`, `lib/rng.cjs` | dependency-free PNG codec for the assets; mulberry32 + `seedFrom` |

## Manifest entry

`id`, `title`, `coverage` (the #899 topics it covers), `areas` (surface-inventory areas —
`engineering/inventory/SURFACES.md` joins on these, and the manifest is an input of the
inventory), `backend` (`document` / `engine` / `sidecar` / `browser`), `requiredCapabilities`,
`tolerance`, `seed`, `generation` (generator, version, builder, format version),
`invariants`, `verification` (`node`: what runs here; `gate`: which harness runs the
rest), `files`, `sha256` per committed file, `checks` counts and a `document` summary.

## Verification levels

| `verify` | Meaning |
|---|---|
| `node` | checked now by `fixtures.test.cjs`, against the shipped code loaded through `lib/sandbox.cjs` (never a copy of it) |
| `node-when-available` | the export encode / probe / decode checks: run when a runnable ffmpeg sidecar is present (`src-tauri/binaries/ffmpeg-<host triple>`, or `NEMO_FFMPEG` for an explicit binary), skipped with the reason otherwise; the binary identity (origin, SHA-256, version) is printed with the result |
| `gate` | needs the WebGPU engine, the browser harness or the full `importJSON` contract (R12 / R13 / R21); only the shape is validated here and the check names its gate |

## Coverage

| Fixture | Covers | Backend | Checks | Inventory areas |
|---|---|---|---|---|
| `static-props` | static | document | 39 now, 0 when a sidecar runs, 0 gated | animation-rigs-expressions, drawing-selection |
| `keyed-props` | keyed | document | 67 now, 0 when a sidecar runs, 0 gated | animation-rigs-expressions, timeline-layers-frames |
| `expression-props` | expression | document | 55 now, 0 when a sidecar runs, 0 gated | animation-rigs-expressions |
| `held-frames` | held-frames | document | 103 now, 0 when a sidecar runs, 0 gated | timeline-layers-frames, animation-rigs-expressions |
| `components` | components | document | 168 now, 0 when a sidecar runs, 0 gated | timeline-layers-frames, storyboard |
| `masks-alpha` | masks-alpha | engine | 5 now, 0 when a sidecar runs, 6 gated | effects-masks, drawing-selection |
| `text` | text | document | 11 now, 0 when a sidecar runs, 1 gated | drawing-selection, animation-rigs-expressions |
| `mesh` | mesh, media | engine | 3 now, 0 when a sidecar runs, 3 gated | media-import-export, effects-masks |
| `media` | media | engine | 6 now, 0 when a sidecar runs, 3 gated | media-import-export, project-lifecycle-integrations |
| `migration` | migration | document | 4 now, 0 when a sidecar runs, 5 gated | project-lifecycle-integrations, animation-rigs-expressions |
| `interaction` | interaction | browser | 1 now, 0 when a sidecar runs, 2 gated | drawing-selection |
| `export` | export | sidecar | 1 now, 5 when a sidecar runs, 0 gated | media-import-export |

## Independent expectations

An expectation is derived from the fixture's own definition and a documented rule,
never from running Nemo: eased values from the default curve's on-curve waypoints
((0.25, 0.156), (0.5, 0.5), (0.75, 0.844)), a two-point curve as the identity ease,
spatial handles as a cubic bezier, hold keys pinning their value, component frames
from the documented placement rule (`elapsed = max(0, f - placedAt) * speed`; loop,
once, pingpong, single), keyframe holds from the frame records, colours from the
hex8 rule and source-over compositing, mesh vertices from the normalised rect. When
the shipped code disagrees with one of them, that is a finding to investigate, not a
fixture to regenerate — and the reverse holds: when a derivation was wrong it is
corrected with its reason recorded (the ProRes 4444 probe first expected the encoder's
10-bit input format; ffprobe reports the decoder's native 12-bit 4444 format, and the
invariants are the 4444 profile and the alpha plane).

## Workload documents (bench, not committed)

Generated by `lib/corpus.cjs benchDocument(params)` for `tests/bench/run.cjs`; the
manifest pins their hash so two receipts are comparable only when they measured
byte-identical input.

| Workload | Parameters | Size | SHA-256 |
|---|---|---|---|
| `bench-vectors-40x24` | 40 layers × 24 frames, keys every 6, 50 strokes per key | 2000 items per keyframe, 8.3 MB | `13807dc8a6c5` |
| `bench-vectors-8x24` | 8 layers × 24 frames, keys every 6, 25 strokes per key | 200 items per keyframe, 0.8 MB | `e7bb306ce728` |
| `bench-images-20x24` | 20 layers × 24 frames, keys every 6, 5 strokes per key, 20 rasters | 100 items per keyframe, 0.6 MB | `a942d357c5da` |

## Format provenance

Field shapes follow the persisted document as the application writes and reads it:
top-level and layer fields from `exportJSON` (`src/js/timeline.js`), stroke dicts from
`serP`/`desP` and raster dicts from `images.js` (`src/js/app.js`, `src/js/images.js`),
frame records `{strokes, isKeyframe, isInterpolated}`, Motion tracks
`{keys:[{frame, v, hOut, hIn, hold?, curvePoints?}]}` and `motionStatic`
(`src/js/motion.js`), `expressions[prop] = {code, enabled}` with `exprControls`,
`symbols[id] = {name, totalFrames, fps, layers}` with instance fields
`symbolId/symPlacedAt/symPlayMode/symSpeed/symSingleFrame` and per-keyframe
`componentFrame`/`blankOverride`, track mattes `matteMode/matteSourceLayerUid`,
in-layer masks `isMask/maskMode`, text roots and glyphs (`isTextRoot`, `vectorChar`,
`charIndex`, `textAnimators`), `mediaLibrary` entries, the image-mesh store entry
`{outline, cols, rows, verts, tris, offsets}` in normalised raster space
(`src/js/image-mesh.js`), and the exact ffmpeg argument lists of `src/js/export.js`.
