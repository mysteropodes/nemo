---
name: nemo-a2a
description: Coordinate authenticated Nemo development work between Codex and Claude agents over Buzz, including capability selection, project-scoped job envelopes, durable receipts, reconnect recovery, handoffs, and local or authorized staging smoke tests.
---

<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Nemo A2A

Protocol version: `NEMO-A2A-1`

Skill version: `1.2.0`

Use this skill for agent discovery, delegation, progress, reconnect recovery, cancellation,
release, handoff, and completion. GitHub remains canonical for source, issues, pull requests,
CI, and review; Buzz carries signed coordination events only. Do not route this workflow
through Nemo's product Rust MCP.

## Documentation publication fast path

This section applies to every Nemo session, including work that does not need A2A. Use it
when the requested change only publishes an already reviewed documentation package to a
named repository path and the user has authorized the push. Its operation and decision-time
limits do not apply to implementation, research, debugging, tests, builds, remediation, or
long-running A2A jobs; those keep their task-specific budgets and milestone rules.

1. Treat it as one local Git task. Do not delegate, dispatch A2A work, request a second
   opinion, or post coordination status unless a real file, branch, worktree, authority, or
   ownership conflict is already present.
2. Use one batched preflight to check repository status, `origin`, the intended base, the
   destination branch/path, and `git var GIT_AUTHOR_IDENT`. A successful identity check is
   sufficient; ask only when it fails. Create the isolated worktree/branch from the verified
   base and copy only the named package.
3. Use one consolidated validation batch: compare the package bytes or manifest, resolve its
   relative links, reject symlinks and sensitive/private paths, and run the staged diff check.
   Test the changed artifact only. Do not run application, unit, browser, or integration suites
   for a documentation-only change.
4. If validation passes, proceed directly to `git commit -s`, the already authorized
   non-force push, and remote-SHA verification. Do not reopen settled policy, identity,
   permission, or architecture research. Do not edit the reviewed source package after staging
   unless a concrete validation failure requires a narrow correction.
5. Do not call a wait tool without a live operation or session identifier. After either twelve
   agent-initiated tool operations or two minutes of agent decision time before commit, stop all
   optional work, collapse to this fast path, and state the exact blocker if a required gate
   cannot pass. Network transfer or a required command that is still running does not consume
   the decision-time limit.
6. Stop when the remote branch resolves to the committed SHA and report that evidence. Queue
   maintenance should be one concise status transition; it must not become a parallel workflow.

## How to operate A2A

Choose the tool from the intent, then stay within its lifecycle:

| Intent | Tool | Required model-supplied fields |
| --- | --- | --- |
| Reply in the current human conversation | `buzz_chat_send` | `content` |
| Delegate one bounded job | `buzz_a2a_dispatch` | fresh `operation_id`; stable non-secret `idempotency_key`; exact `recipient_pubkey`, `capability`, `worktree_id`, repository-relative `paths`; concise `summary`; observable `acceptance`; optional inert `contracts` and GitHub coordinate; bounded `ttl_seconds` |
| Discover addressed work | `buzz_a2a_inbox` | `limit` only; unavailable from a one-shot Job session |
| Follow one request | `buzz_a2a_status` | exact `request_event_id` returned by dispatch/inbox |
| Stop work you dispatched | `buzz_a2a_cancel` | exact `request_event_id` and concrete `reason` |
| Transfer work you currently execute | `buzz_a2a_handoff` | exact `request_event_id`, authorized `handoff_to`, unchanged `worktree_id`, and concrete `reason` |

Before dispatching:

1. Check the active task queue and existing A2A status so two agents do not claim the same
   outcome, files, branch, or worktree. Divide work into non-overlapping repository-relative
   paths and acceptance checks. GitHub issue/PR state remains the public source of truth.
2. Select a recipient already authorized for the exact Project, repository, capability,
   paths, branch, base SHA, and worktree. Project membership and agent sponsorship are
   prerequisites, but neither creates a checkout grant. Never invent or broaden a grant.
3. Generate a new operation UUID. Reuse an idempotency key only for a byte-equivalent retry
   of the same request; changed semantics require a new key. Start `coordinator_epoch` at 1.
4. Put the intended result in `summary` and make every `acceptance` entry independently
   checkable. Send only inert `contract:<id>` references. Paths are relative to the repository;
   never include credentials, private keys, tokens, environment dumps, or host-local paths.

After `buzz_a2a_dispatch`, save its `request_event_id` and inspect it with
`buzz_a2a_status`. The publish result or relay acknowledgement proves only that the relay
stored an event. Work is assigned only after the exact recipient publishes both `processed`
and `accepted`. `processed` proves durable validation; `accepted` proves an atomic ownership
claim before side effects. Neither proves completion, review, merge, or release. Continue
status checks at useful milestones; do not busy-poll. A terminal `completed`, `failed`,
`indeterminate`, `cancelled`, `release`, or `handoff` ends that executor's run.

Cancellation is two-stage after a claim. The requester calls `buzz_a2a_cancel`, which produces
`cancel_requested`; the worker must quiesce and publish `cancelled` before the task is terminal.
Never infer cancellation from silence or disconnect. An `indeterminate` result requires human
or coordinator reconciliation and must not be retried automatically.

A handoff also needs two stages. The current worker calls `buzz_a2a_handoff`; that releases its
ownership but does not assign the successor. The original requester/coordinator then calls
`buzz_a2a_dispatch` with the same operation and request semantics, `coordinator_epoch` advanced
by exactly one, the returned handoff event as `supersedes_event_id`, and the authorized new
recipient. Wait again for that recipient's `processed` and `accepted` claims.

Fail closed when a tool is unavailable, a recipient or grant is missing, Project assignment is
ambiguous, the repository root/origin/branch/HEAD differs from the grant, a requested path
escapes its prefixes, signatures or lifecycle tags disagree, authorization expires, or relay
state cannot be established. Report the exact blocker through normal chat when a human must act.
Do not substitute a shell command, unrestricted Buzz CLI, raw relay request, manual signature,
or guessed credential. One-shot Job sessions may use native local subagents for bounded work,
then return their exact outcome; they may not dispatch unrelated A2A jobs or browse the inbox.

## Required workflow

1. Read [protocol.md](references/protocol.md). Managed agents use only `buzz_chat_send` for
   normal channel replies and the typed `buzz_a2a_dispatch`, `buzz_a2a_inbox`,
   `buzz_a2a_status`, `buzz_a2a_cancel`, and `buzz_a2a_handoff` MCP tools for coordination.
   The tools enforce the session's trusted identity, channel, Project, repository, peer,
   capability, path, branch, and worktree grants. `buzz_chat_send` replies only to the
   current fixed conversation. Start direct agent work with `buzz_a2a_dispatch` from a
   conversation/coordinator session. A one-shot Job session returns only its exact outcome
   JSON; it cannot call chat, dispatch another A2A job, or read the broad inbox in v1. Use
   native subagents for bounded local fan-out, `buzz_a2a_status` for the bound request, and
   `buzz_a2a_handoff` to transfer ownership.
2. Pin the request to one Buzz community, Project address/home channel, canonical GitHub
   repository, base SHA, branch, logical worktree name, repository-relative path scope,
   acceptance checks, exact recipient, and expiry. Before receiving work, configure the
   matching local [checkout grant](references/receiver-grants.md): `path_prefixes` must be
   nonempty and `checkout_root`, base SHA, branch, and worktree ID identify one live checkout.
3. Choose the route using the authorization rules in the protocol. Identity sponsorship is
   not authorization. Never send a cross-owner job by DM.
4. Create a `buzz.jobs.v1` request through `buzz_a2a_dispatch` with a fresh operation UUID
   and a stable, non-secret idempotency key. The trusted MCP retains signing authority and
   enforces exact grants; it is not a generic signing proxy. The unrestricted
   [Buzz CLI](references/commands.md) is for human operators and debugging only. A managed
   agent must not invoke it through a shell or request raw signing, authentication,
   job-control, or provider credentials. Never put a credential or host-local path in a
   prompt, child environment, or signed event.
5. Treat a relay acknowledgement only as storage by the relay, never delivery to the
   recipient. Poll `buzz_a2a_status` for one signed `processed` claim and one signed
   `accepted` claim from the exact recipient before treating work as owned. A signed
   `declined` claim ends the request without work.
6. The production ACP—not a model tool—owns inbound processed/accepted/progress/result
   lifecycle publication. It validates the full signed request and exact tags, computes the
   digest locally, obtains fresh fail-closed `POST /api/jobs/authorize` evidence, compares
   every echoed/current binding to its local grant, and consumes it immediately in the
   durable admission CAS before publishing `accepted` or causing a side effect. Accepted
   ingest still receives the relay's full current-state validation. Replay frozen signed
   outbox bytes for the same key/body; reject the same key with changed semantics. Every
   admission verifies `git rev-parse --show-toplevel`, `origin`, `git symbolic-ref`, and
   `HEAD`; checkout drift fails closed.
7. Emit progress or blocked updates only as kind `43003`. A requester `cancel` after any
   claim is only `cancel_requested`; the worker must quiesce and publish `cancelled` before
   cancellation is terminal. End owned work with exactly one completed (`43004`),
   cancelled/release/handoff (`43005`), or failed/indeterminate (`43006`) event. A root
   cancel before any processed receipt is the sole terminal-cancel exception. Never
   automatically retry an indeterminate execution. A handoff does not prove that the next
   agent accepted.
8. Run exactly one process-wide receiver for each ledger. After reconnect, resume from the
   durable cursor, ledger, and frozen outbox; retry transient acknowledgements with identical
   signed bytes. Never infer completion from a vanished connection or automatically rerun an
   accepted or indeterminate operation.

Run the deterministic proof before changing the transport integration:

```sh
python3 .agents/skills/nemo-a2a/scripts/check_package.py --package-only
python3 .agents/skills/nemo-a2a/scripts/two_agent_smoke.py --terminal result --json
python3 .agents/skills/nemo-a2a/scripts/two_agent_smoke.py --terminal handoff --json
```

The local proof is not a live relay or cryptographic acceptance test. Use
[staging-smoke.md](references/staging-smoke.md) only when the owner authorizes staging
identities and relay writes. For repository installation and fresh-session checks, read
[adoption.md](references/adoption.md).
