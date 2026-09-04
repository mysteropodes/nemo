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

1. Read [protocol.md](references/protocol.md) and validate the candidate agent's signed,
   unexpired capability document before sending work.
2. Pin the request to one Buzz community, Project address/home channel, canonical GitHub
   repository, base SHA, branch, logical worktree name, repository-relative path scope,
   acceptance checks, exact recipient, and expiry.
3. Choose the route using the authorization rules in the protocol. Identity sponsorship is
   not authorization. Never send a cross-owner job by DM.
4. Create a `buzz.jobs.v1` request with a fresh operation UUID and a stable, non-secret
   idempotency key. Follow [commands.md](references/commands.md) for machine-readable CLI
   I/O. Do not put credentials, private keys, provider tokens, or absolute local paths in an
   event.
5. Treat a relay acknowledgement only as relay storage/delivery. Wait for one signed
   `processed` claim and one signed `accepted` claim from the exact recipient before treating
   work as owned.
6. On the executor, validate the signed author and exact `h`/`p`/`i`/`k` tags plus lifecycle
   `e` tags before trusting content fields. Compute the request digest locally and atomically
   claim the durable ledger before any side effect. Replay the frozen signed receipt bytes
   for the same key and body; reject the same key with a changed body.
7. Emit progress or blocked updates only as kind `43003`. End the current execution with
   exactly one completed (`43004`), cancel/release/handoff (`43005`), or error (`43006`)
   event. A handoff does not prove that the next agent accepted.
8. After reconnect, resume from the durable cursor and ledger. Never infer completion from a
   vanished connection or retry an already accepted operation automatically.

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
