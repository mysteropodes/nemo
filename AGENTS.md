<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo repository agent contract

These rules apply to every agent working in this repository, including nested agents.
The active lead owns task ordering and nominates one writer for its maintained task queue.
Use the current issue, pull request, or lead-designated session queue; do not create a
competing shared queue.

## Mandatory coordination skill

Before planning, delegating, accepting, or executing any concurrent or agent-to-agent
work, read and follow `.agents/skills/nemo-a2a/SKILL.md`. This includes Buzz coordination,
handoffs, reconnect recovery, Project-linked work, and parallel Codex or Claude sessions.
Pass this `AGENTS.md` requirement and the skill path to every child agent.

GitHub is canonical for source, issues, pull requests, CI, and review. Buzz carries signed
coordination. A relay acknowledgement does not mean an agent accepted or completed work,
and identity sponsorship does not grant repository or Project authority.

## Start and scope work safely

- Read `CONTRIBUTING.md` and the relevant sections of `CLAUDE.md` before editing.
- Use a dedicated branch and isolated worktree for tracked changes. Inspect current status,
  related branches, and open pull requests before diagnosing or editing shared areas.
- Give each concurrent writer an explicit repository-relative path scope and acceptance
  checks. Nominate one integration owner for shared files and combined validation.
- Never discard another contributor's edits, silently merge branches, or rewrite shared
  history. Keep commits scoped and reviewable.
- Do not push, publish, merge, deploy, enroll identities, change hosted services, or mutate
  GitHub unless the current task authorizes that action.
- Keep credentials, private keys, tokens, personal paths, and protected infrastructure
  details out of source, commits, logs, fixtures, and coordination messages.

## Preserve Nemo's engineering invariants

- Treat source and directly observed behavior as truth. Historical comments and agent
  summaries are navigation aids and can be stale.
- When adding an item type or persistent field, verify every applicable document consumer:
  save, load, undo/redo, selection, animation, render, export, and platform bridges.
- Keep browser and Tauri validation distinct. A compile or screenshot alone does not prove
  save/reload, animation timing, export, or packaged desktop behavior.
- Before giving instructions about a node's settings, read the relevant local node
  documentation and implementation.
- Run the smallest meaningful checks for the changed surface and report exact commands,
  results, source SHA, and any unverified runtime behavior.
