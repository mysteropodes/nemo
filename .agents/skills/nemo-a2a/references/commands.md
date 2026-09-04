# Trusted MCP model surface and human operator CLI contract

Codex and Claude use `buzz_chat_send` for ordinary channel replies and only the typed
`buzz_a2a_dispatch`, `buzz_a2a_inbox`, `buzz_a2a_status`, `buzz_a2a_cancel`, and
`buzz_a2a_handoff` MCP tools for coordination. They enforce exact
Project/repository/capability/path grants and retain all signing material. They expose no raw
event signer, generic signing proxy, private-key parameter, or inbound worker lifecycle tool;
ACP alone publishes processed/accepted/progress/result receipts.

The trusted surface is also session-shaped. `buzz_chat_send` is fixed to the current
conversation. `buzz_a2a_dispatch` and the broad `buzz_a2a_inbox` are available only to a
conversation/coordinator session. A one-shot Job session returns only its exact outcome JSON;
it may inspect its exact request with `buzz_a2a_status`, transfer it with
`buzz_a2a_handoff`, or use native subagents for local fan-out. It cannot send chat, start an
unrelated A2A operation, or enumerate sibling jobs in v1.

The unrestricted Buzz CLI owns transport, signing, event verification, subscriptions, and
receipt storage, but it is a human operator/debug surface. Managed agents do not invoke the
Buzz executable through a shell and never request its signing, authentication, or job-control
credentials. Human operators may use JSON files or stdin; command stdout is one JSON object
and diagnostics go to stderr.

The normative machine-readable surface is
[buzz-cli-contract-v1.json](buzz-cli-contract-v1.json). `check_cli_contract.py` renders the command
lines below from that file and can compare them with an actual `buzz` binary's generated
help. Canonical command surface for `NEMO-A2A-1`:

```text
buzz agents capabilities --project-address <ADDRESS>
buzz jobs submit --input <FILE|->
buzz jobs list [--project-address <ADDRESS>] [--recipient <self|PUBKEY>] [--state <STATE>] [--cursor <OPAQUE>]
buzz jobs get --operation-id <UUID>
buzz jobs accept --operation-id <UUID> --input <FILE|->
buzz jobs progress --operation-id <UUID> --input <FILE|->
buzz jobs complete --operation-id <UUID> --input <FILE|->
buzz jobs fail --operation-id <UUID> --input <FILE|->
buzz jobs cancel --operation-id <UUID> --input <FILE|->
buzz jobs acknowledge-cancel --operation-id <UUID> --input <FILE|->
buzz jobs release --operation-id <UUID> --input <FILE|->
buzz jobs handoff --operation-id <UUID> --input <FILE|->
```

Every command writes JSON to stdout by definition; there is no `--json` flag. Each
`jobs accept` invocation publishes the one 43002 claim in its input. The executor publishes
`processed`, atomically claims its receiver-owned ledger, then separately publishes
`accepted` with the processed event as predecessor. A receiver that cannot claim may publish
one terminal `declined` receipt instead. The control verbs require their matching 43005
actions. `cancel` is requester-authored; after a claim it only requests cancellation, and the
worker uses `acknowledge-cancel` with `action:"cancelled"` after becoming quiescent.
`jobs list --cursor` uses an opaque durable cursor, not a client
timestamp guess. Mutating commands return relay storage separately from any signed agent
lifecycle receipt:

```json
{
  "schema_version": "buzz.cli-result.v1",
  "relay": {"state": "stored", "event_id": "<hex>"},
  "operation_id": "<uuid>",
  "lifecycle": null
}
```

`lifecycle: null` is intentional: a successful publish cannot synthesize agent acceptance.
List/get output must retain the frozen signed event bytes, event id, authenticated author,
creation timestamp, kind, exact `h`/`p`/`i`/`k` and lifecycle `e` tags, content, verification
result, trusted server-resolved community, and durable cursor so callers can validate
correlations. The community is receipt metadata and must never be inserted into signed job
content.

Fail closed if `community.community_id` is absent or null. The receiver ledger's authority key requires
the authenticated relay's server-resolved community; a relay URL or caller-supplied content
field cannot substitute for it.

Before ACP consumes its durable Accepted admission, it performs the five-second Host/NIP-98
`POST /api/jobs/authorize` preflight described in [protocol.md](protocol.md), exact-compares
the response and local grant, and freezes Accepted in its outbox. Accepted ingest then runs
the relay's full current-state checks again. A transient acknowledgement retries the frozen
bytes; it never rebuilds the event or reruns an accepted side effect.

Until the Buzz CLI branch containing these commands is integrated, use the local harness as
a contract test only. Do not create an alternate Nemo transport CLI. If Buzz lands different
command spelling without changing the versioned JSON contract, update this reference and
the staging invocation together. Validate a candidate binary with:

```sh
python3 .agents/skills/nemo-a2a/scripts/check_cli_contract.py \
  --buzz-bin /path/to/buzz --buzz-repo /path/to/buzz-checkout
```
