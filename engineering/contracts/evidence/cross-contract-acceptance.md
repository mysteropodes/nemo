# R09 cross-contract acceptance evidence

Status: design evidence; no foundational contract is adopted by this document.

## Evidence boundary

The production-consumer paths in this matrix retain the original `origin/main` baseline
[`1d652dd5de550bf6f5863faf5ed89c2481a36412`](https://github.com/mysteropodes/nemo/commit/1d652dd5de550bf6f5863faf5ed89c2481a36412).
It maps application consumers and test jobs at that baseline to the second acceptance
line of [R09 #905](https://github.com/mysteropodes/nemo/issues/905). R09 remains
open, and production adoption remains gated by R03 and R05.

The deterministic corpus discussed below exists only in the open, review-required
[PR #946](https://github.com/mysteropodes/nemo/pull/946), pinned here at candidate
commit
[`6d499212fdf447af439d66f5aadb905f6575110d`](https://github.com/mysteropodes/nemo/commit/6d499212fdf447af439d66f5aadb905f6575110d).
The independent review of this exact commit cleared all four original fixture/bench
findings with 45/45 focused tests plus behavioral controls. The former
[`93d6d2ae76ffea04fcbd8e2fbe849f14469c6092`](https://github.com/mysteropodes/nemo/commit/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092)
failures remain historical evidence below. The corrected candidate is stacked on
[PR #944](https://github.com/mysteropodes/nemo/pull/944) at
[`dd39e5215f000031c5f75821913cce2e6f971db9`](https://github.com/mysteropodes/nemo/commit/dd39e5215f000031c5f75821913cce2e6f971db9);
its separate five false-binding findings and Codexalog correction lane remain open.
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
At the original matrix evidence cut, the issue updates for R03 and [R07 #903](https://github.com/mysteropodes/nemo/issues/903)
were read back from GitHub, but the tracking job
`a6815713cdfe4f19e0dce4aaa584a17420f4f2acc3134a6aec4c5e844cf330d9`
had no terminal lifecycle receipt. This refresh does not reassess that lifecycle
or replay the job or any board mutation.

At that same evidence cut, the original R09 Fizz, Honey, and Mochi requests
were request-only: none had processed, recipient-accepted, or terminal receipts,
and the inspected scope contained no matching branch, worktree, or proposal effect. Their request event
IDs are respectively
`70229ab18aab0fe6578996711ef6769d51f90e6bc6fcca897c8c417006aace63`,
`8699383665da490698f407816ca6d9dcfc222139e2d369c74669c34028e02fa5`,
and `3e34e0d10b125825f262690c7621a8d15e2371732ef16ad7943f3747cc6a375b`.
Their proposal paths and any ownership, identity, time, image, or platform choices
remain untouched.

## Candidate fixture inventory

The corrected PR #946 corpus has 12 fixtures and 41 committed files. Its
[`generate.cjs`](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/generate.cjs),
[`corpus.cjs`](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/lib/corpus.cjs),
[`manifest.json`](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/manifest.json), and
[`fixtures.test.cjs`](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/fixtures.test.cjs)
replace the former inventory's fixture names and harness claims. The normal
[`nemo-fixtures.test.cjs`](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/nemo-fixtures.test.cjs)
entry loads that suite.

| Candidate fixture | Independent expectation recorded by the candidate | Actual candidate harness | Acceptance status |
|---|---|---|---|
| `static-props` | Document dimensions, geometry bounds, and static/default motion values | 39 Node checks including production motion evaluation | **Candidate** read evidence; no create/edit/save/load or rendered-pixel workflow |
| `keyed-props` | Keyed values from independent curve/hold rules | 67 Node checks against production motion in a VM | **Candidate** evaluator evidence; no key-creation/edit/history/persistence sequence |
| `expression-props` | Expression values and error behavior | 55 Node checks execute production expression evaluation in a VM | **Candidate** evaluation evidence; no browser sandbox/UI or rendered animation acceptance |
| `held-frames` | Effective keyframe inheritance and evaluated values | 103 Node checks including production frame/motion consumers | **Candidate** frame-read evidence; no edit/promotion/undo/save sequence |
| `components` | Frame-resolution tables for component playback | 168 Node checks against production symbol frame resolution | **Candidate** animation-read evidence; instance creation, duplication, persistence, and render remain unproved |
| `masks-alpha` | Document/alpha behavior and six pixel expectations | Five Node checks; six R13 pixel expectations are gated | **Candidate** data evidence; no WebGPU pixel run |
| `text` | Three glyph members share `groupId`; first glyph is the root; text-unit indices and bounds | Eleven Node checks; independent review also exercised actual Paper.js `desP`/`serP` and group-membership round trips | **Candidate** corrected text-group evidence; glyphs are rectangles, one R13 pixel expectation and interactive text/selection workflows remain open |
| `mesh` | Topology, propagated mesh IDs, and deformed world vertex | Three Node checks including production mesh scene mapping; three R13 pixel expectations gated | **Candidate** data-consumer evidence; no duplicate/history/pixel/export workflow |
| `media` | Embedded asset identity, dimensions, and document shape | Six Node checks; three R13 pixel expectations gated | **Candidate** asset evidence; no app render/save/load/export workflow |
| `migration` | Document structure and five migration expectations | Four Node structure checks; four `importJSON` and one `desP` R12 expectations gated | **Declared** post-migration behavior; app import is not executed |
| `interaction` | Recorded pointer sequence and post-draw/undo expectations | One Node schema check; two R12 record/replay expectations gated | **Declared** interaction behavior; no input dispatch or undo |
| `export` | Generated frame pixels, encoder arguments, stream metadata, and decoded samples | One Node PNG check plus five conditional encode/decode expectations; the independent review ran the committed ffmpeg sidecar test without encode/probe/decode skip diagnostics | **Candidate** CLI artifact evidence; generated PNGs bypass Nemo rendering, and R21 browser/packaged export remains unrun |
| `bench-vectors-40x24`, `bench-vectors-8x24`, `bench-images-20x24` | Generated workload identity pinned by manifest hashes | Production undo clone, separately labeled JSON clone/serialize, and isolated retained-document heap measurements | **Candidate** corrected benchmark evidence; no full undo/redo workflow or R19 budget acceptance |

## Historical failures and corrected evidence

The former candidate's
[`generate.cjs`](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/fixtures/generate.cjs#L34-L250),
[`manifest.json`](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/fixtures/manifest.json), and
[`nemo-fixtures.test.cjs`](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/nemo-fixtures.test.cjs#L22-L163)
remain linked for historical reproduction. These findings apply to `93d6d2a`,
not the corrected `6d49921` candidate:

1. The text generator built a separate one-point root and four ungrouped glyph
   rectangles
   ([candidate lines 165–173](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/fixtures/generate.cjs#L165-L173)).
   Production makes the first glyph the root and gives every glyph one shared
   `groupId`
   ([production lines 308–344](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/vector-text-bridge.js#L308-L344)).
   The former structural assertion therefore passed a document that the real
   text-group consumer sees as one member instead of five.
2. The bench looked for `entry.sha256` on `gen.GENERATED_SCALE`, where the standard
   entries do not contain hashes, so its comparison short-circuited
   ([candidate lines 78–89](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/bench/run.cjs#L78-L89)).
   The missing manifest comparison invalidated its source-identity claim.
3. The heap workload subtracted two process-wide `heapUsed` samples while prior
   loop allocations may be collected, then clamped negative deltas to zero
   ([candidate lines 90–97](https://github.com/mysteropodes/nemo/blob/93d6d2ae76ffea04fcbd8e2fbe849f14469c6092/tests/bench/run.cjs#L90-L97)).
   Review reproduced order-dependent results.
4. The former `copy.undoClone` label described a JSON round trip rather than
   production `_cloneLayersForUndo`; it did not establish product undo cost.

The corrected review established these bounded results:

- **Text grouping cleared:** three members and the first-glyph root survive actual
  `desP` and `serP` → JSON → `desP` on Paper.js items, then the real group consumer.
  Negative controls detect legacy `textGroupId`, a detached root, and a mismatched
  glyph group. The [corrected builder](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/lib/corpus.cjs#L343-L373)
  still uses glyph stand-ins and marks nonroot glyphs `isText`; this is not full
  text-workflow certification.
- **Hash guard cleared:** the [public benchmark runner](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/bench/run.cjs#L68-L83)
  checks `manifest.workloads`. All standard hashes match; a `canvasW=1` tamper
  causes `runBench({quick:true,iterations:1})` to throw, and unlisted identities
  are rejected.
- **Heap finding cleared:** [fresh child processes with GC](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/bench/run.cjs#L89-L105)
  isolate retained parsed-document allocations. Forward, reversed, and repeated
  quick runs used nine distinct child PIDs and returned identical per-workload
  deltas (19,713,584 / 1,971,456 / 1,188,984 bytes respectively for the table's
  three workloads). These are neither renderer/RSS measurements nor undo latency.
- **Undo benchmark finding cleared:** the [production clone loader](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/lib/sandbox.cjs#L116-L135)
  uses the actual `_cloneLayersForUndo` body, `_walkStrokes`, and heavy-field list.
  Independent controls verify detached heavy strings, restored originals, an
  independent nested clone, and restoration after stringify throws. The
  [runner](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/bench/run.cjs#L154-L166)
  calls that clone; `copy.jsonClone` remains separately labeled `node:JSON`.
  This does not exercise the application's undo/redo transaction.

The independent run of
`node --test scripts/nemo/inventory.test.cjs tests/nemo-fixtures.test.cjs tests/nemo-bench.test.cjs`
passed **45/45** on Node 25.9.0, including 463 Node fixture expectations and the
available committed ffmpeg n8.1 encode/probe/decode test. This refresh reuses that
exact-commit review and its controls; it does not rerun application tests.

Fixture generation `--check` passed with Node 24.18.0 / zlib 1.3.1-e00f703
(12 fixtures, 41 files). Node 25.9.0 / zlib 1.2.12 reported all 12 export PNGs and
their manifest hashes stale, while independent decoding found identical RGBA
bytes and the workload manifests remained identical. The
[PNG encoder](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/lib/png.cjs#L1-L50)
conditions compressed-byte determinism on the Node/zlib build; this portability
limit remains visible and generation needs the compatible build.

**Twenty expectations remain pending:** thirteen R13 render/export pixel checks,
four R12 `importJSON` migrations, one R12 `desP` migration, and two R12
record/replay checks. The [test loop](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/fixtures.test.cjs#L264-L278)
validates their declaration and continues without execution; TAP `skipped=0`
does not clear them. CLI ffmpeg evidence does not clear native or packaged export.
The [bench receipt](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/bench/run.cjs#L170-L185)
still records `render.engine.export` and `export.mp4.export` as `not-run`, backend
`null`, with R21 browser/packaged desktop and R19 budgets unaccepted.

**Separate active dependencies:** #944 retains five runtime-confirmed false
bindings (quoted return, reassigned helper argument, anonymous callback shadowing,
block scope, and same-name helper resolution) despite 24 focused tests passing;
Codexalog owns the correction. Clauditron owns combined #946/#949 loader
compatibility. With [#949](https://github.com/mysteropodes/nemo/pull/949) pinned at
[`f4892037ac0c3e165d21efa40a21aa2c75f0662b`](https://github.com/mysteropodes/nemo/commit/f4892037ac0c3e165d21efa40a21aa2c75f0662b),
loading its `motion.js` through the unchanged
[#946 sandbox](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/lib/sandbox.cjs#L70-L80)
reproduced `ReferenceError: SMAnimationCurve is not defined`. Loading
[`animation/curve.js`](https://github.com/mysteropodes/nemo/blob/f4892037ac0c3e165d21efa40a21aa2c75f0662b/src/js/animation/curve.js)
first restored the expected 0.25 value, also returned by standalone #946. The
combined candidate still needs integration validation; the prior R08 browser
result does not cover this loader and is not revoked by it.

## Mutation chain

Each row below is an independently testable R09 scenario. “Same” means exact deep
equality after removing only fields whose eventual contract explicitly marks them
transient or derived; no such exclusions are adopted here.

| Scenario | Real fixture or explicit gap | Independent expected behavior | Actual consumer | Existing harness and exact missing proof |
|---|---|---|---|---|
| Create | **Gap.** `static-props` and `interaction` are candidate seeds, not a create transaction. | Starting from a recorded document, one create gesture adds exactly one aggregate; the new aggregate gets a fresh stable ID, existing IDs/content/order stay the same, and the action creates one undo entry. | `createUserLayer` initializes frames and a `layerUid` ([`app.js` 377–386](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L377-L386)); drawing bridges eventually serialize through `saveActiveLayerFrame`. | No integrated or candidate harness executes a browser gesture and compares before/after document plus history. `interaction` only validates its input script. |
| Edit | **Gap.** Any structural candidate could seed the document. | Editing changes only the targeted contracted values, preserves the aggregate and element identities, materializes the correct held frame, and creates one undo entry. | Selection transforms call `pushUndo`, promote held frames, then save through `saveActiveLayerFrameOrPromote` ([`select-bridge.js` 1001–1017](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/select-bridge.js#L1001-L1017)); persistence writes live Paper items in [`app.js` 4634–4691](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L4634-L4691). | No before/edit/after test uses Paper.js and the real save path. Node source-extraction checks do not prove object identity or held-frame promotion. |
| Key | Candidate keyed fixtures contain pre-authored tracks. | Adding a key at frame `f` preserves holder identity and other keys, stores the displayed value at exactly `f`, and evaluates the independently derived curve/hold values on both sides. Undo removes that key; redo restores the identical key. | `setKeyAtCurrentFrame` / `setKeyAtFrame` mutate tracks ([`motion.js` 2747–2776](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/motion.js#L2747-L2776)); `rawValueAtFrame` reads them ([`motion.js` 1311–1344](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/motion.js#L1311-L1344)). | Integrated `tests/easing-reference.test.cjs` checks the production evaluator against independent values, and the candidate executes production motion in a VM with fixture tracks; combined #949 loader validation remains open. Neither operates the key writer or history/persistence. |
| Undo | `interaction` declares a zero-stroke result; workload documents feed the production clone and a separate JSON-copy benchmark. | From the post-action state, one undo restores the exact pre-action values, IDs, reference targets, order, selection anchor, active context, and project registries. | `pushUndoLayers`, `restoreLayersSnapshot`, and `undo` snapshot and rebuild layers, then refresh selection ([`tweens.js` 4928–5009](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/tweens.js#L4928-L5009)). | Integrated performance tests only verify snapshot/save call counts. The corrected candidate `copy.undoClone.*` calls actual `_cloneLayersForUndo`, with independently verified heavy-field restoration. It does not invoke the full undo transaction or compare the restored application state. |
| Redo | **Gap.** No fixture encodes a pre-action/post-action pair plus restored state. | After the matching undo, one redo recreates the exact post-action values, IDs, reference targets, order, selection, and active context, without inventing another identity. | `redo` mirrors snapshot restore ([`tweens.js` 5012–5022](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/tweens.js#L5012-L5022)). | No integrated or candidate harness calls the real redo path. |
| Reorder | Integrated test data uses layers A/B/C/D; no serialized fixture exists. | Reordering changes only array/z-order. `layerUid`, layer content, parent/matte/time-link targets, active layer, and selected layer identities stay attached to the same layers; undo/redo and save/load preserve the result. | `reorderLayersAtGap` captures object identity, splices both model and Paper layers, and remaps active/multi-selection ([`app.js` 438–487](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L438-L487)). | **Integrated partial:** `tests/feedback-2026-08-22.test.cjs` executes the real function and checks order, active layer, and selection ([lines 41–109](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/tests/feedback-2026-08-22.test.cjs#L41-L109)). It does not cover references, undo/redo, render order, or save/load. |
| Duplicate | `components` and `mesh` are candidate source documents, but no duplicate fixture exists. | The copied layer gets a fresh `layerUid`; project-owned subgraphs that must not be shared, such as an image mesh, get fresh IDs and all internal references are rewritten. References intentionally shared with external aggregates remain equal. Current layer-scoped `strokeId` reuse must be explicitly accepted with its scope or replaced; the test must not guess. | `duplicateLayer` deep-copies fields and remaps mesh IDs ([`timeline.js` 1271–1364](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/timeline.js#L1271-L1364)); `duplicateSelection` allocates fresh clone IDs ([`tools.js` 1600–1646](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/tools.js#L1600-L1646)). | No harness executes either duplicate path over components, text groups, meshes, parents, mattes, rigs, or per-element motion, then checks undo/redo and save/load. |
| Save | Candidate projects imitate selected output fields; they are not produced by the app. | Saving after each preceding mutation preserves every contracted field and ID, omits only explicitly transient/derived fields, and emits a versioned document whose unknown-data policy is test-defined. | `exportJSON` saves frames, allowlists layer/project fields, and strips live rig references ([`timeline.js` 2049–2245](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/timeline.js#L2049-L2245)); `serP` serializes individual Paper paths ([`app.js` 600–836](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L600-L836)). | Candidate hash checks prove generator consistency, not `exportJSON`. No integration/browser harness exports a loaded fixture and compares the contracted projection. Unknown-data behavior is not specified or tested. |
| Load | `migration` declares five R12 migrations; all other candidate projects can be load seeds. | Loading current data reconstructs the contracted state with the same stable IDs and references; legacy input gets exactly the declared defaults/migrations; malformed/unknown data follows a specified policy; a second save is equivalent to the first contracted projection. | `importJSON` validates/defaults the document, rebuilds layers and restores fields/registries ([`timeline.js` 2354–2654](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/timeline.js#L2354-L2654)); `desP` and `loadFrame` reconstruct live items ([`app.js` 837–1001](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L837-L1001), [`app.js` 4746–4890](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/app.js#L4746-L4890)). | Migration checks remain structural with five R12 expectations unexecuted. The corrected text seed passes independent Paper.js serialization/group round trips, but no app-runtime harness asserts full post-import state or save→load→save equivalence. |

## Downstream consumer acceptance

These consumers are required only when the migrated aggregate is applicable. A
contract packet must mark a consumer **applicable** with a fixture/harness or
**excluded** with a reason; silence is not an exclusion.

| Consumer | Candidate input and independent oracle | Production path | Evidence now | Required acceptance harness |
|---|---|---|---|---|
| Selection | `static-props`, `text`, and `interaction`; expected selected identities and bounds must be derived from fixture geometry and group membership. | `selectedPaths` and [document-restore refresh](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/select-bridge.js#L3569-L3578); [text grouping](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/vector-text-bridge.js#L350-L358). | Reorder selection identity has an integrated Node check. The corrected text seed passes the real group-membership consumer after Paper.js serialization round trips. Interactive pointer selection is still unrun. | Browser/Paper harness: load, click/marquee/group-select, edit, undo/redo, reorder/duplicate, and assert selected stable IDs after each rebuild. |
| Animation | `keyed-props`, `components`, `expression-props`, `held-frames`, and `mesh`. Expected values/tables are recorded independently. | [`motion.js` track writers/readers](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/motion.js#L2747-L2776), `tweens.js`, symbol frame resolution, and per-element/mesh motion. | Integrated easing tests and candidate production VM motion/expression/component checks cover reads. Combined #949 loader validation, key writes, browser sandbox/UI, history, persistence, and rendered positions remain open. | Integration plus browser harness: key/edit/undo/redo/save/load, evaluate named frames, then compare rendered transforms and exact frame timing. |
| Render | `static-props`, `masks-alpha`, `mesh`, `media`, and `export`, with geometry/pixel probes in the candidate expectation files. | [`buildSceneJson`](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/engine-bridge.js#L802-L880) and related scene consumers feed the Rust/WebGPU render. | Candidate checks cover data consumers and generated PNG samples, not app-rendered pixels. Thirteen R13 pixel expectations remain gated; no WebGPU capture is available. | Browser WebGPU harness: load through the app, render specified frames, sample inside/outside pixels within tolerance, and compare layer order, mattes, text groups, raster, and mesh output. |
| Export | `export` plus mask/media/mesh fixtures; frame count, dimensions, fps, probes, codec, and pixel-format expectations are recorded. Duration remains an acceptance-harness obligation. | `export.js` selects Paper/engine paths, renders sequences, and invokes ffmpeg for packaged export ([engine decision and frame loop](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src/js/export.js#L445-L620)). | The independent review exercised CLI sidecar encode/probe/decode over generated PNGs. Nemo render and MP4 benchmark workloads remain `not-run`; R21 browser/packaged export is unrun. | Browser export for supported browser formats plus packaged desktop export: inspect decoded frame count/dimensions/duration/pixel probes and preserve source document state after export. |
| Native | `export` supplies generated frames and CLI sidecar checks; native document/media/video workflows remain gaps. | Tauri file APIs, [`run_ffmpeg`](https://github.com/mysteropodes/nemo/blob/1d652dd5de550bf6f5863faf5ed89c2481a36412/src-tauri/src/lib.rs#L9-L55), packaged sidecar, and native video commands bridge document data to Rust/native resources. | `test:desktop` on the pinned baseline reports no defined `tests/desktop` harness; candidate bench has no native backend. The CLI sidecar run and Rust unit tests do not load a Nemo document through the packaged app. | Built-app harness with isolated data roots: load a real fixture, exercise native read/write/encode/decode, inspect artifacts, then reopen and verify stable IDs/state. Record OS, architecture, app commit, sidecar identity, and unsupported-platform result. |

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
PR #946 provides candidate Node fixture/bench wiring; the four original findings
are cleared at `6d49921`. Its twenty R12/R13 expectations, native and R21
browser/WebGPU/packaged-desktop acceptance remain open. R03 also retains the
#944 correction and combined #949 loader gates described above.

R09 can use this matrix to design a contract-specific fixture packet only after
the contract names identifier scope, writer, persisted/transient/derived fields,
reference-rewrite rules, time units, compatibility/unknown-data policy, and
applicable platforms. Acceptance then requires the mutation-chain row and every
applicable downstream row to pass against the same pinned contract fixture. R03
must first provide an accepted fixture basis, and R05 must provide the accepted
module/capability boundary; until then this document is evidence of current
coverage and gaps, not production adoption.
