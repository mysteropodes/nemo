# Native object CPU render-scene input

N25F2/#1663 consumes the accepted N25F1 immutable packet in the existing
native-render-scene module. It prepares real closed cubic contours for future
native consumers. It does not draw, schedule work, publish a view or enable a
public application/MCP capability. Existing opacity rendering remains separate.

`prepare_object_render_scene(packet, expected)` requires every exact packet pin:
instanceId, documentId, contentRevision, documentSnapshotId, scene-root, explicit
Authored/Reference and zero-based frame. Mismatches return typed errors before
geometry compilation, including on empty packets. A valid empty packet returns
an empty scene. Packet admission has already enforced the document frame range;
this consumer requires the same frame, rather than selecting another frame.

`ObjectRenderScene::packet()` borrows the retained immutable packet.
`entries()` returns exact-size borrowed `ObjectRenderEntry` values. Each contains
the original `ObjectFrameRecord` source view, a borrowed real `ClosedCubicPath`
and its identity-space envelope. Complete targets, original relative segments
and straight fractional RGBA Numbers remain in the original source record.
No copy, clamping, premultiplication, u8 paint conversion or ID rewriting occurs
to source data. Compiling f64 cubic coordinates is a derived renderer input;
it does not replace the original serialized Numbers.

The scene retains the Arc-backed object snapshot through its packet, and compiled
contours reside in immutable Arc-backed storage. Scene clones share the source
records and compiled contours; they copy only the packet's selection/provenance
indices and scalar metadata. Dropping the original packet or mutable history
owner cannot change or invalidate a retained scene. No new cache or GPU resource
owner is introduced.

## Admission and refusal

Compilation reads only the original record returned by the admitted evaluation
port. It does not import document/revision/history/host modules or look up a
mutable document. Its only native dependencies are the existing packet port and
same-group render_geometry. The source record's within-crate geometry fields
are read at this port boundary; no new document getter or authority is exposed.

For every record, convert relative anchors/handles into the existing
`CubicSegment`, call `ClosedCubicPath::new`, then call the existing
`transformed_envelope([1,0,0,1,0,0])`. No configurable affine, new precision rule
or opacity-layer wrapper is supplied. Control bounds/envelope are conservative
metadata, never a substituted rectangle for the contour.

Existing renderer admission includes 2..256 segments, finite absolute controls,
finite positive control bounds, positive-area control hull, local f32 curve
nondegeneracy and the conservative quarter-output-pixel precision gate.
Codec admission does not promise renderer admission. A codec-valid repeated,
collinear, overflowing, collapsed or over-budget contour returns
`UnsupportedGeometry { source_object_index, reason }`. The index is original
source provenance, not a draw order. All contours are prepared locally before
the scene is returned; any refusal drops the temporary compiled set and returns
no partial scene. No record is silently skipped, clamped, repaired or replaced.

## Independent source oracles

Frozen native-object-v1 authored frame7 expected commands are:
Move(10.125,20), Curve((13.375,19),(37,22),(40,20)),
Curve((40,26),(29,48),(25,50)),
Curve((20,47),(8.125,20),(10.125,20)), Close.
Expected control bounds/identity envelope are [8.125,19,40,50].
Reference frame12 has three straight cubic edges through (-10,0), (10,0),
(0,10), returning to (-10,0), and bounds [-10,0,10,10].
These controls are fixed from the admitted fixture, not generated from this
consumer. Exact original Number values and pointer identity remain checked.

The existing N24A cap oracle uses anchors (40,40)/(80,40), relative handles
(0,40), and a straight closing chord. Commands are Move(40,40),
Curve((40,80),(80,80),(80,40)),
Curve((80,40),(40,40),(40,40)), Close, with control bounds [40,40,80,80].
CPU tests check the actual curve rather than painting a control-hull rectangle.
No interior/exterior pixel result is claimed by this leaf.

Tests also retain multiple objects per layer and equal IDs across complete scoped
targets; reject every pin/scope/frame mismatch on nonempty and empty packets;
and refuse fixed codec-valid geometry after an earlier valid record. Actual
NativeObjectHistory fill1/undo2/redo3/reopen leaves old scenes unchanged and new
scenes correctly pinned. The same test file is registered in test-compositor and
test-application; focused compositor testing selects the existing history feature
and application already includes it. Both focused runs must execute the actual
history test with exact nonzero counts. No Cargo feature is added.

## Composition and acceptance boundary

Packet/scene encounter order and array indices are provenance only. No layer
painter order, z/blend order, animation of Order, Reference overlay, reference
media/rotoscopy purpose, held-frame interpolation or preview/export policy is
inferred. A separately admitted versioned native composition-plan document/codec
contract is required before multi-object GPU drawing. Parent #917 decision
6068926269 preserves the frozen baseline Order rules and unresolved reference
export evidence. Preparing independent contours needs no invented composition
policy, and cannot authorize drawing without that future contract.

No scheduled WorkId, opacity DocumentSnapshot, presentation/job/resource receipt,
GPU pixels, ordinary desktop selection/Save/reopen/History, public application/MCP,
pinned export, installed M1 or final desktop parity is accepted here. Public
native.object/Save/MCP/viewport/export availability stays unavailable. Browser/WASM
remains deferred R24/#1619. Existing eighteen-module/44-edge policy, Cargo,
floors/ceilings/exclusions and historical RED receipts remain unchanged.

Focused checks, one exact-head registered local quick after review/allocation,
independent review and normal protected integration establish this source-only
leaf. Lead separately owns later GPU/presentation/export and installed acceptance.
