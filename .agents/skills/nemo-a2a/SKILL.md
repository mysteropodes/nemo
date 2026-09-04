---
name: nemo-a2a
description: Work on Nemo and coordinate Codex/Claude agents in its dedicated Buzz community, with shared project access, task ownership, and practical A2A usage.
---

<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo workspace workflow

Protocol: `NEMO-A2A-1`. Skill version: `1.3.1`.

This is the shared working contract for Codex and Claude in Nemo. In Nemo's dedicated
Buzz community, every enrolled collaborator's managed agents participate in the Nemo
Project and have full read/write access to its repository. A2A and these instructions
are supplied automatically at agent startup in channels, direct messages, and background
sessions. Do not ask users to pin instruction revisions, assign agents to the Project,
configure peer grants, or fill out allowed-path forms.

This project access does not authorize unrelated systems or other people's private files.
Community authentication, verified agent ownership, and repository protections still apply.
GitHub remains canonical for source, issues, pull requests, CI, and review; Buzz carries
conversation and coordination. Instructions do not override the user's current request.

## Work on Nemo

- Use the runtime's Nemo checkout and the relevant repository guidance already in context.
  Read missing local guidance once and inspect source relevant to the task; do not repeat
  startup research for each operation.
- Keep the active task and its existing queue. Answer an incoming question briefly, attach
  any new work to that queue, then resume. Update the queue at meaningful transitions;
  queue bookkeeping must not become a separate project.
- Use a branch per change and separate worktrees for concurrent writers. Declare the task,
  files or subsystem owned, and completion criteria. File scopes coordinate ownership;
  they are not a user-maintained permission list. Resolve overlapping work with its owner.
- Preserve another developer's edits, scoped commits, DCO where required, and normal PR/
  branch protection rules. A request to publish includes its ordinary authorized Git
  steps; do not ask again for permissions or identity already established in the session.
- For persistent fields or item types, check every applicable document consumer: save,
  load, undo/redo, selection, animation, render, export, and native bridges. Browser and
  Tauri behavior require their relevant validation; do not substitute one for the other.
- Read local node documentation and implementation before giving node-setting guidance.
- Test the affected behavior. Use a focused regression when fixing a real defect; run
  broader suites only for a concrete remaining risk or a required gate. Avoid tests that
  only restate implementation details or check unrelated application behavior.
- Report the result, relevant validation, and any actual remaining blocker. Stop once the
  requested outcome is verified. Do not add memory research or optional reviews afterward.

## Documentation publication fast path

Publishing an already-reviewed documentation package is one ordinary Git task:

1. Batch repository/branch/remote status, existing work ownership, and Git identity checks.
   Reuse an appropriate existing worktree. Follow known branch protections from the start.
2. Copy only the requested package and validate the artifact once: source comparison,
   relative links, symlinks/private-data markers, and the staged diff. Correct only concrete
   failures. Application tests and independent-agent reviews add no value to this task.
3. Commit, push the authorized branch, and verify the remote commit. Use the normal PR
   route when main is protected. If one connector lacks permission, use another already
   authorized GitHub route if available; otherwise state that exact access limitation.
   Do not claim that only the repository owner can act merely because one connector failed.
4. Give the result and finish. Do not dispatch A2A, broadcast status, repeatedly revalidate,
   reopen settled policy, or look up memory after the publication result is established.

A failed operation warrants a targeted correction, not a new investigation of the whole
project. There is no fixed duration or tool-count limit on legitimate implementation,
research, debugging, tests, builds, or sustained agent work.

## Collaborate through A2A

Use A2A when another agent can independently advance the current task. Small, sequential
jobs should stay local. Call `buzz_a2a_peers` to discover the runtime's verified agent
roster, then use the selected peer's supplied identity rather than inventing one. Give it a concrete outcome, relevant
repository paths, and checkable acceptance criteria. The workspace provides Nemo access;
users do not maintain per-peer grants or refresh a hash after every source commit.

| Intent | Tool | How to use it |
| --- | --- | --- |
| Reply in the current conversation | `buzz_chat_send` | Send the answer once to the current conversation. A direct message needs no `@` prefix. |
| Discover collaborators | `buzz_a2a_peers` | Find verified Nemo agent names and identities before dispatch. An empty inbox is not a peer roster; do not ask the user to paste public keys. |
| Delegate a job | `buzz_a2a_dispatch` | Supply the verified peer, bounded task, acceptance, and required job coordinates from the current runtime. Use a fresh operation ID and a stable retry key. |
| Check addressed work | `buzz_a2a_inbox` | Inspect available addressed jobs when acting as a coordinator. Do not duplicate an already-owned task. |
| Follow one job | `buzz_a2a_status` | Use the returned request event ID; check on meaningful progress, completion, or a live wait. |
| Cancel your dispatched job | `buzz_a2a_cancel` | Use its exact request ID and reason; wait for the worker's terminal cancellation acknowledgement. |
| Hand off an owned job | `buzz_a2a_handoff` | Use the exact request, verified successor, and reason; the successor must separately accept ownership. |

A relay acknowledgement proves storage only. The recipient's `processed` and `accepted`
receipts establish validation and ownership; neither is completion. The worker publishes
progress and a terminal result through the runtime. Reuse a retry key only for the same
request body, and never execute a replayed job twice. A changed task needs a new request.
After a handoff, the coordinator advances the job epoch as the tool contract requires.

Do not call a wait tool without a live operation or session identifier. Avoid busy polling
and repeated status messages. Cancellation is complete when the worker has stopped and
reported `cancelled`; silence or a disconnected agent proves neither failure nor completion.
Report an indeterminate result for reconciliation instead of rerunning it automatically.

One-shot delegated workers stay on their assigned outcome and may use bounded native
subagents; they do not become recursive cross-agent coordinators. Shared repository access
is not a reason to overwrite someone else's worktree or ignore an active claim.

If a required tool or project context is unavailable, report the specific setup failure
promptly. Do not silently retry unchanged configuration failures, fabricate success, guess
credentials, or reconstruct raw signing/relay requests. Use the trusted Buzz tools for
chat and coordination. Keep credentials and private local infrastructure out of messages,
commits, evidence, and model prompts.

## Infrastructure references

Only when changing Buzz's A2A implementation, consult the
[wire protocol](references/protocol.md), [job schema](references/job-envelope.schema.json),
and [staging tests](references/staging-smoke.md). Their explicit-grant examples describe
legacy/general-purpose mode; the dedicated Nemo workspace supplies that authority
internally. They are not onboarding steps or extra prerequisites for normal Nemo work.
