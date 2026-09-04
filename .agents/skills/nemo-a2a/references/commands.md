# Structured command contract

The Buzz CLI owns transport, signing, event verification, subscriptions, and durable receipt
storage. Codex and Claude invoke it with JSON files or stdin; they must not scrape desktop UI
or build raw Nostr events. Command stdout is one JSON object and diagnostics go to stderr.

The normative machine-readable surface is
[cli-contract-v1.json](cli-contract-v1.json). `check_cli_contract.py` renders the command
lines below from that file and can compare them with an actual `buzz` binary's generated
help. Canonical command surface for `NEMO-A2A-1`:

```text
buzz agents capabilities [--project-address ADDRESS]
buzz jobs submit --input FILE|-
buzz jobs list [--project-address ADDRESS] [--recipient PUBKEY|self] [--state STATE] [--cursor CURSOR]
buzz jobs get --operation-id UUID
buzz jobs accept --operation-id UUID --input FILE|-
buzz jobs progress --operation-id UUID --input FILE|-
buzz jobs complete --operation-id UUID --input FILE|-
buzz jobs fail --operation-id UUID --input FILE|-
buzz jobs cancel --operation-id UUID --input FILE|-
buzz jobs release --operation-id UUID --input FILE|-
buzz jobs handoff --operation-id UUID --input FILE|-
```

Every command writes JSON to stdout by definition; there is no `--json` flag. Each
`jobs accept` invocation publishes the one 43002 claim in its input. The executor publishes
`processed`, atomically claims its receiver-owned ledger, then separately publishes
`accepted` with the processed event as predecessor. The three control verbs require the
matching 43005 action. `jobs list --cursor` uses an opaque durable cursor, not a client
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

Fail closed if `community.id` is absent or null. The receiver ledger's authority key requires
the authenticated relay's server-resolved community; a relay URL or caller-supplied content
field cannot substitute for it.

Until the Buzz CLI branch containing these commands is integrated, use the local harness as
a contract test only. Do not create an alternate Nemo transport CLI. If Buzz lands different
command spelling without changing the versioned JSON contract, update this reference and
the staging invocation together. Validate a candidate binary with:

```sh
python3 .agents/skills/nemo-a2a/scripts/check_cli_contract.py --buzz-bin /path/to/buzz
```
