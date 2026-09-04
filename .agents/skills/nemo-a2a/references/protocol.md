# NEMO-A2A-1 protocol

## Trust and scope

GitHub is canonical for code, issues, pull requests, CI, and review decisions. Buzz is the
signed coordination plane. Nemo's Rust MCP is a product integration derived from shared
application capabilities; it is not this development protocol and must not become a second
coordination implementation.

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
| 43002 | claim | `claim.status=processed|accepted`, receiver digest | Durable observation or atomic ownership |
| 43003 | progress | `status=progress|blocked`, message, evidence | Non-terminal update |
| 43004 | result | `outcome=success`, artifacts, evidence, optional candidate SHA/capabilities | Terminal result for this execution |
| 43005 | control | `action=cancel|release|handoff`, reason, optional target | Terminal disposition for this execution |
| 43006 | error | `outcome=error`, code, message, retryable | Terminal execution error |

Kinds `43002` through `43006` repeat the common coordinates and include the exact request
event id. Progress/result/error and release/handoff require `prior_event_id`; the first claim
and a root cancellation may omit it. Processed is the request's first child; accepted follows
processed; and each predecessor has at most one child. A root cancellation is allowed only
before any lifecycle child, any later cancellation follows the current head, and no event may
fork a terminal disposition. A relay `OK` only proves relay storage/delivery. `processed` means
the recipient durably recorded a valid request. `accepted` means the recipient atomically
claimed it before side effects. Neither means completed, reviewed, merged, released, or
accepted by a human.

For one successful execution, emit one request, one processed claim, one accepted claim, and
one terminal result/control/error. Receipt replay must reuse frozen exact signed event bytes,
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
content. Never trust a sender-supplied digest. The same key and digest replays the original
disposition and frozen receipts without another side effect. The same key with a different
digest is a conflict. Claim the ledger atomically before invoking tools, shells, editors,
GitHub, or any other side effect.

An accepted operation with no terminal receipt after a crash is accepted-but-unresolved. It
requires reconciliation; at-most-once execution deliberately forbids automatic re-execution.

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
- optional GitHub issue, pull-request, and run identifiers as strings
- base SHA, branch, opaque `worktree_id`, repository-relative paths, and contract commands
- sender, exact recipient, sponsor audit fields, canonical UTC RFC 3339 expiry, and the
  kind-specific top-level fields

Community/tenant identity is trusted relay context outside signed `buzz.jobs.v1` content; a
caller-controlled content field must never select the ledger's authority domain. Absolute
paths, `..` traversal, backslashes, empty path segments, and secrets are forbidden. Evidence
and artifact references must also omit host-local absolute paths.
Keep machine-local absolute worktree paths in local ledgers or receipts, never signed events.
The receiver must re-check every requested path against the selected capability's advertised
path prefixes.

Capability names are opaque strings. A kind 43004 discovery result may advertise its
`capabilities` string list, but the list does not grant authority. Select a recipient only
after independently verifying access to the exact community, Project, channel, and requested
repository path scope.

The relay applies a 604,800-second (seven-day) cap from both server receipt time and event
creation time, rejects already expired events, and requires creation time not to exceed
expiry. Clients may choose a shorter expiry but cannot extend either server cap.
