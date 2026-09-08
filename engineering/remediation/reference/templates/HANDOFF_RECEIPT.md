<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Handoff / review / completion receipt

> **Optional field reference — 2026-09-07.** Put the compact handoff from the
> [English execution checklist](../../EXECUTION_PLAN.en.md) /
> [French copy](../../EXECUTION_PLAN.fr.md) in the existing task issue. No extra receipt file
> or report PR is required. Update Project #2, then read back; Project #8 is legacy.

- Issue/task:
- Human owner:
- Team and lane: Ilya/Cyrill; O/D1/D2
- Agent/session:
- Actual model/effort:
- Disposition: review-ready / blocked / paused / handoff / integrated-and-accepted
- Base SHA:
- Candidate SHA and dirty digest:
- Branch/worktree ID:
- Changed repository-relative paths:
- Intended behavior and preserved invariants:
- Contract/schema/state-authority changes:
- Generated artifacts and hashes:
- Fixture/version/seed:
- Platform/runtime/backend:
- Coverage/report links and measured source denominator (or not run):

## Verification

| Command or interaction | Result: pass/fail/blocked/not-run | Evidence/artifact | Limitation |
|---|---|---|---|
|  |  |  |  |

## Review and risk

- Independent reviewer:
- Review result:
- Known baseline failures / newly introduced regressions / untested scope:
- Data/compatibility/rollback considerations:
- Visual baseline decision:
- Required downstream revalidation:

## Ownership

- Working state preserved at:
- Claim retained/released; pending unpushed or dirty work:
- Handoff recipient, if any:
- Exact next action:
- Product acceptance owner/result:

An agent-complete state, green branch or merged PR does not imply product acceptance. Tie
acceptance to the declared behavior and identified integrated bytes.
The orchestrator owns integration acceptance and final closure. Link this receipt from
the shared hourly log when relevant; do not duplicate it across reports or interrupt
another owner to manufacture a check-in. Pause the reporting timer when execution stops.
