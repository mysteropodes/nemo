> Dedicated Nemo workspace: skill 1.3.0 is the current operating contract.
> Manual pins, Project assignment, and explicit-grant steps below describe legacy/general
> mode and infrastructure tests, not normal Nemo onboarding or task prerequisites.

# NEMO-A2A-1 protocol

## Trust and scope

GitHub is canonical for code, issues, pull requests, CI, and review decisions. Buzz is the
signed coordination plane. Managed agents use `buzz_chat_send` for ordinary channel replies
and only the typed `buzz_a2a_dispatch`, `buzz_a2a_inbox`, `buzz_a2a_status`,
`buzz_a2a_cancel`, and `buzz_a2a_handoff` MCP tools for coordination. The trusted Rust MCP
keeps signing and job-control credentials outside the model boundary, enforces grants before
signing, and is not a generic signing proxy. Inbound processed/accepted/progress/result
publication belongs to ACP, not to a worker lifecycle tool. Nemo's product Rust MCP remains
a separate product integration derived from shared application capabilities; it must not
become another copy of this development protocol.

Trusted MCP authority is fixed when ACP creates the provider session. A conversation session
may send chat in that exact conversation and dispatch or enumerate work within its local
grants. A one-shot Job session is additionally bound to its operation and request: it may
read that lifecycle, request its cancellation or handoff when the actor rules permit, and
return only the exact outcome JSON consumed by ACP. It cannot send chat, dispatch an unrelated
operation, or enumerate sibling jobs in v1. Use the provider's native subagents for bounded
local fan-out. Nested A2A delegation requires a future signed parent/child protocol contract;
local grant overlap alone is not enough authority.

The transport-authenticated event author and tags are authoritative. Content identity fields
exist for audit and must match them. NIP-AA/NIP-OA sponsorship proves who sponsors an agent;
it does not convey membership, channels, relay roles, GitHub permissions, path grants, or a
work grant. Capability records are usable only after their signatures, sponsorship chain,
expiry, and independently granted community/project access have been verified.

Every event has exactly one canonical `h`, `p`, `i`, `k`, `a`, and
`github-repository` tag. Their values equal `project.home_channel`, `recipient_pubkey`,
`operation_id`, `idempotency_key`, `project.address`, and the normalized `owner/repo` derived
from `repository.canonical`. Optional `repository.github_issue`, `github_pr`, and `github_run`
fields map one-for-one to exact `github-issue`, `github-pr`, and `github-run` tags; an absent
field requires an absent tag. The tag set is closed: unrecognized, malformed, or duplicate
tags are rejected. An initial request has no `e` tag. Every follow-up has
`["e",request_event_id,"","root"]`; when `prior_event_id` is present it also has
`["e",prior_event_id,"","reply"]`. A superseding request instead has exactly
`["e",supersedes_event_id,"","supersedes"]`.

Both signer and recipient must be direct members of the Project home channel. A same-owner
encrypted DM is allowed only when both participants are also independently authorized for
the community and Project. Agents with different sponsors may coordinate only in the shared
Project home channel, addressed to the exact recipient. Cross-owner DM is forbidden.
Before accepting, the receiver must verify that the exact
`(project.address, project.home_channel, repository.canonical)` triple is allowlisted for the
capability. A relay stored receipt grants no repository authority.

## Event kinds and lifecycle

| Kind | Role | Kind-specific top-level fields | Meaning |
| ---: | --- | --- | --- |
| 43001 | request | capability, summary, acceptance | Proposed work; no request/prior id or `e` tag |
| 43002 | claim | `claim.status=processed|accepted|declined`, receiver digest, declined reason | Durable observation, atomic ownership, or terminal refusal |
| 43003 | progress | `status=progress|blocked`, message, evidence | Non-terminal update |
| 43004 | result | `outcome=success`, artifacts, evidence, optional candidate SHA/capabilities | Terminal result for this execution |
| 43005 | control | `action=cancel|cancelled|release|handoff`, reason, optional target | Cancellation request/acknowledgement or terminal release/handoff |
| 43006 | error | `outcome=failed|indeterminate`, code, message, retryable | Terminal known failure or unresolved effect state |

Kinds `43002` through `43006` repeat the common coordinates and include the exact request
event id. Progress/result/error and release/handoff require `prior_event_id`; the first claim
and a root cancellation may omit it. Processed is the request's first child; accepted follows
processed; and each predecessor has at most one child. A worker may instead issue a terminal
`declined` claim from the root with no predecessor and a 1-64 byte lowercase machine reason.
A requester root `cancel` is terminal only before any processed receipt. A later requester
`cancel` follows the current processed/accepted/progress head and projects
`cancel_requested`; it is not terminal until the exact worker publishes `cancelled` with the
cancel event as predecessor. Result and cancel attempts from the same predecessor are
mutually fenced. Release/handoff may follow only accepted/progress. No event may fork a
terminal disposition. A relay `OK` only proves relay storage, not recipient delivery. `processed` means
the recipient durably recorded a valid request. `accepted` means the recipient atomically
claimed it before side effects. Neither means completed, reviewed, merged, released, or
accepted by a human.

For one successful execution, emit one request, one processed claim, one accepted claim, and
one terminal result/control/error. The production ACP receiver owns the durable claim ledger;
the local simulator is only a deterministic contract proof. Receipt replay must reuse frozen exact signed event bytes,
including the original creation timestamp; rebuilding equivalent content can create a new
event id and duplicate the lifecycle. A `handoff` closes the current executor's ownership but
does not accept work for `handoff_to`; that agent must produce its own explicit claim under an
authorized follow-up request. Specifically, a handoff is only a terminal release by the
current worker. The original requester/coordinator and sponsor must issue a new kind 43001 with
`supersedes_event_id` pointing to the handoff, the same operation/idempotency/Project/repository
scope, expiry, and request semantics, exactly the next `coordinator_epoch`, and
`recipient_pubkey` equal to `handoff_to`. Only that new recipient's processed and accepted
claims authorize new work.

## Durable idempotency

Inside each receiver-owned ledger namespace, the executor key is:

```text
(server-resolved authenticated community, authenticated request author, idempotency_key)
```

Two agents may use separate databases or partition a shared database by authenticated
executor/recipient; a global unpartitioned table would make a legitimate higher-epoch
handoff collide with the first worker's digest. The receiver computes a SHA-256 digest over
the canonical JSON form of the validated request
content with object keys recursively sorted and no insignificant JSON whitespace. Never trust
a sender-supplied digest. The same key and digest replays the original disposition and frozen
receipts without another side effect. An exact previously stored signed event may be
acknowledged again after expiry or later scope-state changes once current principal/community
authentication succeeds; it must not re-run TTL, transition, or side-effect checks. The same
key with a different
digest is a conflict. Claim the ledger atomically before invoking tools, shells, editors,
GitHub, or any other side effect.

An accepted operation with no terminal receipt after a crash is accepted-but-unresolved. It
requires an `indeterminate` receipt and reconciliation; at-most-once execution deliberately
forbids automatic re-execution. `indeterminate` always carries `retryable:false`.

## Fresh execution authorization

Relay storage, historical capability advertisements, and sponsorship do not authorize code
execution. Immediately before its local durable admission CAS, the trusted ACP receiver must
make a NIP-98-authenticated `POST /api/jobs/authorize` request to the Host-bound relay tenant.
Production requires HTTPS. Plain HTTP is permitted only in explicit dev/test mode for exact
loopback hosts `127.0.0.1`, `[::1]`, or `localhost`.

The `buzz.job-authorization.v1` request contains exactly `schema_version`, one-shot UUID
`nonce`, `request_event_id`, receiver-computed `semantic_digest`, `/api/context` community
UUID and canonical relay host, project channel UUID and address, the full repository object,
and requester/recipient pubkeys. NIP-98 identity, target URL/body binding, current community
membership, Host tenancy, and both the NIP-98 event and client nonce are fail-closed replay
guards. The authenticated HTTP caller must be the addressed recipient.

A success response contains `authorized:true`, a one-use `authorization_id`, canonical
`issued_at`/`expires_at` separated by at most five seconds, an exact echo under `binding`,
current `project_head_event_id`, `repository_coordinate`,
`repository_announcement_event_id`, and current requester/recipient owner pubkeys. Denials
are non-2xx. There is no response signature: TLS, Host resolution, and NIP-98 authenticate
the exchange. There is no caller-provided community selector, relay pubkey expectation, or
wire `grant_digest`.

The receiver rejects duplicate JSON keys before typed decoding and compares every response
field immediately, including the full repository, event IDs, owner identities, freshness,
nonce, request event, digest, community, host, requester, and recipient. It separately
enforces its local grant over the exact Project, home channel, canonical repository, base
SHA, branch, worktree ID, path prefixes, capability, allowed requester/recipient, and
canonical checkout, and records its own local `grant_digest`. The receiver-local file and
machine-readable schema are documented in [receiver-grants.md](receiver-grants.md). Each
grant has required nonempty `path_prefixes` and scalar `base_sha`, `branch`, `worktree_id`,
and absolute `checkout_root`; plural or wildcard checkout coordinates are invalid.
Immediately before each admission, `git rev-parse --show-toplevel` must resolve to that
canonical root, `origin` must normalize to the exact GitHub repository, `git symbolic-ref
--quiet --short HEAD` must equal the branch, and `git rev-parse HEAD` must equal the base
SHA. A detached head, remote change, branch change, new commit, Git warning, or ambiguous
grant fails closed.
The authorization ID is consumed once inside the same durable CAS that owns the frozen
Accepted outbox record. A preflight token never bypasses full Accepted-event ingest
revalidation; authority revocation between preflight and ingest must still fail closed.

One process-wide receiver owns a ledger at a time. Each outbound lifecycle event is frozen
before publication and retained in the durable frozen outbox until acknowledged; a lost relay ACK
retries those exact bytes. Child/model environments are built from an allowlist and contain
no raw signing, authentication, or job-control credentials. Signing stays in the trusted
MCP/ACP boundary. Managed agents never invoke the unrestricted Buzz executable through a
shell; it remains a human operator and debugging surface.

## Envelope

The normative machine-readable schema is
[job-envelope.schema.json](job-envelope.schema.json). Common coordinates are:

- `schema_version`: `buzz.jobs.v1`; `NEMO-A2A-1` is the skill/protocol document version and
  is deliberately not a signed content field
- `operation_id`: stable UUID for the operation
- `idempotency_key`: stable, opaque, non-secret retry key
- `coordinator_epoch`: positive 32-bit coordinator generation, repeated unchanged through transitions
- `project.address` and UUID `project.home_channel`
- `repository.canonical`: strict lowercase `https://github.com/owner/repo` (no credentials,
  port, query, fragment, trailing slash, or `.git` suffix)
- optional GitHub issue, pull-request, and run identifiers as canonical positive-decimal
  strings (1-20 digits, no zero or leading zero); issue and pull request are mutually exclusive
- base SHA, branch, opaque `worktree_id`, repository-relative paths, and inert contract references
- sender, exact recipient, sponsor audit fields, canonical UTC RFC 3339 expiry, and the
  kind-specific top-level fields

Repository contracts contain only `contract:<portable-id>` coordinates. Artifact and evidence
references may also use `git:<40-or-64-lowercase-hex>`,
`buzz:event:<64-lowercase-hex>`, or credential-free `https://github.com/...` URLs without a
non-default port, query, or fragment. Executable command strings do not belong in signed
contract references; map inert names to commands in trusted local config.

Community/tenant identity is trusted relay context outside signed `buzz.jobs.v1` content; a
caller-controlled content field must never select the ledger's authority domain. Wire paths
forbid absolute paths, `..` traversal, backslashes, empty or dot segments, and every
case-variant of a `.git` segment. Evidence and
artifact references must also omit host-local absolute paths. Before access, the receiver
must resolve every path beneath the authorized canonical checkout and additionally reject
traversal, empty segments, and symlink escapes.
Keep machine-local absolute worktree paths in local ledgers or receipts, never signed events.
The receiver must re-check every requested path against the selected capability's advertised
path prefixes.

Capability names are opaque strings. A kind 43004 discovery result may advertise its
`capabilities` string list, but the list does not grant authority. Select a recipient only
after independently verifying access to the exact community, Project, channel, and requested
repository path scope.

The relay applies a 604,800-second (seven-day) cap from both server receipt time and event
creation time. After expiry, new progress, claims, results, release, or handoff are rejected.
Only worker-authored terminal `declined`, `failed`, `indeterminate`, or `cancelled` audit
receipts may be added for 86,400 seconds after expiry; later new events are rejected. Exact
stored-event replay remains allowed after that window once current principal/community
authentication succeeds.
