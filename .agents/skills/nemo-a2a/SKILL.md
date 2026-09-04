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

Skill version: `1.0.0`

Use this skill for agent discovery, delegation, progress, reconnect recovery, cancellation,
release, handoff, and completion. GitHub remains canonical for source, issues, pull requests,
CI, and review; Buzz carries signed coordination events only. Do not route this workflow
through Nemo's product Rust MCP.

## Required workflow

1. Read [protocol.md](references/protocol.md). Managed agents use only `buzz_chat_send` for
   normal channel replies and the typed `buzz_a2a_dispatch`, `buzz_a2a_inbox`,
   `buzz_a2a_status`, `buzz_a2a_cancel`, and `buzz_a2a_handoff` MCP tools for coordination.
   The tools enforce the session's trusted identity, channel, Project, repository, peer,
   capability, path, branch, and worktree grants.
2. Pin the request to one Buzz community, Project address/home channel, canonical GitHub
   repository, base SHA, branch, logical worktree name, repository-relative path scope,
   acceptance checks, exact recipient, and expiry.
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
   outbox bytes for the same key/body; reject the same key with changed semantics.
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
