<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo repository agent entry point

These instructions apply to Codex, Claude, and their child agents in this repository.
GitHub is canonical for source, issues, pull requests, CI, and review. Keep the active task
in its issue, pull request, or lead-designated queue; do not create a competing shared ledger.

## Find the project contract

- Start with [the remediation handbook](engineering/remediation/README.md) and its
  [manifest](engineering/remediation/MANIFEST.md). Respect each document's Current,
  Proposed, Gate, and Future labels.
- Read [current and target architecture](engineering/remediation/01_CURRENT_AND_TARGET.md),
  then the relevant source and the relevant section of `CLAUDE.md` before editing.
- Use [testing and debugging](engineering/remediation/03_TESTING_AND_DEBUGGING.md) and
  [modularity policy](engineering/remediation/04_MODULARITY_POLICY.md) for quality gates.
- Use [Buzz/A2A and authority](engineering/remediation/06_BUZZ_A2A_AND_ENROLLMENT.md) and
  [parallel work](engineering/remediation/07_GITHUB_PROJECT_AND_PARALLEL_WORK.md) for
  coordination, ownership, worktrees, commits, and integration.
- Start writable work from the [task packet](engineering/remediation/templates/TASK_PACKET.md)
  and report it with the [handoff receipt](engineering/remediation/templates/HANDOFF_RECEIPT.md).

## Work safely

- Run builds and validation locally. Do not enable, dispatch, rerun, or add automatic
  GitHub Actions builds without an explicit human request for that specific hosted run.
  Commits, pushes, PRs, merges, tags, and routine acceptance work are not that request.
  See [the CI policy](engineering/ci/README.md); preserve normal PR review protections.
- Read `CONTRIBUTING.md`, inspect current source, related branches and existing ownership,
  then use a dedicated branch and isolated worktree for tracked changes.
- Reuse a suitable owned checkout within a task; use sparse checkout for narrow report or
  documentation work. At publication, review, integration, handoff, cancellation and completion,
  remove the worktree as soon as it is unneeded, after preserving its work and settling ownership.
  Record removal or a concrete retention reason and next cleanup trigger in the receipt. Follow
  [the worktree lifecycle](engineering/remediation/07_GITHUB_PROJECT_AND_PARALLEL_WORK.md#worktree-lifetime-and-disk-use).
- Record the outcome, scope, dependencies, base, branch/worktree, acceptance checks,
  reviewer, and publication authority before writing. Repository-relative paths coordinate
  ownership; they are not a user-maintained filesystem permission list.
- The dedicated Nemo Buzz workspace supplies authenticated Project/repository participation
  and runtime instructions to enrolled collaborators. Do not ask users to configure manual
  path grants, revision pins, or agent assignments. Tool availability does not broaden the
  current task or authorize publishing, merging, deployment, enrollment, or release.
- Treat relay storage, `processed`, `accepted`, progress, and terminal results as distinct
  A2A states. Never replay an indeterminate operation without reconciling its actual effects.
- Preserve other contributors' edits and scoped commits. Keep credentials, private machine
  paths, private evidence, and protected infrastructure details out of source and receipts.

## Preserve Nemo invariants

- Source and directly observed behavior settle facts; historical prose and agent summaries
  are navigation aids.
- For a persistent field or item type, verify every applicable consumer: save, load,
  undo/redo, selection, animation, render, export, and native bridges.
- Validate browser and Tauri behavior on their relevant surfaces. Compile success or a
  screenshot does not establish save/reload, timing, export, or packaged desktop behavior.
- Before giving instructions about a node's settings, read its local documentation and
  implementation.
- Run focused checks for the affected behavior and record exact source/candidate SHAs,
  results, limitations, and the next acceptance step. A green branch or merged PR alone
  does not close a remediation or product-acceptance gate.
