<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# GitHub Project and parallel-development contract

Status: **proposed team workflow**. The board, fields and automation are not active until a
Project owner creates/adopts them. GitHub issues, PRs, CI and review remain canonical even
when Buzz carries the coordination messages.

## Project

Create one Project named **Nemo Development**, owned where the repository maintainers can
administer it, and link it to the Nemo repository.

Recommended fields:

| Field | Values |
|---|---|
| Status | Inbox → Ready → In progress → Review → Validate → Done; Blocked is a side state |
| Assignee | one accountable human |
| Priority | P0 / P1 / P2 |
| Area | Document / Animation / UI / Renderer / Media / Platform / DevEx |
| Goal | Reliable alpha / Modular core / Measured performance / Reproducible desktop |
| Size | S / M / L; split L before concurrent implementation |
| Milestone/iteration | real release or checkpoint |

Useful views: triage inbox, Ready by priority, active work by owner/area, review/validation,
blocked/dependencies, goal roadmap and performance investigations.

Task bodies and receipts carry detailed base SHA, worktree, paths, contracts, agent sessions,
risks and evidence. Do not create a custom field for every packet detail or assign fictional
agent accounts in place of the accountable human.

## State semantics

- **Ready:** outcome, owner, dependencies, scope and observable acceptance are complete.
- **In progress:** one acknowledged writer owns the scope.
- **Review:** concrete diff and evidence exist.
- **Validate:** integrated/runtime/platform acceptance is still required.
- **Done:** specified behavior is accepted on an identified integrated commit/artifact.
- **Blocked:** exact dependency, owner and next action are recorded.

A merged PR is not automatically Done when desktop, export or visual acceptance remains.
Use issue-closing keywords only when the full issue acceptance is satisfied by merge/CI.
Do not mass-close old issues based on a `resolved` label; triage each against evidence.

## Minimal automation

- Auto-add only intentionally triaged items, for example those with a `tracked` label.
- Link PRs, issues and CI receipts.
- Move merged work to Validate when runtime evidence remains.
- Use one coordinator for milestone/status updates.
- Keep credentialed Project writes separate from untrusted pull-request execution.
- Add a stable required aggregate check only after the workflow exists, passes and reports
  required missing/cancelled jobs as failures.

## Claim and assignment

Every task uses [the task packet](templates/TASK_PACKET.md). One coordinator issues scope.
A claim identifies issue, human owner, unique agent session, base, branch/worktree, allowed
paths, contracts, dependencies, resource slots, checkpoint and acceptance.

Buzz/Project status is not an atomic file lock. Before editing, the writer acknowledges the
exact grant. Scope expiry starts reconciliation; it never permits a second writer to overwrite
an unreachable owner's work.

Reserve whole legacy files during extraction. Line ranges are too fragile for shared monoliths.
Shared manifests, lockfiles, bootstrap, schemas, workflows and policy documents have one active
writer. When a task needs another owner's path, request a scope change or pass a bounded patch
to that owner.

## Isolation

| Resource | Rule |
|---|---|
| source/index | separate branch and worktree per writer |
| browser state | separate profile/context and origin |
| Tauri data/autosave | task-specific configured data root |
| ports/processes | atomic reservation and source handshake; stop only owned processes |
| media/temp/cache | unique run roots |
| build output | per-worktree mutable outputs |
| reports/artifacts | run IDs and unique paths |
| desktop input | serialize when sharing a physical UI |
| GPU benchmarks | exclusive reference-machine slot |
| shared docs/lockfiles | one writer, then consolidation |

A separate worktree does not isolate browser storage, app data, GPU load or desktop input.

## Worktree lifetime and disk use

**Current user-directed policy (2026-09-06): remove a temporary worktree as soon as it is
unneeded.** Each worktree has an owner, a concrete purpose and a cleanup trigger in its task
packet. Reuse a suitable owned checkout for successive steps of the same task. Pushing a
commit, opening a PR or publishing another report does not itself require a new checkout.

Reassess the checkout at every boundary:

| Boundary | Cleanup decision |
|---|---|
| Commit, push, PR creation or report publication | Verify the required preservation/publication result; remove the checkout if no remaining local step needs it. An open PR can keep its branch without keeping a checkout. |
| Review, validation or integration finishes | Preserve the relevant evidence, stop owned processes and remove the checkout when its local purpose is complete. |
| Pause or handoff | Preserve the checkpoint and settle ownership. Retain only for an identified next action or an accepted transfer of the same checkout. |
| Cancellation or interruption | Wait for the worker to stop; reconcile indeterminate effects before deciding whether the checkout is disposable. |
| Completion, supersession or abandonment | Remove the obsolete checkout before the final receipt, or record the exact remaining dependency, owner and next cleanup trigger. |

Before removal, inspect tracked changes, untracked **and ignored** files, local commits,
worktree locks and processes still using the checkout. Preserve required source, fixtures and
evidence at a verified durable destination. A clean status, merged HEAD, empty Git journal or
silent worker alone does not prove that a checkout is obsolete. Unresolved ownership or
unique data requires a specific retention reason, not a guessed deletion.

Run `git worktree remove <path>` from outside the checkout and verify that its directory and
entry in `git worktree list` are gone. Do not force-remove unknown state. Branch retention
is a separate decision: removing a checkout does not delete its named branch or the PR.
`git worktree prune` only removes stale registration metadata; it does not reclaim existing
checkout directories. Record removal or retention in the existing task receipt, not a new ledger.

For report and documentation jobs, materialize only the required paths with sparse checkout.
For example, after selecting unused task-specific branch and path values:

```sh
git worktree add --no-checkout -b <task-branch> <worktree-path> origin/main
git -C <worktree-path> sparse-checkout set --cone 40min-checkins
git -C <worktree-path> checkout <task-branch>
```

Cone mode also includes top-level files and files in ancestor directories. Read relevant
guidance outside the selected paths with `git show HEAD:<path>`, or add the needed directories
with `git sparse-checkout add`. Expand the checkout when a build or validation needs more
source; missing paths are not a reason to omit an applicable check. Do not shrink an active
worker's checkout or share writable source/build outputs through hard links or symlinks.

Git worktrees share repository objects/history but normally materialize separate working
files. Sparse checkout selects paths; it is not a diff overlay that reads missing files from
main. APFS copy-on-write clones can share file storage while keeping writes independent, but
ordinary Git worktree creation does not establish clone sharing. Any clone-based setup needs
separate implementation and validation and does not replace lifecycle cleanup.

References: [Git worktrees](https://git-scm.com/docs/git-worktree),
[sparse checkout](https://git-scm.com/docs/git-sparse-checkout),
[Apple APFS cloning](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html).

## Commit hygiene

1. Fetch and inspect main, relevant branches/PRs and current claims before diagnosis.
2. Confirm the grant covers every changed path and semantic scope.
3. Stage explicit files/hunks and inspect the staged diff.
4. Keep one coherent extraction, fix or contract change with its tests/docs per commit.
5. Separate broad formatting, renames and dependency churn from behavior.
6. Do not hand-edit generated output; regenerate from its source.
7. Check for secrets, local profiles, private fixtures and accidental large artifacts.
8. Follow the repository's authorship, sign-off and signing policy; never invent identities.
9. Do not push directly to `main`. Open a scoped PR.
10. Do not reset/rebase/push another agent's checkout or rewrite a shared branch without an
    explicit coordinated decision.

Nemo contributions are GPL-3.0-or-later and the current contributor guide uses no CLA. Review
third-party notices before changing bundled/native dependencies. If a target repository
enforces DCO, include the verified contributor's matching `Signed-off-by`; authorship,
co-authorship, DCO sign-off and cryptographic signing are separate claims.

Commit and PR descriptions lead with changed behavior or responsibility, state preserved
invariants, link the canonical task and summarize validation plus limitations.

## Review and integration

The reviewer receives the actual diff, task contract, fixtures/results and limits. Different
models are not independent evidence by themselves; reproduce behavior against an independent
expectation.

One integration owner:

1. refreshes main, active claims and dependent PRs;
2. resolves semantic as well as textual conflicts with affected owners;
3. builds the combined candidate and reruns affected plus reverse-dependency checks;
4. records exact base, candidate and artifact identity;
5. integrates only with current authority and required human review;
6. verifies the landed SHA/runtime and updates the canonical receipt.

If main changes during validation, reassess and refresh evidence. Roll back the smallest coherent
change without enabling two state authorities or discarding newer/unknown document data.

## Pause, handoff and urgent regressions

On pause, checkpoint branch/dirty state, fixtures, failures and one exact next action. Keep the
scope held until an explicit release/transfer. A new session rereads history, base and current
grant.

For an urgent regression in a reserved area: reproduce it, pause/checkpoint the extraction,
agree a handoff, land the smallest fix through normal review, then reconcile every affected
worktree and forward-port the regression fixture.
