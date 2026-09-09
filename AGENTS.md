<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
4. **Follow the documented process — never propose a bypass.** Even when elevated access makes a shortcut technically possible (e.g., an admin-bypass merge, skipping a required independent approval), surface and follow the plan's compliant path instead; do not offer the non-compliant option as if it were a normal choice.
5. **Verify before asserting or starting work.** State process, permission, or ownership facts only after checking the live source (git/gh state, the plan text) — label anything unchecked as an assumption. Before claiming or recommending work on an issue/PR/branch, check for an existing claim by another agent to avoid duplicating it.
<!-- nemo-golden-rules:end -->

# Nemo repository agent entry point

These instructions apply to Codex, Claude, and their child agents in this repository.
GitHub is canonical for source, issues, pull requests, CI, and review. Keep the active task
in its issue, pull request, or lead-designated queue; do not create a competing shared ledger.

## Find the project contract

- Start with the single remediation execution checklist:
  [English](engineering/remediation/EXECUTION_PLAN.en.md) /
  [français](engineering/remediation/EXECUTION_PLAN.fr.md). Its human-approved
  2026-09-07 scope and workflow supersede older plans, phase gates and agent playbooks.
  The [short entry point](engineering/remediation/README.md) links the four supporting
  references. The archived handbook is historical context, not a required reading list.
- Read [current and target architecture](engineering/remediation/reference/01_CURRENT_AND_TARGET.md),
  then the relevant source and the relevant section of `CLAUDE.md` before editing.
- Use the execution checklist for current acceptance, board updates, claims, commits,
  pushes and issue handoffs. Use [testing and debugging](engineering/remediation/reference/03_TESTING_AND_DEBUGGING.md)
  and [modularity policy](engineering/remediation/reference/04_MODULARITY_POLICY.md) as supporting
  references; planned tooling is not implemented merely because a document names it.
- Work through Ilya's or Cyrill's local team, at most one orchestrator and two delegates
  each. Reuse one branch per outcome across sprints; keep at most two writable task
  worktrees plus the primary checkout per machine. Progress and handoffs belong in the
  existing issue, not new report PRs or additional ledgers.
- The checklist contains portable workflows and the compact claim/handoff format. No
  personal skill installation or Buzz enrollment is required for this remediation.
  Existing packet/receipt templates are reference material, not extra required documents.

## Remediation scope

- Baseline means the exact observed current state, including identified failures and
  unavailable checks. Preserve that evidence; do not repair unrelated features first.
- Finish module boundaries, one writable state authority, regression tests, enforcement,
  feature declarations and their shared application API/bundled Rust MCP integration.
  Existing broken features need explicit availability and isolated ownership, not a
  product fix as a prerequisite to remediation completion.
- R03/R05 and the other broad issues are tracking parents. Only named executable leaf
  dependencies block work; their whole-issue closure is not a global extraction gate.
- New OpenFX effects, full OCIO/EXR/OTIO implementations, expanded Buzz transport/
  infrastructure and broad product/performance work are deferred. The explicitly
  authorized bounded Buzz workspace configuration editor may proceed in its separate
  owned lane; it is not a prerequisite to Nemo extraction. Preserve suitable ports now.

## Work safely

- Run builds and validation locally. Do not enable, dispatch, rerun, or add automatic
  GitHub Actions builds without an explicit human request for that specific hosted run.
  Commits, pushes, PRs, merges, tags, and routine acceptance work are not that request.
  See [the CI policy](engineering/ci/README.md); preserve normal PR review protections.
- Read `CONTRIBUTING.md`, inspect current source, related branches and existing ownership,
  then use a dedicated branch and isolated worktree for tracked changes.
- Record the outcome, scope, dependencies, base, branch/worktree, acceptance checks,
  reviewer, and publication authority before writing. Repository-relative paths coordinate
  ownership; they are not a user-maintained filesystem permission list.
- Tool availability does not broaden the current task's publication, merge, deployment,
  enrollment or release authority. Preserve existing human authorization across handoffs.
- For explicitly requested Buzz/A2A work, use the current execution checklist and actual
  runtime documentation. The [archived authority chapter](engineering/remediation/archive/2026-09-07-handbook/06_BUZZ_A2A_AND_ENROLLMENT.md)
  provides historical protocol context only. Treat relay storage, `processed`, `accepted`,
  progress and terminal results as distinct states; reconcile actual effects before retrying
  an indeterminate operation.
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
