# Nemo fixture corpus (R03)

Deterministic project documents and interaction scripts that the test harnesses
and the bench characterize the application against. Everything under
`projects/` and `interaction/`, and `manifest.json` itself, is **generated** by
`generate.cjs` from a seed; nothing is edited by hand.

```
node tests/fixtures/generate.cjs            # regenerate
node tests/fixtures/generate.cjs --check    # exit 1 when a committed file differs from the generator's output
```

`tests/nemo-fixtures.test.cjs` runs the check and holds the shipped evaluators
(`rawValueAtFrame`, `resolveSymbolFrameIdx`, lifted out of the browser modules by
`tests/bench/motion-eval.cjs`) to the recorded expectations wherever they can run
under Node. The rest of the expectations are inputs to the browser/desktop
harnesses (R12/R13/R21); the manifest names the consumer for each.

## Manifest entry

| Field | Meaning |
|---|---|
| `id`, `kind`, `file` | stable id; `project` (a Nemo document), `interaction` (a pointer script), or `generated` (built on demand, not committed) |
| `schemaVersion` | the document `version` the fixture is written in (13 = current); the legacy fixture has none |
| `areas` | the surface-inventory areas the fixture covers (`engineering/inventory/SURFACES.md` joins on these) |
| `features` | what the fixture exercises, free-form tags |
| `generation` | script, generator version and seed that reproduce the file byte for byte |
| `sha256`, `bytes` | identity of the committed file |
| `capabilities`, `backend` | what is needed to exercise it: `node` (pure data / extracted evaluators), `app-runtime` (needs the DOM, Paper.js or the expression sandbox), `engine-webgpu`, `desktop` |
| `tolerance` | numeric tolerances the consumer must use (values, geometry, pixels) |
| `expectations` | independent expectations, with a `derivation` note where the value is computed |
| `consumers` | who reads it |

## Independent expectations

An expectation is derived from the fixture's own definition, never from running
Nemo: rectangle bounds from the points that build them, eased values from the
default ease curve's waypoints ((0.25, 0.156), (0.5, 0.5), (0.75, 0.844)) on the
on-curve-waypoint model, component instance frames from the documented placement
rule (`elapsed = max(0, f - placedAt) * speed`; loop = `elapsed mod total`; once =
`min(total-1, elapsed)`; pingpong over a `(total-1)*2` cycle; single =
`symSingleFrame`), keyframe inheritance from the frame records. When the shipped
code disagrees with one of them, that is a finding to investigate, not a fixture
to regenerate.

## Format provenance

Field shapes follow the persisted document as the application writes and reads it:
top-level and layer fields from `exportJSON` (`src/js/timeline.js`), stroke dicts
from `serP`/`desP` and raster dicts from `images.js` (`src/js/app.js`,
`src/js/images.js`), frame records `{strokes, isKeyframe, isInterpolated}`,
Motion tracks `{keys:[{frame, v, hOut, hIn, hold?, curvePoints?}]}` and
`motionStatic` (`src/js/motion.js`), `expressions[prop] = {code, enabled, lastError}`,
`symbols[id] = {name, totalFrames, fps, layers}` with instance fields
`symbolId/symPlacedAt/symPlayMode/symSpeed/symSingleFrame`, track mattes
`matteMode/matteSourceLayerUid`, and the image-mesh store entry
`{outline, cols, rows, verts, tris, offsets}` in normalized raster space
(`src/js/image-mesh.js`). Embedded PNGs are built in `lib.cjs` with stored deflate
blocks so their bytes do not depend on the zlib version.

## Scale documents

`scale-500`, `scale-2000`, `scale-4000` are generated on demand by
`lib.cjs scaleDocument(n, seed)` for the bench (`tests/bench/run.cjs`); the
manifest records their hash and size so a run can prove it measured the same
document, without committing megabytes of JSON.
