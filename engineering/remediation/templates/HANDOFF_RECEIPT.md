<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Handoff / review / completion receipt

- Issue/task:
- Human owner:
- Agent/session:
- Disposition: review-ready / blocked / paused / handoff / completed
- Base SHA:
- Candidate SHA and dirty digest:
- Branch/worktree ID:
- Changed repository-relative paths:
- Intended behavior and preserved invariants:
- Contract/schema/state-authority changes:
- Generated artifacts and hashes:
- Fixture/version/seed:
- Platform/runtime/backend:

## Verification

| Command or interaction | Result: pass/fail/blocked/not-run | Evidence/artifact | Limitation |
|---|---|---|---|
|  |  |  |  |

## Review and risk

- Independent reviewer:
- Review result:
- Known failures/untested scope:
- Data/compatibility/rollback considerations:
- Visual baseline decision:
- Required downstream revalidation:

## Ownership

- Working state preserved at:
- Worktree disposition: removed / retained (owner, concrete reason and next cleanup trigger):
- Cleanup evidence: preserved commit/artifacts; worker/process settlement; directory and registration removal verified:
- Grant disposition requested:
- Handoff recipient, if any:
- Exact next action:
- Product acceptance owner/result:

An agent-complete state, green branch or merged PR does not imply product acceptance. Tie
acceptance to the declared behavior and identified integrated bytes.
