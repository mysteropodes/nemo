# Frozen remediation source index

[P03A / #1116](https://github.com/mysteropodes/nemo/issues/1116) supplies the mechanical
index for [P03 / #1005](https://github.com/mysteropodes/nemo/issues/1005). It does not
complete P03 or admit an extraction queue.

[remediation-scope.json](remediation-scope.json) freezes all **657 Git-tracked paths**
at `59a5a38c514f5da830dd2f2df4c83c63b1b22fba`. Every path has one classification,
Git mode/blob identity, a reason, and historical packet references when a packet's
`files` declaration names it. Multiple references preserve overlapping census evidence;
they do not authorize concurrent writers or settle range ownership.

The sorted source-set SHA-256 hashes UTF-8 records `mode + " " + blob + " " + path + "\n"`.
The index pins all 14 census files by blob and retains each census's original source SHA.
Those older SHAs are deliberately preserved: the index reconciles their references at one
fixed source tree; it does not claim their source-level observations were all repeated there.

There are **329 packet references**: 292 from C01–C07 (C04 has two files), 33 from
C09/C13/C15/C17/C18, and four C08 documentation/process references. The supplements
replace C08's other 29 preliminary packets. The remaining C08 references cover process
artifacts, not executable extraction tasks. Vendor/generated/static entries retain their
provenance obligations. Handwritten shader logic and executable tooling remain included.

All 329 packet admissions are `pending`. The 18 original uncovered-responsibility notes
remain verbatim with `needs-reconciliation`, even where later work may have resolved one.
No stale assertion is silently dropped. Counts of paths, packets, features and executable
issues are different quantities.

## Checks

From the repository root:

```sh
node scripts/nemo/remediation-scope.cjs --integrity-only
node --test tests/nemo-remediation-scope.test.cjs
node scripts/nemo/remediation-scope.cjs
```

The first command verifies sorted file coverage against `git ls-tree` at the frozen SHA,
content/mode identities, digest/counts, census pins, packet provenance, and retained gap
notes. Exit 0 means **index integrity**, while the output still says `complete: false`.
It does not validate semantic classification, line ranges, consumer coverage, source
behavior, issue ownership or extraction readiness. Those require P03's remaining review.

The third command is the completeness gate and currently exits **1**. Version 1 never
accepts completion: it rejects invented accepted admissions, resolved gap dispositions
and `complete: true`. A reviewed successor must implement actual leaf/consumer evidence
before P03 can pass this gate. This guard is opt-in tooling; no hosted workflow or normal
CI job was added by P03A. The regression suite runs through the existing Node test glob.

An explicit comparison with another full commit SHA fails on any source-tree drift:

```sh
node scripts/nemo/remediation-scope.cjs --integrity-only --source FULL_COMMIT_SHA
```

This includes P03A's own newly added files when comparing its candidate with the older
frozen source. It is intentional: the frozen inventory is historical evidence, not an
unannounced rolling baseline. Compare future amendments explicitly; do not auto-refresh
the digest to hide additions or changed bytes. No working-tree or installed-app parity
is implied by checking a Git tree. Git source history must be available locally.

## Remaining admission work

The checker lists paths without a `files`-field packet reference. This conservative list
includes new capability/domain modules since the original census and eight MCP test files
that C07 names as oracles rather than extraction targets. A missing reference here does
not mean the tests are absent or failing; it needs an explicit coverage disposition.

P03's ordered work is the mechanical index, then bounded timeline/Motion gap mapping,
then family-by-family extraction admission. Timeline's recorded uncovered ranges and
Motion's range drift must be reconciled with current symbols and consumers. Broad tooling,
renderer and editor packet groups must be split before Ready. Reuse existing executable
leaves where their exact scopes fit; do not create duplicate writers from packet names.

The existing execution plan and leaf issues remain the queue and ownership authority.
D01 and T05 retain their separate reservations. No application settings, persisted fields,
rendering behavior, runtime availability or known product defects change in this leaf.
