# Frozen remediation source index

[P03A / #1116](https://github.com/mysteropodes/nemo/issues/1116) built the mechanical index
for [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005),
[P03B / #1170](https://github.com/mysteropodes/nemo/issues/1170) refroze it with the
supplemental censuses and the first reviewed dispositions, and
[P03C-a / #1179](https://github.com/mysteropodes/nemo/issues/1179) pinned the C19r supplement
and reconciled the C02/C06/C20 packet-range drift the earlier freezes had left open. None of
these leaves completes P03 or admits the extraction queue; that is the rest of P03C.

[remediation-scope.json](remediation-scope.json) (schema `nemo.remediation-scope/2`) freezes
all **703 Git-tracked paths** at `4bbe332332075d9a25793a58998ea99b33d48538`. Every path has one
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
edits). The checker recomputes the last delta from the two Git
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

All **39 census files** present at the frozen commit are pinned by blob and keep their own
`source_sha` — the 14 original partitions plus C19a–C19r (timeline) and C20a–C20f (Motion).
An unpinned census file in the tree fails integrity. The pins yield **470 packets**: 329 from
the original partitions (C08 keeps only its documentation map) and 141 from the supplements
(C19r/#1178 added its own 6).

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

At `4bbe332…` (P03C-a), **61 files** carry range declarations, **34** have gaps, and
**21 153 code lines in 166 spans** have no packet. This is the honest size of the remaining
census work and the input of the P03C queue:

| File | Lines | Uncovered code lines | Spans |
|---|---:|---:|---:|
| `src/js/timeline.js` | 11 907 | 5 071 | 11 (all from 5710 onward except 2002-2011, 2027-2048, 2247) |
| `src/js/app.js` | 5 410 | 5 000 | 2 (only C06 bootstrap/profile ranges are declared) |
| `src/js/motion.js` | 13 914 | 4 217 | 28 (largest: 1769-2932, 9826-11125, 11412-12405, 12685-13124) |
| `src/css/style.css` | 2 850 | 2 707 | 4 |
| `src/index.html` | 2 485 | 2 250 | 4 |
| `src/js/tweens.js` | 5 249 | 516 | 13 |
| `src/js/engine-bridge.js` | 4 678 | 226 | 17 |
| `src/js/export.js` | 1 389 | 186 | 8 |
| `src-tauri/src/lib.rs` | 284 | 151 | 4 |
| `nemo-mcp/src/contract.rs` | 340 | 134 | 23 |

`src/js/app.js` is the largest finding: it has file-level references, so the C08 set-difference
check passed, yet 5 000 of its 5 410 lines belong to no packet (P03C-a closed the 9-line
`1-9` orphan span left by C06's stale `10-20` declaration, so C21a/#1180 can now start at 41 with
no gap). Overlaps worth reading before admission: `shader-effects-library.js` (1 311 lines
claimed twice), `tweens.js` (158, C01 versus C02) — both out of this leaf's scope. `motion.js`
(190) and `timeline.js` (86) are down from 239/87: P03C-a narrowed every C02-versus-C19/C20
supplement overlap to 0 (5 groups in motion.js, 1 in timeline.js); the residual lines are
C02-internal (two C02 packets sharing an export-block boundary in motion.js) or one C02 packet
(`motion-shape-tree-rendering`) now fully subsumed by C19q's finer slice in timeline.js —
narrowing either without leaving a packet with an empty `files` declaration is out of this
leaf's bounded scope and is queued with the C19s+/C20g+ series.

## Dispositions with evidence

Every reviewed claim carries its evidence and is validated by shape:

- **Packets** — `pending` (470), `covered` (leaf + issue + merged PR + evidence), `admitted`
  (issue) or `deferred` (reason). No delivered leaf matched an entire packet: P20, P21, P22, P23,
  P26, P28, A01, P17 and P18 each extracted part of a larger packet, so all 470 stay pending
  until P03C splits them.
- **Uncovered-responsibility notes** — 35 verbatim notes: **14 `covered-by-packet`** (cite
  existing packets — P03C-a closed C20a:1, C20a:2 and C20b:1 by amending the C02 packet ranges
  they pointed at), 3 `resolved` (measured), 14 `boundary` (scope statements, cross-checked
  against the spans), 2 `human-decision` (C03:3 unwired selection API, C08:3 `40min-checkins/`),
  and **2 `needs-reconciliation`**: C02:10, C08:1 — each still pointing at an open span and now
  naming its queued successor (C20g+ and C19s+ respectively, plus the sibling C21a/#1180 for
  `app.js`).
- **Executable paths without a packet** — 40, all dispositioned: 32 `leaf` (module, test or
  gate data created by a merged leaf, with its PR) and 8 `oracle` (compiled MCP tests named by C07).
- **Spans** — `admitted` (issue), `covered` or `boundary`, each naming one reported span with
  evidence. None is dispositioned yet.

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
amendments, no new owners) and pinned C19r. What remains, in order: map the unmapped monoliths
that no census reached — `app.js` first (C21a/#1180 is the first proposed slice), then the
`timeline.js` tail from 5710 (C19s+) and the `motion.js` spans (C20g+) — as further bounded
census slices; decide `style.css`/`index.html` (presentation, likely `boundary`); split each
pending packet into ≤90-minute leaves under its family parent or mark it `covered`/`deferred`
with evidence; disposition every span. The execution plan and leaf issues remain the queue and
ownership authority; D01 and T05 keep their reservations. No application behavior changes in
this index.
