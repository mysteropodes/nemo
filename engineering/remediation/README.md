<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Using this package

Start with [the manifest](MANIFEST.md), then read the current/target architecture and the
remediation plan. A developer or agent working on one packet reads only the relevant policy,
task packet and module documentation in addition to the repository's contributor guidance.

## Adoption checklist

1. Reconcile every current-state statement with the selected `main` commit.
2. Review and accept, amend or reject the proposed ADR-level decisions.
3. Choose the tracked destination, for example `engineering/remediation/`, and move this
   directory as one unit so its internal links remain valid.
4. Point the repository's Codex and Claude entry files at the adopted documents. Keep those
   entry files short.
5. During adoption, install or retain `.agents/skills/nemo-a2a/` as the canonical A2A skill;
   this proposal package does not include that runtime skill. Test clean-clone discovery in
   both clients.
6. Create the GitHub Project fields/views and real CI workflow before making their checks
   required.
7. Open R00-R22 as parent/child issues only after current owners, dependencies and acceptance
   are reconciled.
8. Validate one small multi-agent fixture before opening broad concurrent extraction.

## Working interpretation

A proposal does not grant file, GitHub, relay, deployment, merge or release authority.
Every writable task needs a human owner, exact base, branch/worktree, allowed paths,
dependencies, observable acceptance and a reviewer. New requests are queued without
silently abandoning the active task.

Implementation status belongs in the corresponding issue and handoff receipt. Do not turn
this package into a shared mutable task ledger. Update a policy document in the same pull
request that changes its contract, and retain dated evidence outside normative prose.

## Safe customization

Replace placeholders such as `<base-sha>` and `<worktree-id>` when creating a task. Keep
secrets and machine-local paths in approved local configuration. Repository-relative paths,
public issue/PR links, commit IDs and sanitized artifact hashes are suitable for shared
receipts.
