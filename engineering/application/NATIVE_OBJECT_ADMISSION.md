# Staged native object admission, immutable reads and fill history

N25B implements the persisted records frozen by
[N25A](NATIVE_OBJECT_IDENTITY.md). N25C1 adds a staged common-envelope read API
over those immutable snapshots. Neither activates object admission in the
running opacity application. N25D2 adds the typed staged fill/history owner below.
The actual host has staged read/fill/history dispatch. N25D5 adds the bounded
raw application client below; public capability/MCP binding and P03 object
consumers remain separate pending gates.

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
a fresh process-local document incarnation and snapshot ID, and a selected revision.
Constructing another snapshot establishes a new incarnation; cloning a snapshot
retains its original immutable data and identity. The N25B constructor starts at
revision zero; the separately staged N25D2 owner below publishes later revisions.

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
contentRevision, atRevision, documentSnapshotId and the full selected
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
Success returns the complete selected record, exact scoped target, selected revision
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
availability, byte policy and the real common-host round trip. N25D2's typed native
fill/revision/history is documented below. No browser, installed, visual projection
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
the complete-result 4096-byte policy remain separate. Active fill/history, C04a
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
JSON parsing. N25A/N25B strict raw-byte admission remains unchanged. N25C2c's
separate raw-envelope gate below retains that evidence before Value parsing;
the real object host gate still precedes object availability.
Neither catalog registration nor private response tests
install an object owner or establish a real common UI/MCP object round trip.

The existing 4096-byte reader preserves the complete envelope or rejects it.
Record schema admission remains separate from transport fit; no geometry, handle,
identity or fill is truncated to make an overlarge response fit. The existing
N25C1 source dispatcher retains its bounded typed-unavailable failure policy.
Active history, P03 C04a, user save/load, browser, render/export and installed desktop
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
is unchanged. N25C2c raw-envelope admission below, real one-slot object host and
common UI/MCP round trip still precede capability availability. `native.object`
remains unavailable/pending, and active opacity continues denying object commands.
No C04a, browser/installed, user save/load/history, animation or render/export
acceptance follows from this pure preparation API.

## N25D2 staged native fill revisions and history

`history::NativeObjectHistory::new(instance_id, ObjectDocument)` validates through
the existing snapshot admission and creates a fresh incarnation at revision zero.
The owner is non-Clone and exposes no snapshot-adoption, replacement, arbitrary
document commit, transaction or requestId retry-cache port. Its private command
owner is the only bridge to private revision storage; history does not import the
read-query layer and revision storage depends only on object document data.
`object_snapshot::ObjectSnapshot` keeps its existing public API through a re-export
of the immutable revision representation.

`prepare_fill_json(bytes)` uses the unchanged strict D1 payload parser at the
current snapshot. `commit_fill(PreparedObjectFill)` consumes the candidate only
after checking instance, document incarnation, snapshot identity and current
content revision, including for a semantic no-op. Foreign instance/document or
stale origins fail without changing content, retained snapshots or either stack.
A changed fill publishes exactly one new immutable revision and one undo entry,
then clears redo. Reusing a changed candidate is stale and has no second effect.
Numeric-equivalent fill preserves exact document bytes, revision, undo and redo.
The return value is the selected content revision, unchanged for a no-op.

`undo(expected_revision)` and `redo(expected_revision)` require the actual current
revision and restore the complete retained before/after document as a new
monotonic revision. They restore content successfully before moving any history
entry. Stale expectations return `StaleRevision`; empty stacks and safe-integer
revision exhaustion return `Unavailable`, preserving all state. Revisions never
exceed 9,007,199,254,740,991. `history_depths()` exposes stack counts and
`acquire_snapshot(revision)` returns immutable pinned reads for retained revisions.
Success and failure read envelopes report the selected snapshot revision; a pinned
snapshot accepts only its own `atRevision`. Old pinned documents, geometry, opaque
scoped IDs, layers and siblings remain unchanged through edits and history moves.

This is typed staged library ownership only. It does not admit a raw transport
envelope, install a host owner, activate `native.object`, add UI/MCP round-trip or
C04a acceptance, or establish browser/installed, user save/load, animation or
render/export behavior. The common-envelope 4096-byte policy and active opacity
denial remain unchanged. N25C2c adds the raw ingress below; host/runtime gates
remain separately admitted.

## N25C2c complete raw-envelope admission

The shared `wire` bounded TCP reader checks complete raw JSON before decoding an
authenticated request, native response or host-status DTO. One private recursive
visitor retains decoded keys within each map, including maps nested in arrays.
Duplicate keys, including escaped-equivalent spellings, fail before a typed DTO
or Value can lose them. Keys in separate maps and JSON-like text inside strings
remain valid. The visitor parses all values and the complete end of input before
classifying a duplicate; malformed syntax, excessive nesting, trailing JSON and
numeric overflow retain their parser failure. No document or transport payload
is reconstructed from the visitor. TCP framing limits, required newline and BOM
rejection remain unchanged.

The bundled MCP executable wraps SDK stdin in a stateful `AsyncRead` guard before
the SDK parses JSON-RPC params and tool arguments into Value. Valid input reaches
the SDK byte-for-byte, including CRLF, leading BOM, numeric spelling, blank lines
and a valid nonempty final EOF fragment. Only the check view strips a leading BOM.
The guard retains partial input, pending output and oversize drain state across
polls and cancelled reads. It buffers at most one input frame plus its underlying
8 KiB reader and reads/drains at most 64 KiB of input per poll. Complete-frame
validation is bounded by the outer byte cap. It drains a rejected frame through
newline or EOF before recovering the next message.

STDIO now admits at most 1,048,574 content bytes: content plus LF must be strictly
smaller than the existing 1 MiB v1 framing budget. Original CR and BOM bytes count;
a final EOF fragment uses the same content limit. The SDK previously had no stdin
length cap. This outer budget is distinct from native's existing 4096-byte
encoded-request and complete-response policy; it does not impose a 4096-byte raw
tool-arguments limit. Neither limit, old coverage floor nor exclusion is relaxed.

Each duplicate or oversized frame becomes one fixed protocol-invalid JSON scalar
at the SDK boundary. The SDK's existing invalid-request path returns `-32600`
with **id omitted** (decoded `None`); it does not echo a tainted correlation ID,
payload or secret. This follows rmcp 3.2's serialization of uncorrelatable errors;
there is no custom stdout encoder or SDK change. Within-budget malformed syntax
is forwarded unchanged for the SDK's existing syntax handling. Rejections precede
tool dispatch, registry selection and endpoint connection; a subsequent valid
legacy request still reaches its registered endpoint.

The compiled raw-stdio controls and shared TCP tests establish transport admission
only. `native.object` remains unavailable/pending and its writes unregistered.
Real object one-slot host installation, terminal cleanup/re-entry and the common
UI/MCP round trip remain separate gates before capability availability, C04a or
installed desktop/user workflow acceptance.

## N25C2d typed host installation and terminal lifecycle

`ApplicationMcp::install_native_object(ObjectDocument)` is a production typed
installation port. It constructs and semantically revalidates a fresh non-Clone
`NativeObjectHistory` before requesting the existing installation reservation.
The host supplies its own UUID instance identity; the engine supplies the fresh
document identity. Both actual envelope identities are checked before install
commit. Layer and stroke identifiers retain their separate opaque-ID contract.
Invalid data does not reserve a slot, invalidate revision subscriptions or evict
an existing owner.

The same `NativeAuthority` holds exactly one boxed owner. Active opacity and object
owners cannot coexist; duplicate or stale installations leave the active owner
unchanged. The existing reservation type moves to the dispatch seam while retaining
its `ApplicationMcp` type-path re-export, generation accessor, lock-poison handling,
rollback and Drop semantics. There is no second application state or cloned owner.

This object owner creates no transactions, export jobs, preview/viewport work,
GPU resources or filesystem artifacts. Its terminal cleanup captures its real
identity, revision and history depths, with explicitly complete empty resource
stages. Cleanup progress is retained and ordinary dispatch remains fenced after
release. The existing C2a terminal path drops the actual owner, retains the full
receipt and permits exclusive re-entry only after successful cleanup. Replay of an
old successful release does not touch a newer owner; stale callbacks, changed retry
bodies and failed cleanup retain their existing rejection/tombstone behavior.
Old immutable library snapshots remain readable after the host owner is dropped.

Ordinary object dispatch returns a fixed nonmutating `Err` through the accepted
fallible seam. Replacement and export are explicitly unsupported; there is no
typed object read/mutation host port, envelope fabrication, Tauri invocation or
automatic bootstrap. The real-opacity preservation control wraps the production
opacity core in a test adapter; it is not DesktopNativeApplication or installed
desktop evidence. The original missing-installer E0599 characterization remains
a zero-executed compile baseline, not a runtime failure or pass.

This slice establishes typed host installation/lifecycle only. `native.object`
remains unavailable/pending and writes remain unregistered. Common UI/MCP object
round trip, capability availability, C04a selection, installed desktop and user
save/load/history/animation/render/export remain separate gates. The existing raw
ingress and complete 4096-byte read policies are unchanged. Whole-MCP coverage
retains its inherited red disposition and reviewed floors; source-tree identity
does not constitute a fresh coverage measurement.

## N25C2e strict raw desktop bootstrap

The registered main-window Tauri command `nemo_native_object_bootstrap` accepts
`requestJson`, a string containing the complete original JSON wrapper. The strict
wrapper requires `apiVersion: 2`, bounded `requestId`, the current host `instanceId`
and `documentJson`, itself a string of original object-document JSON. Optional
`cancelledBeforeDispatch` is a boolean. The complete UTF-8 wrapper, including
escaped document bytes, is bounded to 1,048,576 bytes before parsing. This is a
bootstrap input limit; the complete 4096-byte object read policy is unchanged.

Both JSON layers enter strict object admission directly, without a `Value`
intermediate. Duplicate decoded keys, including escaped duplicates, unknown or
missing members, invalid types, arrays, trailing input and unsupported/invalid
document content are refused. Version/instance/correlation and cancellation
checks precede installation. The existing codec semantically admits the entire
document before the existing typed installer constructs its fresh owner and
reserves the one application slot. No caller-supplied document identity exists.

The installer returns its actual generated identity/revision captured before
moving the owner into the slot, only after successful commit. The correlated
receipt records that commit: API/request/instance/document identity, revision
zero, `resourceCount: 0` and `viewportAvailable: false`. It is not a subsequent
mutable-status read, UI publication, presentation receipt or capability claim.
An occupied slot refuses duplicate attempts without evicting either an opacity
or object owner or invalidating its revision subscriber. There is no new retry
registry; ambiguous transport outcomes must be reconciled before another attempt.

Independent controls exercise original raw bytes, exact/over-limit UTF-8 input,
strict wrappers/documents, real owner/subscriber preservation and actual C2a
release/re-entry with retained terminal receipt identity. The main-window guard
and handler registration have source controls; those alone do not establish an
installed Tauri invocation. The initial characterization is a missing production
source/registration RED, separately retained from the older zero-executed E0599.

Only `object_codec::decode_project` is added to the existing desktop command
adapter's engine ports, with one dedicated test-only edge from host-release tests
to that adapter. Ordinary Rust 500-line budgets, coverage floors, frozen baselines
and exclusions remain unchanged. No GPU, filesystem, viewport or writable JS
mirror is created. Existing opacity bootstrap and common dispatch remain intact.

This source bootstrap can install the real typed owner but does not activate
`native.object`: its descriptor remains unavailable/pending. Common object reads,
fill/history routing, UI import/selection, native rendering, common UI/MCP round
trip, C04a and installed desktop save/load/history/animation/render/export and
full baseline acceptance remain separate gates. No automatic startup caller is
added. Browser/WASM compatibility remains the later R24 scope.

## Consumer and acceptance boundaries

| Consumer | Staged source evidence or remaining gate |
|---|---|
| Persistence | Native codec round trip and temporary disk reopen preserve records. Active user save/load remains unavailable for this new family. |
| Read/selection | Immutable exact-target raw and staged common-envelope library reads. Active selection and host/MCP binding require separate acceptance. |
| Edit/history | Staged typed native fill commit and monotonic undo/redo with immutable pinned reads. Active host/UI history remains unavailable pending separate admission. |
| Animation | Frame-scoped records preserve authored/reference distinctions; no interpolation or cross-frame identity is inferred. |
| Render/export | Unavailable for this family. Finite coordinates and 2–256 segments establish schema admission, not GPU-safe geometry, nondegeneracy or render/export parity. |
| Native bridges/browser/Tauri | Staged typed MCP validation and explicitly unavailable object discovery; no capability activation. Active opacity codec and common Rust dispatcher continue rejecting object documents/operations; N25A contract checks also preserve JS rejection. No installed/browser acceptance credit. |

Register these modules in the existing Rust boundary profile without new edge
allowances, policy changes or size exemptions. N25B acceptance requires focused
codec/read regressions, the existing N25A oracle, full local checks at a clean
exact candidate, independent exact-SHA review and ordinary protected integration.
Source/library acceptance does not close N21 or make any P03 consumer Ready.

## N25C2f actual-host immutable read prerequisite

The actual `NativeObjectHost` installed by C2e now acquires its owner's current
immutable snapshot and delegates a complete typed `NativeDispatch::dispatch`
request to the existing `ObjectSnapshot::dispatch`. The admitted host has only
fresh revision zero: `query.document.object` with `atRevision: 0` returns the
exact frame-scoped object in the correlated bounded envelope. Missing revisions,
targets, cancellation, invalid selectors or unsupported operations use the
snapshot's existing typed refusals; uncorrelatable input and a released owner
remain fallible errors. No new writable mirror, mutation/history routing,
retained-revision selection, retry registry, GPU or resource work is introduced.

This supersedes C2d's unconditional host dispatch refusal only at the private
host seam. `NativeApplicationRequest::validate` still rejects `native.object`
using its unavailable descriptor before common `ApplicationMcp`/RevisionSync
routing. Direct common and external host request tests require that refusal;
no input-validation bypass, fabricated response or descriptor activation is
added. Successful private-host reads are not common UI/MCP or installed credit.

C2f controls bootstrap actual original fixture bytes into the single Rust owner,
query the active authority, compare full independent object/identity/revision
results, preserve content/history/subscriber/lifecycle and exercise complete
4096-byte request/result limits and real terminal release/re-entry. The exact
`object_snapshot::ObjectSnapshot` production port belongs only to the existing
host seam; independent controls stay in the dedicated host-release tests.
Ordinary Rust/Node budgets, baseline, floors and exclusions remain unchanged.

The preflight source/registration RED is separately retained with original
source/fixture hashes. Actual Rust RED was unavailable while the shared heavy
slot was held; this is not compile/runtime characterization. Focused nonzero
Rust validation is a later gate. `native.object` remains unavailable; common
UI/MCP routing, installed Tauri invocation, mutations/history, viewport, C04a
and full agreed desktop baseline acceptance remain separate pending outcomes.

## N25C2g registered-input validation prerequisite

`NativeApplicationRequest::validate_input()` checks the existing typed shape,
API/identity/revision/selector/payload constraints, complete 4096-byte UTF-8
request bound and registration of the operation. Its bounded private contract
child implements this public request-type method; success does not authorize
execution, admit a document or prove an owner. Shape-valid unknown object
targets remain the actual owner's concern. Raw duplicate checks must still run
before typed/Value parsing; this method cannot recover discarded raw members.

The existing `validate()` composes those input checks with capability
availability. Desktop RevisionSync, the bundled MCP tool handler and TCP client
all continue to call full `validate()` at their existing positions. Registered
object input therefore passes only input validation; common object calls still
fail with the same unavailable descriptor reason before host dispatch or TCP
connection. The existing JS adapter still does not admit object operations.
No descriptor/catalog/schema, owner, transport policy or common route changes.

The helper and private tests are registered in both the general Rust source
census and actual MCP size overlay under their existing transport owner. No
native-engine dependency, graph edge or size ceiling is added. Existing file
and aggregate coverage floors, exclusions and historical snapshot stay intact;
only the two new files receive pinned measured per-file registrations. The
production child is measured at 100% lines/regions/functions; the test child
has no exported executable regions and is explicitly listed, not excluded.
Whole-crate coverage retains its inherited 18 ratchet failures. These new-file
registrations are not whole-crate coverage acceptance. Independent fixed selectors, all registered
examples, closed negative inputs and complete UTF-8 byte boundaries preserve
existing full-validator behavior and refusal precedence. The source/registration
RED and original hashes remain separate from unavailable Rust characterization.
Common UI/MCP/client availability, installed Tauri, mutations/history/viewport,
C04a and full desktop acceptance remain pending.

## N25D3 actual-owner scoped fill command and compact retry

The actual non-Clone `NativeObjectHistory` offers a distinct staged
`command.document.object.fill.set` typed-envelope port. The actual installed
`NativeObjectHost` invokes it under its existing single-authority lock before
immutable query fallback, with the released-owner fence first. No public caller
or catalog is activated: the operation stays unregistered, full common
validation rejects it, and it never borrows the available opacity apply route.

After bounded API/identity/complete4096-byte preflight against the real owner,
existing compact StageReceipts retain request fingerprints and exact dispositions.
Identical admitted requests replay the original selected revision/result before
checking current revision; changed bodies cannot replace that receipt. Fresh
cancel/stale/invalid payload/absent target failures preserve content/history;
changed fill delegates D1 preparation and D2 atomic commit, reports one revision
and one added history entry; no-op keeps exact bytes, revision and redo. Later
D2 undo/redo or other commands do not turn a retained retry into another effect.
This typed entry cannot detect duplicate raw members already collapsed into
Value; the original-byte transport gate remains required before typed admission.

Independent actual-owner controls compare the fixed before/after record and
complete sibling/geometry/layer/ID preservation, pinned reads, stable retries,
changed bodies, no-op/redo, cancellation/stale/identity/bounds and terminal
release/new-incarnation refusal. A real pre-production Rust test compiled and
executed RED against the actual raw-installed owner before the dispatcher edit.
Source, compile, host and installed-client evidence remain separate.

Only existing history/command/receipt/revision ownership is used; no second
writable document, new production graph edge, Cargo/MCP/schema/public descriptor
change, frozen floor/baseline/exclusion, GPU/resource or viewport is introduced.
General object common/UI/MCP activation, serialization/replacement/history RPC,
selection/render/export/C04a and full desktop baseline acceptance remain pending.
## N25D4 staged actual-owner history dispatch

`NativeObjectHistory::try_dispatch_object_command` handles the distinct staged
`command.document.object.undo` and `command.document.object.redo` operations as
well as the accepted fill operation. The existing fill-only method remains a
compatibility wrapper. The actual host calls the shared dispatcher after its
release fence, retaining one authority and one request-receipt store.

History payloads are closed objects containing only `command: "object.undo"` or
`command: "object.redo"`, matching their operation. The existing bounded envelope,
identity, safe expected-revision and cancellation rules precede effects. Admitted
identical retries replay their retained revision/result before current-revision
checks; changed bodies, including another operation under the same request ID,
cannot overwrite the original receipt. Empty-stack failures are retained too.
Typed input still requires the unchanged original-byte ingress; it cannot recover
duplicate members already discarded by a Value parser.

A fresh successful move uses D2's existing atomic history authority to restore the
complete retained document in a new monotonic revision and move one stack entry.
Its compact result is `applied: true, historyEntriesAdded: 0`. No-op fill still
preserves bytes/revision/redo, and a fresh changed fill invalidates redo. Engine
and actual raw-installed host tests check full content, old immutable pins,
codec roundtrips, current query readback, retained retries, refused moves,
release/new-incarnation fencing and public denial.

This is a source producer seam only. Both history operations remain unregistered,
the object descriptor remains unavailable, and available opacity operation names
cannot route to this owner. Public UI/MCP, history RPC, save/reopen, rendering,
export, installed desktop and M1 acceptance remain separate open gates. P37's
History UI and N20K's viewport/publication paths are unchanged.

## N25D5 bounded common application client

The registered Tauri command `nemo_native_object_dispatch` takes `requestJson`
(an original UTF-8 JSON string), not a pre-parsed request. It first enforces the
4096-byte **original wrapper** bound and recursively rejects duplicate decoded
keys before any typed or `Value` deserialization. Escaped spellings of the same
key are duplicates. Trailing JSON/text, unknown envelope fields, positional
arrays, invalid types, unsafe revision numbers, and unbounded correlation IDs
fail before owner dispatch. The reusable `wire::decode_json_bounded` utility
also serves the existing wire parser; it grants no operation availability.

Only these operation names enter this bounded route:

- `query.document.object`: the existing closed selector, `atRevision`, and full
  scoped stable target; `expectedRevision` is forbidden.
- `command.document.object.fill.set`: the existing closed `object.fill.set`
  payload containing `stableTarget` and `fill`.
- `command.document.object.undo`: exactly `{"command":"object.undo"}`.
- `command.document.object.redo`: exactly `{"command":"object.redo"}`.

Original-byte admission constructs a private admitted-request token. It enters
`ApplicationMcp` and the **same** `RevisionSync` dispatch core used by the public
typed and MCP paths. That core locks the same `NativeAuthority`, checks the actual
active generation and the owner's explicit bounded-client opt-in, then invokes
the existing dispatcher. Every other owner defaults to denial. Vacant,
installing, replacing, releasing and released owners are unavailable. There is
no direct-host client bypass, additional document authority, writable mirror or
legacy writer fallback. The private core resides in `native_revision_dispatch.rs`
to keep transport/control and dispatch responsibilities below normal size caps.

Command payload and revision/cancellation failures are classified and retained
by the existing actual owner. A command requires current `expectedRevision`;
exact admitted-body retry returns its retained original revision/result before
checking a later current revision. Changed-body or cross-operation reuse of that
request ID cannot overwrite the original receipt or repeat an effect. Missing or
stale expectation, cancellation, malformed payload, empty history and no-op
behavior retain their D3/D4 semantics. Wrong-instance/document checks precede
receipt lookup. Queries retain the host's **current-only** pinned-read policy;
this route does not expose older retained snapshots. The shared pending-revision
barrier blocks re-execution, including after disconnect/indeterminate consumer
synchronization. No new external MCP/TCP transport is activated.

`nemo_native_object_client_status` is a main-window, point-in-time observation of
that same locked authority. It reports `available`, current document/revision and
lifecycle generation, and the four operations only when the active object owner
supports this route. `publicCapabilityAvailable`, `mcpAvailable`, `saveAvailable`,
`viewportAvailable` and `exportAvailable` remain false. The existing general
native status/dispatch command names and signatures are preserved. Their thin
Tauri wrappers share the client module and remain re-exported from ApplicationMcp.

The aggregate `native.object` descriptor stays unavailable, its catalog retains
only the staged read declaration, and every existing public full validator is
unchanged. Public typed dispatch and MCP/TCP continue to deny those unavailable
or unregistered object operations. This bounded source route is not evidence of
installed UI/client operation, Save/reopen, persisted history, viewport or export.
The next dependency-ordered slice must admit the general-object serialization and
persistence/history consumer contract, then connect real UI consumers. Installed
common-client agreement and the M1 vertical demonstration remain separate gates.


## N25E1 private current-owner serialization and persistence consumer contract

The existing raw `nemo_native_object_dispatch` entry also admits
`query.document.object.serialize` for the active object owner. The complete
original request, including whitespace and escaped UTF-8, must fit 4096 bytes;
raw duplicate members are rejected before typed/Value conversion. It uses the
same ApplicationMcp, RevisionSync and NativeAuthority as object read/fill/history.
The unchanged host acquires its current immutable snapshot; the snapshot's
existing dispatcher performs correlation, cancellation and serialization.
There is no separate writable document or response-construction port.

The exact query payload is `{"atRevision": 0}`, using the actual current safe
integer revision. `expectedRevision`, missing or extra selector fields, negative,
fractional and unsafe revisions are invalid. The serializer follows the existing
object read identity/cancellation dispositions. An earlier or future revision
returns `not_found`; a stale document incarnation returns `wrong_document`.
A vacant, reserved, releasing, released or different-kind owner cannot dispatch.
Neither successful nor rejected serialization advances content or changes history.

Success correlates apiVersion/requestId/instanceId/documentId/contentRevision and
has exactly these result fields:

```json
{
  "atRevision": 0,
  "documentSnapshotId": "native-object:<actual-document-id>:0",
  "documentJson": "<exact UTF-8 emitted by object_codec::encode_project>"
}
```

`documentJson` is the complete admitted native object document, not a reconstruction
from selected-object queries. The existing codec revalidates it before encoding.
The COMPLETE encoded success envelope, including identity and escaped document
string, must fit 4096 bytes. Larger results return bounded `unavailable` without
partial JSON, truncated records or simplified geometry. Codec admission supports
larger records/documents than this bounded private delivery route can return.
`ObjectSnapshot::query_json` remains the separate strict selected-object byte
reader; the new serialization operation belongs to common-envelope dispatch.

Private client status declares exactly five operations when an actual object
owner is active: `query.document.object`, `command.document.object.fill.set`,
`command.document.object.undo`, `command.document.object.redo`, and
`query.document.object.serialize`. Otherwise its operations list is empty.
Public capability, bundled MCP, production Save, viewport and export flags stay
false. The public typed/discovery paths still reject this unregistered operation;
the opacity `query.document.serialize` schema and implementation are unchanged.

The consumer oracle serializes full revision0 content, applies actual fill at
revision1, undoes at revision2 and redoes at revision3. Undo restores the original
codec bytes; redo restores the edited bytes, including every untouched scoped
record, layer, opaque target ID, geometry/handle and numeric value. Repeated
queries do not add undo entries or consume redo entries.

For this source-level persistence contract, the consumer writes EXACT returned
codec UTF-8 to a disposable file and reads the identical bytes. It releases the
actual owner to vacant and feeds those bytes to the existing strict raw bootstrap.
Reopen preserves all persisted scoped object identities and full document content.
The runtime instance stays the same for an in-process reopen, but the document
incarnation and snapshot IDs are fresh, contentRevision is zero, and undo/redo
stacks are empty. Runtime revisions, command receipts and history stacks are not
part of this object codec's persisted format. Earlier runtime document IDs cannot
be used against the reopened owner.

This is native producer and filesystem-consumer contract evidence. It does not
publish a user Save result, install a UI Save/Reopen path, restore cross-reopen
history, activate public MCP, or accept installed general-document/M1 controls.
Those consumers and any broader history-persistence policy require their separately
owned leaves; N25D5/#1648 and M1/#1327 remain open.

## N25E2 private saved-file bootstrap consumer

The existing registered main-window `nemo_native_object_bootstrap` command also
accepts a strict file-source wrapper:

```json
{
  "apiVersion": 2,
  "requestId": "object-file-open",
  "instanceId": "<actual-instance-id>",
  "sourcePath": "/selected/project.json"
}
```

`cancelledBeforeDispatch` is the only optional field. File mode forbids
`documentJson`, `documentId`, `expectedRevision` and all other fields. Its complete
original UTF-8 wrapper, including whitespace, must fit 4096 bytes. Direct strict
DTO parsing rejects decoded duplicate members; routing uses neither a Value nor
an untagged intermediate. Invalid file wrappers cannot become permissive inline
input: the fallback is the unchanged strict original `bootstrap_raw` parser,
which rejects file fields. Existing inline `documentJson` inputs retain their
original complete one-MiB budget and require no file permission or read.

The main-window guard runs before either mode. File mode validates API, actual
instance, bounded request identity, absolute path and cancellation before resolving
or opening a file. It canonicalizes once into a resolved path, passes that path to
the existing installed filesystem configuration scope, and opens the same resolved
path. `try_fs_scope` returning no scope denies file input; `Scope::is_allowed`
is an existing configuration check, not a new permission. Scope may internally
resolve its argument; the consumer never reopens the unresolved input alias.
A symlink located in an allowed directory does not authorize a resolved target
outside that scope. No filesystem capability or allowlist is expanded.

Resolved-path metadata must identify a regular file BEFORE the opener is called,
so an already nonregular FIFO is refused without waiting for a writer. The opened
descriptor must also remain a regular file. The metadata check and pathname open
are separate operations: replacement between them remains a race, and this does
not claim TOCTOU freedom or bounded behavior for adversarial replacement. The
reader consumes at most one MiB plus
one refusal byte and rejects over-limit, unreadable, non-UTF-8, corrupt or
unsupported input without truncation or partial admission. The exact UTF-8 read
from the file becomes the original byte bootstrap's `documentJson`; no selected
object reconstruction or geometry/numeric rewrite is performed. That factory
also enforces its original one-MiB COMPLETE escaped wrapper budget: a file within
the read limit can still fail the encoded-wrapper limit. All failures preserve
existing authority, content/history and revision subscription; the consumer never
closes or releases a live/releasing owner to make room.

Only successful semantic admission and actual single-owner installation produce
the original correlated bootstrap receipt. A caller must already have a vacant
authority. Saved full scoped records survive; runtime document identity is fresh,
revision is zero and undo/redo stacks are empty under the N25E1 persistence policy.
Tests use actual native fill/undo/redo/serialization bytes, real files and the same
private router called by the registered command, including nonempty-history and
subscriber preservation on refused reopening.

This native file consumer does not wire an ordinary picker, publish a tab/frame,
implement Save, mark a document clean, update recents/autosave, restore persistent
history or activate public object/MCP/viewport/export capabilities. The private
object client still declares its five accepted operations and all those public
availability flags remain false. UI/installed general-document/M1 and first-Open
presentation acceptance stay separately owned and open.
