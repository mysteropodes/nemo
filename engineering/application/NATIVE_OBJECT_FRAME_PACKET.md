# Internal native object frame packet — N25F1 / #1660

`object_frame_packet::prepare_object_frame` prepares immutable CPU scene inputs
from an already admitted `revision::ObjectSnapshot`. It belongs to the existing
native-evaluation module. Its filename is a narrowly admitted internal core API
for a future native render-scene consumer. It does not activate an application
endpoint, MCP route, ordinary desktop control, viewport or export capability.

## Input and refusal

`ObjectFrameSelector` requires all four exact actual snapshot pins:
`instance_id`, `document_id`, `content_revision`, `document_snapshot_id`.
The producer checks them in that order, returning respectively `WrongInstance`,
`WrongDocument`, `WrongRevision`, or `WrongSnapshot` on a mismatch. Even an empty
frame selection must pass every pin check. No current-owner lookup or replacement
occurs; callers acquire the snapshot through the existing authority contract.

Only the current object codec's exact `scene-root` context is accepted, consistent
with `ObjectTarget::valid`. Any other context, including an empty string, returns
`InvalidContext`. Explicit `FrameScopeKind::Authored` or `Reference` and a zero-based
frame in `[0, total_frames)` are mandatory. Out-of-range frames return
`FrameOutOfRange`; there is no clamping, interpolation, held-frame substitute,
cross-frame search, implicit reference overlay or fallback writer.

A valid `scene-root`, in-range frame with zero matching records returns an empty
packet. Unknown context always refuses, including when no records would match.
The same frame in different scopes has distinct selections. Existing snapshot
construction and codec admission remain responsible for semantic source validity;
the packet has no unchecked-document entry point. `InvalidSource` defensively
refuses a matching record whose layer cannot be resolved before any packet is
returned. There is no partial-success output.

## Output and immutability

`ObjectFramePacket` retains the immutable snapshot and its actual four pins,
context, scope and frame. `source_layers()` exposes the complete original layer
table, including layers without matching records. `records()` exposes only exact
matching scoped records through an exact-size iterator of borrowed views. Each
`ObjectFrameRecord` borrows the unchanged full `ObjectRecord` from the retained
snapshot and carries original `source_layer_index` and `source_object_index`.
Packet and record fields are private; accessors return immutable references or
scalar provenance values. No source owner or mutable document port is stored.

The packet stores only provenance index pairs alongside its immutable snapshot;
preparation and iteration never deep-copy records or geometry segments. Borrowed
views cannot outlive the packet. Records expose the admitted source, including their complete
scoped targets, schema/family, relative cubic anchors and handles, segment order
and straight fractional RGBA `serde_json::Number` values. No RGB8 quantization,
f32 cast, premultiplication, color conversion or geometry reconstruction occurs.
Opaque IDs remain byte-for-byte strings. Distinct full scoped targets with the
same stroke ID remain distinct, and multiple matching strokes in one layer are
retained. Strict codec admission still rejects duplicate complete targets.

Original array positions are **provenance only**. Source encounter order exposes
those positions without sorting IDs or regrouping records. Neither the positions
nor packet encounter order defines z-order, paint order or blending order. The
later compositor must admit overlap order, blending and whether reference records
are displayed or overlaid. This producer makes none of those decisions.

This is not a GPU `RenderScene`. Codec-admitted geometry remains unchanged here,
including cases a later renderer may refuse for precision or degeneracy. The
consumer owns its precision/clip/transform/color/composition admission. No opacity
document is fabricated and no work ID, view generation, presented-frame identity,
resource lease, job or export receipt is created.

## Revision and consumer checks

Retained packets remain unchanged through actual native fill/undo/redo revisions
and owner destruction. New snapshots supply new pins and current values; undo
restores original content under a new monotonic revision. Serialize/reopen admits
a fresh runtime document and snapshot identity with revision0 and empty history.
Old selectors cannot select that fresh incarnation. Packet records and layer
provenance remain immutable after detached serialization or caller-owned copies
are changed.

The fixed authored-frame7 and reference-frame12 geometry/color cases provide the
independent content oracle, including alpha0.75 and relative handles. Additional
controls cover same IDs across layers/scopes/frames, multiple objects in a layer,
valid empty frames versus other contexts, four independent wrong pins, adjacent
fractional channel values, codec-admitted degenerate paths, pointer identity of
borrowed records/segments in the original snapshot, and actual native
history/reopen retention. Tests are registered in both existing evaluation and
application harnesses. The history test requires the existing `history` feature;
normal application enables it. The focused evaluation invocation enables
`test-evaluation,history` so the history oracle executes in both focused targets.

The focused Node boundary file verifies census, the exact internal filename API,
both harness registrations, unchanged eighteen-module/forty-four-edge graph and
injected forbidden renderer/compositor/host dependencies. Existing module and
crate policy, Cargo/features, size ceilings, coverage floors, baselines and
exclusions remain unchanged. No renderer, host or application consumer is wired
in this leaf. All public native object/Save/MCP/viewport/export availability stays
unavailable, pending separately admitted consumers and installed evidence.

Source/local checks establish only this producer contract. They do not establish
GPU pixels, installed first Open, ordinary Save/reopen, public/shared MCP,
export, M1/#1327, full desktop parity or deferred browser compatibility.
Earlier RED receipts, inherited coverage debt and the N25E2 pathname/open
replacement race remain separate preserved limitations.
