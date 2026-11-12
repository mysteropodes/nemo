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
whole; the generator does not rewrite constraints or generate runtime validators.

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
for those source workflows. No Rust build dependency or runtime validator changed.

The migration test pins the independent pre-split aggregate digest for the
contiguous historical prefix (387 definitions and 151 request variants) at source
`ea1b3c62766fbd67f8857ce4432230491bc1df32`, proving preservation of all original
constraints, refs, examples and ordering. Additive entries follow that prefix and require no central pin edit. A deliberate
change to the historical wire contract must update that oracle with its own review
and negative behavior evidence.
The addition fixture proves schema registration only; the issue's complete
handler/runtime fresh-agent acceptance remains separate.
