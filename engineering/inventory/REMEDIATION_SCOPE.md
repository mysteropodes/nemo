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

## P03C-aw frozen auxiliary-panel and media presentation

[P03C-aw / #1595](https://github.com/mysteropodes/nemo/issues/1595) maps frozen
`src/css/style.css` 1856–2131 and 2164–2381 into named UI presentation
responsibilities. Frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5` / CSS
blob `383eeec4be64457fdc7f2028f6d9b04cac32dbc8`; inspected protected base
`543936e8b8bd8896a35dc6921769a94266e976dd` / CSS blob
`a4a9e3cab070289eaa57f07c9188ec00bc203335`. Current CSS inserts four export-modal
lines after frozen 2132: the first interval is byte-identical at the same
coordinates; the second is byte-identical at current 2168–2385. C06 owns frozen
2132–2163 and its settings/modal shell; neither it nor those new export rules
is claimed here. DOM/JS references below describe the inspected protected source.

Each row is one exact presentation owner, including adjacent comments/blanks.
Names are supplemental census responsibilities, not admitted packets. Controllers
retain state, commands and persistence; CSS owns no native or document authority.

| Named UI responsibility | Frozen CSS lines | Selector/DOM, consumer and accessibility/state contract |
|---|---:|---|
| Status hints and transient toast | 1856–1861 | Static `#statusbar`, `.sc`, `#toast` (`index.html:2282+`); `timeline.js` updates `#statusbar-help` and `showToast` toggles `.show` with a timeout. Fixed pointer-inert toast animates opacity/translation; message text and notification semantics belong to DOM/controller. |
| WebKit scrollbar skin | 1862–1865 | Global `::-webkit-scrollbar` track/thumb use theme variables. Applies to supporting browser/WebView scroll containers; no scroll-position owner or cross-engine rendering guarantee. |
| Symbol navigation tabs and component row accents | 1866–1879 | `#symbol-tabs` inside timeline toolbar; `timeline.js:7060+` emits `.sym-tab.act`, close spans and symbol/montage callbacks. `.lico.comp-badge`/`.lrow.is-comp .lnm` distinguish component rows. Overflow, active dot and italic name are readouts; clickable div/span semantics are not keyboard acceptance. |
| Layer kind badges and folder grouping feedback | 1880–1907 | `timeline.js` creates `.lkind-*`, `.lrow.lfolder/.in-folder`, arrows and `.frow.ffolder`; layer-kind metadata supplies icons/kinds. `moveLayerReorder` toggles `.folder-drop-target` on both sides. Group folders and real folder layers are explicitly distinct. Color, collapsed tint and drop outline do not own hierarchy or reparent commands. |
| Shared hover tooltip | 1908–1913 | `ui.js:1174+` creates `#ui-tip`, converts title to `data-tip`, toggles `.show` and clamps position; timeline content sends hints to the status bar instead. Pointer-inert floating text has no CSS-created focus/ARIA semantics. |
| Context action menu shell and disabled faces | 1914–1922 | `ui.js:1135+` exposes `showContextMenu`, creates `.ctx-menu/.ctx-item/.ctx-sc`, binds clicks only for enabled items and dismisses outside/Escape/scroll. Hover/disabled selectors are visual; actual denial belongs to controller, not muted color. Brush/color popovers reuse this shell. |
| Palette tabs, alpha swatches and replace/match states | 1923–1938 | Static `#palette-tabs/#palette-grid/#btn-palette-replace`; `palette-panel.js` builds swatch buttons and sets `--sw-color`, `.armed`, `.sel-match` and replace `.active`. Shadow brush also emits `.palette-swatch`. Checkerboard/pseudo color separates alpha, armed replacement and current color; callbacks retain fill/stroke/palette mutation obligations. |
| Brush preset trigger and catalog rows | 1939–1958 | `brush-preset-picker.js` creates `.bp-picker-pop`, `.bp-item.active/.bp-item-custom`, canvases and delete spans; `motion-preset-picker.js` reuses rows/icons/delete. `.bp-swatch-btn.disabled` and native `:disabled` share styling but only native disabled supplies browser denial. Thumbnails and ellipsis are presentation, not preset storage or brush rendering. |
| Unified asset tab panes and preset tree rhythm | 1959–1978 | `index.html:1243+` hosts `#assets-sec`, `.asset-toptabs/.asset-view/.asset-tree`; `assets-panel.js` toggles the two buttons' `.active` and views' inline display, media-library/motion-preset-picker render contents. Tabs reuse C06 `.settings-tabs/.settings-tab` without claiming those rules; later `.asset-tree .bp-item` overrides the generic rows. |
| Asset folder headers, counts and collapse | 1979–1993 | `asset-tree.js:22+` creates `.asset-folder-*`, sets `--folder-color` and toggles `.collapsed` on header/body; media-library and transplant consume `SMAssetTree.folderGroup`. Hidden body and rotated chevron mirror session UI state. Current helper explicitly does not persist `opts.key` collapse; rebuilding reapplies defaults. Clickable header divs have no keyboard role from CSS. |
| Preset favorites overlay and quick strip | 1994–2011 | `brush-preset-picker.js` creates `.bp-item-fav.is-fav` and `.bp-fav-item.active` under `#p-brushfav-row`; generic/custom row padding reserves separate star/delete space. Stars appear on hover or favorite state; fixed canvas tiles show favorites. Favorites storage and event exclusion are controller responsibilities; hover-only action visibility is not accessibility acceptance. |
| Brush menu fixed header and scrolling catalog | 2012–2029 | `brush-menu-bridge.js:425+` builds `.brush-menu-tabs/.brush-menu-tab.active`, `.brush-menu-params`, `.brush-menu-scroll` inside `.ctx-menu.brush-menu-pop`. Fixed flex header and independently scrolling list retain thumbnail overrides; vector/bitmap mode and parameter writes are not CSS authority. |
| Shadow brush content-sized popover | 2030–2034 | `shadow-brush-bridge.js:128` creates `.ctx-menu.shadow-brush-pop` and palette swatches. Auto width and vertical gap specialize the shared shell; active shadow choice and mutation remain controller-owned. |
| Brush editor preview and parameter form | 2035–2047 | `brush-editor.js:124+` creates `.ctx-menu.bp-editor-pop`, `#bpe-preview`, `.bpe-row/.bpe-slider/.bpe-rotmode/.bpe-name-input` and capture status. Native form controls retain labels and values in the creator; CSS aligns preview, parameter fields and actions, without validating or saving custom presets. |
| Color picker canvas, numeric fields and swatches | 2048–2073 | `color-picker.js:79+` creates `.ctx-menu.color-picker-pop`, SV/hue/alpha canvases/thumbs, preview checkerboard, labeled inputs and none/eyedropper swatches. Thumbs are pointer-inert while tracks receive gestures. Alpha values must preserve CLAUDE.md's hex8 contract; painted gradient/none slash does not accept persistence, color math or keyboard picking. |
| Context separator and layer reorder insertion states | 2074–2082 | `ui.js` emits `.ctx-sep`; timeline reorder logic toggles `.lrow.drag-over/.drag-over-after/.drag-into/.dragging`. Insertion lines versus into-group outline communicate different destinations; opacity is drag feedback. Hierarchy/reorder writes and undo remain operations, not selector behavior. |
| Start overlay, branding and translated tagline | 2083–2100 | Static `#start-screen/#start-inner/#start-brand-*` (`index.html:21+`); `project.js` controls `.hid`. Fixed z-index 500/scroll/max-width and tagline wrap support the translated start shell. Logo's empty alt and div content remain DOM facts; layout is not project-open acceptance. |
| Start action cards, Kitsu row and new-project panel | 2101–2115 | Static `#start-cards/#start-resume`, `.start-card/.start-row`, `#start-newpanel`; `project.js` enables resume with `.has-resume` plus inline display and wires new/open/resume, Kitsu bridge owns remote open. Resume hiding depends on both CSS and inline state. Div cards and hover faces supply no button keyboard semantics. |
| Recent-project list and missing/removal faces | 2116–2131 | `project.js:642+` emits `.start-recent-row`, name/meta and removal div into `#start-recent-list`; the empty list instead receives `.start-empty`, so `:empty` applies only before population. The inspected renderer does not emit `.missing`; its opacity rule is a declared face without a demonstrated producer. Rows call `openPath` and removal stops propagation. The final `/* MODAL */` comment is assigned here as a boundary marker only; all following modal/settings rules remain C06. |
| Feedback cards, labels and inline forms | 2164–2182 | `timeline.js:9451+` builds `.fb-card-*` and feedback chip classes inside dashboard/settings DOM; native textarea/details/summary/pre carry forms and debug text. `.active` and label colors are readouts. Feedback is outside the project per DOM description; remote issue actions, token settings and validation remain controller concerns, not CSS permissions. |
| Audio strip, waveform rows, mute/selection and trim handles | 2183–2203 | Static `#audio-strip` (`index.html:2263`); `audio-bridge.js` builds `.audio-row/.audio-wave.muted`, `.lrow.audio-lrow.act`, `.audio-vol` and `.audio-trim-*`. Trim handles sit outside clipped waveforms and also reuse `.layer-inout-handle` from P03C-av. Pointer-inert dim overlays and canvas layout are not decoding, playback, trim-history or export acceptance. |
| Canvas/timeline import hover targets | 2204–2206 | `drop-import.js` toggles `.drop-hover` on `#canvas-area/#timeline-area`; dashed outline marks destination only. Filesystem/browser decoding, document import and native denial belong to import services. |
| Media drop target and expandable list shell | 2207–2227 | Static `#media-grid` plus media controls in `index.html:1243+`; `media-library.js:731+` binds drop to the grid, creates `.media-drop` empty hint and toggles grid/button `.expanded`. Container highlight works with populated lists; bounded overflow/max-height retains list scroll, not import authority. |
| Media row thumbnails, kind/link/missing and ownership metadata | 2228–2255 | `media-library.js:435+` builds `.media-row-*`, kind badges, linked/missing badges, size, owner/orphan and date; `linked-media.js` supplies broken-link state. Text labels supplement color. Desktop linked paths and browser handles differ; amber linked versus red missing does not prove resolution, relinking or project portability. |
| Media loading/progress and optimization indicators | 2256–2277 | `media-library.js` builds spinner/progress for `status:'loading'`, corner badge for `m.optimizing`; native-video bridge supplies optimization state separately. CSS keyframes are visual indeterminate activity, not measured job progress, decode readiness or animation evaluation. No reduced-motion override occurs in this interval. |
| Media search, filter and settings/status actions | 2278–2316 | Static `.media-search-*`, `.media-gear`, `.media-settings-pop/.media-statusbar`, count/missing badge/import buttons; media-library owns query/filter, `.show/.active/.open` and cleanup/import callbacks. Focus border and `.media-icon-btn:focus-visible` are explicit cues; destructive missing-entry cleanup needs controller policy, not red styling. |
| Media compact density and folder grid tiles | 2317–2348 | `media-library.js:295+` selects `.media-compact-view/.media-grid-view`, builds `.media-tile-*` using asset-tree folder bodies. Compact hides kind/owner/size while retaining linked/missing badges; grid uses `!important` flex overrides. View mode persists in localStorage, filter is session-only; CSS changes neither asset persistence nor insertion selection. |
| Transform anchor preset grid | 2349–2357 | Static `#xform-anchor-grid .xa-dot` buttons (`index.html:456+`); `timeline.js:10698+` toggles `.xa-active`, `.xa-center` changes center glyph. Current handler writes `state.xformAnchorKey` and per-stroke `data.xformAnchorKey`, then saves active frame: the preceding no-undo/no-render comment is incomplete beside this persistence/render code. Preserve that discrepancy; no native support is established by dots. |
| Layer color dot and preset/custom swatch popover | 2358–2372 | Timeline emits `.layer-color-dot` with `--dot-color`; `timeline.js:5850+` builds button `.lcs-swatch.sel/.lcs-custom` in `.lcs-pop`, delegating custom color to ColorPicker. Layer labels affect row/key tint; persistence/history and alpha codec consumers remain owning operations. |
| Align/distribute toolbar button faces | 2373–2377 | Titled static `.align-btn[data-align/data-distribute]` buttons (`index.html:418–432`); `timeline.js:10718+` calls `alignSelection`/`distributeSelection`. SVG sizing and hover are presentation; geometry, selection, history and native availability require separate acceptance. |
| Storyboard pan-space and world transform origin | 2378–2381 | `storyboard.js:231+` creates `#storyboard-space/#sb-world` with inline pan/zoom transforms. Hidden dotted space/grab cursor supplies only the container; later `.sb-module` belongs to the sibling starting 2382. `index.html:2301` declares storyboard frozen in development; no admission or installed availability is inferred. |

Load contract: `index.html:17` loads `css/style.css` before static DOM and the
classic scripts. UI loads before color-picker; brush editor/pickers and project,
timeline, audio, storyboard, drop-import and palette supply their named consumers.
Asset-tree loads before media-library/transplant/motion-preset-picker/assets-panel.
Shared context shell, preset rows, C06 settings tabs and P03C-av trim-handle rules
are cascade dependencies, not duplicate ownership. Inline display, `--sw-color`,
`--folder-color`, `--dot-color` and canvas sizes are part of those DOM interfaces.

| Applicable consumer dimension | Boundary retained by this inventory map |
|---|---|
| Save/load and undo/redo | DOM classes/overlays are not document records. Media entries, linked identities, audio ranges, palette/preset data, layer hierarchy/color, anchor metadata and project replacement retain codec/history obligations. Asset collapse is explicitly session-only; media density is localStorage UI preference. The anchor handler persists per-stroke data despite its older comment. No persistence/history behavior is accepted. |
| Selection and animation | Active tabs, layer/audio selection, swatch matches, favorite choices and transform pivots mirror controller state. CSS keyframes spin indicators only. Symbol navigation, reorder, trim, align/distribute, preset application and media insertion must retain native commands or explicit unavailability; this census grants no editing/evaluation acceptance. |
| Render/export and native bridge | Thumbnail/preview/waveform canvases and DOM hints are presentation consumers, not authoritative viewport/export production. Media decode/resource/optimization and audio export require their native services. The current export-modal insertion is separate C06-area drift. No Rust port, resource lifetime, native callback or final render acceptance is claimed. |
| Browser and installed desktop | Common CSS applies to both UI hosts, with WebKit scrollbar specificity and linked-path versus browser-handle distinctions. Tooltip/menu placement, hover/focus, overflow, canvas previews, WebGPU/AppKit overlay interactions and accessibility require actual surface checks. Titles, native buttons/forms and explicit focus cues coexist with div/span click controls and hover-only favorites; no ARIA/keyboard or reduced-motion acceptance is implied. No browser or installed behavioral run occurred. |

The 31 intervals cover **494 frozen lines exactly once**, disjoint from siblings
through 1855 and from 2382 onward, and C06 91–104, 422–461, 2132–2163. No source
identity, original packet, `rangeCoverage`, whole-span disposition or
`complete:false` changes. The JSON CSS reason points here without refreeze-fragile
new schema fields.

Focused validation parses this table against the independent union 1856–2131,
2164–2381; rejects omission, duplication and C06/sibling intrusion; compares the
two frozen/current slices (second offset +4) and verifies representative DOM and
controller tokens. Integrity-only and 53 scope tests check index consistency,
not semantic completeness or behavior; the wider admission queue remains open.

## P03C-ax frozen specialized-mode and editor presentation

[P03C-ax / #1596](https://github.com/mysteropodes/nemo/issues/1596) maps frozen
`src/css/style.css` 2382–2850, the final 469 lines, into named UI presentation
responsibilities. Frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5` / CSS
blob `383eeec4be64457fdc7f2028f6d9b04cac32dbc8`; inspected protected base
`746c5f19fa43645d2ba9aa88036415b7412209ed` / CSS blob
`a4a9e3cab070289eaa57f07c9188ec00bc203335`. This entire slice is byte-identical
at current 2386–2854: four export-modal lines inserted after frozen 2132 shift
its coordinates. That C06-area insertion is not claimed here. DOM/JS references
below describe the inspected protected source, not a frozen runtime trial.

Each row owns its exact presentation interval, including comments/blanks.
These names supplement the census; they do not admit or complete packets.
CSS/HTML remains UI. Every stateful operation must use the Rust-backed shared
application command/query/job API or be explicitly unavailable; retained legacy
controller code below is evidence of a consumer, not permission to run its writer.

| Named UI responsibility | Frozen CSS lines | Selector/DOM, consumer and accessibility/state contract |
|---|---:|---|
| Storyboard instance card and component entry face | 2382–2388 | `storyboard.js:557+` emits `.sb-module.sb-instance`, thumb, side, edit and truncated name. Inline dimensions/background and hover thumbnails specialize the card; div edit/double-click/drag callbacks own navigation. Storyboard is frozen by `SM_FROZEN_IN_DEV` and the disabled mode button, not accepted through these selectors. |
| Legacy storyboard montage lane shell | 2389–2396 | `.sb-montage/.sb-montage-head/.sb-montage-lane/.sb-hint` describe a lane and play face. The inspected controller instead builds v2 `.sb-montageblock`; no producer of these old lane classes was found in `storyboard.js`. Retain as frozen presentation debt, not demonstrated live DOM or montage functionality. |
| Legacy montage chip, trim and playhead faces | 2397–2403 | `.sb-chip` has no producer in the inspected v2 controller; `.sb-trim.left/.right` is reused by chained instance cards and `.sb-ph` by the ruler. Grabbing/trim cursors and pointer-inert playhead do not implement trim, stretch, timing or history. |
| Storyboard sound card and legacy audio/drop faces | 2404–2412 | `storyboard.js:1160+` emits `.sb-sound/.sb-wave/.sb-wave-cursor/.sb-sound-play`, with canvas dimensions and cursor position inline. No `.sb-audio-row/.sb-audio-blk` or lane `.drop-hint` producer was found there. Waveform display and old faces neither admit audio decode/playback nor unblock frozen storyboard. |
| Storyboard v2 montage anchor, chain and ruler | 2413–2422 | `renderMontageBlock`, chain layout and `positionRuler` emit `.sb-montageblock.active`, `.sb-block-play`, `.chained`, `.sb-ruler/.sb-ruler-lbl/.sb-ph`. Inline world coordinates, duration width and playhead offset supply geometry; CSS is the shared card override, not chain membership/evaluation authority. |
| Motion graph overlay and pinned legend | 2423–2438 | `motion-graph.js:185+` creates `#motion-graph` inside `#fg-wrap`, renders SVG `.mg-key/.mg-ease` and updates `.mg-legend` scroll offset and `#btn-mgraph.on`. Controller inline top/width/height/display wins over defaults. The resize sibling is styled inline outside this slice; graph rows must preserve CLAUDE.md §11 alignment/scroll, without accepting key gestures. |
| Layer parent pill and pickwhip targeting | 2439–2458 | `timeline.js:5975+,6184+` builds `.lparent.none`, `.lpick`, `.lpick-line` and toggles `.lrow.pick-target`; `.lrow.act` remains selection feedback. Ellipsis and fixed pointer-inert dashed line separate menu and drag faces. Parent identity, cycle prevention, assignment and history belong to commands, not CSS. |
| Motion parent crossfade/readout pill | 2459–2473 | `motion.js` parent/follow-path/matte rows reuse `.lparent.motion-parent-pill`; parent B adds `.blendable`, `.mp-fill` width and `.mp-label`. The broader-width cascade overrides the layer-list pill. Blend percentage, click/drag disambiguation and parent evaluation remain controller/native obligations. |
| Follow-path alignment toggle face | 2474–2481 | `motion.js:7939+` creates button `.mp-align-btn.on` from `fp.align`. Hover/on colors indicate alignment state; path selection, alignment mutation and evaluated transforms are not CSS behavior. |
| Collapsed effects count badge | 2482–2489 | `effects-panel.js:775+` appends/removes `.fx-count` under `.phdr` from effect count. A badge preserves a collapsed-section readout; it neither discovers native effects capabilities nor owns effects lists. |
| Script panel widgets and HTML plugin frame | 2490–2513 | `nemo-panel.js` builds `.npanel*` floating panels, drag bar, native buttons/inputs/select/check/slider and callback handles; `nemo-plugin.js:135+` creates `.nplugin-frame` in the shared panel. The plugin is a same-origin unsandboxed iframe per its source contract. CSS grants neither trust isolation nor script mutation/native availability; span field labels are not automatically associated input labels. |
| Fill-gradient trigger thumbnail and availability face | 2514–2525 | Static `#p-fill-grad-btn .fill-grad-ramp` (`index.html:895`) and `timeline.js:8907+` set `.on/.off`. Miniature gradient and dimming communicate state; `.off` alone is not native disabled or denial. Actual gradient editing/selection and codecs retain their command contracts. |
| Effect parameter stopwatch face | 2526–2533 | `effects-panel.js:634+` emits `.lico.fx-stopwatch.on` and an SVG reflecting keyed/current-frame state. Color and cursor supplement the glyph; key creation/removal and evaluation belong to animation commands. |
| Effect key navigation and count faces | 2534–2544 | `effects-panel.js:660+` creates `.fx-keynav/.fx-keynav-btn.off/.fx-keynav-count` for previous/toggle/next. `.off` visually mutes a div; event/controller denial, keyboard semantics and frame navigation need separate acceptance. |
| Composition/layer marker overlays | 2545–2557 | `markers.js:124+` creates `.tl-marker/.tl-marker-layer/.tl-marker-label` inside `#bars-row` or layer `.frow`, setting frame*FC and `--marker-color`. Out-of-range overlays are skipped without clamping stored markers. Drag/rename/remove callbacks and composition/layer scope are state/history obligations; the label is pointer-inert. |
| BPM beat/bar guide overlay | 2558–2565 | `bpm-grid.js:48+` rebuilds `.bpm-line.bpm-bar` inside `#frame-grid`; frame/beat positions are inline and pointer-inert guides sit behind row content. BPM settings, time conversion, persistence and animation evaluation are not owned by the lines. |
| Work-area duration label | 2566–2571 | `ui.js:985+` emits `.wa-dur` in the work-area bar. Centering and pointer-events:none preserve the bar's drag surface; work-area bounds, frame count and export range stay controller/native data. |
| Keyframe tint/current/selection cascade | 2572–2580 | `motion.js` renders `.motion-key.tinted/.cur/.sel` with `--key-color`. Later current and selected rules outrank custom tint and repeat the earlier selection face from P03C-av. This is a cascade dependency, not duplicate interval ownership or acceptance of key selection/color persistence. |
| Selected-key incoming/outgoing influence widgets | 2581–2611 | `motion.js:11720+` builds sibling `.motion-key-ease-box.in/.out`, marker and number `.scrub.motion-key-ease-input`; `.dragging` keeps the box visible. Invisible boxes retain pointer-events:auto for their own hover; focus suppresses native spin controls. Numeric scrubbing, influence limits/history and reduced-motion/keyboard behavior remain unaccepted. |
| Expression raw/evaluated/error and shared action faces | 2612–2621 | `motion.js:8896+` emits `.motion-expr-raw/.motion-expr-out.err` and `.motion-expr-glob`; other motion actions reuse the latter. Error color distinguishes evaluated output from raw value; CSS neither evaluates expressions nor establishes global/action mutation authority. |
| Inline expression editor pane, gutter and textarea | 2622–2633 | `motion.js:9089+` creates `.motion-expr-pane/.motion-expr-gutter/.motion-expr-code` with gutter `.err` spans. Fixed initial height, no resize and preformatted overflow rely on paired panel/grid row height. Textarea/gutter rendering is not expression compilation, enabled-state or persistence acceptance. |
| Inline expression resize and reference pickwhip | 2634–2643 | `motion.js:9166+` builds `.motion-expr-grip`, `.motion-expr-whip` and `.motion-prop-row/.motion-track-row.pick-target`. Custom grip updates paired rows, with legacy-write guards at the editor-height handler. Reference injection, target identity and history require native support or denial. |
| Fill-gradient editor inset shell | 2644–2645 | Static `#p-fill-gradient-editor.fill-gradient-editor` (`index.html:902`) begins inline display:none; timeline controls opening it. Border/background distinguish the inset editor from its section. Gradient data/rendering remain separate from this single shell rule. |
| Render-manager modal, toolbar and queue progress shell | 2646–2655 | Static `#render-manager-modal` and `render-manager.js` toolbar/progress/list use `.rm-progress-*`, `.rm-queue-list/.rm-empty-hint`. Inline progress width/text carries job readouts and the modal reuses C06 shell classes. Width transition and scrolling do not produce native frames, progress truth or export acceptance. |
| Render queue item enable/expand/status/action faces | 2656–2670 | `render-manager.js:336+` builds `.rm-item.expanded/.disabled`, checkbox, chevron, name, badge, `.rm-item-status.rendering/.done/.error`, duplicate/delete divs. Actual checkbox disabled state while rendering comes from JS; dimmed names and status glyphs are presentation, not cancellation, completion or filesystem proof. |
| Render queue output/path parameter form | 2671–2678 | `render-manager.js` field builder emits `.rm-item-fields/.rm-path-row/.rm-path-browse/.rm-scale-computed/.rm-output-preview`; inputs retain values in controller. Break-all preview and browse face do not validate destinations, encoding, scale or browser/native filesystem capability. |
| Linked-media permission banner and actions | 2679–2693 | Static `#linked-media-banner` plus `linked-media.js:183+,708+` toggles `.show` from pending permissions, supplies message, real request button and dismiss. Re-grant uses an actual click on browser FileSystemFileHandles; dismiss hides UI without resolving permission. CSS cannot restore reads silently or establish desktop linked-path portability. |
| Render queue error detail | 2694–2695 | `render-manager.js:376+` emits `.rm-item-error` when status is error, with textContent. Error detail complements the earlier status face; it is not a retry policy or evidence of validated error recovery. |
| Split expression editor stage wrapper and header | 2696–2715 | `expr-code-panel.js` moves existing `#canvas-area` into `#expr-split-row` beside `#expr-code-panel`, building `.ecp-head/.ecp-title/.ecp-close/.ecp-enable`. min-height/min-width preserve stage/timeline layout; controller restores the canvas and dispatches resize on close. It uses the same holder ref and expression commit path as inline editing, not a second expression store. |
| Split expression code/highlight/gutter and token layers | 2716–2745 | `expr-code-panel.js:202+` produces `.ecp-pane/.ecp-gutter/.ecp-code-wrap/.ecp-highlight/.ecp-code`, `.ecp-line.err` and `.tok-*`. Escaped highlight HTML behind transparent textarea shares font/padding/source-line rhythm; native caret/selection remains on the textarea. Syntax coloring and error tint do not compile or evaluate code. |
| Split editor error, width grip and inline launch face | 2746–2753 | `expr-code-panel.js` emits `.ecp-err/.ecp-grip` and stores width under `nemo-expr-panel-width`; `motion.js:9068+` emits `.motion-expr-pop`. Error text, width preference and launcher layout are UI concerns; expression commits still require the common native command or explicit unavailability. |
| Expression examples/functions dropdown | 2754–2762 | `expr-code-panel.js:83+` emits `.ctx-menu.ecp-examples-menu`, category and item classes and `.ecp-close.ecp-examples-btn`; definitions come from expr-functions/expr-examples loaded before the panel. Menu height/indent and SVG sizing reuse P03C-aw context styles; inserting example text is not expression execution or scripting acceptance. |
| Start-screen alpha disclosure | 2763–2775 | Static `#start-alpha-note/.start-alpha-badge/.start-alpha-text` (`index.html:102+`) uses translated text. Amber badge/paragraph style presents the existing alpha warning; CSS does not establish backup, project-format or recovery behavior. |
| Document properties identity block cascade | 2776–2791 | `timeline.js:3394+` moves `#canvas-sec` and toggles `.psec-identity` in Document context. Transparent body/background overrides generic `.psec/.pbdy`, matching selection identity styling. Context movement and field synchronization remain UI/controller obligations, not document ownership. |
| Start-screen repository/community links | 2792–2800 | Static `#start-social/#start-github/#start-discord` (`index.html:106+`) contains real outbound anchors. Wrapping/hover changes link presentation; destinations and host navigation are DOM/browser behavior, not project commands or availability. |
| Document background/view row wrapping and square swatch | 2801–2823 | Static `#canvas-sec .doc-bg-row #p-cbg` (`index.html:817+`) opts into wrapping and fixes swatch size beside view buttons. Generic `.pr` and inline gap are cascade inputs. Width/height/FPS fields remain separately laid out; background persistence and view modes are not accepted by responsive CSS. |
| Combined-shape inline section chrome | 2824–2830 | `timeline.js:3421+` toggles `.psec-inline` when moving the combined-shape section into selection properties. Transparent border/body spacing is an override of existing section chrome; selection, compound geometry and history remain owning operations. |
| Text animator per-character weight ramp | 2831–2839 | `text-animator-panel.js:391+` creates `.ta-ramp` bars or `.ta-ramp-empty`, reading `textAnimatorWeights` and inline bar heights. The renderer also consumes those weights; common input is source evidence, not proof that evaluation and rendered glyphs agree at runtime. |
| Text animator direct-row spacing and separators | 2840–2850 | Static `#p-textanim-list` (`index.html:1124`) receives direct rows from text-animator-panel and `.ta-sep` at its builder. Flex gap owns list rhythm while separator margin stays absent; animator order/selection, animation storage and render/export behavior remain outside this presentation interval. |

Load contract: `index.html:17` loads the shared CSS in the head before body DOM
and downstream app/controller scripts, after the classic gpu-gate/Paper bootstrap.
Motion precedes expression catalog/panel, graph and script consumers; nemo-panel
precedes nemo-script/nemo-plugin, and text-animator precedes its panel. Timeline/UI
provide parent/work-area faces; markers/BPM/effects/storyboard supply their later
consumers. The expression panel calls `exprSnapshotFor`/`applyExprCode` rather than
maintaining independent code, and requires catalog definitions before opening menus.
C06 modal shell, P03C-at/au section fields, P03C-av key faces and P03C-aw context
menus/storyboard world are cascade dependencies only. Inline dimensions, position,
display, progress widths, `--marker-color` and `--key-color` remain DOM interfaces.

| Applicable consumer dimension | Boundary retained by this inventory map |
|---|---|
| Save/load and undo/redo | Markers, BPM/work-area, parent/follow-path/blend, gradients, effects/keys/colors, expressions, storyboard modules and text animators retain document codec/history obligations. Split-editor width is localStorage UI preference; panel geometry, menu/error/status faces are not document records. Retained old JS callbacks are not migration acceptance. No save/reload/history trial occurred. |
| Selection and animation | Active cards, parent targets, key selection/tint, error/readout states and animator ramps mirror their producers. CSS transitions animate UI opacity/width only. Storyboard stays explicitly frozen; graph/key/effect/marker/parent/expression operations require native commands or explicit unavailable denial without a legacy writer fallback. No gesture, expression timing or animation parity is accepted. |
| Render/export and native bridge | DOM/SVG graphs, thumbnails, waveform/ramp previews and progress faces are presentation readouts. Authoritative viewport/evaluation, media decode/permission/resource lifetime, output paths/jobs and render queue completion belong to native services and declared adapters. Split-stage resize must preserve the real surface. No Rust port, GPU/export job or native callback acceptance occurred. |
| Browser and installed desktop | Shared CSS is source-visible on both hosts; linked FileSystemFileHandle permission versus desktop paths is capability-specific. Actual scrolling, paired row alignment, resize, stacking over native viewport, iframe interaction/trust, focus/keyboard and hover-only invisible influence boxes need surface checks. Native form/anchor semantics coexist with div/span drag/click controls and outline:none fields; no accessibility/reduced-motion, browser or installed acceptance is implied. |

The 39 intervals cover **469 frozen lines exactly once**, disjoint from siblings
through 2381 and C06 91–104, 422–461, 2132–2163. The slice ends at frozen EOF.
No source identity, original packet/admission, `rangeCoverage`, whole-span
disposition or `complete:false` changes; exactly 760 original packets remain
pending. The JSON CSS reason points here without adding schema fields.

Focused validation parses this table against the independent union 2382–2850,
rejects omission/duplication/C06 or sibling intrusion, compares the frozen/current
slice at offset +4 and checks representative DOM/controller/load tokens. The
integrity-only check and 53 scope tests establish consistency, not semantic
completeness. Normal completeness still fails while packet admissions remain open.

## P03C-ay frozen start-screen, canvas and tool HTML shell

[P03C-ay / #1597](https://github.com/mysteropodes/nemo/issues/1597) maps frozen
`src/index.html` 1–356 except bootstrap script tags 9–10 into named UI
presentation responsibilities. Frozen source `3f6eed2a500f2ce868b711e063816029eb8fefa5`
/ HTML blob `0f928bd885b0acd32acde1d52726f1e42f1370c5`; inspected protected base
`33426c3fa4e460161491d4c177ce7c95a89de828` / HTML blob
`8eb7b2e2ca81284c9e00b22251391d2292654de9`. The entire 1–356 slice is
byte-identical at current coordinates. Current HTML changes occur after this
slice, beginning with the export-cancel insertion after frozen 1678; later
script changes do not shift these early coordinates. DOM/controller references
below identify the protected source, not a browser or installed behavioral run.

Each row owns its exact HTML presentation interval, including comments/blanks
and structural closure. These are supplemental census names, not admitted
packets. CSS/HTML remains UI; stateful behavior requires the Rust-backed shared
application command/query/job API or explicit unavailability without an old
writable fallback. A retained controller or legacy comment is not runtime credit.

| Named UI responsibility | Frozen HTML lines | Public DOM/load interface, CSS/controller consumers and state boundary |
|---|---:|---|
| Document language, metadata, title and favicon shell | 1–8 | Doctype, `html[lang=fr]`, UTF-8, viewport, versioned title and two PNG favicon links define document metadata. `i18n.js` supplies language at runtime; version/title fallbacks must remain consistent with version policy. Initial language/title are source facts, not runtime localization/version acceptance. |
| Font/style resources and head/body boundary | 11–20 | Google Manrope preconnect/font stylesheet, then `css/style.css` and `css/tutorial.css`, precede body DOM/downstream app controllers. Earlier classic gpu-gate/Paper tags 9–10 remain bootstrap ownership. CSS font fallbacks cover offline presentation; successful network font loading/CSP and startup order require surface evidence. |
| Start overlay branding and tagline | 21–31 | `#start-screen/#start-inner/#start-brand*` and decorative logo `alt=""` use shared start CSS; `data-i18n=startTagline` supplies translation. `project.js` toggles `.hid`. Branding shell does not open, replace or own a project. |
| New/open/resume action cards | 32–49 | `#start-new/#start-open/#start-resume/#start-resume-sub` are div cards; resume starts inline display:none. `project.js:659+` adds `.has-resume`, clears display after localStorage/IndexedDB discovery and uses native open/resume admission and reveal paths. CSS/visible card is not valid autosave, successful replacement or keyboard-button semantics. |
| Kitsu project-opening entry | 50–58 | `#start-kitsu.start-row` includes translated production text and decorative SVG. `kitsu.js:423+` binds the entry; network/auth/project selection stays in that controller and application entry service. A start-screen link grants no remote synchronization or document acceptance. |
| Start tutorial entry | 59–67 | Div `#btn-open-tutorial` reuses start-row CSS; `tutorial.js:1896+` binds this and the topbar button. Text describes lessons while the tutorial owns its state/spotlight/steps. Clickable div semantics and first-run behavior are not accepted by this host row. |
| New-project parameters and actions | 68–86 | `#start-newpanel`, name/preset/custom dimensions/FPS inputs and create/cancel buttons reuse `.phdr/.pbdy/.pr/.pi/.psel/.pbtn/.scrub`. `project.js:697+` controls custom-row display and guarded async `SMProject.newProject` admission, with a generated alert on failure. Form defaults/min values are not authoritative validation or successful project creation; span captions are not associated labels. |
| Recent-project list host | 87–90 | `#start-recent/#start-recent-list` accepts dynamic rows from `project.js:637+`; removal stops propagation, opening uses `openPath`. Empty host and translated heading are presentation; recent paths, missing files and persisted list integrity retain controller obligations. |
| Alpha disclosure | 91–105 | Static `#start-alpha-note/.start-alpha-badge/.start-alpha-text` mirrors README wording with translated text. Comment claims visibility without scrolling; this census records no viewport trial to establish it. Disclosure does not prove backup, recovery or format stability. |
| Repository/community anchors and start closure | 106–111 | `#start-social`, real `#start-github/#start-discord` anchors use target=_blank/rel=noopener and aria-hidden icon SVGs, closing the start containers. CSS wraps the links. Outbound navigation is a browser/host action, not project mutation or a verified destination response. |
| macOS titlebar/update anchors and application root | 112–122 | `#mac-titlebar-strip[data-tauri-drag-region]`, blank-title `#mac-update-btn` and `#app` use macOS overlay CSS. The comment names `mac-chrome-init.js`, but no such file exists: `updater-bridge.js:162+` sets `.mac-overlay-titlebar` and update states. Desktop drag/update/relaunch and accessible button naming require actual host evidence. |
| Topbar/menu shell | 123–129 | `#app-topbar/#app-menu-btn` hosts native button/SVG with translated title. Shared CSS positions it above canvas-scoped tabs; `timeline.js:9075+` opens the context menu. Menu actions remain application operations, not HTML authority. |
| Mode switch and spacer | 130–141 | `#app-mode-switch .app-mode-btn[data-mode]` declares disabled `.in-dev-locked` Storyboard, active anim2d and Motion; spacer fills remaining width. `motion.js.setAppMode` reads C06 `SM_FROZEN_IN_DEV` and changes mode classes. Storyboard stays unavailable; active HTML/default titles grant no animation-mode/native parity acceptance. |
| Comment tool and feedback hosts | 142–152 | `#topbar-comment-btn.tool-btn[data-tool=comment]` retains location-independent tool wiring; `#fb-avatars/#fb-avatars-pop` receive timeline feedback DOM. `.tool-btn` active state is shared with the tools panel. Comment data, identity and remote feedback operations remain their controller/API responsibilities. |
| History trigger and popover | 153–159 | `#history-btn/#history-pop` supply button and host for `history-panel.js:125+`. The comment describes session undoStack/redoStack snapshots; current native history/denial contracts govern authoritative undo/redo. Host placement is not history replay, persisted history or jump acceptance. |
| Topbar tutorial/settings triggers and closure | 160–162 | `#btn-open-tutorial-topbar` and `#project-tabs-settings` are titled native buttons. Tutorial binds the former; C06 settings-modal controller consumes the latter (`timeline.js:8986+`). This row owns trigger markup/closure only, not C06 modal/tab behavior or translated settings semantics. |
| Top-area/tools-container shell | 163–174 | `#top-area/#tools-panel` establishes the dockable tools sibling layout. Its multi-project comment describes historical serialized snapshots; native project/tab authority must govern current admission. `tools-panel-dock.js`/shared CSS position the panel and retain localStorage dock preference. |
| Selection tool group | 175–183 | Native `.tool-btn[data-tool=select/subselect/fsselect]`, shortcut labels, SVGs and separator preserve exact selectors consumed by timeline/tool bridges and tutorial spotlights. Legacy aspect-selection commentary is not persistent fill/stroke selection or native editing acceptance. |
| Drawing and rig tool group | 184–200 | Draw/pen/fillbrush/rig buttons, initial `.active` draw and `.in-dev` rig use shared tool CSS and `SM.setTool`; draw/pen/rig bridges own gestures. Rig's older unfreeze comment describes bootstrap gating, not native implementation parity. No brush/pressure/bone/undo behavior is granted by visible controls. |
| Shape-tool stack | 201–217 | `#shape-tool-stack.tool-stack` retains five real `.tool-btn[data-tool]` buttons; timeline's `SMShapeGroup` toggles `.stack-front`, hiding others with visibility rather than display. Tutorial relies on their real rectangles/selectors. Stack geometry is UI; shape creation/parameters and keyboard picking need their owning operations. |
| Text/eraser/fill/eyedropper tool group | 218–225 | Data-tool text/eraser/fill/eyedropper buttons and separators retain title/shortcut/Motion-key conflicts and icon interfaces. Global setTool wiring, respective bridges and tutorial consume them. Raster/vector text, erase/fill/color sampling and selection/history obligations are not HTML functionality. |
| Hand/zoom/rotate group and removed-tool commentary | 226–242 | Hand/zoom/rotate data-tool buttons retain pan/zoom/rotation cursors/shortcuts via tool controller. Comment records comment-tool relocation and perspective/symmetry entry through Labs. Labs owns those prototype toggles; old 'fully functional' wording is not native/runtime evidence. |
| Stroke/fill swatches, toggles and swap | 243–265 | `#stroke-well/#fill-well.none`, native color inputs, `.cw-eye` enable divs and `#tools-invert-btn` preserve stable IDs. Timeline binds fill/stroke enabling and color callbacks; `ui.js:740+` binds swap. CSS faces/hex6 input defaults do not replace CLAUDE.md hex8 alpha codec contract or accept selection/persistence/history. |
| Tools docking handle and resize anchor | 266–275 | `#tools-panel-handle.tools-dock-handle` is last child; `#tools-panel-resize` follows the panel. `tools-panel-dock.js` finds handle by ID and persists dock position; UI sizing/shared CSS handles resize and `.tools-docked-away`. Pointer div handles are not keyboard docking/resize acceptance. |
| Canvas-column project-tab hosts | 276–282 | `#canvas-col/#project-tabs-bar/#project-tabs-list/#project-tab-add` scope dynamic tabs to canvas width. `project.js:630+` binds add and builds tabs; CSS/expr-code-panel consume the column layout. Tab publication and outgoing/incoming save/load must preserve native admission; DOM hosts do not accept multi-project behavior. |
| Linked-media permission banner | 283–292 | `#linked-media-banner` and message/request/dismiss IDs use `.linked-media-banner*` CSS. `linked-media.js` toggles `.show` from pending browser handles, requests permissions on real button clicks and hides without granting permission on dismiss. Native linked paths/browser FileSystemFileHandles retain distinct capability/lifetime contracts. |
| Canvas viewport/input anchor | 293–294 | `#canvas-area` wraps `<canvas id=drawing-canvas resize>`; app/engine/tool bridges attach sizing, input and presentation. `engine-bridge.js:4055+` locates the canvas and split-editor moves rather than recreates it. Paper resize attribute is legacy host wiring, not authoritative native viewport/frame production. |
| Ruler canvases and corner | 295–304 | `#ruler-corner/#ruler-h/#ruler-v` overlay the viewport in explicit DOM order. Shared CSS places them; `rulers-bridge.js:394+` draws/wires rulers and body `.rulers-off`, with document-level capture handling before canvas tool interception. Guide persistence/snapping and input ordering are separate operations/validation, not ruler pixels. |
| Component-navigation commentary and canvas readouts | 305–322 | Component/precomp comment refers to app/motion symbol entry and sibling symbol-tabs elsewhere. `#canvas-info/#info-frame/#info-badge/#info-strokes/#info-sel` are readouts updated by timeline. Inline hidden badge and selection text/color are UI; component identity, frame timing and authoritative selection remain state/API obligations. |
| Match readout and zoom/fit controls | 323–335 | `#match-info/#canvas-zoom-pills/#canvas-fit-btn/#zoom-scrub.scrub` use shared canvas CSS. Timeline:11046+ applies zoom input and fit context actions; app syncs zoom value, UI generic scrub supplies pointer behavior. Numeric bounds/fit div are presentation interfaces, not native camera/viewport or keyboard acceptance. |
| Canvas closure and Labs sibling-layout lead-in | 336–356 | Closes `#canvas-area`, then the comment explains why Labs must be a sibling to avoid capture interception. Actual Labs DOM starts frozen 357, owned by C06.labs.float-panel; no Labs element/position/state ownership is added here. The historical synthetic-pointer observation in the comment is not a new live validation receipt. |

Load/cascade contract: excluded tags 9–10 execute before head styles. Shared and
tutorial CSS precede body markup; downstream classic app/controller scripts find
these exact IDs/classes/data attributes after their declaration. `i18n.js` owns
the app-wide data-i18n/title sweep, not each translated DOM node's presentation.
Accepted P03C-at/au/aw/ax CSS census rows are read-only cascade dependencies, not
duplicate code-line claims. Controller-generated tabs/recents/feedback/history
and inline display, dock classes, active/tool/mode state, canvas dimensions and
permission `.show` are part of the DOM interface.

| Applicable consumer dimension | Boundary retained by this inventory map |
|---|---|
| Save/load and undo/redo | Start/open/resume/new-project/tab surfaces invoke project admission and persistence; stored autosave existence alone is not a valid/revealed native document. History UI must use native history or deny unavailable actions. Color/style defaults, project dimensions, guides and tool-derived mutations retain codecs/history obligations; dock preference is localStorage UI state. No persistence/history trial occurred. |
| Selection and animation | Tool/mode/default active faces, aspect selection, shape-stack geometry, component navigation, frame/badge/count readouts and tutorials mirror controller state. Stateful tool gestures, component entry, feedback and animation require native API support or explicit unavailability. Disabled/frozen Storyboard is not feature completion, and inherited comments cannot establish current parity. |
| Render/export and native bridge | Drawing/ruler canvases, viewport shell, overlays and zoom/readout fields are presentation/input consumers. Native engine owns authoritative document evaluation, viewport/render/export production and media resources. Open/new/resume/tab publication must retain native admission/reveal; macOS chrome/update is host-specific. No GPU, export, native callback or packaged viewport acceptance is claimed. |
| Browser and installed desktop | Shared shell applies to browser/WebView; macOS drag/update and linked permission capabilities differ. Font fallback/network, overlay stacking, tool capture, docking, resizing, focus/title/localization and responsive start visibility need actual surface evidence. Native buttons/anchors/inputs coexist with div action cards, fit/eye/drag controls, span captions and an initially blank-title update button. No accessibility, browser or installed behavior acceptance occurred. |

The 31 intervals cover **354 frozen lines exactly once**: 1–8 and 11–356.
Excluded bootstrap 9–10, C06 Labs 357–366, settings 1743–1933 and feature flags
2285–2297 remain untouched. The settings trigger is a C06 consumer interface,
not its controller ownership. No numeric C06 overlap occurs here; any later
cross-leaf reconciliation belongs to P03C-bc/#1601. No source identities, original
packet/admission, JSON file order/classifications, `rangeCoverage`, whole-span
disposition or `complete:false` changes; 760 original packets remain pending.

Focused checks parse this table against independent 1–356 minus 9–10, reject
omission/duplication/bootstrap/C06/sibling intrusion, compare frozen/current
coordinates and verify representative DOM/CSS/controller/load tokens. Integrity
and 53 scope tests establish consistency, not semantic completeness or behavior;
normal completeness remains failing while the wider packet queue stays pending.

## P03C-az frozen inspector and document-control HTML

[P03C-az / #1598](https://github.com/mysteropodes/nemo/issues/1598) maps frozen
`src/index.html` 367–1098 into named UI presentation responsibilities. Frozen
source `3f6eed2a500f2ce868b711e063816029eb8fefa5` / HTML blob
`0f928bd885b0acd32acde1d52726f1e42f1370c5`; inspected protected base
`1913e8d5662d16724eff688d693918b386b99050` / HTML blob
`8eb7b2e2ca81284c9e00b22251391d2292654de9`. The 732-line slice is
byte-identical at current coordinates; later export-modal/script insertions do
not shift it. Consumer references below describe that protected source, not a
browser or installed behavioral trial.

Each interval names exactly one presentation owner, including comments/blanks
and closure. Line 367 is a structural-closure marker for C06, not a second Labs
behavior owner: it closes `#labs-float-panel` opened at 357, while C06's numeric
claim ends at 366. Preserve this semantic boundary for the single reconciliation
in [P03C-bc / #1601 comment 6043259956](https://github.com/mysteropodes/nemo/issues/1601#issuecomment-6043259956).
These supplemental names neither admit nor complete packets. HTML/CSS remains
UI; stateful operations require the Rust-backed application command/query/job API
or explicit unavailability, without an old writable fallback. Legacy controllers
and comments are consumer evidence, not native behavior acceptance.

| Named UI responsibility | Frozen HTML lines | DOM/CSS/controller interface and state boundary |
|---|---:|---|
| C06 Labs structural closure boundary | 367–367 | Closing div for `#labs-float-panel` opened at 357. C06.labs.float-panel retains all panel behavior/DOM ownership; this marker completes the numeric slice without assigning another Labs controller. See #1601 reconciliation above. |
| Tween reassignment badge host | 368–378 | Hidden `#tween-reassign-badge` is a fixed sibling of canvas-area to escape tool capture. `tweens.js.updateReassignBadge` positions/tints it and wires reassignment. Badge visibility/legacy next-key commentary is not native target identity, animation/history or reassignment acceptance. |
| Tracking marker and canvas-column closure | 379–384 | Hidden pointer-inert `#track-marker` has inline fixed geometry/z-index, updated by `tracker-panel.js:28+`; outer canvas column then closes. DOM marker avoids live document items, but tracking results/transforms and native service availability remain separate. |
| Inspector resize anchor | 385–386 | `#props-panel-resize` uses the shared tools/props resize CSS and UI sizing bindings. Translated title is an affordance; width persistence, input capture and keyboard resize need surface checks. |
| Inspector context header and collapse rail | 387–408 | `#props-panel`, context/header row, collapse button and rail use CSS `.collapsed` and `timeline.js:11828+` rail rendering. Collapse persists in localStorage; state-dependent title uses afterI18n, deliberately avoiding a static data-i18n-title. Context selection/relevant-section routing remains controller-owned, not document authority. |
| Selection identity wrapper | 409–410 | Hidden `#sel-props-sec` and padded `.pbdy` host selection controls. `timeline.js.updatePropsContext/updateSelPropsPanel` drives visibility/values and moves related sections. Initial hidden wrapper accepts no selection or persistence semantics. |
| Align/distribute toolbar | 411–435 | Hidden `#align-toolbar`, `.align-btn[data-align/data-distribute]` native buttons and `#sel-count` use shared toolbar CSS; timeline:10718+ dispatches align/distribute. Data attributes/geometry are public UI inputs, not accepted native selection transforms or undo. |
| Selection transform fields and proportion lock | 436–451 | `sp-x/sp-y/sp-w/sp-h/sp-rot`, `.sp-xform-row/.scrub` and size-lock button align numeric position/size/rotation. Timeline selection panel and UI generic scrub bind them. Bounds/defaults/title are UI, not authoritative limits, transform evaluation or stable pivot/history. |
| Transform anchor picker | 452–467 | `#xform-anchor-grid .xa-dot[data-key]` emits nine native buttons, center glyph and titles. Timeline:10698+ toggles `.xa-active` and writes per-stroke xformAnchorKey/save data despite the older no-undo/no-render comment. Preserve that discrepancy; this DOM map does not validate pivot codec or render consumers. |
| Selected-point type buttons | 468–472 | Hidden `#sp-pointtype-row` hosts corner/smooth/symmetric buttons; timeline shows it for subselect node selection and corresponding handlers change handles. Native button names do not accept tangent geometry, selection or native history. |
| Destructive boolean controls and selection closure | 473–485 | Hidden `#sp-boolean-row` contains four `btn-bool-*` SVG buttons. Timeline routes immediate union/subtract/intersect/exclude separately from combined-shape controls below. Titles and monochrome icons do not accept geometry/destructive mutation or undo. |
| Revision accept/reject interface | 486–495 | Hidden `#revision-sec`, author row and accept/reject native buttons are filled/wired by timeline:3012+. Document correction/original identity and history are operation obligations; header/empty row are not accepted collaboration or revision application. |
| Mask mode/feather/unset controls | 496–503 | Hidden `#mask-sec`, `p-mask-mode`, feather scrub and unset button are synchronized/bound by timeline:2851+. Modes/500px UI cap describe input interface; mask persistence, compound readers, rendering and export require owning native operations or denial. |
| Dynamic rounded-corner controls | 504–519 | Hidden `#corners-sec`, labeled linked-corners checkbox, four scrub inputs and conditional rows bind timeline:2896+. Linking/shape parameter writes are not CSS state. Live rebuild, save/load/history and renderer parity remain pending where native support is unavailable. |
| Dynamic ellipse arc/donut controls | 520–528 | Hidden `#ellipse-arc-sec`, conversion button and hidden start/sweep/inner rows use timeline arc editing (2937+). Numeric UI limits and conversion visibility do not validate dynamic geometry or record/replay behavior. |
| Dynamic star/polygon controls | 529–536 | Hidden `#star-sec` hosts point count/inner radius/corner scrubs; timeline:2975+ synchronizes and commits parameters. Visible labels/ranges are not native shape construction or persistent metadata acceptance. |
| Typography content/font/style/spacing interface | 537–609 | Hidden `#text-props-sec` hosts tp-content, font/add-font controls, size/color, alignment/style/case buttons, spacing and wrap/fixed-width fields. Timeline:8712+ reads/writes text metadata; external Google font loading and generic scrub supply UI resources/input. Text rasterization, alpha, metadata codecs, frame history and expression/animation consumers need separate acceptance; labeled spacing fields coexist with other caption/title-only inputs. |
| Effects stack and target readout hosts | 610–637 | Hidden `#effects-stack-sec`, adjustment/element-target hints and `#effects-list` are filled by effects-panel.js from its target contract. Comment explicitly distinguishes this ID from older effects-sec naming. Hint/stack DOM is not native effect discovery, resource ownership, persistence or evaluated results. |
| Path-effects list and add selector | 638–646 | `#path-fx-sec/#path-fx-list/#path-fx-add` distinguishes geometry path effects from the visual effect stack. Effects-panel wires add/rendering with path-fx services. Native select/host do not establish geometry mutation/evaluation or available capability. |
| Effects catalog flyout trigger and closure | 647–655 | `#p-add-effect-btn` opens the categorized menu generated by effects-panel.js:876+ with previews. This markup closes stack body/section; catalog/thumbnail presence is not effect registration or application acceptance. |
| Camera key/easing controls | 656–667 | Hidden `#camera-sec`, `#cam-key-info`, add-key/ease buttons use timeline/motion camera state and shared curve editor elsewhere. No duplicate easing-widget ownership. Key timing, camera evaluation and render/export/history require native APIs or explicit unavailable results. |
| Layer blend/matte selector shells | 668–689 | Hidden `#layer-sec`, `#p-blendmode/#p-mattemode` are custom div `.psel` controls with role=button/tabindex=0 and data-value. Timeline builds menus/live previews; motion property paths also consume layer state. Role/focus supplies an interface, not full keyboard/accessibility or compositor/matte acceptance. |
| Reference-media import/popover controls | 690–735 | `#p-ref-menu`, hidden file input/popover, import/remove, name, on checkbox and opacity/offset scrubs bind reference-bridge.js. Disabled initial remove and global state.refMedia commentary distinguish this from per-layer identity. Browser/native decode, references/permission lifetime, persistence and export exclusions remain operation contracts. |
| Elements tree host | 736–751 | Hidden `#shapes-sec/#shapes-list` receives shape/group rows from shapes-panel.js and shared motion-element CSS. Tree selection/order/visibility/group operations and persistent identities are not admitted by a host div; Animation2D/Motion surfaces retain their own acceptance. |
| Document dimension/lock interface | 752–769 | `#canvas-sec` hosts p-cw/p-ch scrubs and `#btn-dims-lock`; timeline moves it into document identity context (`.psec-identity`) and synchronizes values. Max dimensions/lock are UI hints; native document resize, content effects, persistence/history and viewport/export must be validated separately. |
| Document FPS/frame-count fields | 770–778 | `#proj-fps/#proj-frames` bind timeline document timing handlers, using generic scrub and bounds/defaults. Fields do not authorize truncating stored frames/markers or accept timing/animation/export parity; native timing authority remains required. |
| Document background/view-overlay row and closure | 779–828 | `#p-cbg` and fit/reset/clip/safety/rulers/alpha buttons share `.doc-bg-row` with an aria-hidden separator and wrapping CSS. Timeline/app/rulers/engine consume toggles; comment records media-mode relocation outside this section. Background persistence versus view/UI preference and authoritative viewport remain distinct, unaccepted by this layout. |
| Combined-shape create/mode/remove/flatten controls | 829–865 | `#combine-opts-sec`, four `.combine-mode-btn[data-mode]`, existing/remove/flatten row and hint use timeline combine helpers (11555+) and selection-context `.psec-inline`. Parametric combination and destructive flatten are distinct operations; shared icons with booleans do not imply identical mutation/history semantics. |
| Fill swatch/hex/alpha/enable interface | 866–897 | `#fill-sec`, pm-fill/color input, hidden p-fill-on, p-fill-hex, p-opacity and eye/gradient trigger bind timeline color synchronization and shared ColorPicker. p-opacity represents fill color alpha per timeline:6310+, not an interchangeable layer-opacity command. Hex8 codec/selection/history obligations survive native unavailability; dim/hidden fields supply no denial. |
| Fill-gradient editor host and controls | 898–914 | Hidden `#p-fill-gradient-editor`, on/kind controls, stops-list/add-stop/hint bind timeline gradient helpers; canvas gradient-bridge handles gestures. Dynamic stop DOM and inline visibility are interfaces, not color interpolation, document codec, rendering/export or native operation acceptance. |
| Stroke swatch/hex/alpha/enable interface | 915–931 | `#stroke-sec`, pm-stroke/color input, p-stroke-hex/p-stroke-alpha and eye div mirror the fill pattern via timeline/color picker. Stroke color alpha is separate from fill/layer opacity. Span/div faces and hex6 input defaults do not supersede persistent hex8 or enable/history contracts. |
| Stroke-along-path gradient interface | 932–942 | Labeled `#p-strokegrad-along` checkbox and from/to swatches bind timeline:10373+. This gradient follows path length, unlike fill spatial gradients. Endpoint values, serialization/evaluation/rendering and undo remain owning operations. |
| Stroke width/style and one-shot smooth interface | 943–956 | `#p-sw/#p-strokestyle` and selected-stroke smoothing scrub bind timeline style/geometry handlers. Comment distinguishes one-shot selected geometry smoothing from future-stroke Tool Options smoothing. UI bounds and shared width defaults are not geometry/history/native acceptance. |
| Stroke cap/join/miter/paint-order/dash interface | 957–972 | `#p-cap-grp/#p-join-grp/#p-paintorder-grp` buttons use data-value, with miter/dash scrubs. Timeline/UI icon-group bindings synchronize choices. Order labels/icons must retain actual render/codec semantics; no stroke preparation/export parity is implied. |
| Vector-brush preset preview and hidden selector | 973–1001 | `#p-brushpreset-btn` contains preview canvas/label; hidden `#p-brushpreset` retains catalog values. Brush-preset-picker builds presentation/catalog and timeline consumes current choice. Thumbnail/selection is not brush renderer, resource lifetime or preset metadata persistence. |
| Brush favorites host and removed-control commentary | 1002–1024 | Hidden `#p-brushfav-row` receives picker favorites. Comment describes removed Apply/Bitmap Brush sections and remaining UI entry paths; this is historical placement evidence, not native bitmap/vector support. Favorite storage and preset application remain controller obligations; stroke body/section closes here. |
| Selected-colors dynamic body | 1025–1043 | `#selected-colors-sec/#selected-colors-body/#selected-colors-empty` receives color-manager.js rows for selection/project colors; UI collapse sweep explicitly spares this body. Comment distinguishes serialized data from live Paper objects. Palette edits must retain codecs, selection and all-frame/history semantics through native commands or denial. |
| Image-mesh activation/grid/reset controls | 1044–1068 | Hidden `#p-imagemesh-sec/body`, labeled on/edit checkboxes, cols/rows/info/reset bind image-mesh-bridge.js:293+. Mesh outline also masks the image per CLAUDE.md §12; no independent image-mask owner is created. State.imageMeshes, frame/media readers, render/export/history/native bridges remain separate obligations. |
| Rig-widget size/axis range/rest/link controls | 1069–1098 | Hidden `#p-widget-sec` contains size and X/Y range/rest/link controls; rig-widget.js:563+ fills them and conditionally hides Y rows by widget kind. Axis labels/limits/link buttons are UI interfaces, not rig target identity, pose evaluation, history/persistence or native acceptance. |

Load/cascade contract: head styles precede this body markup; the separately
owned gpu-gate/Paper bootstrap precedes styles. Later classic UI/timeline/motion
and specialty controllers locate stable IDs/classes/data attributes after DOM
declaration. UI generic scrubs, icon groups and psec collapse are shared readers;
i18n owns translated text/title sweeps, with afterI18n for collapse's dynamic title.
P03C-at/au/aw/ax CSS rows supply inspector/form/context/preview cascade dependencies,
not duplicate interval ownership. Inline display, `.collapsed/.hid/.ac`, values,
canvas sizes, selection/context and generated list children remain public DOM inputs.

| Applicable consumer dimension | Boundary retained by this inventory map |
|---|---|
| Save/load and undo/redo | Persistent transform anchors, shapes/text/masks, camera/layer/matte/effects, document size/timing/background, colors/gradients/brushes, mesh and widget fields must retain every applicable codec/history consumer. Revisions, tween targets and reference identities require exact ownership. Inspector collapse/width and catalog favorites may be UI preferences, not document fields. Legacy snapshots/callbacks do not establish native persistence; no save/reload/history trial occurred. |
| Selection and animation | Context visibility, counts, active/selected controls, rail entries, tracker/tween markers and effects targeting mirror controller state. Boolean/point/shape/text/mesh/widget edits and camera/FPS/key/reassignment evaluation must use native APIs or explicitly reject unavailable operations. Fill alpha is not layer opacity. No gesture, timing, expression or native parity acceptance is granted. |
| Render/export and native bridge | Preview canvases, typography/font resources, reference images, badges and overlay toggles are UI readers, not authoritative evaluation/viewport/export production. Persistent styling/geometry/mesh/rig/effect/media consumers require native services, fixed-revision render/export and resource/permission lifetime checks. This census ports no UI to Rust and accepts no native callback, GPU or encoded output. |
| Browser and installed desktop | Common DOM/CSS has host-specific media/font/native viewport differences. Inspector reflow, collapse rail, canvas capture exclusion, popovers/stacking, scrub limits/focus, input labels and keyboard controls need surface checks. Native buttons/labels coexist with caption-only inputs, color eye divs and custom role/button selectors. No browser, installed, accessibility or reduced-motion behavior run occurred. |

The 39 intervals cover **732 frozen lines exactly once**. Sibling P03C-ay ends
at 356; C06 Labs 357–366 stays owned there, with 367 only the explicit closure
marker; the next HTML sibling starts at 1099. C06 settings 1743–1933/flags
2285–2297 and all other ranges remain untouched. No source identity, original
packet/admission, JSON order/classification, `rangeCoverage`, whole-span
disposition or `complete:false` changes; exactly 760 original packets stay pending.

Focused checks parse this table against independent 367–1098, reject omission,
duplicate, sibling/C06 intrusion and reassignment of line 367 to a Labs behavior
owner, compare frozen/current bytes and check representative DOM/CSS/controller/load
consumers. Integrity-only and 53 scope tests establish consistency, not semantic
completeness; normal completeness remains failing while the wider queue stays open.

## P03C-ba frozen advanced property, media and export HTML

[P03C-ba / #1599](https://github.com/mysteropodes/nemo/issues/1599) maps frozen
`src/index.html` 1099–1742 into named UI presentation responsibilities. Frozen
source `3f6eed2a500f2ce868b711e063816029eb8fefa5` / HTML blob
`0f928bd885b0acd32acde1d52726f1e42f1370c5`; inspected protected base
`a631d0cb73d792f389c977f85df847280194bc63` / HTML blob
`8eb7b2e2ca81284c9e00b22251391d2292654de9`. Frozen 1099–1678 is byte-identical
at current 1099–1678 (580 lines); frozen 1679–1742 is byte-identical at current
1683–1746 (64 lines). Current 1679–1682 inserts the export-cancel comment/button
after frozen 1678. Record that four-line insertion as current-only drift, not a
new frozen packet or part of this 644-line union. Later script changes do not
affect these body coordinates. Consumer references below are inspected source,
not browser/installed behavior or output-job acceptance.

Each row owns its exact presentation interval, including comments/blanks and
closure. These are supplemental census responsibilities, not admitted packets.
HTML/CSS remains UI; persistent state, evaluation, media, viewport and export
operations must use the Rust-backed shared application command/query/job API or
explicitly report unavailable without an old writable fallback. A visible format,
legacy callback or historical comment does not establish native availability.

| Named UI responsibility | Frozen HTML lines | DOM/CSS/controller/load interface and state boundary |
|---|---:|---|
| Text animator/split interface | 1099–1136 | Hidden `#p-textanim-sec` hosts text-split row/button, animator list/add/empty rows and hidden old preset button. `text-animator-panel.js:438+` builds selector/property controls; timeline owns text split, text-animator the old action. Shared section/ramp CSS is presentation; animator records, text geometry, timing and native history/evaluation remain separate obligations. |
| Brush tool-option preset interface | 1137–1156 | `#tool-opts-sec` hosts `#p-toolpreset-row/#btn-toolpreset-save/#toolpreset-grid.asset-tree`. `brush-tool-presets.js:126+` saves/builds settings presets; shared brush grid styles and UI sections supply layout. Tool-option preset versus vector-brush style preset is a distinct data contract; markup grants neither storage nor application acceptance. |
| Drawing pipeline and placement fields | 1157–1174 | `p-stab/p-smooth/p-drawmode` and hidden fillbrush placement icon group bind timeline/SM tool settings and UI data-value handling. Comment orders stabilizer before smoothing; fields/options describe intended UI pipeline, not measured geometry, stylus latency or native tool support. |
| Fill gap/propagation/thickness options | 1175–1178 | Conditional fill gap close/size, propagation and fillbrush size rows use timeline context routing and fill/draw bridges. Hidden controls/ranges cannot authorize all-frame propagation or infer native geometry/history parity. |
| Vector-brush pressure/nib/live input interface | 1179–1188 | Vector brush, pressure min/max/curve, nib, custom pressure curve, invert and live pressure readout use timeline/UI/draw settings. Actual stylus events and drawing/rendering own input interpretation; a live DOM value is not pressure-device acceptance or persistent stroke codec proof. |
| Taper/trim/shadow/mask/eraser options and closure | 1189–1195 | Taper/trim-ends/shadow/mask toggles and eraser width end Tool Options. Timeline controls visibility/settings; bridges own gestures and tags. Shadow guide versus artwork/mask semantics must survive save/load/render/export; this map does not accept tag writers or eraser operations. |
| Palette tabs/new/list hosts | 1196–1204 | `#swatches-sec` has a stable ID for UI panel-order preference; `#palette-tabs/#btn-palette-new/#palette-grid` are populated by palette-panel.js. Empty hosts/native button do not own palette storage, selected colors or replacement operations. |
| Palette add/swap/replace actions and scope | 1205–1225 | Add fill/stroke, swap and replace buttons plus `#palette-replace-scope` bind palette-panel. Scope UI distinguishes frame/layer/selection (selected layers) targets; armed/match/replace faces use CSS. Replace must preserve all applicable identities, frames and history through native commands or denial, not merely color the active face. |
| Removed-section commentary and Motion lead-in | 1226–1238 | Comments record perspective/symmetry removal to Labs, reference relocation and Motion-only visibility. This interval adds no Labs/reference behavior owner; inherited functionality assertions are historical evidence without native acceptance. |
| Motion inspector dynamic body | 1239–1242 | `#motion-props-sec/#motion-props-body` is hidden outside body.mode-motion by CSS; `motion.js:7431+` populates current-layer properties. Dynamic rows share timeline holder/value contracts. DOM mirror must not become a second document/evaluation authority. |
| Unified assets top tabs and media pane opening | 1243–1250 | `#assets-sec`, media/presets tabs and `#assets-view-media` use settings-style tab CSS. `assets-panel.js` owns active/display switching only; media-library/motion-preset-picker own contents. Shared C06 tab styling is a cascade dependency, not settings ownership. |
| Media search/settings trigger toolbar | 1251–1280 | Comments describe merged media panel/drop-zone changes; `#media-search/#media-search-clear/#media-settings-btn` bind media-library.js search and popup display. Toolbar/search/icon CSS and transient query state are presentation, not import or resource authority. |
| Media embedded/linked/convert settings | 1281–1291 | `#media-settings-pop`, embedded/linked buttons and convert action bind linked-media.js/media-library settings. Native paths and browser handles differ; mode face or conversion button does not grant permissions, successful conversion, portability or media lifetime acceptance. |
| Media filter/count/density/expansion controls | 1292–1306 | Filter chips/count and view/expand toggles bind media-library, which emits compact/grid classes and `.expanded`. Density is localStorage UI preference; filters/session count are readouts. Hidden metadata and expanded height cannot establish asset availability or native collection state. |
| Media grid/status/import/transplant actions | 1307–1326 | `#media-grid` hosts dynamic folders/rows/drop hints; status/import/cleanup/transplant buttons bind media-library/drop/linked-media/transplant. Drag/drop routes through real grid despite hint rebuilds. Missing cleanup, decode/import and transplant require operation policy/history rather than status colors or DOM count. |
| Motion-preset pane actions and catalog hosts | 1327–1336 | `#assets-view-presets`, save/new-tab buttons and motion-preset tabs/grid bind motion-preset-picker.js. Asset top tabs gate visibility; catalog/preset actions retain their own persistence, target identity and application contracts, not native editing acceptance. |
| Transplant file input and tween parameters/actions | 1337–1350 | Hidden `#transplant-file-input` is a transplant reader interface, followed by tween section step/resample/manual/harmonize, generate/reassign/status controls. Timeline/tweens own interpolation and identity paths. File picker/defaults/status grant no native import, tween evaluation or history acceptance. |
| Easing canvas, resize, presets and coordinates | 1351–1367 | `#easing-sec/#curve-canvas-wrap/#curve-canvas/#curve-resize-handle`, preset/custom/save/coords hosts use UI shared easing editor and curve services. The closing onion relocation comment assigns no new onion owner. Canvas drawing/editor dimensions are presentation; authoritative interpolation, target segment, stored curve and animation must be validated separately. |
| Footage metadata/dimensions/count interface | 1368–1384 | Hidden `#footage-sec`, kind/name/dimensions/count rows are synchronized by timeline:7096+ from image/sequence/video state. Counts and source names are readouts, not successful decode, resource lifetime or save/load portability. |
| Footage tracking pick/run/apply/status interface | 1385–1404 | `#footage-track-sec`, point/range inputs, pick/run/target/apply/status bind tracker-panel/tracker services. UI comment names the tracking kernel; geometry results, jobs, cancellation and application/history require native service acceptance or explicit unavailability. |
| Footage interpolation/replacement controls | 1405–1415 | Blend row/interpolation select and replace button bind timeline plus image/native-video paths. Time-remap-dependent visibility does not accept frame blending, media replacement identity or export/evaluation parity. |
| Component instance playback/preview/entry interface | 1416–1430 | `#comp-instance-sec` playmode/singleframe/speed/offset, frame-strip/preview/enter controls bind timeline, comp-preview and app/motion symbol entry. Preview strip is UI; nested component timing, parent identity, persistence and native evaluated output remain obligations. |
| Component lipsync and detach interface | 1431–1451 | Audio track/range/sensitivity/hold/chart/apply and component detach controls bind timeline/lipsync.js. Chart/readouts are UI; audio analysis, generated keys, component duplication/detach and history require native commands or denial. No lipsync availability is inferred. |
| Duplicator mode and object-source pool interface | 1452–1479 | Hidden `#duplicator-sec`, dup-mode and object-source add/list are synchronized by timeline from duplicator configuration/sourceLayerUids. Static source-pool presentation differs from animated per-copy deltas; pool identities/dependencies and serialization retain native authority. |
| Duplicator grid/radial/path layout controls | 1480–1495 | Rows/cols/spacing/count/radius/startangle/orientation/path-layer/alignment inputs are shown by duplicator mode and bound by timeline. UI bounds/hidden branches do not validate instance geometry, transforms, source selection or evaluation. |
| Duplicator seed/randomization toggles | 1496–1504 | Seed/reseed and random position/rotation/scale/opacity/hue controls bind timeline duplicator state. Seeded determinism, evaluation order, persistent records and history are not established by checkbox values. |
| Duplicator temporal stagger controls | 1505–1524 | dup-anim enabled/offset/direction rows and translated descriptions bind timeline/motion duplicator timing. Enabled face/default values are not frame scheduling, per-copy time-remap or animation/export parity. |
| Duplicator effectors and source-edit entry | 1525–1539 | Add-effector/list/edit-source hosts bind timeline and effector-layer services. Dynamic rows depend on duplicator.effectors; falloff/stack combination, source editing/history and native API availability remain operation contracts. |
| Rig workflow modes and assignment interface | 1540–1576 | Hidden `#rig-opts-sec`, draw/assign/move buttons, active-bone readout and conditional auto-assign row bind timeline/rig-bridge. `.ac`/mode visibility is UI; bone drawing, assignment identity and topology/history must use native commands or be unavailable. |
| Rig weight/falloff/rotation/drive-target interface | 1577–1588 | Weight radius/softness/rotation and shapes/mesh drive checkboxes bind rig settings. Shape/mesh target kinds retain distinct readers and native evaluation/media obligations; fields and comments provide no deformation parity. |
| Rig commit/reset/hint and section closure | 1589–1595 | Freeze-pose/reset native buttons and translated hint bind rig-bridge/timeline. Destructive pose baking versus reset requires exact document/history semantics; native button presence is not operation acceptance. |
| Removed onion/project-section commentary | 1596–1610 | Comments explain old right-panel removal and preserved ID anchors. This interval names historical placement only; it neither reintroduces panels nor transfers project/settings controllers. |
| Hidden legacy project/import/action anchors | 1611–1627 | `#legacy-project-actions` retains save/open/new/history/settings/Kitsu/export/import button IDs and hidden JSON/image/video/PSD inputs so controllers find them. C06 consumes btn-settings as an interface; actual settings shell remains C06. Invisible anchors preserve wiring, not browser focus, project API or media/import/export acceptance. |
| Legacy action/inspector/application structural closure | 1628–1631 | Closes legacy-project-actions, props-panel and app before body-level modals. Structural markup owns no extra controller or native window lifecycle. |
| Export modal shell, format options and external hints | 1632–1650 | Hidden `#export-modal`, shared modal classes, close and exp-format/hints bind timeline:11636+ and SMExport. SVG/PNG/TIFF/GIF/video/Lottie/Rive/AE options are advertised UI, not capability availability or codec output proof; Rive/AE client requirements remain explicit hints. |
| Export range/scale/custom dimensions interface | 1651–1673 | exp-range/scale/custom W/H fields bind timeline visibility/size calculations, with generic scrub. Work-area/all and custom bounds are requested inputs; pinned revision/time range, scale fidelity and encoder output require native export acceptance. |
| Export alpha/shadow/progress/run interface | 1674–1678 | Alpha/shadow flags, progress host and run button bind timeline/export services. Runtime alpha-format visibility is controller policy; progress text/disabled state is not job completion. Current-only cancel insertion follows this interval and is recorded separately below. |
| Export modal closure after current-only insertion | 1679–1683 | Frozen closing row/body/box/modal plus blank boundary maps to current 1683–1687. No new frozen cancel owner is invented; native cancel/job UI sits in the separately recorded four-line drift before these closures. |
| Render-manager modal/queue/progress/global fields | 1684–1715 | `#render-manager-modal`, add/delete/render buttons, progress, dynamic index/offset and queue/empty hosts bind render-manager.js:496+. It orchestrates export service calls, not frame encoding itself. Toggle render/cancel, queue state, source/revision pinning, destinations and truthful progress remain service obligations; shared modal/queue CSS owns presentation only. |
| Lottie preview canvas/play/scrub/readout interface | 1716–1732 | Hidden `#lottie-preview-modal`, canvas/play/range/frame label/description bind lottie-preview.js. Playback interprets exported JSON in its preview path; successful canvas preview would not establish authoritative native animation or export parity. No preview run occurred. |
| Version-history modal/list interface | 1733–1742 | Hidden `#history-modal`, close/description/list bind project.js:278+ version snapshots, distinct from the topbar undo history panel. Historical 30-second snapshot text is not accepted recovery cadence or current native persistence. This row ends at the blank before C06 settings shell frozen 1743. |

Current-only drift: current 1679–1682 adds the exp-cancel lead-in and button.
The comment names bounded SVG sequence cancellation (P19); inspected timeline
also wires native PNG AbortController cancellation, while SVG uses its job API.
This is a separately observed consumer/availability contract, not frozen census
coverage, a refreeze, acceptance of either cancellation path or a new packet.
No drift line or current-only control is assigned to C06 settings ownership.

Load/cascade contract: shared head CSS follows the separately owned early
gpu-gate/Paper scripts and precedes body DOM. Later classic controllers find
these stable IDs/classes/data attributes after declaration. UI/timeline/motion
provide section/scrub/icon and property bindings; brush/palette/assets/media/
presets/rig/tracker/lipsync/export/preview/project controllers supply specialized
consumers. asset-tree precedes media/transplant/motion-preset-picker/assets-panel.
Shared settings-tab/modal/psec/context and P03C-aw/ax media/render CSS are cascade
dependencies, not duplicate interval ownership. Inline display, active classes,
values, data-value/target IDs, dynamic children and canvas dimensions are public UI
interfaces; body.mode-motion gates its mirror inspector.

| Applicable consumer dimension | Boundary retained by this inventory map |
|---|---|
| Save/load and undo/redo | Text/brush/palette/tween/curve, media/footage/component, duplicator/effector/rig and export parameters retain every applicable persistent codec/history consumer. Catalog/density/panel-order preferences differ from document records. Hidden legacy anchors do not authorize old project writers; version snapshots differ from undo history. Native import/admission, durable save/reload and recovery remain untested here. |
| Selection and animation | Inspector target/visibility, media/preset choices, tween reassignments, easing/camera/component playback, tracking/lipsync and duplicator/rig faces mirror controller state. CSS keyframes/progress/preview UI do not establish evaluated animation. Stateful creation/application must use native APIs or reject unavailable operations without fallback; no gesture, time-remap or nested animation parity is accepted. |
| Render/export and native bridge | Media/font/thumbnail/curve/Lottie canvases are presentation readers, not authoritative production. Native services own evaluation, decode/resources, viewport and fixed-revision export. Format menus, external Rive/AE hints, render queue progress and observed cancel wiring require exact host/client/job validation. Neither a source callback nor preview output establishes a completed export, successful cancellation or installed/native acceptance. |
| Browser and installed desktop | Common markup/CSS has platform-specific file/linked-handle/decode/export and external-client capabilities. Media drop/import, floating settings/queue modals, dynamic list scroll, pressure controls, fonts, scrub/focus/title/label/keyboard and hidden legacy anchors need actual surface checks. Native buttons/labels coexist with caption-only controls and dynamic icon actions. No browser, installed, accessibility or reduced-motion behavior run occurred. |

The 41 intervals cover **644 frozen lines exactly once**, disjoint from P03C-az
through 1098 and C06 settings starting 1743 (current 1747). C06's other intervals,
prior HTML/CSS siblings and current-only export-cancel drift remain preserved.
No original source identity, JSON order/classification, packet/admission,
`rangeCoverage`, whole-span disposition or `complete:false` changes; exactly
760 original packets remain pending.

Focused checks parse the table against independent 1099–1742 and reject omission,
duplicate and C06/sibling intrusion; compare both drift pieces and the exact
four-line current insertion; verify representative DOM/CSS/controller/load tokens.
Integrity-only and 53 scope tests establish index consistency, not semantic
completeness. Normal completeness still fails while the wider queue stays pending.

## P03C-bb frozen collaboration, popover and timeline HTML

[P03C-bb / #1600](https://github.com/mysteropodes/nemo/issues/1600) maps frozen
`src/index.html` 1934–2284 into named UI presentation responsibilities. Frozen
source `3f6eed2a500f2ce868b711e063816029eb8fefa5` / HTML blob
`0f928bd885b0acd32acde1d52726f1e42f1370c5`; inspected protected base
`63225ce6f1be17eb8e2ec48c7fb6ba741df97b94` / HTML blob
`8eb7b2e2ca81284c9e00b22251391d2292654de9`. All 351 frozen lines are
byte-identical at current 1938–2288, shifted +4 by the separately recorded
export-cancel insertion. C06 settings ends frozen 1933 (current 1937); C06
bootstrap starts frozen 2285 (current 2289). Neither boundary is claimed here.
Consumer references below describe inspected source, not a new behavior trial.

Each row owns its exact presentation interval, including comments/blanks and
closure. These supplemental names are not admitted/completed packets. HTML/CSS
remains UI; stateful operations must use the Rust-backed application command/query/
job API or explicitly report unavailable without an old writable fallback. Remote
feedback/collaboration capabilities require their own host/auth/service contracts;
source bindings, UI labels and legacy comments grant no operation acceptance.

| Named UI responsibility | Frozen HTML lines | DOM/CSS/controller interface and applicability boundary |
|---|---:|---|
| Feedback dashboard modal and filters/list | 1934–1956 | `#fb-dashboard-modal`, close, status/tag filters, refresh and list use shared modal/feedback-card CSS; timeline:9392+ builds and binds the triage dashboard through feedback services. Separate wide modal is not the C06 settings shell. Remote issue editing/resolution/auth and inline text handling remain service/controller responsibilities, not UI permission or acceptance. |
| Kitsu login/browse modal | 1957–1983 | `#kitsu-modal`, login URL/email/password/error/button and browse breadcrumb/list/back/logout bind kitsu.js:309+,435+. Native password input and error host are presentation, not successful authentication, safe credential storage or production/project download acceptance. No credentials are copied into this census. |
| Text editing popover and stale lead-in | 1984–2009 | Hidden `#text-popover`, text/size/font/color/align/apply/cancel controls bind timeline text handling (7980+,8525+). The preceding comment describes an anchored comment pin editor, but this is the text host; actual comment host starts at 2056. Record the stale comment, not a second comment owner. Text geometry/metadata/history/rendering require native support or denial. |
| Onion mode/outline/opacity interface | 2010–2021 | Hidden `#onion-pop`, status/mode/current-outline and previous/next opacity controls bind timeline onion popup handlers. CSS popover placement and fields expose UI parameters; evaluated ghost frames, persistent settings and native render authority remain separate obligations. |
| Onion marker-range presets | 2022–2033 | `#om-span-1/2/5/all` buttons bind timeline marker-span logic and share the former range-control entry. Preset selection is UI; exact frame bounds, marker identity/drag and animation/render readers need native operation acceptance. |
| Onion previous/next tint controls | 2034–2042 | Previous/next color inputs and reset use timeline onion tint synchronization. Native color widgets/labels do not establish ghost color fidelity, alpha codecs, persistence or native viewport parity. |
| Onion temporary shift/trace and follow controls | 2043–2055 | X/Y shift/reset/next and marker-follow checkbox bind timeline:11336+. Comment distinguishes temporary ghost shift from document geometry; controller state/lifetime and actual ghost positioning must preserve that boundary. No new persistent writer or evaluated frame acceptance is introduced. |
| Comment popover core text/author host | 2056–2058 | Hidden `#comment-popover`, author row and textarea bind timeline comment handlers (7802+), distinct from text-popover. Placement relative to a canvas point is a UI interface; comment identity/document storage, access and native mutation require owning operations. |
| Feedback action-recording/tags/blocking controls | 2059–2073 | `#comment-record/#comment-record-status/#comment-fb-tags/#comment-fb-blocking` supplement feedback mode via timeline/feedback-bridge. Recording status and tag faces do not prove captured actions, diagnostic scope, permission or successful remote issue submission. |
| Feedback optional name/email interface | 2074–2086 | Optional name/email rows bind feedback/comment controller modes. Translated labels and conditional visibility provide context; profile attribution, persistence and remote data handling remain explicit service contracts, not inferred from form inputs. |
| Feedback screenshot attachment hosts | 2087–2103 | Drop area/thumbnails/hidden multi-image input bind timeline:7876+ and feedback-bridge attachment handling. Drop/file UI and thumbnail display do not prove valid capture, storage/upload, resource cleanup or submitted attachments. No screenshot evidence was captured here. |
| Comment resolution/delete/save/feedback actions | 2104–2112 | Resolved checkbox, delete, save and feedback-save buttons bind timeline:7858+,7901+. Local document comment versus remote feedback submission are separate operations. Button state/text is not approval, successful issue creation or history/persistence acceptance. |
| Timeline shell/resize and symbol-tab host | 2113–2125 | `#timeline-area/#tl-resize/#tl-toolbar/#symbol-tabs` use shared timeline/resize/tab CSS; timeline and UI populate tabs and resize the panel. Symbol navigation/close/re-entry retain native document identity and availability; host height and tab active state do not accept component behavior. |
| Current-frame and transport/loop controls | 2126–2139 | `#tl-cf/#tl-tf`, first/previous/play/next/last and loop buttons bind timeline playback/frame navigation and UI scrubbing. Visible frame value/icons are readouts/request inputs, not clock authority, evaluated revision or native animation timing acceptance. |
| Playback bake-cache trigger | 2140–2149 | `#btn-bake-cache` binds timeline:11114+ manualBakeCache/playback-cache.js; comment describes an automatic/manual fallback. Presence of a button or historical performance description does not establish native cache/resource lifetime, correct fixed-revision frames or playback reliability. |
| Timeline FPS/frame-count fields and separators | 2150–2154 | `.ti` contains `#tl-fps/#tl-total`, synchronized from document values and bound at timeline:11362+. Shared scrub/range UI mirrors Document fields elsewhere. Defaults/limits do not authorize timing/length mutation or truncated stored data; all native save/history/evaluation/export consumers remain required. |
| Onion/outline and ghost-selection toolbar toggles | 2155–2161 | `#btn-os/#btn-os-outline/#btn-ghost-all/#btn-ghost-select` plus removed-marker-control comment bind timeline/UI onion/ghost state. CSS/icon active faces are presentation; cross-frame ghost selection/rendering and range gestures need native support or explicit unavailability. |
| Shadow/revision/cycle/tween-curve visibility controls | 2162–2172 | Shadow guides, revision view, cycle and tween-curve buttons bind timeline and bridge rendering. Guide versus artwork/export, original/revision comparison and animated curve overlays retain separate data readers. Toggle visibility alone accepts no document mutation or rendered output. |
| Motion graph/shy/BPM/blur mode switches | 2173–2184 | BPM/blur/shy/graph buttons use mode-dependent visibility and timeline/motion/motion-graph/bpm-grid bindings. Comments describe intended panel hiding and effects. CSS/header switches are not accepted layer selection, timing conversion, motion-blur evaluation or native graph edits. |
| Timeline toolbar customization entry | 2185–2190 | `#btn-toolbar-customize` uses SVG/title and timeline:11119+ menu/persistence logic. Toolbar preference is UI state; hiding a control neither changes capability availability nor satisfies accessibility/keyboard acceptance. Toolbar closure adds no operation owner. |
| Timeline content/layer shell and paired header | 2191–2208 | `#tl-content/#layer-panel/#layer-hdr` establish the left-column header. CSS height 42 matches frame-header 20 plus bars-row 22; comments/CLAUDE.md §11 require paired panel/grid rows and 1:1 scroll. Static geometry is not a live alignment or reflow check. |
| Layer create/camera/audio/delete/duplicate/component actions | 2209–2225 | `#layer-ctrls` contains layer/camera/audio/delete/duplicate/component buttons and hidden audio input. Timeline/audio-bridge bind operations and import. Native button/file interfaces do not establish persistent layer IDs, audio decode, destructive history or component extraction acceptance. |
| Layer-list host and resize/column lead-in | 2226–2231 | `#layer-list/#layer-panel-resize` accept dynamic rows and UI width sizing. Following comment explains the grid column wrapper. Native selection/order/history and layer-scroll-sync are controller contracts; DOM row count/resize position is not state authority. |
| Frame-grid column/scroll/header/bars shell | 2232–2243 | `#fg-col/#fg-wrap/#frame-hdr/#bars-row` preserve ruler-before-bars DOM order and scrolling host, used by timeline/Motion/timeline-zoom/layer-scroll-sync. Sticky offsets and hidden native scrollbar must preserve CLAUDE.md §11; no timing/hit-test/native viewport acceptance is granted by layout. |
| Onion/work-area range marker hosts | 2244–2248 | `#onion-bar/#wa-bar/#om-in/#om-out` provide range bars/handles inside bars-row. UI/timeline marker logic positions them with frame scale. Bounds, drag/history, follow-playhead and export-range semantics remain operation obligations. |
| Frame-grid dynamic rows and work-area tint | 2249–2258 | `#frame-grid/#wa-tint` receive timeline/Motion rows and pointer-inert work-area shading. Dynamic keys/markers/BPM/grid row alignment depend on the same frame/selection contracts. Shading is not accepted work-area data, selection or evaluated animation. |
| Audio strip and viewport-pinned playhead/flag | 2259–2272 | `#audio-strip` receives audio rows; `#playhead/#playhead-flag` exposes the draggable flag while the stem is pointer-inert. Timeline viewport pinning and motion/audio handlers preserve hit-testing/scroll and timing identity. DOM waveform/playhead is not decoded playback, synced audio, native clock or export acceptance. |
| Timeline structural closures | 2273–2277 | Closes frame/layer content and timeline containers before status bar. This is structural ownership only, with no extra document, scrolling or native window lifecycle authority. |
| Status/help/version readout and surrounding closure | 2278–2282 | `#statusbar/#statusbar-help/#status-text` use shared status CSS and timeline contextual hints/version text. Static shortcut/version fallback is not verified keyboard routing, current runtime/build identity or tool availability. Final surrounding closure creates no additional state owner. |
| Transient toast host and bootstrap boundary blank | 2283–2284 | Empty `#toast` receives showToast messages/classes/timeouts from timeline/UI, with pointer-inert opacity/translation CSS. Text/status presentation is not completed work or accepted notification/accessibility behavior. Last blank precedes C06 bootstrap comment at 2285. |

Load/cascade contract: early separately owned gpu-gate/Paper tags precede head
styles; shared styles precede these body nodes and later classic controllers.
Stable IDs/data attributes and dynamic list children bind timeline/UI/Motion,
feedback, Kitsu, playback-cache, audio and scrolling/zoom consumers. i18n owns
translated text/title/placeholder sweeps. Shared modal/feedback/onion/transport/
grid/audio/status CSS from prior censuses is a cascade dependency, not duplicate
numeric ownership. Inline hidden display, active/resolved/mode classes, frame
values, positions, scroll offsets and paired row/header dimensions are DOM inputs.

| Applicable consumer dimension | Boundary retained by this inventory map |
|---|---|
| Save/load and undo/redo | Text/document comments, frame/FPS/length/loop/onion/BPM/blur/shy settings, layer/component/audio identities and ranges retain applicable codecs/history. Remote feedback/Kitsu state and optional identity/attachment forms follow their own service contracts; toolbar/scroll preferences differ from project records. Legacy handlers cannot serve as native write fallback; no persistence/history or remote operation trial occurred. |
| Selection and animation | Transport/frame fields, graph/shy/layer/grid keys, onion/ghost/range markers, revision/curve overlays and playhead/audio faces mirror controller state. All stateful tool/frame/layer/animation operations require native API support or explicit denial. Comment/text hosts are distinct despite the stale lead-in. CSS toast animation and cache descriptions grant no playback, evaluated timing or native parity acceptance. |
| Render/export and native bridge | DOM grids/overlays/playhead/audio and text/capture previews are UI readers. Native services own document evaluation, media/resource lifetime, fixed-revision viewport/render/export and applicable clock behavior. Guide visibility, work-area markers and cache frames must preserve production consumers. Feedback recordings/screenshots and Kitsu data require real capture/service validation, not a successful source lookup. No render/export/native callback acceptance occurred. |
| Browser and installed desktop | Shared shell requires host-specific authentication/network/media/capture/viewport checks. Grid reflow, paired scroll, sticky headers, resize, capture listeners, drag flags, custom popovers/drop zones, focus/labels/keyboard/shortcuts and password/attachment lifetimes need actual surface evidence. Native form/buttons coexist with div drag/marker/status hosts. No browser, installed, accessibility or reduced-motion behavior trial occurred. |

The 30 intervals cover **351 frozen lines exactly once**, disjoint from C06
settings 1743–1933 and bootstrap/flags from 2285, and from the earlier HTML
siblings. Any full-file reconciliation remains P03C-bc/#1601. No original source
identity, JSON order/classification, packet/admission, `rangeCoverage`, whole-span
disposition or `complete:false` changes; exactly 760 original packets stay pending.

Focused checks parse against independent 1934–2284, reject omission/duplicate/
C06 or adjacent-sibling intrusion, compare protected bytes at offset +4, verify
actual text/comment host bindings and reject the stale-comment interpretation.
Representative DOM/CSS/controller/load controls, integrity and 53 scope tests
establish consistency, not semantic completeness. Normal completeness remains
failing while the wider packet queue stays pending.
