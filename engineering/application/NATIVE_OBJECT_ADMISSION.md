# N25B: staged native object admission and immutable reads

This library slice implements the persisted records frozen by
[N25A](NATIVE_OBJECT_IDENTITY.md). It does not activate object admission in the
running opacity application. N25C transport/read binding and N25D fill/history
are separate admission and acceptance gates. P03 object consumers remain pending.

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

The decoder parses directly into strict typed structs, rejecting duplicate raw
JSON members and unknown fields before admission. The whole document fails for
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

Errors are typed library results, not transport failure envelopes. A future
admitted transport must provide correlated failures and its existing byte/resource
limits; this standalone API does not claim those transport protections. Its
process-local identity must not be interpreted as durable identity across restarts.

## Consumer and acceptance boundaries

| Consumer | N25B evidence or remaining gate |
|---|---|
| Persistence | Native codec round trip and temporary disk reopen preserve records. Active user save/load remains unavailable for this new family. |
| Read/selection | Immutable exact-target library reads pass. Active selection and common transport binding require N25C acceptance. |
| Edit/history | Unavailable; N25D must prove native fill commits, pinned old reads and undo/redo. |
| Animation | Frame-scoped records preserve authored/reference distinctions; no interpolation or cross-frame identity is inferred. |
| Render/export | Unavailable for this family. Finite coordinates and 2–256 segments establish schema admission, not GPU-safe geometry, nondegeneracy or render/export parity. |
| Native bridges/browser/Tauri | No capability activation. Active opacity codec and common Rust dispatcher continue rejecting object documents/operations; N25A contract checks also preserve JS rejection. No installed/browser acceptance credit. |

Register these modules in the existing Rust boundary profile without new edge
allowances, policy changes or size exemptions. N25B acceptance requires focused
codec/read regressions, the existing N25A oracle, full local checks at a clean
exact candidate, independent exact-SHA review and ordinary protected integration.
Source/library acceptance does not close N21 or make any P03 consumer Ready.
