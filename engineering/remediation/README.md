<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo remediation: start here

The single execution checklist is available in [English](EXECUTION_PLAN.en.md) and
[French](EXECUTION_PLAN.fr.md). Both copies describe the same human-approved strategy of
**2026-09-07**. Use it for the complete task sequence, team roles, model/effort settings,
claims, tests, check-ins, commits, pushes, board updates and handoffs.

Its scope and workflow supersede the older R00-R22 execution order, forecasts, global phase
blockers, remote-agent assumptions and local-agent playbooks. A baseline records the exact
observed state, including known defects. Completion means modular ownership, appropriate
tests, enforced boundaries and feature contracts served through the bundled Rust MCP;
unrelated product repairs and new product breadth are outside that completion gate.

Use [Cyrill's Project #2](https://github.com/users/mysteropodes/projects/2/views/1) for
live execution and the [shared hourly log](https://github.com/mysteropodes/nemo/issues/1062)
for team summaries. Ilya's Project #8 is a legacy snapshot. Detailed receipts stay in
their task issues; the log is not another executable leaf.

## Supporting references

Read the [manifest](MANIFEST.md) to find architecture, source invariants, quality references
and historical evidence. Then read only the relevant source, local module/node documentation
and `CLAUDE.md` section for the selected leaf. Historical proposals and acceptance statements
do not override the current checklist or establish current implementation status.

The handbook and GitHub projects already exist. Do not repeat the original adoption setup,
create another backlog or require a personal skill/Buzz installation. Use the portable
workflows and issue handoff format in the checklist. The Node runner comparison is recorded
in [ADR 001](../animation/ADR-001-curve-runner.md); retain Node rather than repeating that
trial. The new c8, Rust coverage and expanded diagnostics work remains planned until its
named tasks are implemented and validated.

The approved parallel Buzz workspace configuration editor is a bounded collaboration
task. Expanded transport/infrastructure work and remote-agent enrollment remain outside
Nemo's remediation prerequisites.

## Working interpretation

A proposal does not grant file, GitHub, relay, deployment, merge or release authority.
Every writable leaf needs a human owner, exact base, branch/worktree, allowed paths,
dependencies, observable acceptance and a reviewer. Reuse its branch across sprints and
leave progress/handoffs in its existing issue. New requests are queued without silently
abandoning the active task.

Implementation status belongs in the corresponding issue and board. Do not turn the
reference handbook into another task ledger. Update a policy document in the same pull
request that changes its contract, and retain dated evidence outside normative prose.

## Safe customization

Replace placeholders such as `<base-sha>` and `<worktree-id>` when creating a task. Keep
secrets and machine-local paths in approved local configuration. Repository-relative paths,
public issue/PR links, commit IDs and sanitized artifact hashes are suitable for shared
receipts.
