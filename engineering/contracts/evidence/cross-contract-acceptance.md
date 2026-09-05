# R09 cross-contract acceptance evidence

Status: design evidence; no foundational contract is adopted by this document.

## Evidence boundary

This matrix is pinned to fresh `origin/main`
[`1d652dd5de550bf6f5863faf5ed89c2481a36412`](https://github.com/mysteropodes/nemo/commit/1d652dd5de550bf6f5863faf5ed89c2481a36412).
It maps the current application consumers and test jobs to the second acceptance
line of [R09 #905](https://github.com/mysteropodes/nemo/issues/905). R09 remains
open, and production adoption remains gated by R03 and R05.

The deterministic corpus discussed below exists only in the open, review-required
[PR #946](https://github.com/mysteropodes/nemo/pull/946), pinned here at candidate
commit
[`93d6d2ae76ffea04fcbd8e2fbe849f14469c6092`](https://github.com/mysteropodes/nemo/commit/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092).
It is stacked on the still-open [PR #944](https://github.com/mysteropodes/nemo/pull/944).
Neither filename presence nor a passing structural test is treated as R03 or R09
acceptance.

Evidence terms in this document have these meanings:

- **Integrated**: present at the pinned `origin/main` commit and exercised by an
  existing harness.
- **Candidate**: present at the pinned PR #946 commit, but not on `main` and not
  accepted.
- **Declared**: a fixture records an expectation or intended consumer, but that
  consumer does not execute it.
- **Gap**: no fixture and harness currently prove the behavior.

[R03 #899](https://github.com/mysteropodes/nemo/issues/899),
[R08 #904](https://github.com/mysteropodes/nemo/issues/904), and R09 are all open.
The issue updates for R03 and [R07 #903](https://github.com/mysteropodes/nemo/issues/903)
were read back from GitHub, but the tracking job
`a6815713cdfe4f19e0dce4aaa584a17420f4f2acc3134a6aec4c5e844cf330d9`
still has no terminal lifecycle receipt. This evidence work does not replay that
job or any board mutation.

The original R09 Fizz, Honey, and Mochi requests also remain request-only: none
has processed, recipient-accepted, or terminal receipts, and the inspected scope
contained no matching branch, worktree, or proposal effect. Their request event
IDs are respectively
`70229ab18aab0fe6578996711ef6769d51f90e6bc6fcca897c8c417006aace63`,
`8699383665da490698f407816ca6d9dcfc222139e2d369c74669c34028e02fa5`,
and `3e34e0d10b125825f262690c7621a8d15e2371732ef16ad7943f3747cc6a375b`.
Their proposal paths and any ownership, identity, time, image, or platform choices
remain untouched.

## Candidate fixture inventory

PR #946 defines its corpus in
[`tests/fixtures/generate.cjs`](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/fixtures/generate.cjs#L34-L250),
records hashes, capabilities, tolerances, expectations, and consumers in
[`manifest.json`](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/fixtures/manifest.json),
and executes its Node-compatible checks in
[`tests/nemo-fixtures.test.cjs`](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/nemo-fixtures.test.cjs#L22-L163).

| Candidate fixture | Independent expectation recorded by the candidate | Actual candidate harness | Acceptance status |
|---|---|---|---|
| `static-shapes` | Canvas, frame count, key at frame 0, three geometry/color records | Hash/size, document shape, and anchor-point bounds under Node | **Candidate** structural seed; no create, edit, save/load, Paper.js, or pixel run |
| `keyed-position-default-ease`, `keyed-position-linear`, `keyed-hold-and-static` | Values at named frames from recorded curve waypoints, hold rules, static values, and defaults | Extracted production `rawValueAtFrame` compared with the recorded values | **Candidate** evaluator evidence; no key-creation/edit/history/persistence sequence |
| `expression-rotation` | `frame * 15` yields 0/90/180/270/360 at frames 0/6/12/18/24 | Structure only | **Declared** for app-runtime/browser characterization; expression sandbox is not run |
| `held-frames-on-sixes` | Previous-key inheritance across all 24 frames | Generator helper checked under Node; independent review also exercised production `getEffectiveStrokes` | **Candidate** frame-read evidence; no edit/promotion/undo/save sequence |
| `component-instances` | Loop, ping-pong, once, and single-frame tables from placement and speed | Extracted production `resolveSymbolFrameIdx` | **Candidate** animation-read evidence; instance creation, duplication, persistence, and render are unproved |
| `matte-alpha` | Matte source UID resolves; inside/outside pixels have specified colors | UID structure only | **Declared** for WebGPU/R12/R21; no pixel consumer runs |
| `text-root` | Root precedes four glyph records | Candidate structural assertions only | **Blocked candidate**: it is not production-shaped; see review blockers below |
| `image-mesh` | Normalized topology, 32 triangles, one 40 px vertex displacement | Candidate structure; independent review exercised production mesh load/serialize/world mapping | **Candidate** data-consumer evidence; no duplicate/history/pixel/export run |
| `media-raster` | Same embedded PNG every frame, exact source hash and bounds | Candidate data assertions | **Candidate** structural seed; no Paper.js load, render, save/load, or export run |
| `migration-legacy-frames-only` | Import defaults to one layer, 6 frames, 12 fps, 1920×1080, white background | Legacy input structure only | **Declared** for `importJSON` in a browser; the stated post-import result is not executed |
| `export-12f-320x240` | Twelve independently positioned squares and exact frame probes; MP4 metadata | Geometry/probe containment only; render/export workloads are emitted as `not-run` | **Declared** for WebGPU and packaged desktop; no pixels or encoded artifact are inspected |
| `pen-stroke` | One stroke, at least three segments, bounded result, undo returns count to zero | Pointer schema, monotonic time, and bounds only | **Declared** for a future browser harness; it does not dispatch input or call undo |
| `scale-500`, `scale-2000`, `scale-4000` | Seeded documents with recorded hashes and sizes | JSON clone/serialize and V8 heap microbenchmarks | **Blocked candidate** for hash and heap claims; it is not product undo behavior |

Three reproduced review findings prevent treating the candidate corpus or bench as
accepted:

1. The text generator builds a separate one-point root and four ungrouped glyph
   rectangles
   ([candidate lines 165–173](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/fixtures/generate.cjs#L165-L173)).
   Production makes the first glyph the root and gives every glyph one shared
   `groupId`
   ([production lines 308–344](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/vector-text-bridge.js#L308-L344)).
   The existing structural assertion therefore passes a document that the real
   text-group consumer sees as one member instead of five.
2. The bench looks for `entry.sha256` on `gen.GENERATED_SCALE`, where the standard
   entries do not contain hashes, so its comparison short-circuits
   ([candidate lines 78–89](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/bench/run.cjs#L78-L89)).
   It must compare standard sizes with `manifest.generated` before its source
   identity is trustworthy.
3. The heap workload subtracts two process-wide `heapUsed` samples while prior
   loop allocations may be collected, then clamps negative deltas to zero
   ([candidate lines 90–97](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/bench/run.cjs#L90-L97)).
   Review reproduced order-dependent results. Each input needs isolated retained
   ownership and order-independence evidence before it can seed R19.

## Mutation chain

Each row below is an independently testable R09 scenario. “Same” means exact deep
equality after removing only fields whose eventual contract explicitly marks them
transient or derived; no such exclusions are adopted here.

| Scenario | Real fixture or explicit gap | Independent expected behavior | Actual consumer | Existing harness and exact missing proof |
|---|---|---|---|---|
| Create | **Gap.** `static-shapes` and `pen-stroke` are candidate seeds, not a create transaction. | Starting from a recorded document, one create gesture adds exactly one aggregate; the new aggregate gets a fresh stable ID, existing IDs/content/order stay the same, and the action creates one undo entry. | `createUserLayer` initializes frames and a `layerUid` ([`app.js` 377–386](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L377-L386)); drawing bridges eventually serialize through `saveActiveLayerFrame`. | No integrated or candidate harness executes a browser gesture and compares before/after document plus history. `pen-stroke` only validates its input script. |
| Edit | **Gap.** Any structural candidate could seed the document. | Editing changes only the targeted contracted values, preserves the aggregate and element identities, materializes the correct held frame, and creates one undo entry. | Selection transforms call `pushUndo`, promote held frames, then save through `saveActiveLayerFrameOrPromote` ([`select-bridge.js` 1001–1017](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/select-bridge.js#L1001-L1017)); persistence writes live Paper items in [`app.js` 4634–4691](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L4634-L4691). | No before/edit/after test uses Paper.js and the real save path. Node source-extraction checks do not prove object identity or held-frame promotion. |
| Key | Candidate keyed fixtures contain pre-authored tracks. | Adding a key at frame `f` preserves holder identity and other keys, stores the displayed value at exactly `f`, and evaluates the independently derived curve/hold values on both sides. Undo removes that key; redo restores the identical key. | `setKeyAtCurrentFrame` / `setKeyAtFrame` mutate tracks ([`motion.js` 2747–2776](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/motion.js#L2747-L2776)); `rawValueAtFrame` reads them ([`motion.js` 1311–1344](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/motion.js#L1311-L1344)). | Integrated `tests/easing-reference.test.cjs` checks the production evaluator against independent values, and the candidate repeats this with fixture tracks. Neither operates the key writer or history/persistence. |
| Undo | `pen-stroke` declares a zero-stroke result; scale fixtures only feed a JSON-copy benchmark. | From the post-action state, one undo restores the exact pre-action values, IDs, reference targets, order, selection anchor, active context, and project registries. | `pushUndoLayers`, `restoreLayersSnapshot`, and `undo` snapshot and rebuild layers, then refresh selection ([`tweens.js` 4928–5009](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/tweens.js#L4928-L5009)). | Integrated performance tests only verify snapshot/save call counts. The candidate `copy.undoClone.*` calls `JSON.parse(JSON.stringify(doc))`, not Nemo undo. No fixture drives undo or compares the full state. |
| Redo | **Gap.** No fixture encodes a pre-action/post-action pair plus restored state. | After the matching undo, one redo recreates the exact post-action values, IDs, reference targets, order, selection, and active context, without inventing another identity. | `redo` mirrors snapshot restore ([`tweens.js` 5012–5022](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/tweens.js#L5012-L5022)). | No integrated or candidate harness calls the real redo path. |
| Reorder | Integrated test data uses layers A/B/C/D; no serialized fixture exists. | Reordering changes only array/z-order. `layerUid`, layer content, parent/matte/time-link targets, active layer, and selected layer identities stay attached to the same layers; undo/redo and save/load preserve the result. | `reorderLayersAtGap` captures object identity, splices both model and Paper layers, and remaps active/multi-selection ([`app.js` 438–487](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L438-L487)). | **Integrated partial:** `tests/feedback-2026-08-22.test.cjs` executes the real function and checks order, active layer, and selection ([lines 41–109](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/tests/feedback-2026-08-22.test.cjs#L41-L109)). It does not cover references, undo/redo, render order, or save/load. |
| Duplicate | `component-instances` and `image-mesh` are candidate source documents, but no duplicate fixture exists. | The copied layer gets a fresh `layerUid`; project-owned subgraphs that must not be shared, such as an image mesh, get fresh IDs and all internal references are rewritten. References intentionally shared with external aggregates remain equal. Current layer-scoped `strokeId` reuse must be explicitly accepted with its scope or replaced; the test must not guess. | `duplicateLayer` deep-copies fields and remaps mesh IDs ([`timeline.js` 1271–1364](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/timeline.js#L1271-L1364)); `duplicateSelection` allocates fresh clone IDs ([`tools.js` 1600–1646](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/tools.js#L1600-L1646)). | No harness executes either duplicate path over components, text groups, meshes, parents, mattes, rigs, or per-element motion, then checks undo/redo and save/load. |
| Save | Candidate projects imitate selected output fields; they are not produced by the app. | Saving after each preceding mutation preserves every contracted field and ID, omits only explicitly transient/derived fields, and emits a versioned document whose unknown-data policy is test-defined. | `exportJSON` saves frames, allowlists layer/project fields, and strips live rig references ([`timeline.js` 2049–2245](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/timeline.js#L2049-L2245)); `serP` serializes individual Paper paths ([`app.js` 600–836](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L600-L836)). | Candidate hash checks prove generator consistency, not `exportJSON`. No integration/browser harness exports a loaded fixture and compares the contracted projection. Unknown-data behavior is not specified or tested. |
| Load | `migration-legacy-frames-only` declares legacy defaults; all other candidate projects can be load seeds. | Loading current data reconstructs the contracted state with the same stable IDs and references; legacy input gets exactly the declared defaults/migrations; malformed/unknown data follows a specified policy; a second save is equivalent to the first contracted projection. | `importJSON` validates/defaults the document, rebuilds layers and restores fields/registries ([`timeline.js` 2354–2654](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/timeline.js#L2354-L2654)); `desP` and `loadFrame` reconstruct live items ([`app.js` 837–1001](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L837-L1001), [`app.js` 4746–4890](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L4746-L4890)). | Candidate tests check document shape and legacy input shape only. No app-runtime harness asserts the declared post-import state or save→load→save equivalence. The broken text fixture shows why structure alone is insufficient. |

## Downstream consumer acceptance

These consumers are required only when the migrated aggregate is applicable. A
contract packet must mark a consumer **applicable** with a fixture/harness or
**excluded** with a reason; silence is not an exclusion.

| Consumer | Candidate input and independent oracle | Production path | Evidence now | Required acceptance harness |
|---|---|---|---|---|
| Selection | `static-shapes`, `text-root`, and `pen-stroke`; expected selected identities and bounds must be derived from fixture geometry and group membership. | `selectedPaths` and [document-restore refresh](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/select-bridge.js#L3569-L3578); [text grouping](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/vector-text-bridge.js#L350-L358). | Reorder selection identity has an integrated Node check. The candidate text seed fails the real group-membership consumer; pointer input is never dispatched. | Browser/Paper harness: load, click/marquee/group-select, edit, undo/redo, reorder/duplicate, and assert selected stable IDs after each rebuild. |
| Animation | Keyed position/hold, component instances, expression rotation, held frames, image mesh. Expected values/tables are recorded independently. | [`motion.js` track writers/readers](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/motion.js#L2747-L2776), `tweens.js`, symbol frame resolution, and per-element/mesh motion. | Integrated easing evaluator tests and candidate extracted-evaluator/component checks cover reads. Writes, expression sandbox, history, persistence, and rendered positions remain gaps. | Integration plus browser harness: key/edit/undo/redo/save/load, evaluate named frames, then compare rendered transforms and exact frame timing. |
| Render | `static-shapes`, `matte-alpha`, `image-mesh`, `media-raster`, and `export-12f-320x240`, with geometry/pixel probes in the candidate manifest. | [`buildSceneJson`](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/engine-bridge.js#L802-L880) and related scene consumers feed the Rust/WebGPU render. | Candidate checks stop at JSON structure and probe placement. No WebGPU pixels are captured. | Browser WebGPU harness: load through the app, render specified frames, sample inside/outside pixels within tolerance, and compare layer order, mattes, text groups, raster, and mesh output. |
| Export | `export-12f-320x240` plus matte/media/mesh fixtures; expected frame count, dimensions, fps, probes, codec, pixel format, and duration are independently recorded. | `export.js` selects Paper/engine paths, renders sequences, and invokes ffmpeg for packaged export ([engine decision and frame loop](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/export.js#L445-L620)). | PR #946 explicitly records render and MP4 workloads as `not-run`; a declared filename is not an exported artifact. | Browser export for supported browser formats plus packaged desktop export: inspect decoded frame count/dimensions/duration/pixel probes and preserve source document state after export. |
| Native | `export-12f-320x240` is the only candidate requiring `desktop`; media/video cases are otherwise gaps. | Tauri file APIs, [`run_ffmpeg`](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src-tauri/src/lib.rs#L9-L55), packaged sidecar, and native video commands bridge document data to Rust/native resources. | `test:desktop` on the pinned baseline reports no defined `tests/desktop` harness; candidate bench has no native backend. Rust unit tests do not load a Nemo document through the packaged app. | Built-app harness with isolated data roots: load a real fixture, exercise native read/write/encode/decode, inspect artifacts, then reopen and verify stable IDs/state. Record OS, architecture, app commit, sidecar identity, and unsupported-platform result. |

## Existing job routing and remaining gate

At the pinned baseline, the named jobs describe the missing harnesses directly:

- `test:integration` returns `not-run` because `tests/integration` is absent.
- `test:browser` requires Playwright and `tests/browser`; neither is supplied by
  `main`.
- `test:desktop` requires a packaged app and `tests/desktop`; no desktop harness
  is defined.
- `bench` returns `not-run` because `tests/bench` is absent on `main`.

Those gates are implemented in
[`scripts/nemo/lib/jobs.cjs` lines 142–175](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/scripts/nemo/lib/jobs.cjs#L142-L175).
PR #946 proposes a Node fixture suite and bench wiring, but it does not add the
integration, browser, WebGPU, or packaged-desktop acceptance harnesses and it
must first resolve the three blocking review findings above.

R09 can use this matrix to design a contract-specific fixture packet only after
the contract names identifier scope, writer, persisted/transient/derived fields,
reference-rewrite rules, time units, compatibility/unknown-data policy, and
applicable platforms. Acceptance then requires the mutation-chain row and every
applicable downstream row to pass against the same pinned contract fixture. R03
must first provide an accepted fixture basis, and R05 must provide the accepted
module/capability boundary; until then this document is evidence of current
coverage and gaps, not production adoption.
