# N25B: staged native object admission and immutable reads

N25B implements the persisted records frozen by
[N25A](NATIVE_OBJECT_IDENTITY.md). N25C1 adds a staged common-envelope read API
over those immutable snapshots. Neither activates object admission in the
running opacity application. N25C2 host/MCP binding and N25D fill/history remain
separate admission and acceptance gates. P03 object consumers remain pending.

## Persisted format and admission

`object_codec::decode_project(bytes)` accepts only this distinct document family:

```json
{
  "format": "nemo.native-object-document",
  "formatVersion": 1,
  "totalFrames": 21,
  "layers": [{"layerUid": "legacy layer:alpha"}],
  "objects": []
}
```

Each object is a complete N25A `closed-solid-fill-cubic-path` record, schema
version 1: target, cubic segments and solid fill. See the independently frozen
[positive and negative cases](examples/native-object-v1/cases.json). Layers are
identified declarations, not the active opacity layer representation. Empty
objects are supported; totalFrames must be positive and layers nonempty.

The decoder requires map/object input at every DTO layer, including read
envelopes and payloads; positional arrays cannot substitute for schema objects.
Scope kinds must be literal `authored`/`reference` strings; tagged enum objects
are rejected.
It parses directly into strict typed fields, rejecting duplicate raw JSON members
and unknown fields before admission. The whole document fails for
unsupported versions/families, duplicate layer IDs or complete scoped object
keys, unresolved layer references, empty IDs, unknown context, out-of-range
frames, malformed segment counts or nonfinite coordinates, and invalid fill
channels. No partial records are retained, IDs assigned or unsupported fields
silently dropped. The object key includes context, authored/reference kind,
frame, layerUid and strokeId; equal IDs in different frames or scope kinds do
not imply cross-frame identity.

`encode_project(document)` validates again before serialization. Opaque ID
strings, scope kinds/frames, segment order, relative handles and numeric values
survive round trips. JSON whitespace and numeric token spelling are not promised;
coordinates and channels preserve the finite JSON number value supported by
serde_json. Disk write/read tests establish codec persistence only. The library
does not publish a user Save result or replace an active document.

Public DTO deserialization alone is not semantic admission. Both the codec and
`ObjectSnapshot::new(instance_id, document)` validate all invariants before an
owned snapshot can be returned. Fields are private outside this crate; consumers
receive borrowed immutable data or detached read-result copies.

## Immutable pinned read

`ObjectSnapshot` owns an Arc-backed immutable document, a nonempty instance ID,
a fresh process-local document incarnation and snapshot ID, and revision zero.
Constructing another snapshot establishes a new incarnation; cloning a snapshot
retains its original immutable data and identity. This slice has no mutable head,
revision advance, edit, undo/redo, replacement publication or history authority.

`query_json(bytes)` accepts a strict version-2-shaped staged read:

```json
{
  "apiVersion": 2,
  "requestId": "object-read",
  "instanceId": "caller-instance",
  "documentId": "native-object-document-1",
  "operation": "query.document.object",
  "payload": {
    "atRevision": 0,
    "stableTarget": {
      "contextId": "scene-root",
      "frameScope": {"kind": "authored", "frame": 7},
      "layerUid": "legacy layer:alpha",
      "strokeId": "ink/path_A"
    }
  }
}
```

Use the actual constructor-returned document ID, never this illustrative value.
Success correlates apiVersion, requestId, instanceId, documentId,
contentRevision (zero), atRevision, documentSnapshotId and the full selected
record. The target must match every scoped component. Duplicate members, unknown
write fields/operations and invalid request shapes fail as `InvalidRequest`;
wrong instance/document fail as `WrongInstance`/`WrongDocument`; an unavailable
revision or scoped target fails as `NotFound`. Revisions above the JavaScript
safe-integer limit fail. Queries and failures do not alter stored content.

`query_json` errors remain typed library results, not transport failure envelopes.
This standalone byte entry does not impose transport byte/resource limits. Its
process-local identity must not be interpreted as durable identity across restarts.

## N25C1 staged common-envelope reads

`ObjectSnapshot::dispatch(&self, OpacityRequest)` returns
`Result<ResponseEnvelope, DispatchErrorCode>` using the existing native application
envelope types. It reads the same admitted immutable snapshot without a mutable
head, request cache, second authority, command/history entry or host installation.
Only `query.document.object` is supported; `expectedRevision` is forbidden.
Success returns the complete selected record, exact scoped target, revision zero
and snapshot ID in a correlated common envelope. Repeated reads and retained
snapshots survive construction of separate document incarnations unchanged.

Envelope requestId, instanceId, documentId and operation use the existing separate
transport identifier rule: 1..128 ASCII characters, first character alphanumeric,
then alphanumeric or `._:/-`. Object layerUid and strokeId remain opaque nonempty
strings, including spaces and numeric-looking strings. The standalone snapshot
constructor retains N25B's broader nonempty instance-ID boundary; a snapshot with
an instance ID unsuitable for an envelope cannot dispatch through this API.

Invalid/unrepresentable correlation IDs return `Err(InvalidRequest)` before an
envelope is constructed. They are not echoed, truncated or replaced with fabricated
IDs, and receive no correlated-envelope claim. With representable identities,
invalid version/operation/payload, unsupported writes, expectedRevision and requests
above 4096 encoded bytes return `Ok` containing an `invalid_request` failure
envelope. Wrong instance/document return their closed codes with the actual owner
identity; wrong-document details preserve the requested document ID. Cancellation
after envelope/identity preflight returns `cancelled_before_dispatch`. Missing
revision/fully scoped target returns `not_found`; malformed selectors return
`invalid_request`. Every such read or failure preserves snapshot data.

The entire serialized success envelope must fit 4096 bytes. A codec-admitted
record whose complete result cannot fit returns a bounded `unavailable` failure;
no geometry, identity, handles or fill is truncated or simplified. This deliberately
does not promise transport parity for every admitted 2..256-segment record.
The typed request entry cannot detect duplicate raw JSON members already lost
in its payload Value; callers needing that evidence retain `query_json(bytes)`
and its strict duplicate-member regression controls.

N25C1 does not register a capability, install an object owner, enable host/MCP/UI
dispatch, or change active opacity admission. N25C2 must separately prove one-slot
host installation/release, strict MCP request/result validation, capability
availability, byte policy and the real common-host round trip. N25D remains the
native fill/revision/history successor. No browser, installed, visual projection
or P03 C04a acceptance follows from this staged library API.

## N25C2a terminal host cleanup prerequisite

The existing single `NativeAuthority` slot releases its installed owner through
explicit `NativeDispatch::release_project` and `release_progress` ports. The
shared internal progress result retains transaction, export and preview evidence;
terminal cleanup no longer requires a `DesktopNativeApplication` downcast. The
desktop implementation delegates to its existing staged cleanup. The generic
core application and export-pump test owner explicitly reject unsupported host
cleanup; the trait has no successful default.

The real host release path retains the admitted instance/document/revision, catches
cleanup and progress panics, preserves completed stages, and marks unreconciled
pending stages unknown. Unsupported cleanup, changed cleanup identity, unresolved
work, incomplete stages and poisoned authority locks produce a failed terminal
tombstone that denies re-entry. A successful release permits one exclusive newer
installation generation; identical request replay retrieves the retained receipt
without repeating cleanup or touching a reentered owner.

N25C2a's non-desktop test owner proves this host lifecycle prerequisite only. It
does not install a real object application, activate an object capability, validate
object MCP requests/results, or enable UI/MCP object reads. Those N25C2 gates and
the complete-result 4096-byte policy remain separate. N25D fill/history, C04a
visible selection, browser, installed desktop and full parity remain unavailable
or pending as recorded below.

## N25C2b staged typed MCP contract

The bundled MCP catalog now registers only `query.document.object` under
`native.object`, with explicit `unavailable` availability and pending UI/MCP ports.
A well-shaped request returns typed `unavailable` before endpoint lookup or TCP
connection. Malformed typed selectors return `invalid_request`; unsupported
object writes remain unregistered. The existing opacity capability stays available.

The private transport validator checks closed map shapes, required safe-integer
atRevision, scene-root context, authored/reference u32 frame scope and nonempty
opaque layer/stroke IDs. Transport envelope IDs retain their separate syntax.
Results require the complete frozen record, exact scoped target and selected
revision, and `native-object:<documentId>:<atRevision>` snapshot token. The staged
immutable common-envelope result requires outer contentRevision equal atRevision;
this does not activate the prospective later-head/history contract. Closed cubic
geometry retains every finite point and relative handle, solid fill channels in
0..1, and 2..256 segments. These shape checks cannot establish document provenance,
resolved layer references, GPU safety or visible selection.

Typed Value validation cannot detect duplicate raw members already collapsed by
JSON parsing. N25A/N25B strict raw-byte admission remains unchanged. A separately
admitted strict raw-envelope/host gate must resolve that boundary before making
object reads available. Neither catalog registration nor private response tests
install an object owner or establish a real common UI/MCP object round trip.

The existing 4096-byte reader preserves the complete envelope or rejects it.
Record schema admission remains separate from transport fit; no geometry, handle,
identity or fill is truncated to make an overlarge response fit. The existing
N25C1 source dispatcher retains its bounded typed-unavailable failure policy.
N25D history, P03 C04a, user save/load, browser, render/export and installed desktop
acceptance remain separate gates.

## N25D1 detached solid-fill preparation

`commands::prepare_object_fill_json(snapshot, payload_bytes)` prepares a detached
candidate from an admitted immutable object snapshot. Its payload is exactly
`command: "object.fill.set"`, the full frame-scoped `stableTarget`, and a complete
solid `fill`. The byte entry parses directly through the existing strict map-only
DTOs; duplicate members, positional arrays, missing/unknown fields and malformed
numbers fail before a Value intermediary can discard evidence. It validates the
scene-root target, authored/reference frame scope and finite fill channels in 0..1.
Malformed payloads return `InvalidRequest`; a well-shaped absent scoped target
returns `NotFound`, without a partial candidate or changing the source snapshot.

The candidate exposes only immutable borrows of its complete codec-valid document
and target. Only the selected record's fill changes; geometry, layers, other scoped
objects, opaque IDs and numeric values remain intact. Numeric-equivalent channels
are a semantic no-op that keeps the original fill representation and document bytes.
Repeated preparation from the same snapshot/payload is deterministic. Encoding and
redecoding the candidate preserves the fixed N25A afterRecord/preservation oracle.

`base_instance_id`, `base_document_id`, `base_snapshot_id` and
`base_content_revision` identify the preparation origin. They do not assign the
changed document to that authoritative snapshot or establish a new revision. The
original snapshot's pinned reads still return the original record. N25D1 has no
owner commit, revision advance, history/retry cache, undo/redo, ID assignment or
host installation. A future D2 owner must separately check origin/current revision
and atomically commit or reject under its sole state authority.

This raw **payload** entry is not whole JSON-RPC/TCP request/result admission and
claims no transport envelope byte limit. Existing common-envelope 4096-byte policy
is unchanged. Strict raw-envelope MCP admission, real one-slot object host and
common UI/MCP round trip still precede capability availability. `native.object`
remains unavailable/pending, and active opacity continues denying object commands.
No C04a, browser/installed, user save/load/history, animation or render/export
acceptance follows from this pure preparation API.

## Consumer and acceptance boundaries

| Consumer | N25B evidence or remaining gate |
|---|---|
| Persistence | Native codec round trip and temporary disk reopen preserve records. Active user save/load remains unavailable for this new family. |
| Read/selection | Immutable exact-target raw and staged common-envelope library reads. Active selection and host/MCP binding require separate acceptance. |
| Edit/history | Unavailable; N25D must prove native fill commits, pinned old reads and undo/redo. |
| Animation | Frame-scoped records preserve authored/reference distinctions; no interpolation or cross-frame identity is inferred. |
| Render/export | Unavailable for this family. Finite coordinates and 2–256 segments establish schema admission, not GPU-safe geometry, nondegeneracy or render/export parity. |
| Native bridges/browser/Tauri | Staged typed MCP validation and explicitly unavailable object discovery; no capability activation. Active opacity codec and common Rust dispatcher continue rejecting object documents/operations; N25A contract checks also preserve JS rejection. No installed/browser acceptance credit. |

Register these modules in the existing Rust boundary profile without new edge
allowances, policy changes or size exemptions. N25B acceptance requires focused
codec/read regressions, the existing N25A oracle, full local checks at a clean
exact candidate, independent exact-SHA review and ordinary protected integration.
Source/library acceptance does not close N21 or make any P03 consumer Ready.
