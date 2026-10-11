# Authored object composition order — staged v2 contract

N25F3A/#1666 freezes storage, migration/refusal and the independently expected
baseline behavior for a later codec and evaluated-order producer. This packet
changes no product schema, codec, renderer, bridge or capability. Current strict
native-object **v1** still has no admitted paint-order meaning. N25F1 packet and
N25F2 scene indices/encounter order remain provenance. The schema below is a
contract for separately admitted implementation, not a claim it decodes today.

The oracle is `examples/native-object-order-v2/baseline.json`, version
`nemo.native-object-order-baseline/1`. It names frozen source
`ded641bd763379f36d629bf1686fcce8a6137a66`, exact Git blobs/content SHA256 and
renderer-excerpt SHA256. The test reads those actual Git objects, fails if absent
or different, loads the complete frozen `curve.js`, its required `opacity.js`
domain and `motion.js` in an isolated VM and calls their public Order evaluators.
It runs the actual layer-insertion
and element-sort statements from `engine-bridge.js`; it does not copy them into
an expected-order implementation. Missing history is a failed prerequisite, not
a skipped test or permission to substitute current source. The frozen export
source is pinned and its engine Order routing is identified, not executed.

## Baseline meaning

All sequences below mean **back to front**. Natural layers follow their authored
positions. Explicit layer Order means a rank counted from the front: evaluate the
rank, remove explicit layers from the natural stack, then use the frozen producer's
front-ranked insertion with `Math.round(rank) - 1` clamped to the insertion list.
It sorts explicit entries by rank then natural index before inserting. A later
equal-rank insertion displaces the earlier entry. This is not a flat z sort.
`[L0,L1,L2]` with only `L2=2` paints `[L0,L2,L1]`; `L0=L2=1` paints
`[L1,L0,L2]`. Negative/oversized ranks clamp; fractional ranks round as JavaScript
does, including halfway values. Values remain finite f64; do not round storage.

Elements sort **within their own layer** by evaluated value descending, then
natural position ascending. Higher value goes farther back; natural elements
have value 0. Only `b=2` in `[a,b,c]` yields `[b,a,c]`; the `a=b=2` tie preserves
their original relative order. Element values are not rounded or clamped.

A missing layer Order reads its natural front rank and is not explicit. A static
holder array `[0]` is present/truthy and **is explicit**; it requests the front.
This differs from a missing holder. Layer activation follows the actual frozen
`layerHasExplicitOrder`, not numeric truthiness or a new expression policy.
Expression-only layer holders without a static/keyed base remain nonexplicit and
use the natural fallback. The element getter can evaluate an expression-only
holder, but the producer's per-layer guard depends on static/track presence on
some element of that layer. Expression presence alone does not activate engine
Order routing. Getter values, layer explicitness and producer/export routing are
distinct observations; preserve their differences rather than fixing them.

Outgoing hold belongs to the left key and jumps at the next key's frame. Other
segments use their real waypoint curve; missing baseline curves use the baseline
default while empty curves are linear. Storage preserves missing/null versus
empty/populated curves so migration cannot change that distinction. Expressions
override evaluated raw values only as the frozen evaluator permits. Simple real
frame/value expressions are exercised; complete expression-language parity is
a later evaluation gate. Isolated runtime compilation/seed/error bookkeeping is
an observed baseline effect. It must not be confused with mutating native document
data. The fixture remains byte-immutable; test instances are clones.

## Exact proposed storage

Retain `format: "nemo.native-object-document"`; **document formatVersion 2** keeps
the existing identified `layers`, complete `objects` and `totalFrames` fields and
adds one required `compositionOrder`. Object record `schemaVersion` stays 1;
geometry/paint and complete targets remain unchanged. A v2 document cannot be
decoded by today's v1 codec. No global default upgrades v1.

`compositionOrder` is the exact plan object demonstrated by the fixture's
`storageExample`. Its required fields are:

| Field | Type / admitted meaning |
| --- | --- |
| `format` | literal `nemo.native-object-order-plan` |
| `formatVersion` | integer 1, independent plan version |
| `documentFormatVersion` | integer 2, must match the enclosing document |
| `contexts` | array of uniquely identified context plans; covers every authored object context |
| `migration` | literal `explicit-plan-required`; never authorize inference from legacy indices |
| `referencePolicy` | literal `unresolved`; no Reference paint or export policy granted |

Each context has exactly `contextId`, `layers`, `scopes`. `contextId` is nonempty
and unique. Its `layers` MUST cover **every enclosing document-declared layer**
exactly once, including empty layers. Each entry has exactly `layerUid`,
`naturalPosition`, `order`; positions form `0..declaredLayerCount-1` exactly once.
This one context-wide layer table owns natural order and layer Order metadata for
ALL authored frame scopes in that context. A scope cannot repeat, override, vary
or omit the global layer stack. No per-frame second layer-order owner exists.

Each scope has exactly `frameScope`, `elements`. `frameScope` is exactly
`{kind:"authored",frame:u32}` with `frame < totalFrames`; it identifies source
records, **not** an evaluated sample frame. Scope keys are unique within their
context and cover every authored object scope. Element entries have exactly
`layerUid`, `strokeId`, `naturalPosition`, `order`. IDs must resolve to the
context's declared layers and actual complete objects in that scope; every
authored object is covered exactly once. Natural element positions form
`0..elementCountInLayer-1` exactly once **per layer**. Entry array order carries
no extra meaning; explicit positions give the natural sequence. An empty context
or scope may be explicitly declared but still uses the complete context layer
table; it creates no object identity and grants no Reference composition.

Full element identity is `contextId + frameScope(kind,frame) + layerUid + strokeId`.
Layer-plan identity is `contextId + layerUid`, global across its frame scopes;
it never borrows a stroke ID. Repeated `strokeId` across contexts, layers or
source frames cannot alias. The element table stores only order metadata referring
to the sole native document's complete records, not a second object store.

`order` is a closed record with **independent optional** `static`, `track` and
`expression` members, not a mutually exclusive mode. `{}` means untouched.
Absence is retained as absence, never serialized as a manufactured default.
All three members may coexist; decoding/encoding/history must preserve each.

- `static?: [f64]`: exactly one finite scalar, preserving the singleton authored
  array. `[0]` is present/truthy and explicit for a layer. Negative/fractional
  values are retained without rounding. This maps to `motionStatic.order`.
- `track?: {keys:[key,...]}`: **empty keys are valid and preserved**. Nonempty keys
  have strictly increasing unique finite f64 frames, including signed/fractional
  and beyond-timeline values; do not clip them to totalFrames or source-scope u32.
  A key requires `frame` and `v:[f64]` (one finite value); optional `hold:boolean`,
  `curvePoints:null|[point,...]`, `hIn:[f64,f64]`, `hOut:[f64,f64]` preserve exact
  presence/content. Missing `hold` evaluates false without adding a field.
  Missing/null curves use the frozen default; `[]` is linear. Populated curves
  have at least two `{x:f64,y:f64,tx?:f64,ty?:f64}` points with x=0/1 endpoints
  and strictly increasing x in [0,1]; optional tangent components retain their
  independent presence (missing ty reads 0 in the frozen evaluator). All numbers
  are finite; y/tangents may overshoot. Do not materialize/erase default curves or
  spatial-handle metadata during a round-trip. This maps to `motion.order`.
- `expression?: {enabled:boolean,code:string}`: exact authored enabled flag and
  UTF-8 code, **including empty code, disabled nonempty code and enabled empty
  drafts**, preserved byte-for-byte. Code is at most 4096 bytes in this staged
  format; an over-budget legacy conversion refuses unchanged rather than truncate
  or discard. This maps to `expressions.order`. No nested expression/base union
  exists and no native compiler/language parity is asserted here.

Observed evaluation is derived from these members: nonempty keys take precedence
over static; an empty track falls back to static/default without being erased.
An expression evaluates only when enabled AND code is nonempty. Layer explicitness
depends on nonempty keys OR static-array presence, then applies expression/raw
evaluation; otherwise the layer keeps its natural rank. Producer/export routing
separately observes track-object OR static-array presence (even an empty track),
not `hasKeys` or expression presence. Element evaluation follows its own frozen
getter plus per-layer guard. None of these transient decisions replaces storage.

An expression's compiled cache, last error, transient seed/context and evaluated
rank/sequence are not authored storage fields. Random/seed semantics must receive
their own admitted persistence/evaluation contract before those expressions can
be consumed natively; never manufacture a seed to make an unsupported mode work.
Finite authored data can still yield an unsupported/nonfinite evaluation; C must
return unavailable rather than publish a fabricated natural/static result.

## Migration and atomic refusal

Keep strict v1 decode/encode and semantic round-trip behavior unchanged; preserve
v1 save/reopen/history source credit. V1 records have no promised natural painter
order. A v1 → v2 migration requires a caller-supplied, complete, validated plan;
array encounter indices, lexical IDs, F1/F2 provenance or a new default must not
supply it implicitly. A valid v2 retains all geometry/paint/identity plus its plan
through save/load and immutable revision/history snapshots. No v2 → v1 downgrade
or unknown-field dropping exists. Migration is prepared before any owner install
or replacement, with one atomic commit only after complete validation.

Duplicate decoded JSON member names at **every DTO level**, unknown fields or
tags/versions, nonfinite/wrong numeric types, positional-array DTO substitutes,
invalid/duplicate/gapped natural positions, duplicate context/scope/full keys,
incomplete declared-layer coverage, per-scope layer tables or overrides,
cross-layer/unresolved/missing records, invalid curves/keys, oversized expression
code, and Reference entries inside authored plans
refuse the whole v2 document. No partially installed document/plan or rewritten
legacy fallback is permitted. Unsupported expression compilation/evaluation is
typed `unavailable`, preserving authored bytes; it is not a codec success that
pretends the expression is static. Current code implements none of these v2
refusals yet; implementation and consumer tests belong to the next codec leaf.

Reference object identities/records continue to survive save/load separately.
They do not receive inferred authored order. Reference composition/evaluation
requests remain unavailable pending the explicit human display/export decision.
No codec gate may silently delete them or claim Reference is baseline rotoscopy.

## Successor delivery and acceptance

1. **B — strict storage/codec**, separately admitted within 90 active minutes:
   document/codec types, full-v2 round-trip and atomic negative/migration tests;
   history/save/reopen consumers preserve all added persistent metadata. Static
   consumption does not accept held/eased/expression evaluation. Exact Rust module,
   Cargo/profile and shared-file grants are checked before writing.
2. **C — evaluated order**, bounded per-frame producer from the immutable native
   revision, keyed to full source scope, separately recording evaluated frame,
   values and one deterministic back-to-front sequence. It consumes admitted
   tracks/expressions or returns unavailable. Held geometry/source-frame selection
   is separately admitted; storing Order never implies it exists.
3. **D — one composition consumer** for both viewport and export, using that same
   evaluated sequence. No duplicate evaluator, mutable JS order mirror or opacity
   document/scheduler identity. GPU, installed pixels and M1 have their own gates.

This A oracle covers hand-authored natural/static collision/tie, round/clamp,
zero/holder activation, endpoint/hold/nonlinear ease and simple real expressions,
distinct repeated-stroke scopes and separate Reference identity. Actual mutated
flat-sort, reversed element direction/tie and frame/hold/ease/expression bypasses
must fail independent expectations. It is an **excerpt/facade source oracle**,
not full `buildSceneJson`, actual export frames, native v2 decode/evaluation,
pixels, ordinary UI Save/Open, public MCP or installed desktop acceptance.

Focused Node + existing animation tests precede independent exact-head review.
Protected-base boundaries, check13, unchanged inventory and one registered local
quick follow the granted resource order. Normal protected integration and issue/
Project #2 readback close only this contract/oracle packet. Historical baseline
Git objects are a required fixture dependency; shallow checkouts must obtain the
pinned history before testing, without substituting another source revision.
