# ADR-001: Native document and evaluation authority

Status: **accepted for N02/#1331; implementation remains prospective**
Date: 2026-09-20
Decision scope: the approved native-engine migration application boundary.

## Decision and version boundary

Nemo keeps its Tauri/JavaScript interface, but a Rust native engine becomes the
sole authority for persisted document revisions, command/history application,
immutable evaluation, scheduling, media/resource/GPU lifetime, viewport frame
production, and export jobs. UI, SDK/scripts, tests, and bundled Rust MCP call
the same native application dispatch; none receives a writable document mirror.

The prospective native application API is **`apiVersion: 2`**, a deliberate
breaking successor to existing opacity transport v1. The
`nemo.native-transition-oracle/1` schema and `native-transition-v1` directory
version the examples only; neither is the application API version. Capability
versions and stored project format versions remain independent.

| Existing v1 baseline | Native v2 successor |
| --- | --- |
| `revision` is an owner-managed JS counter that includes UI frame/context changes; current operation names are opacity/history/diagnostic operations. | `contentRevision` is monotonic committed content for one `documentId`; context/time/view changes do not advance it. V2 defines the operation families below. |
| Current JS retains writes in a 256-entry in-memory map. | V2 retains compact replay evidence for a document lifetime; it cannot silently inherit v1 eviction. |
| D02 v1 is historical/prospective lifecycle vocabulary. | D02 remains immutable and byte-for-byte unchanged; this ADR is a new v2 contract, not a D02 edit. |

## Identity and authority

`documentId`, `contentRevision`, and `contextId` are distinct:

| Name | Meaning | Changes when | Never substitute |
| --- | --- | --- | --- |
| `documentId` | Opaque authoritative open-document incarnation. | Document replacement, including another project load. | Path, tab, context, format version, or layer ID. |
| `contentRevision` | Monotonic committed persisted-content revision for one document. | Successful command, transaction commit, undo, or redo. | Frame/time, undo depth, viewport generation, or UI refresh. |
| `contextId` | Opaque scene/component/montage evaluation or editing context. | Caller selects a declared context; selection alone does not mutate. | Document identity, stable item identity, or stale-write permission. |

Existing `layerUid`, `strokeId`, and UID references remain opaque persisted
values. A characterized legacy adapter may assign missing IDs, but cannot
renumber existing IDs, use indices as identity, or infer cross-frame shape
identity from an ordinary `strokeId`. Only the native dispatcher applies
committed mutations/history; adapters invoke it and never synchronize a second
editable copy.

## Immutable document snapshots and evaluation

The owner first performs:

```text
documentSnapshotId = acquire(documentId, contentRevision)
```

That immutable ID names document content only. A separate evaluation key is:

```text
(documentSnapshotId, contextId, time/frame, quality, outputSpec,
 declaredResourceVersions)
```

A multi-frame job pins one `documentSnapshotId` and its complete job input
(range, context, quality, output specification, resource versions), then
evaluates every requested frame/time from that document snapshot. An edit may
advance revision 7 to 8 without changing a pinned revision-7 job. A revision
label paired with re-reads of mutable globals is not a snapshot.

Viewport results carry `documentSnapshotId` plus monotonically increasing
adapter `viewGeneration`; the presenter discards stale generations. Scheduler
deduplication requires identical complete evaluation keys. Snapshots/leases stay
pinned through GPU submission and output completion, and are evicted only after
all dependent job/frame/viewport leases are terminal or cancelled and released.

## V2 envelopes, transactions, and concurrency

Every v2 request carries `apiVersion`, `requestId`, `instanceId`, `documentId`,
`operation`, and `payload`.

Every requested operation returns this common v2 envelope:

```text
{
  apiVersion, requestId, instanceId, documentId, contentRevision, ok,
  result | error
}
```

`requestId` is echoed. `instanceId`, `documentId`, and `contentRevision` are
always the authoritative current owner identity at response time. Exactly one of
`result` (`ok: true`) or `error` (`ok: false`) is present. `error` is always
`{code, message, details?}`, never a bare string. The closed dispatch codes are
`invalid_request`, `wrong_instance`, `wrong_document`, `stale_revision`,
`busy_conflict`, `unavailable`, `not_found`, `cancelled_before_dispatch`, and
`internal`. A capability declares any additional closed domain codes it uses.
When a request names a replaced document, the envelope identifies the current
document/revision and `error.details.requestedDocumentId` records the stale
target for reconciliation.

Stage result shapes under that envelope are frozen:

- Revision query `result` identifies selected `contentRevision` or
  `documentSnapshotId` and the queried value.
- Command `result` includes `applied` and `historyEntriesAdded`; the envelope's
  `contentRevision` is authoritative after the operation.
- Transaction begin/update/commit/cancel/status `result` includes
  `transactionId`, `baseRevision`, working generation/state, and committed
  revision/terminal disposition where applicable.
- Job begin/status/cancel `result` is exactly one `JobReceipt` below.

`JobReceipt` is:

```text
{
  jobId,
  status: running | succeeded | failed | cancelled,
  pinnedRevision,
  documentSnapshotId,
  progress,
  artifact: object | null,
  cleanup: { status: not_required | pending | complete | failed, error? },
  externalEffectDisposition: none | contained | committed | indeterminate,
  error?: { code, message, details? }
}
```

Repeated terminal `job.status` and `job.cancel` return the exact retained
`JobReceipt`; they do not turn `cleanup.complete` into another label. Dispatch
receipt retrieval can have `ok: true` even when the contained JobReceipt has
`status: failed` or `cancelled`: the terminal job error belongs in the receipt.

- A revision-selected query identifies `atRevision` or `documentSnapshotId`,
  returns identity/revision, and never mutates. Normal queries observe committed
  state unless explicitly scoped to an authorized transaction.
- A command carries `expectedRevision`, validates stable targets, and either has
  no effect or creates exactly one next `contentRevision` and history entry.
- `transaction.begin` carries `expectedRevision` and stable targets, returning
  `transactionId` and `baseRevision`. `transaction.update` carries that ID and
  alters only engine-owned working state/generation. Conflicting document
  commands/transactions while open fail explicitly as `busy_conflict`; they are
  not queued or silently merged. `transaction.commit` requires base/current
  match and creates one revision/history entry. `transaction.cancel` discards
  working state with no history.
- `job.begin` carries `expectedRevision` and returns a job/snapshot/input
  receipt. `job.status` and `job.cancel` bind `jobId` and `documentId` but do
  not require `expectedRevision`, so a pinned job can finish/cancel after an
  unrelated revision advances.

Identical stage retries return their retained receipt; changed-body request-ID
reuse is rejected. A disconnect/client cancellation does not prove rollback:
reconnecting callers query the native owner before retrying. Replacement
cancels/reconciles transactions/jobs before new identity admission; late old
document work is `wrong_document`.

## Retry lifetime and terminal receipts

For one `documentId` lifetime, v2 retains a compact canonical request fingerprint
and disposition sufficient to reconstruct the original receipt. It may be
storage-backed/compacted, but must not promise unbounded in-memory response
objects. An identical retry is never re-executed, even after later revisions;
changed-body reuse fails. Replacement ends retention and the old identity is
`wrong_document`. This is intentionally stricter than the current v1 256-entry
map, which is observed baseline but noncompliant with v2.

The only terminal statuses are `succeeded`, `failed`, and `cancelled`.
`indeterminate` is never a fourth terminal; it is only
`externalEffectDisposition` on a failed/cancelled receipt when external effects
cannot be reconciled. Terminal `status` and `cancel` are idempotent receipt
retrieval and never rerun work/cleanup.

This is a deliberate v2 divergence from D02 v1 prose, which says a terminal job
accepts no further update/commit/cancel. D02 is unchanged. Current JS export
has unresolved late-cancel drift: it does not yet supply a v2 pinned snapshot,
replacement reconciliation, or terminal receipt/cleanup contract.

## Resource leases

Handles are opaque leases owned by a request, transaction, or job; a handle ID
is never reused after release. The owner pins them through GPU/output completion
and releases each exactly once on success, failure, cancellation, or replacement.
Descriptors declare kind, format, dimensions, color interpretation, and
straight-versus-premultiplied alpha. This adds no color-management pipeline or
new current-project color semantics. Cleanup failure yields `failed` with
`error.code: cleanup_failed`, `artifact: null`, observable cleanup information,
and (only if needed) `externalEffectDisposition: indeterminate`.

## UI, MCP, Paper, and platforms

JavaScript UI and Rust MCP are transports to the same v2 dispatch; MCP has no
second document store, feature implementation, or validation/history bypass.
Paper.js remains a temporary interface-edge editor/selection/hit-test/overlay
adapter. It may keep ephemeral overlays and send intent, but cannot own persisted
content, export evaluation, or native resource caches after its slice migrates.
Browser/WASM support is separately declared: unavailable native-only
media/filesystem/encoder/GPU/MCP capability is reported, never silently routed
to another authority. Browser and installed-Tauri acceptance are separate gates.

## Migration gates and baseline

This candidate is not evidence that the current owner changed. Each vertical
slice needs predecessor SHA, capability/availability declaration, exact
contracts, stable-ID parity fixture, and consumer evidence for save/load,
undo/redo, selection, animation, render, export, native bridge, browser, and
installed desktop where applicable. Retire a legacy writer only after those
checks pass at one candidate, old paths become non-writing/remove, and the issue
records SHA, checks, limits, and next gate. Compile/schema/revision labels alone
never retire a writer.

D02/#1045's operation schema remains historical/prospective; the value schema
records JS/Paper as current owner. Both stay byte-for-byte unchanged. Current
export still rereads mutable UI/global state, so live export versus pinned-
snapshot export remains an open implementation/acceptance gap. The two adjacent
JSON files are machine-readable specifications, not freshly executed receipts.
