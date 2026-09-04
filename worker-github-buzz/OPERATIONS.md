# Operator runbook

No secret or deployment-specific identifier is committed. Complete these steps
only after the bridge change is reviewed and approved for deployment.

## Required inputs

| Binding | Cloudflare type | Purpose / minimum authority |
|---|---|---|
| `GITHUB_REPOSITORY` | checked-in var | Exact `owner/repo` allowlist; currently `mysteropodes/nemo` |
| `GITHUB_WEBHOOK_SECRET` | secret | Random secret shared only with this GitHub webhook |
| `BUZZ_BRIDGE_PRIVATE_KEY` | secret | Dedicated 32-byte hex Nostr signing key; never a human or agent key |
| `BUZZ_RELAY_HTTP_URL` | secret | HTTPS origin of the authenticated community relay, without a path |
| `BUZZ_PROJECT_ADDRESS` | secret | Exact Nemo NIP-MP coordinate (`30621:<owner>:<dtag>`) |
| `BUZZ_HOME_CHANNEL_ID` | secret | Exact Project home-channel UUID |
| `RECONCILE_TOKEN` | secret | Independent random bearer token for `/reconcile` |
| `BUZZ_AUTH_TAG` | optional secret | Current owner authorization attestation when the signer uses NIP-OA |
| `GITHUB_READ_TOKEN` | optional secret | Fine-grained, read-only metadata/actions token for higher reconcile limits |

The deploy workflow also needs the existing repository Actions secrets
`CLOUDFLARE_API_TOKEN` (Workers Scripts: Edit) and
`CLOUDFLARE_ACCOUNT_ID`. They deploy code and do not become Worker runtime
secrets.

The bridge signer must be enrolled in the same Buzz community and added to the
Project home channel with the smallest message-publishing role. Record its
accountable human sponsor. If admission uses owner-backed NIP-OA, store the
attestation as `BUZZ_AUTH_TAG`; otherwise omit it. Never reuse a developer,
owner, or coding-agent private key.

## Provision Worker secrets

From this directory, run each command and paste the value only at Wrangler's
hidden prompt:

```bash
npx wrangler secret put GITHUB_WEBHOOK_SECRET
npx wrangler secret put BUZZ_BRIDGE_PRIVATE_KEY
npx wrangler secret put BUZZ_RELAY_HTTP_URL
npx wrangler secret put BUZZ_PROJECT_ADDRESS
npx wrangler secret put BUZZ_HOME_CHANNEL_ID
npx wrangler secret put RECONCILE_TOKEN
```

Add `BUZZ_AUTH_TAG` only for an attested signer. Add `GITHUB_READ_TOKEN` only
if unauthenticated public-repository API limits are insufficient. The token
needs metadata, issues, pull requests, and Actions **read** access; it needs no
contents or administration write scope.

Deploy with `npx wrangler deploy` or merge and run the dedicated Actions
workflow after all secrets exist. The declarative Worker export provisions a
SQLite-backed Durable Object namespace on first deployment. A health probe is
available at `GET /health` and reveals no configuration.

## Create the GitHub webhook

Repository administration is required for this one-time GitHub change:

1. Open **Settings → Webhooks → Add webhook** for the configured repository.
2. Set the payload URL to the deployed Worker origin plus `/github`.
3. Choose `application/json` and paste the exact `GITHUB_WEBHOOK_SECRET`.
4. Select only **Issues**, **Pull requests**, and **Workflow runs**.
5. Keep SSL verification enabled and activate the webhook.

Do not modify or replace unrelated existing webhooks. A successful test
delivery must yield one Project-home message and a repeated delivery must yield
none.

## Reconcile a missed interval

`POST /reconcile` is an explicit, read-only repair path. It requires the bearer
token and a JSON body:

```json
{
  "since": "2026-09-03T20:00:00Z",
  "source": "issues",
  "limit": 10,
  "page": 1
}
```

`source` is one of `issues`, `pull_requests`, or `workflow_runs`; reconcile each
source separately. `since` must be within seven days, `limit` is 1–20, and
`page` is 1–10. The endpoint reads public GitHub metadata, normalizes it through
the same projector, and submits the same semantic IDs to the Durable Object.
Repeating the request is safe. If `possibleMore` is true, continue that source
with the next page. No GitHub API credential is required for the public
repository, though unauthenticated rate limits are lower.

## Acceptance and rollback

Before enabling the webhook, exercise a non-production Worker with fixture
payloads and a test Project channel. Acceptance requires: invalid signature and
wrong-repository rejection; one event after a repeated delivery; no issue body
or raw title mention; a valid Buzz signature; and recovery after a simulated
relay failure.

To stop projection, deactivate only this GitHub webhook. Preserve the Worker
and Durable Object during diagnosis so delivery evidence remains available.
Removing the Worker or its namespace is a separate destructive cleanup step.
