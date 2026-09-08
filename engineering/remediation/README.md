<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo remediation: start here

Use the single execution checklist in [English](EXECUTION_PLAN.en.md) or
[French](EXECUTION_PLAN.fr.md). It contains the approved task sequence, team roles,
model/effort guidance, tests, check-ins, Git workflow, board updates and completion gate.
The baseline is the exact observed state, including defects. The goal is modular ownership,
testing, enforced boundaries and feature contracts served through the bundled Rust MCP.

Track execution on [Cyrill's Project #2](https://github.com/users/mysteropodes/projects/2/views/1)
and team summaries in the [shared hourly log](https://github.com/mysteropodes/nemo/issues/1062).
Detailed claims and handoffs stay in their task issues. Project #8 is a legacy snapshot.

Read only the supporting reference needed for the selected task:

- [Current and target architecture](reference/01_CURRENT_AND_TARGET.md)
- [Testing and debugging](reference/03_TESTING_AND_DEBUGGING.md)
- [Modularity and file-size policy](reference/04_MODULARITY_POLICY.md)
- [Capability contracts, Rust MCP and future standards](reference/05_CAPABILITIES_MCP_AND_STANDARDS.md)

Optional expanded field templates are under `reference/templates/`; the checklist already
contains the required issue formats. [The archived handbook](archive/2026-09-07-handbook/)
preserves superseded plans, collaboration assumptions and acceptance gates for historical
context. It is not an additional execution plan or prerequisite. Current source and dated
runtime evidence establish implementation; reference prose alone does not.
