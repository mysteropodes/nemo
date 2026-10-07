# Frozen remediation source index

[P03A / #1116](https://github.com/mysteropodes/nemo/issues/1116) built the mechanical index
for [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005),
[P03B / #1170](https://github.com/mysteropodes/nemo/issues/1170) refroze it with the
supplemental censuses and the first reviewed dispositions, and
[P03C-a / #1179](https://github.com/mysteropodes/nemo/issues/1179) pinned the C19r supplement
and reconciled the C02/C06/C20 packet-range drift the earlier freezes had left open, and
[P03C-b / #1278](https://github.com/mysteropodes/nemo/issues/1278) refroze the index at the
post-census `main` so the 50 census supplements merged after P03C-a (C19s–C19al, C20g–C20s,
C21a–C21q) are pinned and the residual gap is published from facts, and
[P03C-c / #1362](https://github.com/mysteropodes/nemo/issues/1362) reconciled C02:10 to the
eleven accepted C20g–C20i census packets without admitting or implementing them. None of
these leaves completes P03 or admits the extraction queue. P03C-d / #1401 reconciles C08:1
against the accepted, pinned C19 timeline census series; it changes no packet admission or
frozen source identity. The rest of P03C remains open.

[remediation-scope.json](remediation-scope.json) (schema `nemo.remediation-scope/2`) freezes
all **756 Git-tracked paths** at `3f6eed2a500f2ce868b711e063816029eb8fefa5`. Every path has one
classification, Git mode/blob identity, a reason, and the packet references that name it.
Multiple references preserve overlapping census evidence; they do not authorize concurrent
writers or settle range ownership.

The sorted source-set SHA-256 hashes UTF-8 records `mode + " " + blob + " " + path + "\n"`.

## Amendments, not rolling baselines

`amendments` records every freeze in order. The first entry is P03A's freeze of 657 paths at
`59a5a38c…`; the second is P03B's refreeze at `1bfcbad…` with the exact delta (**42 added**,
**0 removed**, **20 modified** paths); the third is P03C-a's refreeze at `4bbe332…` with the
exact delta (**4 added**, **0 removed**, **10 modified** paths — the C19r census file plus the
checker's own tooling/doc drift merged upstream since P03B, and this leaf's own C02/C06/index
edits); the fourth is P03C-b's refreeze at `3f6eed2…` with the exact delta (**53 added**,
**0 removed**, **7 modified** paths — the 50 census supplement files, the
Rust boundary tooling (`geometry-wasm.edges.json`, `boundaries-rust.cjs` and its test) merged
upstream since P03C-a, and the index/doc/gate files those leaves touched; no packet range, owner
or disposition changed). The checker recomputes the last delta from the two Git
trees, so the digest can only move by an explicit amendment that names its leaf and reason.
Comparing any other commit with `--source` still fails on drift, including a candidate's own
new files; that is intentional.

`node scripts/nemo/remediation-scope.cjs --refreeze FULL_SHA --id LEAF --issue N --reason TEXT`
produces the next amendment. It recomputes the mechanical fields (identities, census pins,
packets, references from newly pinned censuses, range coverage) and keeps every reviewed
field (classification, reason, coverage, packet admissions, gap and span dispositions).
Paths that are new since the previous freeze are classified by shape; vendor/generated
shapes are refused and must be classified by hand.

## Censuses, packets and references

All **89 census files** present at the frozen commit are pinned by blob and keep their own
`source_sha` — the 14 original partitions plus C19a–C19al (timeline, 38 files including C19b2),
C20a–C20s (Motion, 19) and C21a–C21q (app.js, 17). An unpinned census file in the tree fails
integrity. The pins yield **762 packets**: 329 from the original partitions (C08 keeps only
its documentation map) and 433 from the supplements (220 C19, 78 C20, 135 C21;
P03C-a had 141, P03C-b added 292 from the 50 newly pinned files).

`censusRefs` on a path are P03A's reviewed references plus the mechanical references of
censuses pinned for the first time or re-pinned with a different blob. Declarations are parsed as written (`path`, `path:12-46`,
`path:20,25-30 (symbols)`, `path: symbol list`, comma lists); glob, brace and directory
declarations match no tracked path literally and add nothing.

## Range coverage

`rangeCoverage` is recomputed by the checker from the census declarations and cannot be edited
by hand. For each file that only has line-range declarations, ranges written against an older
census SHA are mapped onto the frozen tree through `git diff --unified=0` hunks (deleted or
rewritten lines drop out), then every line with no packet is reported. Spans made only of
blank lines or closing brackets are dropped; `uncovered` counts code lines inside the kept
spans; `overlap` counts lines claimed by more than one packet.

At `3f6eed2…` (P03C-b), **61 files** carry range declarations, **33** have gaps, and
**7 185 code lines in 151 spans** have no packet — down from 21 153 lines in 166 spans at
P03C-a, because the 50 pinned supplements closed `app.js` entirely (C21a–C21q; the 1-40 and
314-359 lines that C21a left to C06 are C06's declared bootstrap/profile ranges, so `app.js`
now reports 0 uncovered lines and 0 spans), the `timeline.js` tail from 5710 (C19s–C19al) and
the large `motion.js` spans (C20g–C20s). This is the honest size of the remaining census work
and the input of the next P03C leaf:

| File | Lines | Uncovered code lines | Spans |
|---|---:|---:|---:|
| `src/css/style.css` | 2 850 | 2 707 | 4 |
| `src/index.html` | 2 485 | 2 250 | 4 |
| `src/js/tweens.js` | 5 249 | 516 | 13 |
| `src/js/motion.js` | 13 914 | 293 | 22 |
| `src/js/engine-bridge.js` | 4 678 | 226 | 17 |
| `src/js/export.js` | 1 389 | 186 | 8 |
| `src-tauri/src/lib.rs` | 284 | 151 | 4 (1-13, 49-89, 131-137, 178-284) |
| `nemo-mcp/src/contract.rs` | 340 | 134 | 23 |
| `geometry-wasm/src/track.rs` | 432 | 88 | 2 |
| `src/js/layer-inout.js` | 1 660 | 74 | 3 |
| `src/js/project.js` | 743 | 70 | 9 |
| `geometry-wasm/src/fill.rs` | 1 037 | 67 | 2 |

The remaining spans fall in four groups. (1) `style.css` (2 707) and `index.html`
(2 250) are presentation files awaiting Ilya's census-versus-`boundary` decision. (2) P03C-g/#1424
dispositioned the four `timeline.js` spans: 2002-2011 maps to C02 playback; 2027-2048 maps to
C04a transform/brush geometry; 2247 maps to the C01 team-sync facade and C21d merge-caller
chain; 7449 is the keyboard section comment adjoining C06. The three `covered` entries record
responsibility ownership only: they add no packet or computed-range coverage and assert no
implementation acceptance. P03C-h/#1427 dispositioned four `motion.js` comment/blank preambles
as `boundary`: 1562-1566 before the C02 expression helpers, 8435-8438 before the C02 time-link
cycle helper, 9429-9435 before the C02 Elements helper, and 13256-13321 before initGridMarquee
at 13322, whose registration is assigned to C02 and dispatches to C20q key-marquee behavior.
P03C-i/#1430 dispositioned nine expression-runtime spans: eight `covered` responsibility
mappings for the named C02 expression wrappers (1571, 1578, 1712, 1719, 1724, 1743-1745,
1752-1753 and 1759), plus the comment-only Box-Muller lead-in at 1748-1750 as `boundary`.
The eight mappings remain pending C02 packet responsibilities; none adds computed range
coverage or implementation acceptance. P03C-j/#1433 maps `keysLockedTo` at 8530-8553 to the
pending C02 key-selection responsibility with C20c evidence, and the `selectKeys`/curve-widget
bridge at 13854-13879 jointly to pending C02 key-selection and curves/easing responsibilities
with C20o evidence. These are responsibility mappings only. The remaining seven `motion.js`
spans (561-580, 13351-13352, 13372-13394, 13455-13476, 13565-13621, 13808-13825, 13896-13908)
are gaps between adjacent census slices —
C20s's own boundary note names 12406-12684 and 13125+ as the two gaps it leaves unassigned;
the refrozen coverage shows 12406-12684 fully covered by the original C02 packets, while 13125
onward keeps the seven open spans listed above — candidates for one reconciliation leaf.
P03C-k/#1436 dispositioned five frozen `tweens.js` spans: 411-415 is a comment-only `boundary`,
and 472, 564, 630 and 1020 map to still-pending C02 matching/resampling responsibilities.
Accepted P23/#1025 (PR #1166) identifies only the extracted assignment solver; these mappings
do not assert C02 acceptance, matcher/warp parity or numeric coverage. The 516 Tweens and 7 185
global uncovered code lines remain unchanged. (3) `tweens.js`, `engine-bridge.js`, `export.js`, `lib.rs`, `contract.rs` and 24 further
files carry small remainders (module headers, export blocks, trailing helpers) left by the
original partitions. P03C-f/#1421 marks six frozen geometry-WASM module preambles as
`boundary`: the comment/import lines before C03/#1038 packet starts at eraser.rs:10, fill.rs:34,
shapes.rs:8, strokemodeler.rs:14, track.rs:22 and tweenmatch.rs:15. This accounts for six spans
without changing the computed 7 185 uncovered code lines, frozen source or packet state. P03C-l/#1438
maps project.js:11-19 and the getOpenTabs API in 412-432 to pending C01 document-I/O/project-tab
responsibilities, while marking six comment/blank preambles (21-24, 51-53, 220-227, 311-321,
486-497 and 634-635) as boundaries before their adjacent C01 owners. The executable module opener
1-9 stays unresolved; these are responsibility/boundary records only, not packet admission or
implementation acceptance. Project.js's 70 computed uncovered code lines and global 7 185 are unchanged.
P03C-m/#1440 dispositioned eleven frozen `nemo-mcp/src/contract.rs` spans: ten `covered`
responsibility mappings to pending C07 Operation/wire-contract responsibilities and the
comment-only 205-207 `boundary`. In particular, 131-146 is the plural `Operation::labels()` helper,
which the older C07 symbol list does not name (it names singular `Operation::label`); accepted
P07/#1009 source commit `bd907d230` / PR #1113 is cited only as the added-helper provenance. C07
remains pending; contract.rs's 134 and global 7 185 computed uncovered code lines are unchanged.
P03C-n/#1443 dispositioned the twelve remaining frozen `nemo-mcp/src/contract.rs` spans: eleven
`covered` mappings and the comment-only 37-39 `boundary`. The mappings reference C07's broader,
still-pending request/response contract, not packet acceptance or numeric source coverage. C07's
older symbol list does not name `RequestError` or the test-only module registration; P07/#1009
commits `35d7eac66`, `bd907d230` and `5aa8026ef` provide the later typed-error/import/test-wiring
provenance. The 337-340 span is specifically `cfg(test)` / `contract_tests.rs` / `mod tests`, not
`NativeApplicationRequest`. contract.rs's 134 and global 7 185 computed uncovered code lines remain
unchanged.
(4)
The 40 previously dispositioned executable paths without a packet
(`leaf`/`oracle`) are unchanged; **P03C-e/#1419 dispositioned six additional paths**, bringing
the total to 46 and the unmapped count to zero.
The three Rust boundary tooling paths (`geometry-wasm.edges.json`, `boundaries-rust.cjs` and its
test) cite P12/#1014, merged as PR #1176: adopted crate boundary policy, a Rust-aware analyzer,
and positive/negative scanner controls. The three inventory checker modules
(`remediation-scope-build.cjs`, `remediation-scope-census.cjs` and
`remediation-scope-verify.cjs`) cite P03B/#1170, merged as PR #1175: index build/refreeze,
census declaration/range mapping, and integrity/disposition/completeness verification. The
P03C-e inventory disposition records these path roles and provenance; it does not itself prove
native runtime feature acceptance.

Overlaps are unchanged by this refreeze — **1 832 lines** in total, identical to P03C-a:
`shader-effects-library.js` (1 311, claimed twice), `motion.js` (190, C02-internal), `tweens.js`
(158, C01 versus C02), `timeline.js` (86, one C02 packet subsumed by C19q), plus 87 lines across
`image-mesh-bridge.js`, `feedback-bridge.js`, `index.html`, `tools.js`, `layer-inout.js`,
`engine-bridge.js`. The C02-versus-supplement overlap that P03C-a narrowed to 0 stays at 0: none
of the 50 newly pinned files overlaps a C02 range. Narrowing the residual overlaps is
reconciliation work for the next P03C leaf, not this index-only refreeze.

## Dispositions with evidence

Every reviewed claim carries its evidence and is validated by shape:

- **Packets** — `pending` (762), `covered` (leaf + issue + merged PR + evidence), `admitted`
  (issue) or `deferred` (reason). No delivered leaf matched an entire packet: P20, P21, P22, P23,
  P26, P28, A01, P17 and P18 each extracted part of a larger packet, so all 762 stay pending
  until P03C splits them (the 292 packets pinned by P03C-b enter as `pending`).
- **Uncovered-responsibility notes** — 35 verbatim notes: **16 `covered-by-packet`** (cite
  existing packets — P03C-a closed C20a:1, C20a:2 and C20b:1 by amending the C02 packet ranges
  they pointed at; P03C-d reconciled C08:1 to pinned C19 timeline packets), 3 `resolved`
  (measured), 14 `boundary` (scope statements, cross-checked against the spans), and 2
  `human-decision` (C03:3 unwired selection API, C08:3 `40min-checkins/`). No note remains
  `needs-reconciliation`. C08:1's original note is verbatim; its evidence now identifies the
  59a5a38c source pin and C19 packet ranges. P03C-g mapped the four `timeline.js` spans without
  changing its 27 uncovered code lines. P03C-h marked four `motion.js` comment/blank preambles
  as boundaries, P03C-i mapped eight C02 expression responsibilities plus one comment
  boundary, and P03C-j mapped two key-selection/curve-widget spans to pending C02
  responsibilities; P03C-k maps four frozen Tween solver call sites to pending C02 matching and
  resampling responsibilities while citing the narrower accepted P23 assignment extraction;
  P03C-l maps Project lifecycle declarations/API and six preambles to pending C01 owners, leaving
  opener 1-9 unresolved. The 70 Project, 293 Motion, 516 Tweens and global computed uncovered code
  totals are unchanged. P03C-m maps eleven contract.rs spans to pending C07 wire-contract ownership;
  the plural `Operation::labels()` helper is attributed to accepted P07/#1009 PR #1113 rather than
  misrepresented as a symbol named in the older C07 census. The 134 Contract and global computed
  uncovered line totals remain unchanged. P03C-c reconciled
  C02:10's exact 1,300-line Motion span to the eleven accepted C20g–C20i census packets; their
  admissions remain pending and this records census ownership only, not implementation.
- **Executable paths without a packet** — 46, all dispositioned: 38 `leaf` (module, test or
  gate data created by a merged leaf, with its PR) and 8 `oracle` (compiled MCP tests named by C07).
- **Spans** — `admitted` (issue), `covered` or `boundary`, each naming one reported span with
  evidence. P03C-f/#1421 dispositioned six preambles, P03C-g/#1424 dispositioned four timeline
  spans (three responsibility mappings and one comment boundary), P03C-h/#1427 dispositioned
  four Motion comment/blank preambles, P03C-i/#1430 dispositioned nine Motion expression spans,
  P03C-j/#1433 mapped two Motion key-selection/curve-widget spans, and P03C-k/#1436 dispositioned
  five Tween assignment-seam spans, P03C-l/#1438 dispositioned eight Project lifecycle spans,
  P03C-m/#1440 dispositioned eleven MCP contract spans, and P03C-ae/#1482 dispositioned three
  `engine-bridge.js` render-entry/boundary spans. P03C-af/#1484 dispositions four more
  `engine-bridge.js` spans; 33 spans remained undispositioned at that snapshot. P03C-ag/#1486
  dispositions the final two C03 module headers; 31 spans remained undispositioned at that
  snapshot. P03C-ah/#1489 dispositions the final six frozen `motion.js` spans; 25 remained
  at that snapshot. P03C-ai/#1491 dispositions six frozen `tweens.js` spans; 19 remained.
  P03C-aj/#1493 dispositions two frozen `linked-media.js` spans; 17 remain.

`complete: true` is accepted only when no packet is pending, no note needs reconciliation, no
executable path is undispositioned and every span is dispositioned. Today the gate exits 1
with that queue printed; integrity exits 0.

## Checks

From the repository root:

```sh
node scripts/nemo/remediation-scope.cjs --integrity-only
node --test tests/nemo-remediation-scope.test.cjs
node scripts/nemo/remediation-scope.cjs
node scripts/nemo/remediation-scope.cjs --integrity-only --source FULL_COMMIT_SHA
```

Integrity verifies sorted file coverage against `git ls-tree` at the frozen SHA, identities,
digest/counts, the amendment chain and last delta, census pins (all census files in the tree),
packet provenance, reference validity, disposition shapes and the recomputed range coverage.
It does not validate semantic classification, consumer coverage, source behavior, issue
ownership or extraction readiness. This is opt-in tooling; no hosted workflow runs it.

## Remaining admission work (P03C)

P03C-a/#1179 closed the five open notes and the C02/C06/C20 range drift (packet range
amendments, no new owners) and pinned C19r. P03C-b/#1278 pinned the 50 census supplements that
completed the `app.js`, `timeline.js` and `motion.js` censuses (C21a–C21q, C19s–C19al,
C20g–C20s) at `3f6eed2…` and published the residual gap above; it changed no packet range, owner
or disposition. P03C-c/#1362 reconciled only C02:10 to the eleven accepted C20g–C20i census
packets; all eleven packet admissions remain pending and no implementation is claimed.
P03C-d/#1401 reconciled C08:1 to the C19 census evidence without changing the frozen source,
packet admissions, or implementation state. P03C-f/#1421 records six geometry-WASM preamble
boundaries against C03/#1038's exact packet start lines. P03C-g/#1424 maps three timeline
responsibilities to existing C02/C04a/C01/C21d evidence and marks the adjacent keyboard header
as a boundary. P03C-h/#1427 marks four comment/blank Motion preambles as boundaries against
existing C02/C20 evidence. P03C-i/#1430 maps eight C02 expression wrappers and marks one comment
boundary, with C20k evidence. P03C-j/#1433 maps frozen `keysLockedTo` to pending C02 key-selection
with C20c evidence and the `selectKeys`/curve-widget bridge jointly to pending C02 key-selection
and curves/easing with C20o evidence. Packet admissions and computed range coverage remain
unchanged. P03C-k/#1436 records the frozen Tween assignment call sites against pending C02
matching/resampling responsibilities, while P23/#1025 (PR #1166) remains limited to its accepted
Hungarian solver extraction; 516 Tweens and 7 185 global computed uncovered code lines are
unchanged. P03C-l/#1438 maps frozen project.js declarations and getOpenTabs to pending C01
document-I/O/project-tabs responsibilities, with six comment/blank preambles marked as boundaries
to adjacent C01 owners; executable opener 1-9 remains unresolved. This changes no coverage or
packet admission, and project.js stays at 70 computed uncovered code lines. P03C-m/#1440 maps
eleven frozen contract.rs spans to pending C07 responsibilities; C07's older symbol list names
singular `Operation::label`, while accepted P07/#1009 PR #1113 is cited only for the added plural
`Operation::labels()` helper. C07 remains pending, and contract.rs's 134 computed uncovered code
lines are unchanged. P03C-o/#1446 dispositioned ten frozen `engine-bridge.js` image-budget spans:
eight responsibility mappings to pending C03 image-store, scene-build and public-API packets, and
comment-only boundaries at 640-643 and 2621. Accepted P26/#1028 PR #1168 provides narrower
image-budget policy provenance, not C03 completion or runtime/pixel acceptance. The 226 Engine
Bridge and global 7 185 computed uncovered code lines are unchanged. P03C-p/#1448 maps six frozen
`nemo-mcp/src/server.rs` spans to the pending C07 tool-router responsibility. C07's census pins
`server.rs:1-177` at source `ded641bd763379f36d629bf1686fcce8a6137a66`; the exact source diff maps its
command-validation lines 169-170 to frozen 179-180. P07/#1009 PR #1113 is cited narrowly for
registered-capability discovery provenance, not whole-packet or runtime/protocol acceptance.
The computed uncovered counts (18 in server.rs; 7 185 globally) remain unchanged. P03C-q/#1451
maps three frozen `src/js/export.js` spans to pending C05 frame-compositor and Tauri-I/O
responsibilities. The C05 census at `419f1926` names the source ranges; accepted P17/#1019 PR #1160
and P18/#1020 PR #1163 provide narrow SVG-wrapper and job-boundary provenance only, not full
packet, export-format, runtime or parity acceptance. The 186 computed uncovered lines in export.js
and global 7 185 remain unchanged. P03C-r/#1454 maps the frozen `geometry-wasm/src/track.rs:345-432`
test-module tail to the pending C03 pyramidal point-tracker packet's test-oracle responsibility.
The census oracle is specifically the four tests at 388-429; this disposition does not claim the
whole module is covered, test correctness, or packet acceptance. The tracker and global computed
uncovered counts remain 88 and 7 185. P03C-s/#1456 marks frozen `src/js/export.js:1161-1174` as a
boundary: line 1161 is blank and lines 1162-1174 are Render Manager batch-export lead-in comments;
the first executable begins at 1175. The comments describe adjacent pending C05 Tauri-I/O batch
export responsibility but implement none. The export.js uncovered count remains 186 and global
7 185. P03C-t/#1458 dispositions six frozen `src/js/select-bridge.js` seams against pending C04a
selection packets: comment-only boundaries at 271-279 (combine hit confirmation), 759-763
(motion-arc hit test), 2905-2914 (context menu) and 3293-3308 (per-element tween toggle), plus
covered responsibility mappings at 461-464 (`ROTATE_CURSOR` at 463) and 772-773 (`lastPt` at 773).
The executable opener 1-23 remains unresolved. C04a packet admissions, selection behavior parity
and numeric coverage are unchanged; select-bridge.js retains 64 and the global report 7 185
computed uncovered code lines. P03C-u/#1461 maps frozen `src/js/motion.js:13896-13908` to the
pending C02 key-selection/clipboard responsibility: executable exports at 13896-13901 name
`nudgeSelectedKeys`, `deleteSelectedKeys`, `copySelectedKeys`, `pasteKeys`, `hasKeySelection` and
`hasKeyClipboard`; 13902 closes the export object. Lines 13903-13908 are comment-only lead-in for
the separate workspace-continuity packet, whose `restoreLastAppMode` starts at 13909 outside this
span. This is a responsibility mapping only; C02 remains pending and numeric coverage is unchanged.
P03C-v/#1462 maps frozen `src/js/layer-inout.js` tail spans 1601-1646 and 1648-1660 to pending
C02 bar-rendering, selection-and-marquee and drag-and-batch-ops responsibilities. The first span
records Alt-reveal wiring (1601-1606), empty-grid marquee registration (1620-1626), exported bar
API (1629), drag/batch API (1633-1634) and marquee/selection API (1640-1641); 1642-1646 are only
comments. In the second, 1648-1653 are the `retimeLayers` body/close under the declaration at 1647
outside the span; getter/setter bar-selection exports are at 1654/1658 with comment-only anchor
notes at 1655-1657. The opener 1-22 remains unresolved. These are responsibility mappings only:
C02 remains pending and layer-inout/global computed uncovered counts remain 74/7 185.
P03C-w/#1464 marks frozen `geometry-wasm/src/hit.rs:1-8` as a boundary: all eight lines
are explanatory comments; the executable import and pending C03 hit-test responsibility start
at line 9, outside the span. This does not admit C03 or change numeric coverage.
P03C-x/#1466 maps frozen `src/js/tweens.js:4860` (folder-map snapshot field) and `:4925`
(guarded folder-map restore field) to pending `C01.tweens.undo-redo-engine`. Adjacent folder
comments and distinct link-group fields are outside these singleton spans. This is responsibility
mapping only; C01 remains pending and tweens.js/global uncovered counts remain 516/7 185.
P03C-y/#1468 marks frozen `geometry-wasm/src/tween.rs:1-16` as a structural preamble/import
boundary before pending `C03.wasm-geometry-ops.tween-interpolation-math` at line 17: lines 1-13
are comments, 14-15 are executable imports, and 16 is blank. It does not extend the boundary into
the packet or accept it; tween.rs/global computed uncovered counts remain 15/7 185.
P03C-z/#1470 marks frozen `src/js/playback-cache.js:1-19` as a comment-only boundary: the
descriptive playback-bake header ends before the executable module IIFE and pending
`C03.js-effects-and-playback.playback-bake-cache` packet begins at line 20. It claims no covered
playback behavior or packet acceptance; playback-cache.js/global uncovered counts remain 19/7 185.
P03C-aa/#1474 maps frozen `src/js/export.js:369-376` (`exportFrameRange`) to pending
`C05.export.frame-compositor`, and `570-598` (`exportRenderPNGsToDir`) to pending
`C05.export.tauri-io-ffmpeg`, with its separate engine-routing branch accounted for by the
pending routing-gate packet. It marks `873-883` as the Lottie comment boundary before
`lottieHexToRGBA` at 884, then maps line 1255 (blank), 1256 (section heading) and the
`window.SMExport` facade at 1257-1389 to the applicable pending C05 frame-compositor, Tauri-I/O,
browser fallback and Lottie packets; the Rive exporter separately augments that facade from
`rive-export.js`. Accepted P17/#1019 and P18/#1020 remain narrow wrapper/job provenance only.
These are responsibility and boundary records, not packet admission, numeric coverage or
export/runtime parity acceptance. The frozen `export.js` uncovered count remains 186 and the
global count remains 7 185. P03C-ab/#1476 maps frozen `src/js/bootstrap/opacity-application.js`
spans 30, 40-41 and 44-54 to the still-pending `C07.application-service.bootstrap-bindings`:
line 30 reuses or creates the capability registry; 40-41 supply the `afterMutation` and
`capabilities` ports; 44-48 are P06 comments describing opacity self-registration and per-request
registry dispatch; 49 registers `NemoOpacityCapability`; 50 opens `root.NemoApplication`; 51
dispatches by the opacity descriptor ID; 52-53 expose `setInstanceId` and `capabilities`; and 54
closes the object. P05 registry and P06 opacity-registration provenance do not accept C07.
These are responsibility mappings only: the bootstrap file's 13 uncovered code lines, 762
pending packets and global 7 185 computed uncovered code lines are unchanged.
P03C-ac/#1478 maps four frozen C03 spans to pending census responsibilities: `engine.rs:1056-1059`
to `C03.gpu-engine-core.effect-pipeline-library`, `engine.rs:2741-2771` to
`C03.gpu-engine-core.composite-scene-pipeline`, the explicitly named `circle_lens_tests` oracle
at `fill.rs:1000-1037` to `C03.wasm-tween-matching.fill-region-tracer`, and `interp.rs:1-33`
(including its imports and `FLOW_MAX_SIDE`) to `C03.wasm-geometry-ops.motion-flow-interp`.
These are responsibility mappings only: all four packets remain pending, and no production
migration, packet admission or numeric coverage is claimed. The 48 previously undispositioned
spans become 44; all 762 packets remain pending, computed uncovered lines remain 7 185, and the
scope remains incomplete.
P03C-ad/#1480 maps frozen `src-tauri/src/lib.rs:1-13` to the pending C05 composition-root
responsibility and `178-284` to `run()`'s plugin, command, menu and setup wiring. The `run()`
call to `start_tablet_pressure_monitor` does not map the monitor implementation. Lines 49-89
are a boundary because they mix historical comments about removed feedback-token routing with
Google-font command rationale; they are not assigned to one packet. Lines 131-137 are the
comment/blank preamble before the tablet-pressure monitor function at 138. These records are
responsibility/boundary mappings only: C05 remains pending, no implementation or runtime
acceptance is claimed, the frozen `lib.rs` computed uncovered code lines remain 151, and global
computed uncovered code lines remain 7 185. The 44 previously undispositioned spans become 40;
all 762 packets remain pending and the scope remains incomplete.
P03C-ae/#1482 maps frozen `src/js/engine-bridge.js:4257-4274` (renderNow preamble and
`viewportRafId`) and `4355-4381` (effects-export rationale and saved render state) to the still-
pending `C03.js-engine-bridge.render-entry-points` responsibility. It marks `4482-4492` as the
comment-only second-viewer boundary before the public `window.SMEngineBridge` API at 4493. These
records cite frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5` and C03 census blob
`337c9035f94f5290b2adf898d406550558560bfa`; they add no packet admission, numeric range
coverage or runtime/render/export acceptance. The engine-bridge uncovered count remains 226 and
global computed uncovered code lines remain 7 185. The 40 previously undispositioned spans become
37; all 762 packets remain pending and the scope remains incomplete. P03C-af/#1484 dispositions
the remaining frozen `src/js/engine-bridge.js` spans: `1-10`, `260`, and `4618-4628` are
comment-only boundaries (the last immediately precedes `autoEnable` at 4629); `91-224` maps
jointly to pending `C03.js-engine-bridge.render-entry-points` and `engine-lifecycle` for suspended
render scheduling, `build-scene-json` and `editor-overlay-builders` for color/coordinate rounding
and scene/overlay serialization, `bounded-image-store` for `registeredImageIds`, and
`retained-path-store` for its rationale. This explicitly mixed span is not assigned wholesale to
one packet. The frozen source and C03 census pins remain unchanged; all six cited packets remain
pending, and no packet admission, computed coverage, or runtime/render acceptance is claimed.
Engine-bridge retains 226 computed uncovered code lines but has zero undispositioned spans; global
computed uncovered code remains 7 185. The 37 previously undispositioned spans become 33; all 762
packets remain pending and the scope remains incomplete.
P03C-ag/#1486 marks frozen `geometry-wasm/src/timeline.rs:1-13` as a structural boundary:
lines 1-10 document frame-resolution provenance, 11-12 import `serde`/`wasm_bindgen`, and 13 is
blank; pending `C03.wasm-geometry-ops.timeline-frame-resolution` starts its implementation at 14.
It marks frozen `src/js/render-manager.js:1-33` as a comment-only boundary before the batch-queue
IIFE starts at 34 under pending `C03.js-effects-and-playback.render-manager-queue`. Both records
cite source `3f6eed2a500f2ce868b711e063816029eb8fefa5` and pinned C03 census blob
`337c9035f94f5290b2adf898d406550558560bfa`. The two spans add no separate responsibility,
packet admission, numeric coverage, or native/browser/Tauri/render/export acceptance. The 33
previously undispositioned spans become 31, with zero C03 spans remaining; all 762 packets stay
pending and global computed uncovered code remains 7 185.
P03C-ah/#1489 dispositions all six remaining frozen `src/js/motion.js` spans. `561-580` maps
the time-link reload helper and `PROP_DIM_LABELS` to pending C02 property metadata while its
duplicator comments lead into the separate C02 duplicator packet at 581. `13351-13352` is
the structural `window.SMMotion` API opening. The four interleaved export spans
`13372-13394`, `13455-13476`, `13565-13621`, and `13808-13825` map their exact operations
to pending C02/C20 expression, property, transform, selection, Motion UI, text animator,
element-style and key-writing packets as recorded beside each span in the JSON index. These
responsibility mappings cite frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5`,
motion blob `6617bac29042c22399cac202bf054c09e4fdf30e`, and pinned C02/C20 census
packets. Historical C02 numeric-range drift is left untouched. No packet is admitted and no
code, browser, Tauri, native, render or export behavior is accepted. The 31 previously
undispositioned spans become 25, with zero `motion.js` spans remaining; all 762 packets stay
pending, global computed uncovered code remains 7 185, and frozen `motion.js` retains 293
computed uncovered code lines and 190 overlapping lines.
P03C-ai/#1491 dispositions all six remaining frozen `src/js/tweens.js` spans. `1-103` maps
the feature flags to the pending C02 matching, resampling and interpolation packets by their
actual read sites; notably `TW_CORRECTION_PASS` is read in `_applyFoldCorrection` under the
resampling packet. The mixed `1564-1958` span maps UID-keyed easing, shared scalar `lerp`,
persistent tween-arc handles/rekey, style and intrinsic interpolation, crossing probes and
MLS preparation to separate C02 generation, arc, interpolation and resampling
responsibilities using frozen `generateTweens`, `renderArcs` and `interpStroke` callers.
`3735-3742` maps keyframe stroke-ID deduplication used by tween generation. `4810-4811` and
`4977-4979` map the frame-only capture/apply seams to pending C01 undo/redo, with P21's
extracted entry as context only. `4992-5016` splits the redo branch from manual reassignment
lead-in and `_reassign` state. The JSON records the exact subrange evidence against frozen
source `3f6eed2a500f2ce868b711e063816029eb8fefa5`, tweens blob
`6eca7339bbd608b98676b600c9e2f42ca9ce1863`, and pinned C01/C02 censuses. Their
older numeric-range drift is unchanged. These responsibility mappings admit no packet and
accept no code, browser, Tauri, native, render, export or history behavior. The 25 previously
undispositioned spans become 19, with zero `tweens.js` spans remaining; all 762 packets stay
pending, global computed uncovered code remains 7 185, and frozen `tweens.js` retains 516
computed uncovered code lines and 158 overlapping lines.
P03C-aj/#1493 dispositions the two remaining frozen `src/js/linked-media.js` spans. The
`1-39` comment-only boundary describes the media-mode, linked-reference and persistence/cache
contract before executable code begins at 40; its historical claims do not establish current
behavior. The mixed `751-774` public facade maps resolver/cache/handle/relink exports,
including `readLinkedDesktop` at 758, to pending `C05.media.linked-resolve-core`; conversion
exports to pending `C05.media.linked-bulk-convert`; and `syncUI` to pending
`C05.media.mode-setting-ui`. Frozen `app.js`, `images.js`, `media-library.js`,
`native-video-bridge.js` and `timeline.js` callers support that split. The old C05 census
does not list `readLinkedDesktop` and its approximate numeric ranges drift; its blob pin
`725d7875da509cd99a77bc404fc62296280399a0` remains unchanged. The JSON records
the precise evidence against frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5`
and linked-media blob `ba81900d0ea1805fc206fc19ce992674522de6c5`. No packet is
admitted and no linked-media, browser, Tauri, render, export or persistence behavior is
accepted. The 19 previously undispositioned spans become 17, with zero `linked-media.js`
spans remaining; all 762 packets stay pending, global computed uncovered code remains 7 185,
and frozen `linked-media.js` retains 61 computed uncovered code lines.
P03C-ak/#1498 dispositions the frozen `src/js/feedback-bridge.js:572-596` mixed public
`window.SMFeedback` facade. Its action/click trail, local read and local approval/resolution/
deletion exports map to pending `C04b.content-tools.feedback-local-log-storage`; the
`submitFeedback` export bridges that local write and pending
`C04b.content-tools.feedback-worker-publish`; its declared range also contains the
best-effort team-Sync write paired with the incoming flow. Incoming team-Sync and GitHub token/issue
triage exports map to pending `C04b.content-tools.feedback-triage-sync`; incoming import
uses the local store without making the whole facade a local-storage packet. The JSON
records exact frozen export and definition lines against source `3f6eed2a500f2ce868b711e063816029eb8fefa5`,
feedback-bridge blob `b87a6fc24b20f136c05df123d772b224e81ba212` and pinned C04b census
blob `f76259d59dedb715edc18b9f84db98ac00ee7cb8`. Its older range declarations stay
unchanged. The 17 previously undispositioned spans become 16, with zero feedback-bridge
spans remaining; all 762 packets stay pending, global computed uncovered code remains
7 185 and frozen feedback-bridge retains 23 computed uncovered code lines. No packet,
feedback delivery or browser/Tauri behavior is accepted.
P03C-al/#1500 marks frozen `src/js/vectorize-bridge.js:72-77` as a structural
`boundary`: line 72 is blank and lines 73-77 explain the vtracer absolute-control to
Paper.js relative-handle conversion before executable `splineToSegments` begins at 78.
That function remains in pending `C04b.content-tools.vectorize-shape-fitting`; the
original C04b range starts at 78 and its pin stays unchanged. The JSON cites frozen
source `3f6eed2a500f2ce868b711e063816029eb8fefa5`, vectorize blob
`3d1e3f817e981cdebc86919502515da64663157e` and C04b census blob
`f76259d59dedb715edc18b9f84db98ac00ee7cb8`. The 16 previously undispositioned
spans become 15, with zero vectorize-bridge spans remaining. All 762 packets stay
pending, global computed uncovered code remains 7 185 and frozen vectorize-bridge
retains 5 computed uncovered code lines. No packet or runtime behavior is accepted.

P03C-am/#1506 maps frozen `nemo-mcp/src/lib.rs:2-3` to the pending
`C07.mcp-server.binary-entrypoint` module-root responsibility: the executable
`capabilities` and `capability_contract` declarations were added by P07 source
commits `35d7eac66` and `bd907d230`. C07's pinned `lib.rs:1-7` range and
module-root description support the `covered` responsibility disposition;
neither module body nor the packet is admitted. The JSON pins this finding to
frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5`, `lib.rs` blob
`2d8f0dd07102d3b2aa1df154b84fcba7eeae5247` and C07 census blob
`1ccb542a681d1781161e382889dc611b97681f51`. The current protected
`lib.rs` contains a later `native_contract` declaration and has a different
blob; this disposition does not classify the current whole file. The 15
previously undispositioned spans become 14, with zero frozen `lib.rs` spans
remaining. All 762 packets stay pending, global computed uncovered code remains
7 185 and frozen `lib.rs` retains 2 computed uncovered code lines. No MCP
functionality, numeric range coverage or runtime behavior is accepted.

P03C-an/#1508 marks frozen `src/js/layer-inout.js:1-22` as a structural and
documentary `boundary`. Lines 1-11 and 13-22 are comments, but line 12 is the
executable `(function () {` IIFE opener enclosing all three pending C02
layer-inout responsibilities; it is not comment-only or a separate feature
writer. `inPointOf`/`outPointOf` first begin at line 23 in pending
`C02.layer-inout.bar-rendering`, with the selection/marquee and drag/batch
packets owning later areas. The whole preamble is not assigned solely to bar
rendering. The JSON cites frozen source
`3f6eed2a500f2ce868b711e063816029eb8fefa5`, layer-inout blob
`131156e2f419e17e8669c1980c3ae0f77164e786` (also current protected) and
pinned C02 census blob `3e920d4f112028ed486df697b5885aa4c67df316`.
The 14 previously undispositioned spans become 13, with zero frozen
`layer-inout.js` spans remaining. All 762 packets stay pending, global
computed uncovered code remains 7 185 and frozen `layer-inout.js` retains 74
computed uncovered code lines. No numeric range coverage, layer behavior,
browser/Tauri/export or runtime acceptance is claimed.

P03C-ao/#1510 marks frozen `src/js/select-bridge.js:1-23` as a structural and
documentary `boundary`: lines 1-22 are comments, but line 23 is the executable
`(function () {` IIFE opener enclosing all pending C04a Select responsibilities.
It is not comment-only or assigned solely to `C04a.select.hover-helpers`, whose
first census range starts at line 24. The JSON cites frozen source
`3f6eed2a500f2ce868b711e063816029eb8fefa5`, select-bridge blob
`a831eb065ed611f70a77692c61846eaddcd97a5a` and pinned C04a census blob
`8ffb669c5018190bd448b787a09ade35bd594b30`. The current protected
`select-bridge.js` has a different blob, so this disposition classifies only
the frozen span. The 13 previously undispositioned spans become 12, with zero
frozen `select-bridge.js` spans remaining. All 762 packets stay pending,
global computed uncovered code remains 7 185 and frozen `select-bridge.js`
retains 64 computed uncovered code lines. No numeric range coverage, Select
behavior, browser/native parity or runtime acceptance is claimed.

P03C-ap/#1512 marks frozen `src/js/tools.js:3370-3372` as a comment-only
`boundary`. Those three lines explain `gapThr` as a plain world-space
distance, the stroke-end gap still counted as one closed shape, and the Gap
Size presets without scale/resolution conversion. The separate WASM-path
comment begins at line 3373 in pending `C04a.fill.find-wasm-js-raster`;
executable `_wallSegments` begins at 3381. Neither belongs to this span.
The JSON cites frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5`,
tools blob `3c25962dbe314d5e791489da30b2b642ff899357` and pinned C04a census
blob `8ffb669c5018190bd448b787a09ade35bd594b30`. Current protected
`tools.js` has a different blob, so this disposition classifies only the
frozen span. The 12 previously undispositioned spans become 11, with zero
frozen `tools.js` spans remaining. All 762 packets stay pending, global
computed uncovered code remains 7 185 and frozen `tools.js` retains 3
computed uncovered code lines. No packet admission, numeric range coverage,
fill behavior, JS/Rust parity or runtime acceptance is claimed.

P03C-aq/#1513 marks frozen `src/js/project.js:1-9` as a structural and
documentary `boundary`. Lines 1-8 are New/Open/Save/Recent comments; line 9 is
the executable `(function(){` IIFE opener around the entire project module,
not comment-only or attributable solely to one C01 packet. First feature
data `RECENTS_KEY`/`MAX_RECENTS` begins at line 10 in pending
`C01.project.start-screen`; the opener also encloses the other pending C01
project responsibilities. The JSON cites frozen source
`3f6eed2a500f2ce868b711e063816029eb8fefa5`, project blob
`e2248533b47b0a067a61e4b641f83c7d24b00e79` and pinned C01 census blob
`d81b618308f9f4849b0ba24fa512af6be90aa72c`. Current protected
`project.js` has a different blob under native work, so this disposition
classifies only the frozen span. The 11 previously undispositioned spans
become 10, with zero frozen `project.js` spans remaining. All 762 packets
stay pending, global computed uncovered code remains 7 185 and frozen
`project.js` retains 70 computed uncovered code lines. No packet admission,
numeric range coverage, project/save/load/browser/Tauri/native behavior or
runtime acceptance is claimed.

P03C-ar/#1515 maps two frozen `src/js/application/opacity-application.js`
spans to the accepted P06/#1008 registered-capability discovery seam. Lines
78-93 define `capabilitySummary()`: it obtains the registry-backed
`ports.capabilities()`, clones the full descriptors, derives legacy
`properties` only from descriptors with `property.get`, and returns those
alongside operations, retention and document identity. Line 96 routes the
`perform` capabilities operation through that summary. P06 source commit
`35bcb02392bdd44c8d341c94c3ae6b505eb3379a` introduced both changes;
the production capability test checks the full export-plus-opacity descriptors,
opacity-only properties and a fresh registry read. The surrounding `perform`
command/query route remains in pending `C07.application-service.command-core`;
the older C07 census described a hard-coded single-opacity response and is
not being admitted. The JSON cites frozen source
`3f6eed2a500f2ce868b711e063816029eb8fefa5`, opacity core blob
`d0b10900eb36a43c66237473009027eaa6680d28` and pinned C07 census blob
`1ccb542a681d1781161e382889dc611b97681f51`. Current protected opacity
source has a different blob, so these dispositions classify the frozen spans
only. The 10 previously undispositioned spans become 8, with zero frozen
opacity-core spans remaining. All 762 packets stay pending, global computed
uncovered code remains 7 185 and frozen opacity core retains 15 computed
uncovered code lines. No packet admission, numeric range coverage, capability
runtime or installed-client behavior is accepted.

## Bounded C01 admissions and residual queue

[P03C-as / #1523](https://github.com/mysteropodes/nemo/issues/1523) admits exactly two
C01 boundaries: `C01.idb-store.kv` → [P34 / #1520](https://github.com/mysteropodes/nemo/issues/1520)
under R18.1/#915, and `C01.asset-tree.folder-widget` →
[P35 / #1521](https://github.com/mysteropodes/nemo/issues/1521) under R18.4/#918.
These are executable adoption/extraction leaves for existing unclassified modules with
implicit dependencies, real production consumers and missing direct tests. Admission
records their whole-file scopes, public API/lifetime contracts, three observable checks
and known-defect exclusions; it does not claim implementation or accepted behavior.

C01/#1036 remains pinned to source `ded641bd763379f36d629bf1686fcce8a6137a66` and
census blob `d81b618308f9f4849b0ba24fa512af6be90aa72c`. At the admission base,
protected `194d275b102a5604bc9ad31281be65c8487bfa5c`, the selected source blobs still
match that census: `src/js/idb-store.js` is `17fc7be9aba13784625261b496aee7120dc5245b`
and `src/js/asset-tree.js` is `a9cfc79050e98dd3a194e2eacfdea0c07400cd39`.
The inventory's own frozen commit, digest, pins and computed range coverage are unchanged.

At the 4 October 2026 readback, both leaves have sole Ilya ownership and Ilya/O
validation, with Project #2 Blocked / Validation Planned / roadmap Needs work /
Forecast / Unscheduled. Accepted C01/#1036 is their sole native predecessor. The
original blocker was the T08/#1057 shared application-profile/provenance slot.
[T08's source-stage handoff](https://github.com/mysteropodes/nemo/issues/1057#issuecomment-5890957293)
released its writer slot on 29 September, and
[T08B's terminal handoff](https://github.com/mysteropodes/nemo/issues/1532#issuecomment-5970737178)
released its later exclusive shared-file claim on 3 October. The retained Blocked
board state therefore needs re-evaluation against current capacity and whole-file
ownership; it does not establish that T08 still holds those files or must close first.
P34/P35 require fresh board-first Ready review and explicit writer/checkout allocation.
This inventory change updates neither leaf's board and allocates no implementing
writer or shared source/profile slot.

The other **eleven C01 packets remain pending**. The following architecture-first
decompositions are the residual queue from #1523, not source reservations. Live
characterization, whole-file owner release and ≤90-minute leaf definitions are required
before Ready; a partial successor never disposes the entire broad packet.

| Pending C01 packet (prefix `C01.` omitted) | Next bounded decomposition and ownership constraint |
| --- | --- |
| `project.document-io-and-dirty-tracking` | Separate path/name/dirty-baseline bookkeeping over the existing native serialization port, New/reset orchestration, and file open/save adapters. Serialize whole `project.js` after N20/#1352, N20F–H and N21 owner release. Preserve `NemoNativeOpacityProjectEntry.documentJSON`; do not recreate fixed-revision serialization or a JS document writer. Unsupported families require their own native revision leaves. |
| `project-document.validation` | [N23A/#1525](https://github.com/mysteropodes/nemo/issues/1525) admits only an immutable Rust structural-codec sub-slice with a production-JS parity oracle and no revision authority. Path helpers and supported-family admission/cutover remain pending. Subtract N20-owned opacity schema/projection/composition; N23A needs a separate Rust registration/wiring slot. |
| `tweens.undo-redo-engine` | Split full-layer history capture/apply, stack/labels/context policy and native family consumer routing. Exclude accepted P21/#1023 frame-entry work (PR #1171, integrated `b4a4fe249821bc534dd1a63dbb4b037df6455ec3`) and H02/#1059; retain wrong-frame redo and entered-Component limits. N09 already owns native opacity history. Whole `tweens.js` and any `app.js` writes await owner release. |
| `project.version-history` | Split disk snapshot key/list/retention from restore transactions; preserve native pinned serialization and restore admission. Requires whole `project.js`, replacement port and serializer-contract acceptance. The Untitled shared slug remains a characterized question. |
| `project.project-tabs` | Split per-tab snapshot/dirty-baseline model, DOM strip and close/switch transitions. Requires the native replacement port and whole `project.js` release; snapshots must not become concurrent writable documents. |
| `project.close-unsaved-work-guard` | One close-event adapter with injected dirty query/dialog/destroy ports. Verify confirmed destroy does not repeat the close event, cancel preserves the document, and browser beforeunload availability is explicit. Serialize with dirty-baseline and whole `project.js`. |
| `timeline.autosave-tick` | One stoppable periodic persistence boundary reads one authoritative revision snapshot and sends exact bytes to storage/history, retaining playback skip and timer behavior. Requires serializer/history interfaces and whole `timeline.js` release after P24/#1026 / PR #1473 with native owner coordination; do not enable blocked legacy writes. |
| `history-panel.ui` | Read/jump presentation over a public history snapshot/command interface after full-layer/stack extraction. The renderer owns DOM only; mutations remain with the accepted authority. Preserve click-time position and known history limits. |
| `project.start-screen` | Split recent-project storage/list model from New/Open/Resume DOM bindings, preserving N20/N21 first-open/replacement transitions. Requires document I/O, the selected persistence adapter and whole `project.js`; makes no CSS/HTML span decision. |
| `project.team-sync` | Split shared-folder path/profile storage, publish/list snapshot jobs and merge orchestration. Define the native family boundary of `SM.mergeRemoteSnapshot` outside C01 before writable merge routing; no expanded collaboration infrastructure. |
| `project-entry.repaint` | First assess whether the existing small, directly tested presentation seam warrants an evidenced covered disposition. Preserve `SMProjectEntry.repaint` and browser/Paper adapter limits. N20D0/N20/N21 owns native viewport presentation; do not invent a duplicate extraction. |

**P03 remains incomplete**: 762 packets total now comprise **760 pending, 2 admitted,
0 covered and 0 deferred**. `complete:false` remains unchanged; the normal scope gate
still exits 1 with no notes needing reconciliation, 0 unmapped paths and 8
undispositioned spans; the computed range report still has 7 185 uncovered code lines.
All eight spans are in `style.css`/`index.html`; Ilya's presentation-census decision
is recorded in [P03/#1005 comment 6038377096](https://github.com/mysteropodes/nemo/issues/1005#issuecomment-6038377096),
and the bounded leaves are listed in [P03 comment 6038585344](https://github.com/mysteropodes/nemo/issues/1005#issuecomment-6038585344).
Other pending packets still need bounded leaves under their family
parents or evidenced `covered`/`deferred` dispositions. The execution plan and live leaf
issues remain the queue and ownership authority. No C01-family-complete, whole-native-
engine or whole-remediation denominator, application behavior or runtime acceptance is
established by these admissions.

## P03C-at frozen shell, titlebar and properties presentation

[P03C-at / #1592](https://github.com/mysteropodes/nemo/issues/1592) records a
human-reviewed, inventory-only partition in this document, referenced by the
CSS file entry's preserved `reason` in the JSON index. Its source remains
the frozen `3f6eed2…` CSS blob `383eeec4…`; the protected `c9a53b8…` blob is
`a4a9e3c…`. The only protected CSS addition is after frozen line 2132, so lines
1–897 in this leaf have the same coordinates and content. The index source digest,
mechanical `rangeCoverage`, packet admissions and `complete: false` are unchanged.

| Named responsibility | Frozen CSS lines | Primary selectors and consumer boundary |
|---|---:|---|
| UI tokens and bundled icon font | 1–24 | Material Symbols `@font-face` at 1–3, universal reset at 4, `:root`; its applying class is owned below at 61–64 |
| In-place text overlay | 25–47 | `#tp-inplace-editor`, resize handle; `timeline.js`/vector text bridge own behavior |
| App layout and icon application | 48–69 | `html,body`, `.material-symbols-rounded`, `#app`, `#top-area`; dock controller owns placement |
| Topbar mode and comment tool | 70–90 | `#app-topbar`, `.app-mode-btn`; mode/tool controllers own state |
| Topbar spacer | 105 | `#app-topbar-spacer`; layout only |
| macOS titlebar/update badge | 106–133 | `body.mac-overlay-titlebar`, `#mac-update-btn`; updater bridge and Tauri configuration |
| Tabs/history/feedback popovers | 134–220 | `.project-tab`, `#history-pop`, `#fb-avatars-pop.open`; project/history/comment controllers own state |
| Tools dock/rail | 221–368 | `#tools-panel`, `.tool-btn`, flyout, wells; dock/tool/color controllers own behavior |
| Canvas chrome | 369–421 | viewport, rulers, guides and zoom; the trailing Labs comment describes C06-owned selectors |
| Canvas state tail | 462–466 | sculpt cursor and zoom focus/scrub; C06 Labs and zoom controllers own behavior |
| Properties and shared controls | 467–682 | collapse rail, sections, form states and `.fb-tag.active` at 640; panel/feedback controllers own values |
| Effects menu and thumbnails | 683–830 | `.fx-row`, `.fx-addmenu`, `.fx-prev-*`; effect menu previews are CSS approximations |
| Paired property grids | 831–883 | `.pair-grid`, `.sp-xform-row`, lock button; document/selection controllers own values |
| Easing curve widget | 884–897 | curve canvas and presets; Motion/tween APIs own easing data |

The owner named above is the **UI presentation responsibility**, not a transfer of
state or document ownership. The consumer/state matrix makes the split explicit:

| Responsibility | DOM/JS consumer and accessibility/state selectors | Platform and downstream state boundary |
|---|---|---|
| UI tokens and bundled icon font | Material Symbols `@font-face` at 1–3, universal box-model reset at 4 and `:root` palette at 5–24; Material Symbol spans in `index.html` consume that face through `.material-symbols-rounded` at 61–64, owned by App layout below. | Browser + Tauri. The font resource, reset and tokens have no stored field, history, selection, animation, render, export or native bridge. |
| In-place text overlay | `timeline.js` creates `#tp-inplace-editor` textarea and `#tp-inplace-resize-handle`; `vector-text-bridge.js` supplies glyph font assets. Textarea focus/edit state is native DOM, not a CSS command. | Browser + Tauri. The text controller owns the selected item's text, fixed width, save/load, history and glyph render/export; CSS only mirrors the face and handle. |
| App layout and icon application | `index.html` owns `#app`/`#top-area` and icon-only controls with HTML `title`/label semantics; `tools-panel-dock.js` anchors overlays there. `html,body` suppress selection during dragging, including WebKit-prefixed behavior; `.material-symbols-rounded` at 61–64 applies the bundled face and ligature styling. | Browser + Tauri, with the prefixed selection rule pertinent to macOS WebKit. Icon styling and layout have no document state or native bridge. |
| Topbar mode and comment tool | `index.html` button titles and mode labels; StoryBoard is declared `disabled` in `index.html:137`. `motion.js:13126` toggles `.app-mode-btn.active`, while `initModeSwitch` at 13165–13167 skips click wiring for buttons already disabled in HTML. Tool selection supplies `.tool-btn.active` to `#topbar-comment-btn`. | Browser + Tauri. Mode/tool commands remain controller/API work; the HTML disabled state declares StoryBoard unavailable here, and CSS only displays that state. It does not prove the mode works or persists. |
| Topbar spacer | `index.html` `#app-topbar-spacer` flex item; no interaction or state class. | Browser + Tauri. No downstream state dimensions. |
| macOS titlebar/update badge | `index.html` `#mac-titlebar-strip` drag region and `#mac-update-btn`; `updater-bridge.js` adds `body.mac-overlay-titlebar`, `.state-available`, `.state-downloading`, `.state-installed`. Button semantics remain in HTML. | Tauri/macOS overlay only; browser keeps the strip/button hidden. Updater transport/install availability is native bridge behavior, not CSS acceptance. |
| Tabs/history/feedback popovers | `project.js` builds `.project-tab.act`/`.dirty`; `history-panel.js` toggles `#history-pop.open`, `.hist-item.current`/`.future`; `rig-bridge.js` builds `.rig-weight-popover`. `timeline.js:9298–9375` reads `state.userProfile` and project `state.comments`, builds `#fb-avatars`/`#fb-avatars-pop.open` with author titles, `.fb-pop-status.resolved`, and navigates to a clicked comment frame with `goToFrame`; these are team-comment pins, not debug feedback entries. HTML has tab/history titles; project admission feedback uses a live `role=alert`. | Browser + Tauri. Project save/load owns tabs and team comments; profile persistence remains with `saveUserProfile`. History APIs own undo/redo and jumps; `goToFrame` owns playhead navigation. Avatar CSS reflects author/resolution state but does not write or resolve comments, save history or render/export frames. |
| Tools dock/rail | `tools-panel-dock.js` toggles dock body classes and `.tools-dock-zone.active`; `timeline.js` builds `.shape-tool-flyout-item.active`; HTML tool buttons have titles/shortcuts. `.tool-btn.active`, `.stack-front`, `#fill-well.none`, `.phdr-toggle.off` show current state. | Browser + Tauri. Dock preference, tool selection, fill/stroke values, selection and history remain with their existing UI/application controllers; render/export consume document color elsewhere. |
| Canvas chrome | `rulers-bridge.js` toggles `body.rulers-off` and guide elements; `timeline.js`/`app.js` read `#zoom-scrub`; `index.html` provides zoom/ruler anchors and titles. | Browser + Tauri. Guides and zoom alter viewport/UI state through their controllers; selection, animation, frame production, render/export and native presentation stay with those APIs. The 410–421 comment is context for C06's 422–461 selectors. |
| Canvas state tail | `labs/vector-sculpt.js` creates `#vector-sculpt-cursor.resizing`; `ui.js` toggles `#zoom-scrub.scrubbing`/focus interaction; zoom field remains an HTML input. | Browser + Tauri. Sculpt command and selection effects stay in C06 Labs/tool APIs; zoom stays with viewport state, not CSS. |
| Properties and shared controls | `index.html` inspector form labels, titles and inputs; `timeline.js` toggles `#props-panel.collapsed` and renders `#props-panel-rail`; `ui.js` manages `.psec.floating`/drag, `.phdr.closed`, `.pbdy.hid`, `.pi.scrub.scrubbing`. `input[type=checkbox]:focus-visible` preserves keyboard focus; `.icon-btn.sel` and `.color-eye-toggle.off` show values. The shared `.fb-tag.active` rule at 640 styles debug-feedback tag buttons in `index.html:2071–2075`; `timeline.js:7825,7846–7851` clears/toggles their class and `_activeFbTags`. | Browser + Tauri. Selection drives inspector sections; their values and applicable save/load, undo/redo, animation, render/export or native effects remain with owning panel/application APIs. Feedback tags are a separate transient submission choice passed to `SMFeedback.submitFeedback` at 7904–7906; they are not project comment tags or persisted document/history/render state. Disabled-looking controls do not prove capability availability. |
| Effects menu and thumbnails | `effects-panel.js` creates `.fx-row.expanded`/`.disabled`, `.fx-addmenu-search:focus` and dynamic `.fx-prev-*` classes; `timeline.js` builds `#blend-pop`/`#matte-pop` with `.sel`. HTML provides effect section/button titles. | Browser + Tauri. CSS preview filters/gradients are deterministic menu art, not effect frame evaluation. Effect enablement, parameters, history, persistence, animation and render/export/native consumers remain in the effects/application pipeline. |
| Paired property grids | `index.html` document/selection `.pair-grid` and `.sp-xform-row`, labeled `#sp-rot`, titled lock buttons; `timeline.js` toggles `.dims-lock-btn.on` for document/selection fields. | Browser + Tauri. Width/height, position, rotation, selection and aspect-lock command/state, with applicable save/history/animation/render/export consumers, stay in document and selection APIs. |
| Easing curve widget | `index.html` `#curve-canvas`, presets, resize handle and titles; `ui.js` renders preset buttons and handle, `motion-graph.js` interacts with shared widget. `.active` and `#curve-custom-presets:empty` are display state. | Browser + Tauri. Easing curves and selected keys belong to Motion/tween data, history, animation, render/export and any native evaluation bridge; CSS has no authority over them. |

These 14 ranges cover the three assigned spans exactly once: 1–90, 105–421 and
462–897 (843 total lines). They do not claim C06's accepted 91–104 and 422–461
feature flag/Labs presentation, or its separate 2132–2163 interval. The JSON
`reason` is a durable pointer: the refreeze builder preserves that field, whereas
it would drop an unrecognized top-level map. The existing checker does not
mechanically enforce this human-reviewed mapping. The CSS selectors only render
UI state; save/load, undo/redo,
selection, animation, render, export and native bridge behavior stays with the
named controllers and application contracts where applicable. Desktop-only
titlebar/updater states have no browser updater claim. No CSS or HTML was edited,
no existing census packet was admitted, and no runtime behavior was tested by
this index change. Partial-span presentation mapping is recorded separately
because `rangeCoverage.dispositions` accepts only a whole uncovered span; later
leaves must reconcile the full CSS spans before declaring them dispositioned.

Focused checks for this leaf compare the range union and an omitted/duplicate-line
negative control against the frozen CSS, verify protected/frozen slice identity
and the C06 exclusions, and inspect the named selectors in frozen/current CSS
and HTML plus the cited JS consumers. The existing integrity-only checker and
`nemo-remediation-scope.test.cjs` verify the unchanged mechanical index. They
do not validate this presentation mapping's semantic completeness or certify
browser, Tauri, installed desktop, or feature behavior.

## P03C-au frozen Motion controls and property-row presentation

[P03C-au / #1593](https://github.com/mysteropodes/nemo/issues/1593) adds the
following inventory-only UI responsibility packets for frozen `src/css/style.css`
898–1356. Source pin: `3f6eed2a500f2ce868b711e063816029eb8fefa5`, CSS blob
`383eeec4be64457fdc7f2028f6d9b04cac32dbc8`; inspected protected source/base:
`5374eed51a806a87f5fba4b8fc0c979e5c137995`, CSS blob
`a4a9e3cab070289eaa57f07c9188ec00bc203335`. The four added current CSS lines
follow frozen line 2132; this leaf's coordinates and bytes are unchanged. JS/DOM
locations below refer to the inspected protected source, not the frozen CSS tree.

Each name owns only presentation in its exact interval, including adjoining
comments/blanks. The public interface is the listed selector/DOM contract; named
controllers own its state and actions. This supplemental map is preserved through
the CSS JSON `reason` pointer, not a new top-level field that refreeze would drop.

| Named UI responsibility | Frozen CSS lines | Selector/DOM, consumer and accessibility/state contract |
|---|---:|---|
| Timeline shell and vertical resize | 898–901 | `index.html:2117–2118` supplies `#timeline-area`/`#tl-resize`; `ui.js:774–797` handles pointer resize and restores/saves `nemo-timeline-height`. Cursor/hover reflect that gesture; the handle is a div, not a keyboard button. |
| Toolbar layout, mode visibility and customization | 902–933 | `#tl-toolbar`, `.toolbar-user-hidden`, eight `body.mode-motion #btn-*` drawing controls, `.toolbar-custom-*`; `timeline.js:11118–11191` creates labeled checkbox rows and persists `nemo-timeline-toolbar-hidden`, protecting six transport buttons from hiding. `motion.js` mode switch supplies the body class. The static customize button has title and `aria-label`; CSS mode hiding is separate from the stored user choice. |
| Transport and timing control faces | 934–943 | `.tb.active/.playing/.pingpong`, `.ti` numeric fields; `index.html:2122–2144` buttons have titles; FPS/Frames inputs at `index.html:2155–2156` have `scrub`/bounds. `timeline.js:41,192,537` sets play/loop classes and title; `setFps` and `#tl-total` handlers own timing. Spin-button removal and `outline:none` are existing CSS, not proof of keyboard accessibility. |
| Split panel, layer rows and reorder feedback | 944–957 | Static `#tl-content`, `#layer-panel`, `#layer-list`, titled `#layer-panel-resize`; `ui.js:817–822` toggles resize `.active` and stores width. `timeline.js:6896–7005` creates `.layer-reorder-grip`, `.layer-drag-ghost`, `.layer-drop-indicator`; `.frow/.lrow.dragging` dims the source. Overlay pointer-events prevent interception; cursor/grip feedback does not establish keyboard reordering. |
| Paired camera row and key marks | 958–975 | `.lrow/.frow.camrow.compact`, `.fc.camkey`; `camera.js:361,408–414,504` builds both halves, shrinks them together to 16px and marks actual keys. Camera tool state supplies `.act`/`.compact`; CSS diamond pseudo-elements have no independent command or accessible name. |
| Motion group headings and unanimated filter | 976–1001 | `.motion-group-row`, `#frame-grid` padding override, `.motion-filter-btn.on` rotated SVG; `motion.js:8533–8551` builds a titled span filter and rerenders both sides; `renderPathGroupTrackRow` and grid spacers consume the shared heading. Rotation augments color; the span click handler is not a native keyboard button. |
| Property-row selection and mixed-value faces | 1002–1007 | `.motion-prop-row`, `.motion-prop-select-target`, `.motion-track-row.prop-selected`, `.motion-val.mixed`; `motion.js:11343,11437–11439` applies paired selection and targets, `scrubField` at 6637–6648 creates `pi scrub motion-val` number inputs with mixed placeholder/title. CSS does not aggregate or commit values. |
| Element rows and color/icon swatches | 1008–1020 | `.motion-elem-row`, `.motion-elem-swatch.has-motion/.icon`; `motion.js:9710–9771`, `timeline.js:5544–5585` and `shapes-panel.js:603,669` construct element/group rows. Swatch SVG/color is presentation; `.has-motion` is animation indication, not an enabled editing capability. |
| Property key toggle and expression editor | 1021–1041 | `.motion-stopwatch.on`, `.motion-expr-btn.on/.err`, `.motion-expr-editor`, checkbox label, textarea `.motion-expr-code:focus` and error text; `motion.js:8595,8647–8658,8870+` builds these, with three-state key titles from `stopwatchTitle`. Inline SVG controls fill versus outline; `.err` reflects expression error state. Textarea focus gets an accent border; controls' labels/titles belong to DOM, not CSS. |
| Anchor picker and scale lock | 1042–1057 | `.motion-anchor-grid-btn.on`, `.motion-scale-lock.on`, `.motion-anchor-grid-row`, `.motion-anchor-cell.center`; `motion.js:8669,8787,8821+` creates the controls and nine button cells. Center marker and hover are visual; anchor and scale writes remain controller operations. |
| Inspector mirror and numeric field layout | 1058–1089 | Static `#motion-props-sec`/`#motion-props-body` at `index.html:1239–1241`; `renderMotionPropsPanel` (`motion.js:7431+`) builds `.motion-props-layername`, `.motion-prop-name`, `.motion-fields`, `.pi.motion-val`, unit/dimension labels. Mode hides the whole section; expression rows override fixed inspector height. Dimension labels are visual; actual numeric inputs own interaction. |
| Boolean/color fields and right-edge key control | 1090–1100 | `.motion-ctrl-check`, `.motion-ctrl-swatch`, stopwatch margin; `motion.js:8705,8717` creates nonnumeric controls beside the common property fields. Native checkbox semantics remain in DOM; a clickable color swatch is not itself evidence of keyboard access. Row height remains shared with tracks. |
| Property tracks and key silhouettes | 1101–1146 | `.motion-track-row`, `.fc.motion-fc`, `.motion-key.cur/.sel/.linear/.hold/.smooth/.tinted`; `trackRowHtml` (`motion.js:10600–10722`) builds tracks/diamonds and derives interpolation, selection and current-frame classes. Relative z-index preserves diamond hit priority over connectors; shape/color reflects data without evaluating animation. |
| Key connectors, tint and Alt retime cursor | 1147–1183 | SVG `.motion-key-connect:hover/.sel/.tinted`, `--key-color`, `#frame-grid.timelink-alt`; `motion.js:10643–10676` creates connector rects and their event targets, `layer-inout.js:1602` toggles shared Alt class. Cascade order retains user tint under hover/selection. The retime cursor is an affordance; commands and modifier interpretation stay with the gesture controllers. |
| Marquee, selection box and stagger/space handles | 1184–1242 | `.motion-marquee-rect`, `.motion-keysel-box/fill/edge-*`, `.grabbing`; `motion.js:11842,11932–11969,12051,12235` creates screen-fixed overlays for keys/layers. Frame ignores pointer events; fill/edges enable them, with corner z-order and grab/resize cursors. `updateKeySelectionBox`/`updateLayerStaggerBox` own geometry; CSS provides no keyboard alternative or key mutation. |
| Motion density and paired active/selected state | 1243–1291 | `body.mode-motion` panel width, 22px `.lrow/.frow/.fc`, expression auto-height, compact icons/keys, `.act.motion-selected` in both halves. `motion.js:53` `ROW_H=22`, `renderLayerListMotion`/`renderTimelineMotion` and selection synchronization supply matching rows/classes. Highlight overlay is pointer-transparent; active position and selected state remain distinct. |
| Shared layer icons, names and element visibility | 1292–1313 | `.lrow.act/.sel`, `.lnm`, `.lico.off`, `.solo-btn.on`, `.elem-vis`, `.elem-hidden`; `timeline.js` layer-list renderer and `motion.js:7046–7052` supply titled visibility/lock/solo controls; `shapes-panel.js:642–644` supplies element hidden/eye state. Ellipsis truncates text visually; title/control semantics stay with DOM. |
| Aligned layer header and Motion context trigger | 1314–1337 | Static `#layer-hdr`/`#layer-ctrls` and titled add/camera/audio/delete/duplicate/component buttons (`index.html:2212–2230`); `ensureMotionHeaderTools` (`motion.js:6844–6862`) adds `#motion-header-tools`, titled context label and native button `#motion-filter-trigger`. 42px header matches grid header/bars; Motion narrows button spacing without hiding creation controls. |
| Motion filter popover and column presets | 1338–1356 | `.motion-filter-pop/search:focus/select/check/hint`, `#layer-panel[data-motion-columns]` and `.motion-col-*`; `motion.js:6862–6895` creates native search/select/checkbox controls, focuses search and persists filter/columns/snap local preferences. `motion.js:7049–7117` adds column tokens; CSS hides only specified Motion columns. Visible label text/placeholder are existing DOM, not a claim of programmatic label association. |

All packets apply to the shared browser and Tauri WebView UI. Source inspection
establishes selector/consumer responsibilities only; no browser, Tauri, installed
desktop, accessibility interaction or functional test was performed for this map.
CSS/HTML/JS were read-only and the application was not launched.

The downstream applicability is explicit:

| Packet family | Save/load and undo/redo | Selection and animation | Render/export and native/API boundary |
|---|---|---|---|
| Shell, customization, split layout, header and filters | UI controllers own local/workspace preferences; `ui.js` height/width and toolbar/filter localStorage are not Rust document history. Search/context/drag overlays are transient. Workspace inclusion is owned by `SMWorkspace`, not CSS. | Layout/filter controllers must render matching list/grid row plans; snap is gesture configuration. These packets do not store keyframes or select objects. | UI layout is not image/frame output. No persistent document, evaluator or native bridge is introduced by selectors. Any creation button still needs its declared command/capability. |
| Transport/timing and camera | FPS/total-frame and camera data need authoritative document save/load/history where applicable; playback/loop face state is not a history record. | Navigation/playback/camera keys belong to timeline/camera and native adapters; CSS only reflects frame/tool/key state. | Timeline native playback uses `NemoNativeOpacityCutover` (`timeline.js:34+`); applicable camera evaluation/frame production/export remains native-owned or unavailable. A visible button/diamond cannot grant camera or timing capability. |
| Property, element, expression, anchor, numeric, checkbox/color and key-track/connector packets | Document values, expression definitions, interpolation, user key color and keys require authoritative codec/history support. DOM/mixed-value/error state does not save or undo them. | Selection targets, holder identity, current keys, interpolation and expression evaluation remain Motion/application contracts. `propsFor`/row plans must keep inspector/list/grid coherent. | `motion.js:2727–2733` resolves the native Motion surface and throws if blocked legacy authority lacks that surface; `renderedOpacityRoute` at 8573 and `native-opacity-motion-surface.js` own native routing. Only supported native operations may mutate; unmigrated transforms, elements, expressions and other controls must remain explicitly unavailable, without a legacy writer fallback. CSS does not certify those gates or accepted save/render/export behavior. |
| Reorder, marquee, selection/stagger and shared layer/element states | Reorder, visibility, lock/solo and retime writes require applicable native command/history and persistence; overlays and selection are transient intent. | Motion and layer-inout controllers own selection/drag/modifier semantics. Shape/element identity and unsupported gestures retain their separate native admission obligations. | Render/export consume authoritative revision state elsewhere; overlay CSS does not alter output or own GPU/viewport resources. Native opacity adapters and admission guards are boundaries to inspect for a later behavioral leaf, not acceptance gained here. |

The 19 intervals cover **898–1356 exactly once (459 total lines)**. They are
disjoint from accepted P03C-at's last line 897, the next P03C-av leaf starting
1357, and C06's 91–104, 422–461 and 2132–2163. No `rangeCoverage`, whole-span
disposition, original packet count/admission, source identity or `complete:false`
changes: partial-span maps cannot disposition the whole frozen 462–2131 span.
The 19 UI packet names are supplemental human-reviewed responsibilities, not
additional mechanically pinned/admitted census packets or Rust port obligations.

Focused validation parses this section's range table against independent expected
898–1356, rejects an omitted line and a duplicated line, checks C06/sibling
exclusions and byte identity with the protected CSS, and verifies representative
selectors/DOM/JS consumers. The integrity-only checker and 53 focused scope tests
cover index consistency, not semantic or runtime acceptance. Normal completeness
remains failing while the wider packet/disposition queue is pending.

## P03C-av frozen frame-grid and in/out presentation

[P03C-av / #1594](https://github.com/mysteropodes/nemo/issues/1594) maps frozen
`src/css/style.css` 1357–1855 into the following inventory-only UI responsibility
packets. Frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5`, CSS blob
`383eeec4be64457fdc7f2028f6d9b04cac32dbc8`; inspected protected source/base
`28abfc97c994bf37fa4c538e9871ae1da438d3e7`, CSS blob
`a4a9e3cab070289eaa57f07c9188ec00bc203335`. The current CSS adds four lines
after frozen 2132; this slice retains identical coordinates and bytes. DOM/JS
locations below refer to that inspected protected source.

Each named owner owns only presentation in its exact interval, including comments
and blanks. Controllers own state, gestures and commands; no owner here acquires
document, evaluation or native authority. The JSON CSS `reason` points to this
supplemental map without adding fields that refreeze would discard.

| Named UI responsibility | Frozen CSS lines | Selector/DOM, consumer and accessibility/state contract |
|---|---:|---|
| Compact property faces and paired group/discrete row alignment | 1357–1373 | `body.mode-motion #layer-list` hides `.motion-expr-btn`/`.motion-anchor-grid-btn` while keeping `.pi.motion-val`; paired `.motion-group-row` height/margin and right `.motion-discrete-row` margin preserve alignment. `motion.js:8534,10285,10373,11101` creates both sides; `renderDiscreteKeyGridRow` explains shared selection class versus plain left property row. Hidden editors remain in the inspector; CSS does not commit numeric values. |
| Motion snap guide and text badge | 1374–1375 | `.motion-snap-guide`/`::after` paints a nonintercepting vertical line and `data-label`. `showMotionSnapGuide`/`clearMotionSnapGuide` (`motion.js:12820–12830`) creates/removes it inside `#fg-wrap` and positions it from frame/FC. Snap computation and retime commands remain Motion operations; generated text is visual feedback. |
| Paired bottom band and grid scroll viewport | 1376–1390 | `#layer-panel`, `#fg-col`, `#fg-wrap` share 8px bottom reserve; `index.html:2236–2237` supplies the right wrappers. `timeline-zoom.js:61,108,152` shares `BOTTOM_BAND_PX=8`, `--fc` and scrollbar positioning; `layer-scroll-sync.js` mirrors list/grid vertical scroll. Overflow and padding own layout only, not a second scroll or frame authority. |
| Sticky work-area/onion strip and header alignment | 1391–1419 | Static `#bars-row` follows `#frame-hdr` (`index.html:2246–2252`); sticky top 20px plus height 22px matches `#layer-hdr` (`index.html:2212`), whose 42px height comes from `style.css:1317` in P03C-au's interval. Frozen CSS comment 1407's inline-height attribution is stale; the DOM has no inline height. `timeline.js:4188+` sets strip width after ruler rebuild. z-index 2 keeps both headers above the grid; DOM order and height must move together. |
| Work-area body, edge handles and passive grid tint | 1420–1444 | `#wa-bar .wa-handle.left/.right` and `#wa-tint` come from `index.html:2249,2262`. `ui.js:975–1006` (`updateWaBar`) mirrors frame range/FC into left/width and tint height; `initWaDrag` owns dragging. Tint is pointer-inert while bar/handles carry grab/resize cursors. These divs gain no keyboard semantics from CSS. |
| Onion range edge markers | 1445–1457 | `.onion-marker.om-in/.om-out` are titled static divs (`index.html:2250–2251`), positioned in the lower strip at top 15px. `ui.js:1027+` (`initOmDrag`) updates frame datasets and calls `SM.setOnionRange`; `updateOmMarkers` controls visibility. Red/blue differentiates endpoints visually, without establishing an accessible noncolor alternative. |
| Layer range capsule and overflow caps | 1458–1481 | `.layer-inout-bar.full-range`, `.layer-inout-overflow-left/right`; `buildBar` (`layer-inout.js:1459+`) creates the titled bar, `updateBar` (153–215) sizes it, toggles full range and creates hatched overflow overlays. Inline per-layer color overrides base fill/border. Caps are pointer-inert; duration/range writes remain controller/API obligations. |
| In/out pill handles, expanded hitbox and hot/selected states | 1482–1538 | `.layer-inout-handle.left/right`, `::before`, Motion width override, `.hot/.sel`; `layer-inout.js:1462–1469` creates titled div handles, `applySelClasses` (384–398) selects edges, and onDown/mouseup (627,982) retains/removes hot state. Invisible hitbox expands beyond the pill; the cascade retains dark selected core and visible halo. Mouse handlers are not keyboard acceptance. |
| Parent-in-Time anchor visibility, position and role faces | 1539–1599 | `.timelink-anchor.in/.out/.whole`, `.is-child/.is-parent`, `#frame-grid.timelink-alt` and `.has-timelink`; `layer-inout.js:1481–1498` creates titled anchors and delegates mousedown to `SMMotion.startTimeLinkPickwhip`. `setTimeLinkAltReveal` (1600–1606) toggles on Alt and clears on keyup/blur; updateBar toggles bar hint. Engaged points remain visible; free points require Alt. Role fill/border and expanded hitbox confer no cycle/history or native-link acceptance. |
| In/out marquee, whole-bar selection and dimmed cells | 1600–1610 | `.layer-inout-bar.sel`, `.layer-inout-marquee-rect`, `.fc.io-dim`; `layer-inout.js:384–403` applies selection and creates the fixed pointer-inert marquee; updateBar (211–215) dims cells outside the range. Motion's shared marquee forwarding (`CLAUDE.md` section 11) remains controller-owned. Visual selection and trim bounds are distinct states. |
| Drawing-key ticks inside and outside the range | 1611–1629 | `.layer-inout-key:hover/.outside`; `renderKeyTicks` (`layer-inout.js:298–330`) creates full drawing-key ticks, titles/frame positions and outside state. CSS z-index 6 keeps them above playhead 5. Frozen CSS comments call outside ticks inert, but inspected JS binds mousedown to every tick (feedback #772), including outside; preserve this explicit comment/implementation discrepancy rather than treating dark styling as denial. Retime routes to `SM.moveKeyframe`, not CSS. |
| Empty drawing-gap overlays and hollow endpoints | 1630–1638 | `.layer-inout-seg-gap`/`.layer-inout-segdot`; `renderContentGaps` (`layer-inout.js:240–276`) removes/recreates gap spans and hollow blank-key endpoints from drawing frames. Pointer-events none allows bar gestures through both; CSS neither classifies content nor changes drawings. |
| Onion range gradient body | 1639–1643 | `#onion-bar` is static (`index.html:2248`); `updateOnionBar` (`ui.js:1012–1024`) uses endpoint geometry for left/width, and `updateOmMarkers` toggles display. Top/height aligns with the separate endpoint packet. Pointer-inert gradient is a range readout, not onion-rendering ownership. |
| Frame ruler, labels and tick/current/second hierarchy | 1644–1682 | `#frame-hdr`, `.fhc.tick/.tickMaj/.cur/.sec` and pseudo ticks; `timeline.js:4160–4180` emits per-frame labels/tick classes with seconds at zero-based `i%fps===0`; `updatePlayhead` (216–227) marks current header cell. `--fc` aligns ruler/grid widths. CSS hierarchy follows emitted classes, not the historical comment's nth-child claim. Sticky sizing must agree with bars-row and left header. |
| Grid stacking context, paired frame rows and frame cell lattice | 1683–1719 | `#frame-grid` z-index 1 contains descendants below sticky headers; `.frow.act`, `.fc:nth-child(5n)/:hover/.cur` supply 34px rows and grid ticks. `timeline.js:4377,4830` builds drawing rows/cells; Motion uses its separate shorter track overrides and paired row plan. Current/active classes are state readouts; CSS does not select a frame or evaluate a scene. |
| Full/empty drawing-key and held-span tint | 1720–1744 | `.fc.kf-full/.kf-empty/.span-full/.span-empty` and pointer-inert `::before`; `renderKeyframeCellsInto` (`timeline.js:4802–4862`) derives full/empty/held content, component outer placement and `--dot-color/--dot-rgb` from owning layer color. Tint is presentation of drawing content, not Motion property keys or authoritative evaluation. |
| Held-span end handle and trim-drag preview cascade | 1745–1797 | `.span-end`, `.span-drag-band/.span-drag-preview/.span-drag-dot-cell`, `.tl-outdrag-source-end/key`, `.km.drag-key-preview`; `timeline.js:4898–5063` creates/clears source and moving preview states. Source dims, same-cell preview restores opacity, bordering key preview occupies the adjacent cell. Real trim/retime writes and undo remain controller/API work; CSS's old grab-zone comments must not substitute for current full-cell handler. |
| Drawing-key silhouettes and automatic/manual tween markers | 1798–1813 | `.fc.tw/.tw-manual`, `.km.fl/.hl/.td/.manual`; `timeline.js:4840–4850` constructs square full/hollow key markers or smaller tween ticks and manual correction state. Hover scales the marker; shape plus color differentiates key/tween. The cursor is an affordance, not permission to mutate or evidence that manual tween protection works. |
| Playhead line and draggable frame-number flag | 1814–1845 | `#playhead`/`#playhead-flag` from `index.html:2275`; `timeline.js:215–227` positions the line and updates 1-based text. Parent is pointer-inert, flag overrides to auto with title; `translateZ(0)` retains the documented WKWebView compositing workaround. `syncPlayheadToViewport` (`timeline.js:4031–4057`) pins the playhead on scroll/resize; frame navigation and actual browser/Tauri painting need separate acceptance. |
| Work-area dimming, frame selection and key-drag ghosts | 1846–1855 | `.fc.outside-wa/.sel/.sel.cur`, `.tl-drag-ghost .fc`, `.fc.tl-drag-fading`; `timeline.js:4830–4831` paints range/selection, 5414 builds ghost, 5468–5478 removes/applies source-span fading. Ghost is pointer-inert and above cells; `.sel.cur` preserves stronger selected-current cue. Selection/clipboard/drag state remains timeline intent and command responsibility. |

Load contract: `index.html:17` loads `css/style.css` before the static timeline
DOM and ordered classic scripts: timeline at 2384, Motion at 2403, layer-inout at
2433, layer-scroll-sync at 2434 and timeline-zoom at 2437. The stylesheet consumes
their DOM/classes/custom properties; it does not own initialization or commands.
Exact order, paired row plans, `--fc`, inline bar tint and later Motion overrides
must be preserved by a future UI extraction. Titles and frame text reside in DOM;
generated snap text, cursor/hover/color and div handles do not prove keyboard or
assistive-technology accessibility. No accessibility repair is included here.

| Applicable consumer dimension | Boundary retained by this inventory map |
|---|---|
| Save/load and undo/redo | CSS selectors, overlays and classes are not persistent fields or history entries. Drawing keys, in/out points, time links, property values, work-area configuration and any persisted onion preferences retain their owning codec/history obligations. Inspect the corresponding controller/application API before modifying a persistent field; this map does not accept those consumers. |
| Selection and animation | Timeline/Motion/layer-inout own selection intent, modifiers, row plans, snap calculations, frame navigation, drawing retime and link gestures. `renderKeyTicks` shows the explicit outside-tick comment/handler discrepancy above. Frame/key colors and range readouts do not establish animation evaluation or enabled native editing. |
| Render/export and native bridge | Render/export consume authoritative revision/evaluation elsewhere, not these DOM overlays. `timeline.js:48–50,2102–2104` routes native playback/persistence when cutover blocks legacy; `motion.js:2727–2733` requires its native Motion surface. This inspection supplies no end-to-end proof that all drawing/range/link gestures are guarded or supported. Stateful operations require the Rust-backed shared API or explicit unavailability; no CSS fallback writer or new native admission is authorized. |
| Browser and installed desktop | Static DOM and CSS apply on both UI surfaces, including the documented WebKit playhead workaround. Browser/WASM availability and installed Tauri row alignment, pointer targets, scrolling, playback and painting remain separate behavioral checks. No browser run, installed artifact or physical interaction was tested or accepted by this census. |

The 20 intervals cover **1357–1855 exactly once (499 total lines)**, disjoint
from P03C-at/au through 1356, the next CSS leaf at 1856, and C06's 91–104,
422–461 and 2132–2163. No original packet, `rangeCoverage`, source identity,
whole-span disposition or `complete:false` changes. These are supplemental UI
responsibility names, not mechanically admitted packets or Rust port obligations.

Focused validation parses this table against independent expected 1357–1855,
rejects omitted/duplicated lines, checks sibling/C06 exclusion and frozen/current
byte identity, and verifies representative selectors, DOM anchors and controller
consumers. Integrity-only and the 53 scope tests verify index consistency, not
semantic completeness or behavior. Normal completeness remains failing while the
wider packet/disposition queue remains pending.
