<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# GitHub Project and parallel-development contract

> **Workflow superseded — 2026-09-07.** Use the
> [English checklist](EXECUTION_PLAN.en.md) / [French copy](EXECUTION_PLAN.fr.md)
> for the existing boards, claims, handoffs, publication and integration. Ilya and Cyrill
> direct local teams of at most one orchestrator and two delegates each. Keep one branch
> per outcome across sprints and at most two writable task worktrees plus the primary
> checkout per machine. Notes belong in the existing issue, not report PRs. Do not create
> another board, require Buzz enrollment or treat broad R parents as global blockers.

**Primary board:** [Cyrill's Project #2](https://github.com/users/mysteropodes/projects/2/views/1).
Update its `Remediation status`, validation and applicable metadata as directed by the
checklist. Ilya's Project #8 is a legacy snapshot; no ongoing dual-board parity is required.
The [shared hourly log](https://github.com/mysteropodes/nemo/issues/1062) contains team
summaries and is excluded from executable-leaf counts. Prepare reporting timers paused;
enable only while actually executing and pause when the session stops.

Status: **historical setup/workflow proposal and ownership reference**. The projects and
issues already exist. The creation instructions below are historical, not a new setup task.

## Historical initial Project proposal

The original proposal was to create one Project named **Nemo Development**. The existing
Project #2 now supplies that role; this historical name is not an instruction to create or
rename a board. The table below records the original minimal field proposal; the execution
checklist defines current fields, options and status mappings.

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

## Historical automation options (not enabled by this reference)

The current workflow uses explicit read-before/write/readback on Project #2. Do not install
automation or change hosted workflows merely to implement this old option list.

- Auto-add only intentionally triaged items, for example those with a `tracked` label.
- Link PRs, issues and CI receipts.
- Move merged work to Validate when runtime evidence remains.
- Use one coordinator for milestone/status updates.
- Keep credentialed Project writes separate from untrusted pull-request execution.
- Add a stable required aggregate check only after the workflow exists, passes and reports
  required missing/cancelled jobs as failures.

## Claim and assignment

Every task uses the execution checklist's compact issue claim. The
[task packet](templates/TASK_PACKET.md) is an optional field reference, not a required new
file. Each human's orchestrator issues its team's scope and reconciles shared files with
the other orchestrator.
A claim identifies issue, human owner, unique agent session, base, branch/worktree, allowed
paths, contracts, dependencies, resource slots, checkpoint and acceptance.

An issue claim or Buzz/Project status is not an atomic file lock. Before editing, the writer
acknowledges the exact grant. Scope expiry starts reconciliation; it never permits a second writer to overwrite
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
