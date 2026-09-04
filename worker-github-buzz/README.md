# Nemo GitHub → Buzz bridge

This standalone Cloudflare Worker projects GitHub status into the existing Nemo
Project home channel. GitHub remains the source of truth for issues, pull
requests, and CI. The bridge publishes compact link/status messages; it never
creates a Buzz issue, task, branch, review, or GitHub mutation.

## Trust boundary

- `POST /github` accepts GitHub webhooks only after checking the exact raw body
  against `X-Hub-Signature-256` with HMAC-SHA256.
- Only `issues`, `pull_request`, and `workflow_run` events for the configured
  repository are projected. Other signed event types return `202`.
- Issue and pull-request bodies, comments, patches, logs, and actor-provided
  links are never copied. Titles are bounded, control characters and mention
  syntax are neutralized, and canonical links are constructed from the fixed
  repository coordinate.
- The outgoing message is a native Buzz kind-9 channel event signed by a
  dedicated bridge identity. Its NIP-98 request binds the exact `/events` URL,
  method, and body digest.
- A per-repository/project Durable Object records the GitHub delivery ID, raw
  body hash, semantic update ID, and deterministic Buzz event ID. Retries reuse
  the same event timestamp, so even an ambiguous relay response cannot create
  two visible events with different IDs.
- Delivered records expire after 30 days. Pending publications retry with
  bounded exponential backoff.

The Worker intentionally has no browser CORS surface and returns generic relay
errors. It never logs webhook bodies, titles, tokens, signing material, relay
responses, or identity keys.

## Local verification

Requires Node.js 20 or newer:

```bash
cd worker-github-buzz
npm ci
npm test
npm run check
```

`npm run check` runs the Node test suite and a Wrangler bundle/config dry run.
It does not contact GitHub, Buzz, or Cloudflare deployment APIs.

See [OPERATIONS.md](OPERATIONS.md) for enrollment, deployment, webhook setup,
reconciliation, and rollback.
