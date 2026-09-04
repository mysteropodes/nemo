# Authorized staging acceptance

The deterministic harness uses a local SQLite event store and deterministic event hashes. It
does not produce Nostr signatures and does not prove NIP-42 authentication, NIP-98 requests,
NIP-AA/NIP-OA sponsorship, relay ACLs, encryption, or live reconnect behavior. Never call it
a live staging pass.

Run the following only after the owner authorizes the staging relay writes, identities,
community, Project, and repository coordinate. Use disposable operation/idempotency values
and a no-side-effect test capability.

1. Verify both signed capability records, sponsorship chains, independent memberships,
   Project access, exact recipient, and expiry with `buzz agents capabilities`.
2. Prepare one request fixture with a correlation token and validate it locally. Record event
   counts for that token before publishing.
3. Run `buzz jobs submit --input request.json`. Record the relay receipt separately; do not
   label it accepted. Stop if its trusted server-resolved `community.id` is absent or null.
4. On the authorized receiver, invoke `buzz jobs accept` once with the processed-claim JSON,
   atomically claim the receiver ledger, then invoke it again with the accepted-claim JSON
   whose predecessor is the processed event. Each invocation publishes exactly the single
   43002 claim supplied through `--input`; neither invocation runs the executor. Run the
   configured no-side-effect executor once only after the accepted claim is durable.
5. Disconnect after the accepted receipt is durable but before advancing the consumer cursor,
   reconnect, and replay the request. Do not delete the executor ledger.
6. Complete or hand off once. Query signed events by operation id and correlation token. For
   handoff, first prove the target remains idle; only the original requester may then issue a
   next-epoch 43001 with its `supersedes_event_id`/tag bound to the 43005 event.
7. Require exactly one 43001 request, one 43002 processed claim, one 43002 accepted claim,
   exactly one 43004 completed result or 43005 handoff, and an executor ledger count of one.
   Verify authors, signatures, the closed exact routing/GitHub tag set, request-event
   correlation, Project placement, and absence from unauthorized DMs/channels.
8. Submit the same key/body again and require a replayed disposition with no second side
   effect. Submit the same key with one semantic field changed and require a conflict.
9. Attempt a second terminal/progress event from the accepted predecessor and require a
   lifecycle-fork rejection. Attempt a TTL over seven days and require server rejection.

Preserve redacted CLI output, relay event ids, event counts, reconnect trace, and executor
ledger evidence. Do not preserve credentials. Human acceptance remains separate from this
transport proof.
