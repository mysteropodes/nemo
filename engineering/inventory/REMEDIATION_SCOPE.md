# Frozen remediation source index

[P03A / #1116](https://github.com/mysteropodes/nemo/issues/1116) built the mechanical index
for [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005) and
[P03B / #1170](https://github.com/mysteropodes/nemo/issues/1170) refroze it with the
supplemental censuses and the first reviewed dispositions. Neither leaf completes P03 or
admits the extraction queue; that is P03C.

[remediation-scope.json](remediation-scope.json) (schema `nemo.remediation-scope/2`) freezes
all **693 Git-tracked paths** at `95e4970754f27162553f4025ded42f964cb2079b`. Every path has one
classification, Git mode/blob identity, a reason, and the packet references that name it.
Multiple references preserve overlapping census evidence; they do not authorize concurrent
writers or settle range ownership.

The sorted source-set SHA-256 hashes UTF-8 records `mode + " " + blob + " " + path + "\n"`.

## Amendments, not rolling baselines

`amendments` records every freeze in order. The first entry is P03A's freeze of 657 paths at
`59a5a38c…`; the second is P03B's refreeze at `95e4970…` with the exact delta (**36 added**,
**0 removed**, **16 modified** paths). The checker recomputes the last delta from the two Git
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

All **36 census files** present at the frozen commit are pinned by blob and keep their own
`source_sha` — the 14 original partitions plus C19a–C19o (timeline) and C20a–C20f (Motion).
An unpinned census file in the tree fails integrity. The pins yield **449 packets**: 329 from
the original partitions (C08 keeps only its documentation map) and 120 from the supplements.

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

At `95e4970`, **61 files** carry range declarations, **34** have gaps, and **21 933 code lines in
171 spans** have no packet. This is the honest size of the remaining census work and the input
of the P03C queue:

| File | Lines | Uncovered code lines | Spans |
|---|---:|---:|---:|
| `src/js/timeline.js` | 11 907 | 5 806 | 13 (all from 5028 onward except 2002-2011, 2027-2048, 2247) |
| `src/js/app.js` | 5 410 | 5 009 | 3 (only C06 bootstrap/profile ranges are declared) |
| `src/js/motion.js` | 13 914 | 4 262 | 32 (largest: 1769-2932, 9826-11125, 11412-12405, 12685-13124, 583-940) |
| `src/css/style.css` | 2 850 | 2 707 | 4 |
| `src/index.html` | 2 483 | 2 248 | 4 |
| `src/js/tweens.js` | 5 249 | 509 | 11 |
| `src/js/engine-bridge.js` | 4 678 | 226 | 17 |
| `src/js/export.js` | 1 389 | 186 | 8 |
| `src-tauri/src/lib.rs` | 284 | 151 | 4 |
| `nemo-mcp/src/contract.rs` | 340 | 134 | 23 |

`src/js/app.js` is the largest finding: it has file-level references, so the C08 set-difference
check passed, yet 5 009 of its 5 410 lines belong to no packet. Overlaps worth reading before
admission: `shader-effects-library.js` (1 311 lines claimed twice), `motion.js` (239, C02 versus
C20), `tweens.js` (158, C01 versus C02).

## Dispositions with evidence

Every reviewed claim carries its evidence and is validated by shape:

- **Packets** — `pending` (449), `covered` (leaf + issue + merged PR + evidence), `admitted`
  (issue) or `deferred` (reason). No delivered leaf matched an entire packet: P20, P22, P23,
  P26, P28, A01, P17 and P18 each extracted part of a larger packet, so all 449 stay pending
  until P03C splits them.
- **Uncovered-responsibility notes** — 35 verbatim notes: 11 `covered-by-packet` (cite existing
  packets), 3 `resolved` (measured), 14 `boundary` (scope statements, cross-checked against the
  spans), 2 `human-decision` (C03:3 unwired selection API, C08:3 `40min-checkins/`), and
  **5 `needs-reconciliation`**: C02:10, C08:1, C20a:1, C20a:2, C20b:1 — each pointing at open spans.
- **Executable paths without a packet** — 36, all dispositioned: 28 `leaf` (module or test
  created by a merged leaf, with its PR) and 8 `oracle` (compiled MCP tests named by C07).
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

In order: reconcile the five open notes and the C02/C20 range drift (packet range amendments,
no new owners); map the unmapped monoliths that no census reached — `app.js` first, then the
`timeline.js` tail from 5028 and the `motion.js` spans — as further bounded census slices;
decide `style.css`/`index.html` (presentation, likely `boundary`); split each pending packet
into ≤90-minute leaves under its family parent or mark it `covered`/`deferred` with evidence;
disposition every span. The execution plan and leaf issues remain the queue and ownership
authority; D01 and T05 keep their reservations. No application behavior changes in this index.
