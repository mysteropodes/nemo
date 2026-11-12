# Native v2 transport schema inputs

This is a contract-authoring guide. The public artifact remains
[`../native-transport-v2.schema.json`](../native-transport-v2.schema.json), including
its existing `$id`, definition names, request ordering and relative reference bases.
Existing Rust, JavaScript and descriptor consumers keep reading that path.

Each `capabilities-v2/<name>.json` descriptor owns the matching
`native-transport-v2/<name>.json` fragment. The generator discovers descriptor names;
there is no central fragment registration. A fragment declares its capability ID,
`definitions` and `requests`. Every declared transport operation must occur exactly
once in its owner's request variants. A const or enum operation branch is retained
whole; assembly preserves its constraints. MCP compiles those constraints as
described below.

A definition entry is `{ "schema": <JSON Schema>, "order": <optional integer> }`.
A request entry has the same shape. Historical ordinals preserve the original
aggregate's object and array order. New entries can omit `order`; they follow
historical entries in deterministic file/key order. Duplicate definitions,
operations, ordinals, wrong owners, missing fragments, orphan fragments and
unresolved references are errors. References are resolved relative to the public
aggregate, not relative to input fragments.

`shared.json` owns the transport envelope and definitions used by multiple
capabilities. It also deliberately retains compatibility definitions with no
request or descriptor input/output reference establishing an exclusive owner.
The initial split assigns all 265 definitions exclusively reachable from one
capability's request variants or descriptor references to that capability; 122
shared/envelope/compatibility definitions remain here. These are source ownership
counts, not functional acceptance. When an exclusive owner for a retained
compatibility definition is established, move its entry intact, retaining its
historical ordinal. `Request.oneOf` in shared input must remain empty: the
capability fragments own every request variant.

After editing inputs, run:

```sh
node scripts/nemo/lib/native-transport-schema.cjs --write
node scripts/nemo/lib/native-transport-schema.cjs --check
node --test tests/native-transport-fragments.test.cjs tests/capability-drift.test.cjs
```

Commit the inputs and resulting aggregate together. `npm run check` checks
freshness through its existing `native-capability-catalog` gate. The Tauri
`beforeBuildCommand` runs the same `--check` before building the MCP sidecar,
preserving the private-Labs guard. Neither gate silently rewrites the contract. Generation publishes the complete
aggregate by exclusive sibling temporary file and atomic rename.
Direct Cargo invocations do not run the Tauri hook; use the schema check explicitly
for those source workflows.

The one-time migration comparison and digest belong in the PR receipt, rather
than a permanent wire-contract pin. Ongoing
tests check live aggregate freshness and deterministic serialization/order using
a small independent fixture, so legitimate contract evolution does not require a
central digest edit. The addition fixture proves schema registration only; the
issue's complete handler/runtime fresh-agent acceptance remains separate.

## Tauri raw-client allowlist

A descriptor flags an operation `rawClient: { "<op>": { "revisionDelta": 0|1 } }`.
The same `--write`/`--check` entry points generate
[`../native-raw-client.json`](../native-raw-client.json) from those flags (sorted by
operation; a query declares delta 0, a command 1). `src-tauri/src/native_object_client.rs`
embeds it with `include_str!` and keeps no operation list of its own, and the
`native-raw-client-allowlist` drift gate fails if it regrows one. Adding a raw-client
capability is therefore its descriptor, its fragment and its handler; the allowlist,
the aggregate and the MCP request validators follow from `--write` with no edit to
central Rust. `tests/native-transport-fragments.test.cjs` adds one end to end. The
client keeps only the semantics of the three operations it special-cases (the object
read shape, the pinned serialize host operation and the visibility probe); its
envelope checks use the shared `NativeApplicationRequest::validate_envelope`.

The existing v2 declaration schema requires at least one operation even when a
capability is temporarily unavailable. Availability does not suppress its request
variants; the generator preserves this discovery and refusal contract.

## MCP request validation

The actual original-byte MCP request decoder reaches `validate_input`, which now
uses these schema-derived payload and envelope validators. `build.rs` discovers
all sibling `.schema.json` resources in sorted order and embeds them locally;
a new capability or sibling contract requires no central Rust resource list.
Sibling refs cannot escape that directory or retrieve remote resources. The
assembler checks the supported two-part `allOf` envelope/property representation
before generation, and runtime compilation errors are cached refusals with an
internal diagnostic, rather than traffic-triggered panics.

The cached payload validator uses the complete schema's original definition refs.
Envelope validation keeps identity, cancellation and revision policy from the same
branch; it does not copy or validate large payloads a second time. An independent
source test compares the two projections with the complete Request union for every
descriptor example and metadata/payload mutations. Another compiles both validators
for every declared operation, including operations without examples. Existing raw
member, byte, typed input, diagnostic, registration and availability gates remain.

Pure request rules for layer policy, visibility, object undo/redo, pinned
serialize/evaluate and empty diagnostics/reproduction payloads now use the schema.
Other family predicates remain for semantic protections, including byte lengths,
uniqueness, correlated targets and non-singular transforms. Native execution
validation is authoritative and unchanged. JavaScript and native-engine validators
are separate consumers; this source delivery does not make those schema-derived.

The chosen `jsonschema` 0.56.0 crate was already pinned for source tests. Its default
features are disabled, including network retrieval. Reusing its full refs, regex
and conditional semantics avoids introducing another partial JSON Schema matcher.
Static predicates or compact tables would reduce runtime dependencies but would
need their own equivalent keyword compiler and negative controls. The measured
tradeoff on the source candidate is 66 to 109 normal MCP dependency nodes, with
Rust 1.85 required by jsonschema versus the checked local 1.91.1 toolchain. A cold
isolated debug test measured 31.84 ms including registry preparation, 10,000 cached
trivial checks took 4.04 ms, and the whole test process peaked at 24.25 MB RSS.
These are source-test observations, not packaged release size or app timing.
