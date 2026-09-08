<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo foundation remediation package

Status: **current execution contract with supporting architecture references**<br>
Strategy approved: **2026-09-07**<br>
Package version: **0.3.0**
Repository: <https://github.com/mysteropodes/nemo>

The [English execution checklist](EXECUTION_PLAN.en.md) and its
[French copy](EXECUTION_PLAN.fr.md) are one operating plan in two languages. They govern
the approved remediation scope and workflow. The remaining handbook preserves architectural
contracts and historical reasoning; it is not a second execution plan or task ledger.
Implementation status must be verified against source, current issues and identified evidence.

## Reading order

| File | Purpose |
|---|---|
| [EXECUTION_PLAN.en.md](EXECUTION_PLAN.en.md) / [EXECUTION_PLAN.fr.md](EXECUTION_PLAN.fr.md) | Current scope, checkable leaves, Ilya/Cyrill teams, settings, workflows and completion gate |
| [README.md](README.md) | Short entry point and reference interpretation |
| [01_CURRENT_AND_TARGET.md](01_CURRENT_AND_TARGET.md) | Architecture reference; reconcile dated current-state claims with source |
| [02_REMEDIATION_PLAN.md](02_REMEDIATION_PLAN.md) | Historical R00-R22 reasoning; execution order, forecasts and broad gates superseded |
| [03_TESTING_AND_DEBUGGING.md](03_TESTING_AND_DEBUGGING.md) | Quality reference; Node decision retained, expanded coverage/diagnostics still planned |
| [04_MODULARITY_POLICY.md](04_MODULARITY_POLICY.md) | Layer rules, file/function budgets, exceptions and migration method |
| [05_CAPABILITIES_MCP_AND_STANDARDS.md](05_CAPABILITIES_MCP_AND_STANDARDS.md) | Capability/application/MCP contracts and future standard ports; new OFX/OCIO/EXR/OTIO breadth deferred |
| [06_BUZZ_A2A_AND_ENROLLMENT.md](06_BUZZ_A2A_AND_ENROLLMENT.md) | Reference only for explicitly requested Buzz/A2A work; no remediation enrollment prerequisite |
| [07_GITHUB_PROJECT_AND_PARALLEL_WORK.md](07_GITHUB_PROJECT_AND_PARALLEL_WORK.md) | Historical setup/workflow proposal plus ownership and isolation reference |
| [08_ACCEPTANCE_AND_MAINTENANCE.md](08_ACCEPTANCE_AND_MAINTENANCE.md) | Evidence vocabulary and maintenance reference; former broad close gate superseded |
| [templates/TASK_PACKET.md](templates/TASK_PACKET.md) | Optional detailed scope reference; use the checklist's issue format for current leaves |
| [templates/HANDOFF_RECEIPT.md](templates/HANDOFF_RECEIPT.md) | Optional evidence reference; no additional handoff document required |

## Authority

The human-approved execution checklist governs current scope and workflow where older
documents conflict. Source, tests, accepted ADRs and observed runtime behavior settle
implementation facts; approving a plan does not mark its work implemented or accepted.
GitHub remains canonical for code, issues, pull requests, CI and review. Local teams use
existing issue claims/handoffs and board updates. Tool availability or a reference document
does not independently grant publication, merge, deployment or release authority.

The package deliberately separates:

- **Current:** present in Nemo source or directly accepted collaboration behavior.
- **Proposed:** designed but not yet implemented or adopted.
- **Gate:** evidence required before a proposal can be called delivered.
- **Future:** product breadth beyond foundation remediation.

## Migration note for 0.3.0 — 2026-09-07

Replace competing execution/playbook documents with the English/French checklist. Preserve
the exact observed baseline, including defects; atomize broad R parents into bounded leaves;
use human-led local teams and issue handoffs, with one branch across sprints and bounded
writer worktrees. Finish modular ownership, tests, enforcement and feature registration via
the bundled Rust MCP. New product breadth and expanded Buzz transport/infrastructure are
deferred; the explicitly authorized bounded Buzz workspace configuration editor has its
own lane and does not gate Nemo extraction. Project #2 is primary; Project #8 is legacy.

The [Node runner decision](../animation/ADR-001-curve-runner.md) already exists. New c8,
Rust coverage and expanded diagnostics are planned work, not newly delivered capabilities.
Source reviewed at `66ece0641708122eb8447e85ad8dd7e3402aaf6c` already includes the opacity
application service, Rust MCP transport/bridge, schema and stdio tests. General feature
declaration discovery and full surface/installed-client acceptance remain distinct work.
The original handbook adoption, forecast and remote-enrollment requirements below are
historical; do not rerun them as prerequisites for current remediation.

## Historical migration note for 0.2.0

The adoption candidate adds portable Codex and Claude entry points and aligns the handbook
with the dedicated Nemo workspace contract: enrolled collaborators receive Project/repository
participation and current A2A instructions from the runtime. Repository-relative task paths
coordinate concurrent ownership; contributors do not configure manual path grants or revision
pins. That version required BZ0 transport acceptance and a clean-clone Codex/Claude
rehearsal; the current checklist replaces its execution prerequisites.

## Historical migration note for 0.2.1

Aligns the collaboration chapter with runtime contract 1.5.0: ordinary host tools in conversations
and delegated jobs, existing automatic Project participation, thread-visible tasks and peer
consultation, persistent following, timer delivery, and indeterminate-effect reconciliation.
This documentation update does not itself establish installed behavior or release acceptance.
