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
  five Tween assignment-seam spans, P03C-l/#1438 dispositioned eight Project lifecycle spans, and
  P03C-m/#1440 dispositioned eleven MCP contract spans; 102 spans remain undispositioned.

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
and global 7 185 remain unchanged. **P03 is not complete and no extraction is admitted**: the gate
still exits 1 with 762 pending packets, no notes needing reconciliation, 0 unmapped paths and 71
undispositioned spans; the computed range report still has 7 185 uncovered code lines.
What remains, in order: disposition the residual `motion.js` spans between adjacent slices as
`covered`/`boundary` or one last census slice, the small remainders in the original-partition
files, and `style.css`/`index.html` once Ilya decides census versus `boundary`; split each
pending packet into ≤90-minute leaves under its family parent or mark it `covered`/`deferred`
with evidence. The execution plan and leaf issues remain the queue and ownership authority;
D01 and T05 keep their reservations. No application behavior changes in this index.
