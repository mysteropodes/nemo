<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo foundation remediation package

Status: **R01 adoption candidate for maintainer review**<br>
Prepared: **2026-09-04**<br>
Package version: **0.2.1**
Repository: <https://github.com/mysteropodes/nemo>

This package is the portable starting point for Nemo's foundation remediation. It contains
architecture, execution, quality, collaboration and maintenance guidance that can be reviewed
and tracked with the source. It contains no credentials, private relay address, machine-specific
checkout path, private evidence, or deployment procedure.

## Reading order

| File | Purpose |
|---|---|
| [README.md](README.md) | How to adopt the package and interpret status labels |
| [01_CURRENT_AND_TARGET.md](01_CURRENT_AND_TARGET.md) | Current architecture, confirmed constraints and target direction |
| [02_REMEDIATION_PLAN.md](02_REMEDIATION_PLAN.md) | Dependency-ordered R00-R22 implementation plan |
| [03_TESTING_AND_DEBUGGING.md](03_TESTING_AND_DEBUGGING.md) | Test-runner decision, fixture stack and built-in diagnostics |
| [04_MODULARITY_POLICY.md](04_MODULARITY_POLICY.md) | Layer rules, file/function budgets, exceptions and migration method |
| [05_CAPABILITIES_MCP_AND_STANDARDS.md](05_CAPABILITIES_MCP_AND_STANDARDS.md) | Shared capability registry, Rust MCP and OpenFX/OCIO/EXR/OTIO boundaries |
| [06_BUZZ_A2A_AND_ENROLLMENT.md](06_BUZZ_A2A_AND_ENROLLMENT.md) | Human/agent enrollment and the NEMO-A2A-1 operating contract |
| [07_GITHUB_PROJECT_AND_PARALLEL_WORK.md](07_GITHUB_PROJECT_AND_PARALLEL_WORK.md) | Project fields, ownership, worktrees, commits and integration |
| [08_ACCEPTANCE_AND_MAINTENANCE.md](08_ACCEPTANCE_AND_MAINTENANCE.md) | Phase gates, evidence vocabulary and update rules |
| [templates/TASK_PACKET.md](templates/TASK_PACKET.md) | Ready-work and scope-grant template |
| [templates/HANDOFF_RECEIPT.md](templates/HANDOFF_RECEIPT.md) | Review, handoff and completion evidence template |

## Authority

These documents become team policy only after maintainer review and adoption. Source, tests,
accepted ADRs and observed runtime behavior settle implementation facts. GitHub remains
canonical for code, issues, pull requests, CI and review. Buzz carries authenticated
coordination and A2A lifecycle events; it is not the product backlog or source of merge
authority.

The package deliberately separates:

- **Current:** present in Nemo source or directly accepted collaboration behavior.
- **Proposed:** designed but not yet implemented or adopted.
- **Gate:** evidence required before a proposal can be called delivered.
- **Future:** product breadth beyond foundation remediation.

## Migration note for 0.2.0

The adoption candidate adds portable Codex and Claude entry points and aligns the handbook
with the dedicated Nemo workspace contract: enrolled collaborators receive Project/repository
participation and current A2A instructions from the runtime. Repository-relative task paths
coordinate concurrent ownership; contributors do not configure manual path grants or revision
pins. BZ0 transport acceptance and clean-clone Codex/Claude rehearsal remain required gates.

## Migration note for 0.2.1

Aligns the collaboration chapter with runtime contract 1.5.0: ordinary host tools in conversations
and delegated jobs, existing automatic Project participation, thread-visible tasks and peer
consultation, persistent following, timer delivery, and indeterminate-effect reconciliation.
This documentation update does not itself establish installed behavior or release acceptance.
