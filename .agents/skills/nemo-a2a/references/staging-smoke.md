# Authorized staging acceptance

This is a human operator procedure. Managed agents use `buzz_chat_send` and the typed
`buzz_a2a_*` MCP tools; they do not run the Buzz executable through a shell. The command
examples below are only for an authorized operator collecting release evidence.

The deterministic harness uses a local SQLite event store and deterministic event hashes. It
does not produce Nostr signatures and does not prove NIP-42 authentication, NIP-98 requests,
NIP-AA/NIP-OA sponsorship, relay ACLs, encryption, or live reconnect behavior. Never call it
a live staging pass.

The authorized Nemo acceptance is pinned to this receiver allowlist triple; stop if live
Project discovery disagrees:

```text
project.address = 30621:1c7b17a0f192078060df6a59865f3610919b161d6c4743478ddd62a7ba1cbedf:nemo
project.home_channel = 40bdd8ad-8cf1-4757-bf43-9c7b301a9b50
repository.canonical = https://github.com/mysteropodes/nemo
```

Run the following only after the owner authorizes the staging relay writes, identities,
community, Project, and repository coordinate. Use disposable operation/idempotency values
and a no-side-effect test capability.

1. Verify both signed capability records, sponsorship chains, independent memberships,
   Project access, exact recipient, and expiry with
   `buzz agents capabilities --project-address <address>`.
2. Prepare one request fixture with a correlation token and validate it locally. Record event
   counts for that token before publishing.
3. Run `buzz jobs submit --input request.json`. Record the relay receipt separately; do not
   label it accepted. Stop if its trusted server-resolved `community.community_id` is absent
   or null.
4. Start exactly one real ACP receiver process for the ledger. Confirm its spawned model/child
   environment contains no raw signing, authentication, or job-control credentials. The
   unrestricted CLI is operator/debug-only; do not expose it as a model tool or use manual
   CLI lifecycle publication as proof of ACP acceptance.
5. Require ACP to persist `processed`, perform a fresh HTTPS/NIP-98
   `POST /api/jobs/authorize`, exact-compare its one-shot five-second response and current
   owner/event bindings to the receiver-local grant, and consume it in the durable admission
   CAS that freezes Accepted. Confirm Accepted ingest still runs full relay revalidation.
   The local grant must follow [receiver-grants.md](receiver-grants.md) and bind the exact
   Project/channel/repository, scalar base SHA/branch/worktree ID, required nonempty path
   prefixes, capability, requester, and absolute canonical checkout root. Immediately before
   admission, require live Git root/origin/branch/HEAD equality.
   Record its `grant_digest`; it is not a wire field.
6. Disconnect after the accepted receipt is durable but before advancing the consumer cursor,
   reconnect, and replay the request. Do not delete the executor ledger.
7. Complete or hand off once. Query signed events by operation id and correlation token. For
   handoff, first prove the target remains idle; only the original requester may then issue a
   next-epoch 43001 with its `supersedes_event_id`/tag bound to the 43005 event.
8. Require exactly one 43001 request, one 43002 processed claim, one 43002 accepted claim,
   exactly one 43004 completed result or 43005 handoff, and an executor ledger count of one.
   Verify authors, signatures, the closed exact routing/GitHub tag set, request-event
   correlation, Project placement, and absence from unauthorized DMs/channels.
9. Force one transient/lost relay ACK and require ACP to retry the byte-identical frozen
   outbox event. Submit the same key/body again and require a replayed disposition with no second side
   effect. Submit the same key with one semantic field changed and require a conflict.
10. Attempt a second terminal/progress event from the accepted predecessor and require a
   lifecycle-fork rejection. Attempt a TTL over seven days and require server rejection.
11. Separately prove cancellation and refusal: root cancel before `processed` is terminal;
    cancel after accepted projects only `cancel_requested`; the worker becomes quiescent and
    invokes `buzz jobs acknowledge-cancel` before state becomes `cancelled`. Race cancel and
    result from one predecessor and require exactly one winner; reject a duplicate cancel.
    Also require a root `declined` receipt with a machine reason to end without execution.
12. Exercise authorization denials for stale/expired responses, reused nonce/NIP-98 request,
    wrong Host/community/request event/digest/project/channel/repository/requester/recipient,
    changed owner or repository-announcement event, receiver-local capability/path scope,
    a `.GiT` path segment, relative checkout root, branch drift, HEAD drift, and origin drift.
    Prove a preflight success cannot bypass an authority revocation at Accepted ingest.
13. Move the clock just beyond expiry and prove only worker `declined`, `failed`,
    `indeterminate`, or `cancelled` terminal audit receipts are accepted within the 86,400
    second grace. Reject new work/progress and reject all new receipts after grace; exact
    stored-event replay must still succeed.

The receiver in this acceptance must be the real ACP 43001 consumer with its production
durable ledger, not the local Python simulator or manual CLI-only event publishing.

Preserve redacted CLI output, relay event ids, event counts, reconnect trace, and executor
ledger evidence. Do not preserve credentials. Human acceptance remains separate from this
transport proof.
