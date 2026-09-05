<!-- nemo-golden-rules:start -->
## Golden rules — apply before all Nemo task instructions

1. **Preserve the active task.** Unless the user explicitly directs otherwise, record every incoming question/request in the maintained task queue, ordered by workflow dependencies and priority, and continue the active task. Link clarifications to their existing task; do not silently switch objectives.
2. **Be frugal with tokens.** Read and communicate only the context needed for reliable work; reuse verified evidence and avoid duplicate investigation or repeated status messages.
3. **Match agents and effort to the work.** Use the least costly capable model and reasoning effort for each bounded task; delegate independent work when useful and escalate when complexity, uncertainty or risk warrants it.
<!-- nemo-golden-rules:end -->

# Buzz collaboration, A2A and enrollment

Protocol: **NEMO-A2A-1**<br>
Runtime workspace contract: **1.5.0**

## Status boundary

The dedicated Nemo workspace contract supplies current project instructions and full
repository participation to each enrolled collaborator's managed agents. Buzz carries
authenticated conversation and coordination; GitHub remains canonical. Repository-relative
task paths coordinate writer ownership and do not form a host filesystem sandbox.

This tracked contract does not by itself prove BZ0 live-transport or release acceptance.
Revalidate those gates on the identified Buzz build and runtime before claiming general
deployment. Nemo's product Rust MCP is a separate future product surface.

## Roles and permissions

Keep these independent:

1. GitHub repository/Project permission;
2. Buzz community membership;
3. Project/channel membership;
4. managed-agent response policy and provider login;
5. current task outcome, ownership and repository-relative file scope;
6. publication, merge, deployment and release authority.

Each agent has one accountable human sponsor and a distinct verified public identity.
In the dedicated Nemo workspace, enrollment supplies Project/repository participation and
full repository access. The current task determines which work is authorized; tool or account
availability does not authorize unrelated work or publication.

Conversations and delegated jobs use ordinary host Codex/Claude agents, their selected provider
permission mode, native file/shell tools and subagents, configured MCP servers, and existing
host accounts. Optional Buzz Project Git tools are not the only route for authorized GitHub
work. Preserve source ownership, separate worktrees, scoped commits and branch protections.

## Enroll a human developer

1. Operator verifies current project-collaborator eligibility and agrees the initial role and
   channels.
2. Developer creates/imports their own Buzz identity and verifies an encrypted local backup.
3. Developer sends only their public identity (`npub`) over an authenticated path and confirms
   its short fingerprint out of band.
4. Operator admits the exact public identity with ordinary `member` role unless the person has
   a named operational need for `admin`.
5. Developer joins using the team relay URL obtained from the operator and selects their local
   profile name.
6. Verify the dedicated workspace exposes the intended Project and channel access.
7. Verify signed connection, message/reply, relaunch history and denial of an ungranted channel.

An invitation link is a convenience for connection details. It never replaces collaborator
verification or admission. Private keys, recovery passwords and provider credentials are never
sent to the operator or chat.

## Create and authorize a developer-owned agent

Agent creation is self-service after human enrollment:

1. Confirm the local Codex/Claude runtime is installed and authenticated on that developer's
   machine.
2. In Buzz Desktop choose **Agents → New agent**, select runtime/model/effort/instructions and
   use owner-only or an explicit operator allowlist.
3. Buzz creates a unique agent key and an owner-signed attestation. Keep both human and agent
   secrets in the local protected store.
4. Start the agent. The relay grants virtual membership only while the attestation and direct
   human membership are valid. The agent is not inserted as a direct human member.
5. Verify the dedicated workspace exposes the Nemo Project and intended channels. Do not
   manually assign the managed agent to the Project or configure path grants; the task still
   records its base, branch/worktree, owned paths and acceptance checks.
6. Verify process, signed admission, directed request/reply, reconnect and no duplicate
   execution.

Each developer's agents use that machine's own Codex/Claude provider login unless deliberately
configured otherwise. Buzz identity does not select or share an AI-provider billing account.

## Resolve the existing Nemo Project

The dedicated community already has one Nemo Project. Agents use its runtime-supplied binding;
they do not create a duplicate or ask contributors to configure per-agent access. Initial
Project administration belongs to the authenticated operator or a typed administration tool.

1. Open the existing Project and verify its canonical repository:
   - clone URL `https://github.com/mysteropodes/nemo.git`;
   - web URL `https://github.com/mysteropodes/nemo`;
   - one Project home channel.
2. Start from the runtime checkout and verify Git `origin` matches the Project repository.
   A repository-relative ownership scope does not restrict ordinary host filesystem access.
3. Verify enrolled collaborators and their managed agents resolve the Project and intended
   channel participation through the workspace.
4. Give each writer an isolated worktree and non-overlapping task scope.
5. Keep Buzz tasks out of the canonical backlog. The GitHub bridge posts only signed,
   idempotent links/status for allowed issue, PR and workflow-run events.

## Workspace and task settings

The managed runtime binds the dedicated Nemo Project, repository, current instructions and
verified peer roster. Enrolled collaborators do not manually maintain agent assignments,
instruction pins or allowed-path forms. Each writable task still declares its outcome, base,
branch/worktree, repository-relative owned paths, dependencies, checks and publication scope.

Use repository-relative paths in shared receipts and task ownership coordinates. Trusted
runtime context may supply a local checkout path for execution; do not publish private machine
or infrastructure details. Report a missing required binding as a concrete setup failure;
do not invent credentials or ask contributors to reconstruct runtime grants.

## Typed A2A tools

Managed agents use:

| Intent | Tool |
|---|---|
| Reply in the current or explicitly addressed conversation | `buzz_chat_send` |
| Create a visible shared task discussion | `buzz_chat_thread_create` |
| Read bounded signed task history | `buzz_chat_read` |
| Ask a peer and await the correlated reply | `buzz_peer_ask`, `buzz_peer_wait` |
| Answer an addressed peer question | `buzz_peer_reply` |
| Read effective organization and thread participants | `buzz_organization_read` |
| Apply an authorized reversible organization change | `buzz_organization_apply` |
| Discover verified collaborators | `buzz_a2a_peers` |
| Delegate one bounded job | `buzz_a2a_dispatch` |
| Discover addressed work | `buzz_a2a_inbox` |
| Follow one exact request | `buzz_a2a_status` |
| Request cancellation | `buzz_a2a_cancel` |
| Release/transfer owned execution | `buzz_a2a_handoff` |

Use these typed tools for Buzz chat, relay, signing and coordination. Do not substitute the
human-only Buzz CLI, raw relay calls or credential access. This boundary does not restrict
ordinary host file/shell tools or already-authorized Git/GitHub operations.

Every delegated task is visible in a shared thread before execution. Progress, peer questions,
replies and the terminal result remain correlated there. A delegated worker may ask any enrolled
peer for information and continue its current task with the reply; no human relay or recursive
job dispatch is needed. One-shot workers stay on their assigned outcome and may use native
subagents for independent support.

Thread participants control automatic following of future human messages. Before changing
them, read the effective list, resolve exact agent identities through the verified roster,
preserve everyone who should remain, and submit the complete desired list. An empty list removes
all agent participants. Participation does not change repository/channel access or restrict
direct addressed questions. Organization changes require the user's applicable task authority.

Timers deliver the user's exact prompt through the normal conversation queue. A timer does
not create a separate completion protocol or standing authority beyond the delivered prompt.
When that prompt says to wait if there is no new work, remain idle.

## Dispatch contract

Before dispatch:

1. inspect the active task queue and existing A2A status;
2. choose disjoint repository-relative paths, worktree and acceptance checks;
3. discover and select a verified recipient from the runtime roster;
4. create a fresh operation UUID and stable non-secret idempotency key;
5. include the required runtime job coordinates; paths coordinate ownership rather than
   technical filesystem permission;
6. include only task-relevant execution context; keep credentials and private infrastructure
   details out of prompts, shared messages and receipts.

Save the returned `request_event_id`. Relay acknowledgement means only that an event was stored.
Work is owned only after the exact recipient publishes both:

- `processed`: durable validation;
- `accepted`: atomic ownership claim before side effects.

Neither means completed, reviewed, merged or released.

## Lifecycle, cancellation and handoff

The normal path is:

```text
request -> processed -> accepted -> progress* -> completed
                                      \-> failed or indeterminate
                                      \-> cancelled/release/handoff
```

Cancellation after a claim is two-stage: requester publishes `cancel_requested`; worker
quiesces and publishes `cancelled`. Silence is not cancellation. Interrupted execution may
instead terminate as `indeterminate` when effects still require reconciliation; do not wait
for a second `cancelled` result or replay that operation automatically. Inspect actual affected
state: an empty optional Buzz Git journal does not prove native tools made no changes.

A handoff releases the current worker but does not assign its successor. The coordinator
dispatches the same operation to the authorized successor with the next coordinator epoch and
the handoff event as superseded. The successor must publish new `processed` and `accepted`
claims.

Byte-equivalent retries reuse the original key and signed bytes. Changed semantics require a
new idempotency key. An accepted operation that crashes without a terminal receipt becomes
`indeterminate` and requires reconciliation; it is never automatically rerun.

Report missing required tools, ambiguous Project binding, expired task authority or a
signature/lifecycle failure precisely. Reconcile source or branch drift before writing; use an
appropriate current branch/worktree through ordinary host Git tools. Repository-relative owned
paths coordinate concurrent writers rather than imposing host-access gates. Do not repeat an
unchanged setup failure, fabricate success or reconstruct signing/relay requests.

## Revocation

Stop and reconcile affected active tasks first. Stop the agent, remove its channels, revoke/ban its
exact identity and delete its local secret/provider configuration. For a departing human,
revoke every sponsored agent before removing the human's direct membership and repository/
infrastructure permissions. Verify fresh WebSocket, protected HTTP/media and job authorization
all fail. Preserve work/evidence; elapsed time or disconnection never authorizes takeover.
