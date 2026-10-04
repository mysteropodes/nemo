# Staged native object identity — N25A

Status: contract only, for [N25A/#1576](https://github.com/mysteropodes/nemo/issues/1576).
The schema and executable cases freeze a future admission boundary; they do not
add a native object store, dispatcher operation, capability, importer or editor.
The accepted opacity runtime and its fixtures remain unchanged.

## Identity and scope

One record represents one closed solid-fill cubic path in `scene-root`. Its
target is `(contextId, frameScope.kind, frameScope.frame, layerUid, strokeId)`
within a selected document incarnation and content revision. `authored` names
an explicitly authored frame; `reference` names the inspected reference frame
used for per-element references. Neither requests interpolation, held-frame
materialization or an automatic search across frames. A later owner must prove
that the selected frame and target exist. Equal IDs at different frames are
distinct scoped targets, not evidence of shape continuity.

Preserve every assigned `layerUid` and `strokeId` byte for byte. They are opaque
nonempty strings, without UUID, prefix or numeric-string restrictions. Numeric
values and `layerIndex`, `strokeIndex`, `renumber`, `assignIds`, `allFrames` or
other undeclared fields are rejected. A string such as `"0"` cannot reveal
whether it was a legitimate old ID or derived from an index: provenance and
before/after preservation require runtime evidence. Missing IDs are rejected;
this leaf supplies no assignment or migration policy. Array order is geometry
order, never object identity.

The schema validates one record at a time. Duplicate IDs, duplicate JSON member
keys lost during parsing, frame-range existence, reference resolution and ID
renumbering that still produces valid strings cannot be settled by this schema.
N25B must reject duplicate scoped keys and unresolved references at admission,
and prove exact identity retention against source records before reads are
accepted. No claim of global or cross-frame uniqueness is made.

## Geometry and supported family

The staged path has 2..256 ordered anchors, matching `ClosedCubicPath::new` in
`native-engine/src/render_geometry.rs`, each with a `point` position
and relative `handleIn`/`handleOut` vectors in document/world units (x right,
y down). For anchors A and B, one cubic uses A.point,
A.point + A.handleOut, B.point + B.handleIn, B.point; the final segment wraps
to the first anchor because `closed` is exactly true. Zero handles are valid.
Preserve numeric values and segment order; do not flatten handles into absolute
control points or screen coordinates. Coordinates and handles are bounded to
[-1.7976931348623157e308, 1.7976931348623157e308], the finite f64 range. Supported
schema minimum/maximum checks reject JSON exponent tokens such as `1e400` that
JavaScript parses as Infinity; JSON syntax alone does not guarantee finiteness.
NaN is not a JSON value and is outside this JSON-data contract. Finite values do
not guarantee finite sums, GPU representation, nondegenerate segments or positive
control-hull area: the renderer's additional checks remain later admission gates.
No new precision or rounding rule is imposed:
the existing `serP` serializer uses three-decimal `_r3` rounding, and a future
adapter must characterize that boundary rather than repeatedly round values.

`fill` is one solid straight-alpha RGBA value, each channel in [0,1], using the
existing document color interpretation. This adds no OCIO or conversion policy.
There is no stroke, transform, blend mode or separate opacity property in this
record. Transformed paths require a separately characterized future contract;
no flattening permission is implicit here. Open paths, null/unknown geometry,
compound paths, groups, components, brush companions, strokes, gradients, text,
rasters, meshes and media are unavailable. Reject the whole unsupported record
before admission; never drop fields or substitute a fallback path.

## Versions and prospective operations

`schemaVersion: 1` versions this staged object schema only. It is independent of
application `apiVersion: 2`, capability schema versions, legacy project versions
and the active `nemo.native-opacity-document` stored `formatVersion: 1`.
There is deliberately no new stored-format declaration. An object record must
not be inserted into an opacity project or sent through its active operations.

[cases.json](examples/native-object-v1/cases.json) contains prospective
`query.document.object` and `object.fill.set` request/result examples under the
[ADR-001](ADR-001-native-document-and-evaluation.md) envelope. The definitions
in this schema validate their shapes separately from the record. They are not
registered in the active transport schema, JS adapter, Rust owner or MCP.

A read selects `atRevision` plus the complete scoped target. Its result names
that revision and an immutable `documentSnapshotId`; the envelope may identify
a newer current head. Matching request IDs, document identity, selected target
and pinned revision must be enforced by the later owner, not inferred from
shape validation. A single `object.fill.set` command names `expectedRevision`
and the same scoped target. A successful changed fill adds exactly one revision
and one history entry, retaining target and geometry. No-op semantics, retry
retention and transaction implementation are not introduced by these cases.

The fixed failure cases specify `wrong_document` (current owner identity in the
envelope; requested old document in details), `not_found` (missing scoped target),
`unavailable` (unimplemented or unsupported family), and `stale_revision`
(no mutation/history on stale write). Errors preserve ADR-001's closed codes.
These are prospective outcomes, not observed dispatch receipts. N25B and a
separately admitted command leaf must implement owner checks and fail atomically.

## Executable acceptance and remaining gates

Run `node --test tests/native-object-contract.test.cjs`. The tests reuse the
repository's existing schema subset validator, validate every fixed positive
and negative case, and mutate identity, frame, geometry, fill and protocol fields
to demonstrate that controls fail. The shared validator does not implement
`maxItems`; the test oracle explicitly supplements the declared segment maximum
and demonstrates that the shared validator alone accepts 257 segments. Fixed
1/2/256/257 count controls establish this contract boundary only; they do not
prove the generated contours survive native rendering. Paired case checks enforce the fixed read
correlation and command identity/geometry oracle; they are not a runtime mock.
Active opacity schema/adapter controls continue to reject object fields and
commands.

This is schema/example acceptance only. Native admission/read, save/load,
undo/redo, selection, animation, rendering/export, native bridge, browser and
installed desktop acceptance remain future gates. N23A opaque reads and N24A/B
immutable cubic rendering do not establish persisted editable object identity.
No runtime slot or legacy writer is touched by N25A.
