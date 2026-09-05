# R09 inventory-to-contract decision gaps

Status: design evidence. This document does not adopt a foundational contract or
close R03, R05, or R09.

## Evidence boundary

All references below are read **as of 2026-09-05**. Every open PR is cited at an
explicit head SHA; a PR head that has advanced since is called out where it matters,
and no citation silently means "current".

The application evidence is pinned to fresh `origin/main`
[`a87eb54a33d225f77cff903809a0538f7e9d0179`](https://github.com/mysteropodes/nemo/commit/a87eb54a33d225f77cff903809a0538f7e9d0179).
The existing [R09 cross-contract matrix](https://github.com/mysteropodes/nemo/blob/b53fccae6426f14b6541e927869a0abc1515c415/engineering/contracts/evidence/cross-contract-acceptance.md)
is read-only input from open [PR #950](https://github.com/mysteropodes/nemo/pull/950),
pinned at `b53fccae6426f14b6541e927869a0abc1515c415`. Its prior job disposition
remains indeterminate; this work does not replay it or change that branch, file,
or PR.

The evidence states below are intentionally different:

- **Integrated** means present on the pinned `origin/main`. R05's checker and its
  `scripts/nemo/**` profile are integrated, but the checker explicitly covers a
  bounded declared set and is not wired into `check` or `verify`
  ([current README](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/engineering/boundaries/README.md#L1-L17)).
  That is an accepted tooling increment, not acceptance of R05 for application
  source.
- **Candidate** means review-required work on an open PR, read at that PR's
  current head. The R03 surface inventory is
  [PR #944](https://github.com/mysteropodes/nemo/pull/944), head
  [`dd39e5215f000031c5f75821913cce2e6f971db9`](https://github.com/mysteropodes/nemo/commit/dd39e5215f000031c5f75821913cce2e6f971db9);
  the stacked fixture corpus is [PR #946](https://github.com/mysteropodes/nemo/pull/946),
  head
  [`6d499212fdf447af439d66f5aadb905f6575110d`](https://github.com/mysteropodes/nemo/commit/6d499212fdf447af439d66f5aadb905f6575110d),
  whose parent is #944's head. Both replace the earlier reads at `075cac5` and
  `93d6d2a`; the counts, fixture ids and line anchors below are the current ones,
  not those. The R05 application classification is
  [PR #948](https://github.com/mysteropodes/nemo/pull/948), still cited at
  [`33e248795a0b94b4c93dde210c1d29f3fdab5039`](https://github.com/mysteropodes/nemo/commit/33e248795a0b94b4c93dde210c1d29f3fdab5039)
  because that is the read this document was written against; that PR head has since
  advanced to `c5c9e079c3fed3dfa52348ed4cd1b52a8ce10c8f`, where the 63-of-143
  exclusion still stands but the line anchors have moved and a two-population
  63-vs-60 clarification was added. Treat the #948 citations as a pinned historical
  read, not as that PR's current state.
- **Observed** means a bounded behavior was directly exercised. The packaged
  desktop export recorded in [PR #934](https://github.com/mysteropodes/nemo/pull/934)
  used the fixture then called `export-12f-320x240`, renamed `export` in the current
  corpus; that one consumer result does not accept the still-open fixture corpus or
  establish the other R09 contracts.
- **Gap** means the source has behavior, but no accepted contract plus independent
  probe fixes its intended semantics.

R03's candidate inventory contains 902 rows: 874 `inventoried`, 27 `unmapped`
(each a control that another handler only reads or writes) and 1
`unavailable-with-reason`. The earlier "901 inventoried" reading came from the
superseded generator, which harvested events from a fixed window after any lookup
and matched inside comments and string literals; the current pass requires a real
event registration reached from the element itself, and a lookup without one is a
reference, never a binding. The consumer detector still follows named calls for at
most three hops and 600 visited functions, over-approximates through hub functions
(`updateUI`, `renderNow`) and cannot see dynamic dispatch. Its own output says a
consumer is only something a surface *may touch*, never proof of coverage
([candidate inventory lines 38-52](https://github.com/mysteropodes/nemo/blob/dd39e5215f000031c5f75821913cce2e6f971db9/engineering/inventory/SURFACES.md#L38-L52)).
One truthful limitation of the artifact itself: at `dd39e52` its generated header
still stamps source `075cac5b3ebc` and a `-dirty` describe
([line 3](https://github.com/mysteropodes/nemo/blob/dd39e5215f000031c5f75821913cce2e6f971db9/engineering/inventory/SURFACES.md#L3)), so the file's own provenance
line lags the commit shipping it; the corrected stamp appears only at #946's head.

The candidate fixture corpus was rebuilt between the two reads, so fixture names in
this document changed meaning: 14 fixtures at `93d6d2a` became 12 at `6d49921`, with
`static-shapes` renamed `static-props`, `matte-alpha` renamed `masks-alpha`,
`media-raster` renamed `media`, `image-mesh` renamed `mesh`, `text-root` renamed
`text`, `migration-legacy-frames-only` renamed `migration`, `export-12f-320x240`
renamed `export`, and the keyed/expression/component fixtures consolidated into
`keyed-props`, `expression-props` and `components`. `pen-stroke` has no successor in
the current corpus. The Node suite no longer executes extracted evaluator copies: it
loads production modules into a `vm` sandbox (`motion.js` whole, the `app.js`
document helpers and `tweens.js` `_cloneLayersForUndo` lifted, `image-mesh.js` with
Delaunator, `vector-text-bridge.js` grouping), while expectations stay independent
because `lib/reference.cjs` restates documented rules and imports no application
code ([fixture guide lines 37-43](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/README.md#L37-L43),
[lines 62-75](https://github.com/mysteropodes/nemo/blob/6d499212fdf447af439d66f5aadb905f6575110d/tests/fixtures/README.md#L62-L75)).

**A generated reference PNG is not an app-runtime or WebGPU capture.** Of the
corpus's 488 checks, 463 run in Node, 5 run only when a runnable ffmpeg sidecar is
present, and 20 are `gate` checks needing the WebGPU engine (R13), the browser
harness (R12) or the full `importJSON` contract. Every `pixel` check — the manifest
glossary defines it as "rendered pixel colour at (x, y) on frame within tolerance
(R13 gate)" — is gated and runs nowhere today: 6 in `masks-alpha`, 3 in `mesh`, 3 in
`media`, 1 in `text`. The one pixel-shaped check that does run now is `export`'s
`frame-pixels`, and it decodes the fixture's own committed PNG frames, which
`tests/fixtures/lib/png.cjs` generated; it establishes that the reference asset's
bytes are what the generator says, not that Nemo or the engine ever produced them.
`masks-alpha`, `mesh` and `media` declare `backend: engine` with
`requiredCapabilities: ["webgpu"]` for exactly this reason. Independent PNG oracles
are what a runtime capture would eventually be compared *against*; on their own they
accept nothing about the running application.

Two further R03 inventory candidates exist only locally and are not part of #944's
reviewable head. `ebd074410f4b0b1e5c3dfe3091b637ca9c0805c6` (branch
`codex/r03-binding-repair-1a65df38`, stacked directly on `dd39e52`) repairs lexical
ownership in the DOM binding pass across five files, and a further two-file
successor touching `scripts/nemo/inventory.cjs` and `scripts/nemo/inventory.test.cjs`
is uncommitted working-tree state on a local follow-up branch with no commit to pin.
Neither is cited anywhere below, and neither changes what #944 currently offers for
review.

R05 has the inverse limitation. The integrated checker can enforce declared
module edges, but the open application profile excludes 63 of 143 JavaScript
files, including the central document, motion, history, render, and export
consumers, because its lexical scanner cannot parse them
([candidate classification lines 59-97](https://github.com/mysteropodes/nemo/blob/33e248795a0b94b4c93dde210c1d29f3fdab5039/engineering/boundaries/profiles/app-surfaces.md#L59-L97)).
The remaining 80-file profile uses one explicitly non-normative `app-legacy`
layer and has almost no static import edges
([candidate classification lines 99-138](https://github.com/mysteropodes/nemo/blob/33e248795a0b94b4c93dde210c1d29f3fdab5039/engineering/boundaries/profiles/app-surfaces.md#L99-L138)).
Neither filename inclusion nor a clean result for the declared subset assigns
the application ownership R09 needs.

## Persisted UID references and current remap/preserve behavior

The identity row below names `layerUid`, but the decision surface is wider than
parenting and mattes, and the omission matters because the copy paths already
disagree. `exportJSON`'s per-layer allowlist persists ten distinct layer-UID-bearing
shapes, plus one on the media library
([`timeline.js` 2117-2125](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2117-L2125),
[`timeline.js` 2213](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2213)):

| Persisted reference | Shape | Resolver |
|---|---|---|
| `parentLayerUid` | uid | `legacyParentChainMats` walks uid -> index ([`motion.js` 3978-3992](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3978-L3992)) |
| `parentLayerUidB` | uid | dispatches to `blendedAncestorMat` ([`motion.js` 3862](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3862), [3970-3977](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3970-L3977)) |
| `parentsMore` | `[{uid}]` | weighted-parent blend ([`motion.js` 3903-3910](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3903-L3910)) |
| `parentKeys` | `[{frame, uid\|null}]` | frame-aware re-parent via `layerParentUidAt` ([`motion.js` 4509](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L4509)) |
| `matteSourceLayerUid` | uid | converted to a final wire index ([`engine-bridge.js` 2550-2562](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L2550-L2562)) |
| `mattesMore` | `[{uid, mode}]` | converted to final wire indices ([`engine-bridge.js` 2573-2591](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L2573-L2591)) |
| `effectsFrom` | uid | Instance Effect inheritance ([`engine-bridge.js` 750-756](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L750-L756)) |
| `timeLink` | `{uid, ...}` | in/out time link |
| `followPath.targetLayerUid` | uid | `sampleFollowPathAt` ([`motion.js` 3470-3474](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3470-L3474)) |
| `effector.targetLayerUid` | uid | effector target, shape-guarded on import ([`timeline.js` 2465-2468](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2465-L2468)) |
| `mediaLibrary[].layerUid` | uid | media picker, uid first then name fallback |

An eleventh class is textual: uid literals inside persisted expression code.
`exprLayer` and `exprLayerControl` resolve `layer(uid)` and `layerControl(uid, name)`
at evaluation time ([`motion.js` 2042](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L2042),
[`motion.js` 2103](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L2103)), and the pickwhip emits the uid as a
JSON string literal into the code
([`motion.js` 9445-9450](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L9445-L9450)). No copy path parses
expression source, so no copy path can remap it.

Four different policies are in force today, none of them stated as a contract:

1. **Preserve — the copy aliases the source's targets.** `duplicateLayer`'s `dupOne`
   copies every UID reference verbatim, so the duplicate points at exactly what the
   source pointed at: matte ([`timeline.js` 1284-1286](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1284-L1286)),
   parents ([`timeline.js` 1418-1420](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1418-L1420)),
   expressions ([`timeline.js` 1435](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1435)) and effector
   ([`timeline.js` 1482](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1482)). `splitLayerAtPlayhead` does
   the same for the second half through two explicit allowlists
   ([`timeline.js` 1235](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1235),
   [`timeline.js` 1244](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1244)). `importJSON` restores every
   persisted uid unchanged behind per-field shape filters
   ([`timeline.js` 2478-2493](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2478-L2493)).
2. **Remap — the copy rebuilds the subgraph.** Duplicating a *folder* re-points the
   copied children's `parentLayerUid`/`parentLayerUidB` at the new folder's uid
   instead of the source's ([`timeline.js` 1508-1518](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1508-L1518)).
   `transplant.js` builds an old-to-new uid table up front and remaps
   `parentLayerUid`, `parentLayerUidB`, `matteSourceLayerUid`, `parentsMore`,
   `mattesMore` and `timeLink.uid` through it; an unresolvable single reference
   becomes `null` and an unresolvable array entry is dropped
   ([`transplant.js` 280-306](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/transplant.js#L280-L306)).
3. **Drop or refuse to carry.** `transplant.js` deliberately nulls
   `mediaLibrary[].layerUid` so the picker falls back to name resolution rather than
   coincidentally hitting an unrelated local layer
   ([`transplant.js` 313-319](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/transplant.js#L313-L319)), and it never
   copies `expressions`, `parentKeys`, `followPath`, `effector` or `effectsFrom` at
   all, so those references have no remap path and simply do not travel
   ([`transplant.js` 218-275](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/transplant.js#L218-L275)). Splitting a
   layer deletes `timeLink` from both halves with a toast, because the cut
   materialises hard in/out values the link would override
   ([`timeline.js` 1251-1252](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1251-L1252)).
4. **Mint.** `importJSON` migrates a pre-uid matte by generating a `layerUid` on the
   implicit `i+1` source and writing it into `matteSourceLayerUid`
   ([`timeline.js` 2559-2573](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2559-L2573)); `ensureLayerUid`
   allocates lazily for any older layer still lacking one
   ([`motion.js` 3560](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3560)).

The policies disagree on the same field. `parentKeys` is preserved by duplicate and
by split, restored verbatim by import, and absent from transplant, so a keyed
re-parent survives three copy paths and vanishes on the fourth with no diagnostic.
Nothing binds the four lists together: each is an independently maintained allowlist
whose own comments record repeated omissions found by later sweeps
([`timeline.js` 1224-1233](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1224-L1233),
[`timeline.js` 1396-1417](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L1396-L1417)).

Dangling behavior is likewise already inconsistent, and only the matte case is
documented at the boundary: a missing, dangling or self-referencing
`matteSourceLayerUid` leaves `matteSourceIndex` unset and `engine.rs`'s
`resolve_matte_source` applies the legacy implicit `i+1` fallback, while a dangling
entry in `mattesMore` is dropped outright because `resolve_all_mattes` gives the
extras no fallback and a half-resolved entry would mask against index 0
([`engine-bridge.js` 2544-2591](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L2544-L2591)). A dangling
`parentLayerUid` merely ends the chain walk
([`motion.js` 3988-3992](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3988-L3992)); a dangling
`followPath.targetLayerUid` returns `null`
([`motion.js` 3470-3474](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3470-L3474)). These are four distinct
unwritten answers to one question.

## Decisions still required

Every row is a contract decision, not a proposed answer. The acceptance probe is
the smallest independently checkable experiment that would make the chosen
answer reviewable. Expected values must come from the fixture definition or the
adopted contract, not from copying the application's output back into the test.

| Gap | Actual source consumer and present evidence | Consequence while unresolved | Required contract decision | Smallest independent acceptance probe |
|---|---|---|---|---|
| Document and editing-context authority | `state.layers` is mutated in place during reorder to preserve its alias to a component's `sym.layers` ([`app.js` 438-487](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/app.js#L438-L487)). `exportJSON` switches among the live scene, component snapshot, and montage snapshot before saving ([`timeline.js` 2049-2086](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2049-L2086)). R03 `static-props` and `migration` are candidate inputs; neither executes a nested edit/save, and `migration`'s five gated checks wait on the full `importJSON` contract (R12). | A migration can introduce a second writer or save a synthetic/nested view over the outer scene while both representations look valid in memory. | Name the authoritative aggregate in scene, component, and montage contexts; the sole writer at each transition; when aliases are permitted; cutover and rollback behavior; and how concurrent autosave observes an in-progress context change. | Load one document containing an outer scene and one component, enter the component, change one recorded value, trigger `exportJSON`, and compare the result with an independently authored expected JSON projection: exactly the component changes, the outer scene does not, and exiting/re-entering shows the same value. |
| Mutation and history ownership | `layersSnapshotNow` copies layers plus selected project registries, while `restoreLayersSnapshot` rebuilds Paper layers and rebinds symbol aliases and selection ([`tweens.js` 4877-4989](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/tweens.js#L4877-L4989)). R03's `pen-stroke` fixture, which declared an undo result, has no successor at `6d49921`; the bench now times the production `_cloneLayersForUndo` on generated workload documents. Neither exercises Nemo's undo/redo through a real user gesture. | New persistent state can miss history, transient state can leak into it, and one user gesture can split into several entries without any fixture detecting the loss. | For every aggregate, classify history-owned, persistence-owned, transient, and derived fields; define transaction boundaries and selection/active-context restoration; name who invalidates derived caches. | Start from a fixture with explicit pre/post contract projections, perform one mutation through the real writer, then assert `post -> undo == pre -> redo == post`, including IDs, references, order, active context, and exactly one history entry. |
| Layer identity and cross-layer references | New layers receive a generated `layerUid` ([`app.js` 377-386](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/app.js#L377-L386)); older layers receive one lazily ([`motion.js` 3551-3563](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3551-L3563)). Parenting and mattes both resolve by UID, but by different routes and only mattes cross the wire as indices. Parenting stays entirely in JS: `parentChainMats` walks uid to index to composed ancestor matrices and only the resulting transform is sent ([`motion.js` 3970-3992](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L3970-L3992), consumed at [`engine-bridge.js` 971-977](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L971-L977)). Matte references — and, by the same rule, folder children — are converted in a final pass, after every `unshift`/`splice`, into positions in the final wire array ([`engine-bridge.js` 2544-2591](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L2544-L2591)). Four further layer-UID references (`effectsFrom`, `timeLink`, `followPath.targetLayerUid`, `effector.targetLayerUid`), `mediaLibrary[].layerUid`, and uid literals inside expression code are also persisted, governed by four different unwritten copy policies (see the section above). R03 `masks-alpha` records the UID relation in a `document` check ("matte references its source by uid"); its six `pixel` checks are R13-gated and do not run. | Reorder, duplicate, import, or migration can silently retarget a parent/matte, create collisions, or choose inconsistent behavior for dangling references. | Define the uniqueness scope and generator for `layerUid`; one duplicate/remap/drop rule per UID reference across all four copy paths (duplicate, split, import, transplant), including the references transplant currently cannot carry and the uid literals inside expression code; legacy allocation order; collision handling; and reject, preserve, or degrade behavior for dangling/self references, replacing the four divergent answers now in source. | Use three layers with independently named UIDs carrying one parent edge, one matte edge, and one of each remaining reference kind (`effectsFrom`, `timeLink`, `followPath`, `effector`, an expression `layer(uid)` read). Reorder, duplicate the referenced subgraph, transplant it into a second document, undo/redo, save/load, and assert the exact expected UID graph after each path, plus a pixel probe on the live engine showing the matte still reads the intended source. The pixel half of this probe is an app/WebGPU capture, not a decode of a committed reference PNG. |
| Element, group, and project-subgraph identity | `strokeId` is lazily generated on live Paper items ([`tools.js` 1910-1914](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/tools.js#L1910-L1914)); vector text uses a generated shared `groupId` and makes the first glyph its root ([`vector-text-bridge.js` 260-344](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/vector-text-bridge.js#L260-L344)); mesh topology is project-owned under `imageMeshes[meshId]` and duplication allocates a new entry ([`image-mesh.js` 25-30](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/image-mesh.js#L25-L30), [`image-mesh.js` 534-545](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/image-mesh.js#L534-L545)). R03 `text` now persists `groupId` with the first glyph as root and checks membership against production `vector-text-bridge.js` grouping, but its glyph outlines are boxes standing in for font geometry and its rendered placement is R13-gated; `mesh` checks topology invariants and `meshId` propagation but does not duplicate or round-trip. | Code currently relies on different, implicit scopes. A blanket "preserve IDs" or "always mint IDs" rule would break tween correspondence, text grouping, or mesh copy isolation. | Define scope, stability, collision policy, and clone semantics separately for `strokeId`, `groupId`, `meshId`, symbol/control IDs, and references; specify which duplicate operations preserve, remap, or intentionally share each identity. | Duplicate one vector-text group and one meshed raster. Assert the contract's exact ID mapping, edit the duplicate's glyph and one mesh vertex, and prove the source remains unchanged after undo/redo and save/load. |
| Selection and other derived identity | After document restoration every Paper object is new; `refreshAfterDocumentRestore` reconstructs `selectedPaths` from persisted child indexes and clears gesture-local object references ([`select-bridge.js` 3569-3580](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/select-bridge.js#L3569-L3580)). R03 reports 398 rows that may reach selection, and the corpus's `interaction` fixture records a pointer gesture, but its only Node check is schema/bounds consistency; replay through the real pointer handlers is R12-gated (`backend: browser`, `requiredCapabilities: ["browser-harness"]`). | Index-based selection can follow the wrong element after reorder or reconstruction, while persisting raw object references is impossible. | Decide whether selection identity is transient or document state, which stable key rebinds it, whether active layer/anchor belong in history, and what happens when the selected target no longer exists. | Select a known element, reorder its siblings, perform undo/redo and save/load, and assert the selected stable identity after each rebuild; then delete that target and assert the chosen empty/fallback state. |
| Frame, time, and duration semantics | Motion tracks store frame numbers and evaluate hold/interpolation in `rawValueAtFrame` ([`motion.js` 1298-1343](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/motion.js#L1298-L1343)); expressions expose both `frame` and `time = frame / fps`; the renderer sends seconds as `frame / fps` ([`engine-bridge.js` 2648-2655](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L2648-L2655)). Export uses `state.fps` for native ffmpeg and browser `MediaRecorder` timing ([`export.js` 409-435](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/export.js#L409-L435), [`export.js` 647-686](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/export.js#L647-L686)). R03 `keyed-props` and `components` now execute production `SMMotion.valueAtFrame`, `getEffectiveStrokes` and `resolveSymbolFrameIdx` in Node, and `expression-props` runs the shipped expression compiler; the export consumer still needs a runnable ffmpeg sidecar (5 `node-when-available` checks) and the renderer still needs WebGPU. | Fractional frames, endpoints, FPS changes, remap, component-local FPS, and rounding can disagree across preview, expressions, renderer, and encoded duration without failing structural tests. | Define the canonical time unit and numeric domain; inclusive/exclusive range endpoints; FPS-change/retime policy; rounding; component-to-scene conversion; and the exact preview/export duration rule. | At 12 fps, evaluate independently specified keys at frames 0, 3, 6, 9, and 11, render those frames, and export 12 frames. Assert values and pixels at those indices and exactly 1.0 s decoded duration; repeat only the conversion assertions at 24 fps to expose hidden constants. |
| Persistence, schema compatibility, and unknown data | `exportJSON` writes schema `version:13` through explicit top-level and layer allowlists ([`timeline.js` 2112-2139](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2112-L2139)); `importJSON` warns on a future version but still loads and defaults known fields ([`timeline.js` 2365-2398](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/timeline.js#L2365-L2398)). `serP` is another explicit per-item projection ([`app.js` 600-678](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/app.js#L600-L678)). R03 fixture documents follow these shapes by construction from `exportJSON`/`serP`/`desP` provenance; they are still not outputs of a save/load transaction, and the full `importJSON` contract is R12-gated. | A newer or plugin-owned field can be silently dropped on the next save. Adding it to one allowlist can still miss history, components, render, or export. | Decide version compatibility and migration ownership; reject/read-only/preserve/drop behavior for unknown fields at every nesting level; canonical serialization; defaults; and whether extensions have a namespace. | Add known canaries and unknown canaries at document, layer, frame, stroke, key, and registry levels. Run import -> save -> import -> save and compare with a hand-authored expected projection for the adopted policy, including the exact warning or refusal for a future version. |
| Image precision, color, and alpha | Path serialization rounds segment coordinates to three decimals ([`app.js` 590-603](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/app.js#L590-L603)); meshes use normalized float coordinates ([`image-mesh.js` 34-56](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/image-mesh.js#L34-L56)); `buildSceneJson` chooses real transparent pixels versus an editor checkerboard from render context ([`engine-bridge.js` 802-841](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/engine-bridge.js#L802-L841)). R03 `static-props`, `masks-alpha`, `media`, and `mesh` carry geometry and pixel expectations, but no pixel check in that suite runs: all 12 are R13-gated, and the corpus's one running pixel-shaped check decodes its own committed reference PNG rather than capturing an app or WebGPU render. | Preview, save/load, WebGPU, browser export, and desktop export may apply different precision, color-space, alpha, or premultiplication rules and still match structural JSON. | Define coordinate/mesh precision, stored color representation and color space, alpha convention, conversion/tolerance rules, and which preview-only overlays are excluded from output. | Render the same matte + raster + mesh fixture through live WebGPU, transparent PNG, and packaged export — three actual runtime captures, none of them satisfied by regenerating the fixture's reference PNGs. Sample independently chosen inside/outside/edge pixels, decode to one named color space, and compare within the adopted tolerance; also round-trip the geometry and assert its allowed error. |
| Platform capability and native-resource ownership | Project save/open uses browser download/upload or Tauri filesystem APIs ([`project.js` 115-220](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/project.js#L115-L220)); video export selects Tauri ffmpeg or browser `MediaRecorder`; native commands are separately registered in Rust ([`src-tauri/src/lib.rs` 174-205](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src-tauri/src/lib.rs#L174-L205)). Native video persists a path/handle descriptor while keeping the decode session transient ([`native-video-bridge.js` 1170-1205](https://github.com/mysteropodes/nemo/blob/a87eb54a33d225f77cff903809a0538f7e9d0179/src/js/native-video-bridge.js#L1170-L1205)). R03 records required capabilities per fixture (`export` declares `ffmpeg-sidecar` and `videotoolbox`; `masks-alpha`/`mesh`/`media` declare `webgpu`; `interaction` declares `browser-harness`); only one packaged export path has observed evidence. | A feature can appear inventoried on both platforms while lacking the required backend, persisting an unreopenable resource, or silently choosing a lower-fidelity fallback. | Define capability IDs and availability states; authoritative JS/Rust command schemas; persistent resource descriptors versus process-local handles; fallback/refusal behavior; and required OS/architecture evidence. | Run one capability table against browser and packaged desktop: save/open, linked media reopen, and one export. Assert the exact available/unavailable result, artifact identity, and state preservation. Restart before reopen so no process-local handle can satisfy the probe accidentally. |
| Generated boundary artifacts and drift | The R03 candidate emits surfaces, consumers, platforms, and fixture joins, and at #946's head the fixture manifest is itself an input of the inventory's staleness gate, while the integrated R05 checker accepts only paths explicitly declared in a profile. The R05 application candidate shows that surface reachability is not a dependency graph and that most central JS by volume is outside its profile ([classification limits lines 313-340](https://github.com/mysteropodes/nemo/blob/33e248795a0b94b4c93dde210c1d29f3fdab5039/engineering/boundaries/profiles/app-surfaces.md#L313-L340)). No generated Rust/JS/schema contract binds the `window.SM*` calls, Tauri commands, document fields, and test fixtures together. | A surface can stay `inventoried` while its actual owner, native port, schema field, or consumer changes. Separate stale checks can all pass over mutually inconsistent subsets. | Select the normative contract source; assign version and owner; define generated JS/Rust/schema outputs; map each applicable consumer and platform; define excluded-with-reason; and make protected-base versus candidate provenance explicit. | For one narrow capability, generate both sides and its fixture manifest from one pinned contract. Deliberately change a field name, native command argument, consumer applicability, and platform state one at a time; the standard check must fail each mismatch and pass only after every generated artifact is refreshed. |

## Minimum packet before adoption

An R09 contract packet is reviewable only when it names the authoritative writer,
identifier scopes, persistent/transient/derived projection, time units, compatibility
and unknown-data policy, image/color/alpha semantics, applicable platforms, and the
generated-boundary source. It must bind those choices to accepted R03 fixtures and
an accepted R05 application boundary, then run the relevant mutation and downstream
consumer probes from the cross-contract matrix against the same pinned fixture.

Until those decisions and probes exist, the current source links establish where
behavior lives and the candidate inventories establish useful seeds. They do not
make the behavior normative.
