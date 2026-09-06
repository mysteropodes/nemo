# R09 schema generation feasibility evidence

Status: **preparation candidate, not production schema adoption**. Observed 2026-09-05.
Source baseline: `d4995210e8142cf6c13a3fa5dab865aca263410f` (fresh `origin/main`).
Issue: [R09 #905](https://github.com/mysteropodes/nemo/issues/905).
Author lane: Codexalog; coordination, review and integration: Codexitron.
Scope: this evidence document only; scratch code, generated files and Cargo build outputs
remain temporary. No production interface, dependency manifest or lockfile is changed.

The observed path is viable for an initial **Rust-owned vectorization DTO boundary**:
extract the existing declarations into a temporary crate, add a schema derive, generate
JSON Schema and a small TypeScript declaration, then compare regenerated artifacts and
validate a payload produced by the current JS expression. This does not establish a
canonical Rust project-document model or a shared application command dispatcher.

## Source-backed contract map

All source links below pin the baseline above. They identify implementation, not a
claim of browser or packaged-desktop acceptance.

| Boundary | Current writer and consumers | Generation/check opportunity and constraint |
|---|---|---|
| Saved document v13 | [SM.exportJSON/importJSON](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/timeline.js#L2049), with explicit layer export fields around L2112 and import validation/migrations around L2354 | JS/Paper state is authoritative today. Generate a reviewed compatibility schema from a characterized field inventory before moving ownership. Rust renderer structs cannot describe this file. Export emits `version:13`; importer warns for newer versions but proceeds. |
| Stored geometry/history | [serP/desP and frame persistence](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/app.js), [layersSnapshotNow/restoreLayersSnapshot](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/tweens.js#L4877) | A schema can check shape, but cannot replace explicit save/load/undo consumers, live-reference relinking, or identity/cache invariants. Shared stores and nested component contexts need separate fixtures. |
| Renderer scene | [JS buildSceneJson](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/engine-bridge.js#L802) → [ItemIn/LayerIn/SceneIn](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/geometry-wasm/src/engine.rs#L28) → `render` and `render_to_pixels` | Existing Serde DTOs are candidates for Rust-derived scene schemas. `SceneIn.layers` and `LayerIn.items` are required; `time` defaults, and many item fields default or are nullable. This is evaluated, transient render data. `pathRef`, resource lifetimes and field precedence need semantic checks beyond shape. |
| Vectorization configuration/results | [four shared structs and core](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/vectorize-core/src/lib.rs#L25), [JS payload producer](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/vectorize-bridge.js#L420) | Strongest small probe: ten camelCase optional configuration fields; required result dimensions/shapes and nested contours. Four declarations are source-extracted, not independently rewritten. The current UI expression emits six fields. |
| WASM vectorization adapter | [Rust wrapper](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/vectorize-wasm/src/lib.rs#L19), [worker](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/vectorize-worker.js), [loader](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/vectorize-wasm-loader.js) | Bytes plus configuration JSON string in; result JSON string out. Empty string maps to default configuration. Worker `jobId` correlates messages but is not a document revision or durable entity ID. Validate the JSON contents, not merely the string signature. |
| Native vectorization adapter | [Tauri vectorize_image](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src-tauri/src/vectorize.rs#L20), [registration](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src-tauri/src/lib.rs#L197) | Base64 image plus typed config → `Result<VectorizeResult,String>`, delegating to the same core. Native transport envelope differs from WASM; response/error parity still requires adapter tests. Current JS vectorization chooses WASM. |
| Native media commands | [VideoInfo](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src-tauri/src/video_decode.rs#L680), [open/decode/close commands](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src-tauri/src/video_decode.rs#L1180), [JS caller](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/js/native-video-bridge.js#L287) | VideoInfo uses snake_case output (`session_id`, `frame_count`); JS invokes decode with camelCase argument keys (`sessionId`, `frameIndex`). Session ID is `u32`, frame count `u64`, requested frame index `i64`. Raw RGBA8 `Response` is bulk binary, not a JSON array schema. Handle ownership, buffer dimensions and close/error behavior are separate contracts. |
| Generated ABI declarations | [vectorize_wasm.d.ts](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/wasm-vectorize/vectorize_wasm.d.ts#L4), [geometry_wasm.d.ts](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src/wasm/geometry_wasm.d.ts#L133) | Existing wasm-bindgen output already declares `Uint8Array`/`string`/return signatures. It leaves scene/config/result JSON interiors opaque; regeneration of ABI glue alone cannot detect those field mismatches. |

The [capability proposal](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/engineering/remediation/05_CAPABILITIES_MCP_AND_STANDARDS.md)
requires namespaced IDs, versions, scopes, units/bounds, availability, revision checks,
transactions/history and shared handlers. Those descriptors and the dispatcher are
**proposed**, not generated by the current Tauri command list. Do not reinterpret native
media commands as complete editing capabilities.

## Practical canonical-source options

| Option | Fit with inspected tooling/source | Decision for this preparation lane |
|---|---|---|
| Rust Serde DTO → Schemars JSON Schema → JS validation/types | `serde`/`serde_json` are already application dependencies. Schemars 0.8.22 is present transitively in [src-tauri/Cargo.lock](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/src-tauri/Cargo.lock#L3298) and was cached locally. Derive generation works in the offline scratch crate. | Preferred first adoption experiment for Rust-owned boundaries. A dedicated, opt-in schema build dependency/feature and generator crate need review; a Tauri transitive dependency is not a production schema API. |
| Rust DTO → ts-rs TypeScript plus schema generation | Adds a direct type generator not declared in inspected Nemo manifests. [ts-rs documents Serde support and unsupported-attribute warnings](https://docs.rs/crate/ts-rs/latest). | Viable future comparison after a pinned version and parity fixtures are chosen. Not installed or executed here. TS declarations alone do not validate runtime JSON. |
| JSON Schema → generated Rust and TS | [Typify generates Rust from JSON Schema](https://github.com/oxidecomputer/typify). Could serve a language-neutral format once persistence rules are characterized. | Unproven here; requires selecting/installing generators and runtime validators. Replacing existing Serde DTOs can change defaults/unknown-data policy. Do not introduce a second handwritten source of truth for the same contract. |
| JS field allowlists/fixtures → schema/JSDoc; existing wasm-bindgen for ABI | Matches current classic JS loading and current generated ABI artifacts; [package.json](https://github.com/mysteropodes/nemo/blob/d4995210e8142cf6c13a3fa5dab865aca263410f/package.json) declares no TS compiler or application schema generator. | Useful transitional document characterization. It must remain explicitly legacy-owned until cutover. Static field extraction and fixture inference cannot establish requiredness, union semantics or all allowed values. |

Schemars supports `JsonSchema` derivation and `schema_for!`; the probe pins **0.8.22**,
whose generated dialect here is Draft-07. See the [versioned Schemars documentation](https://docs.rs/schemars/0.8.22/schemars/).
Do not substitute the latest release or an implicit dialect in CI. The scratch declaration
generator below is deliberately config-only, not a proposed replacement for a maintained TS generator.

## Compatibility rules that need adoption decisions

- **Required vs omitted vs null:** all ten `VectorizeConfigIn` fields are `Option`; actual
  Serde accepts `{}` and explicit null. The generated declaration uses `?: T | null`.
  Missing config selects downstream defaults, not zeros invented by a schema. The WASM
  empty-string shortcut is transport behavior outside the config-object schema. Result
  fields are non-optional serialization outputs; their presence is required in the schema.
- **Names and enum meaning:** config uses `rename_all = "camelCase"`; VideoInfo does not.
  `colorMode`, `hierarchical` and contour `kind` are strings in Rust, not enum types.
  Their supported vocabulary and contour `1+3n`/coordinate-pair constraints are semantic
  rules absent from generated shape schemas. No probe runs vtracer or validates geometry.
- **Number ranges and portability:** Schemars 0.8.22 emits numeric formats and unsigned
  minima here, without maxima. `pathPrecision = 4294967296` passes the narrow JS shape
  check but fails real Serde `u32` parsing. `usize` is target-width-dependent; native `u64`
  counts and `i64` indices need an explicit JS safe-integer/decimal-string policy. Float
  precision, finite values, units and tolerances require explicit constraints and tests.
- **Stable identity:** document `layerUid`/parent links and stroke/mesh identities are
  separate domains from render `pathRef`, positional matte indices, media session IDs,
  worker job IDs and future request/revision IDs. Generate distinct named ID types only
  after scope/lifetime/relink/duplication rules are adopted. A string-shaped schema cannot
  prove reference existence or identity survival.
- **Version and unknown data:** file version 13 is separate from application semver and
  future API/schema versions. The importer permits future versions with a warning and uses
  explicit allowlists, so lossless preservation is not established. The scratch config
  parser accepts unknown fields; the schema does not forbid additional properties.
  Decide preservation/reporting/rejection independently for each versioned boundary;
  adding `deny_unknown_fields` globally would change current behavior.
- **Ownership/cutover:** name one writer per migrated aggregate and a rollback path.
  Validate create/edit/key/undo/redo/reorder/duplicate/save/reload plus selection,
  animation, render/offscreen export and native bridges. Include symbols, shared mesh
  stores and older/future files. Generated types do not update the consumers' allowlists.
- **Native/WASM/JS:** share transport-neutral DTOs and semantics while retaining explicit
  envelopes, errors and binary handles. Browser/WASM and Tauri need their own runtime
  evidence; neither is established by the host-only scratch compile.

## Reproduce the observed scratch experiment

Prerequisites: Git, Python 3, Node, Cargo/Rust, and locally cached dependencies. Observed
Node `v25.9.0`, Cargo/Rust toolchain `1.91.1`; direct scratch pins are Schemars `0.8.22`,
Serde `1.0.228`, serde_json `1.0.150`. No dependency downloads or installations occurred.
An initial scratch pin to unavailable serde_json `1.0.149` failed offline resolution;
using cached `1.0.150` resolved it. An offline cache miss on another host is a prerequisite
failure, not a drift failure; do not silently remove `--offline`.

Save the following Python block as a temporary `reproduce.py`. From a checkout containing
the pinned commit, use a detached temporary source worktree to avoid branch-dependent inputs:

```sh
probe_source=$(mktemp -d)
git worktree add --detach "$probe_source/source" d4995210e8142cf6c13a3fa5dab865aca263410f
python3 /path/to/temporary/reproduce.py "$probe_source/source"
git worktree remove "$probe_source/source"
```

The script creates another temporary directory and prints its location. It generates a
scratch lockfile offline, then uses `--offline --locked` for generation and parsing.
Keep that lockfile with any reproduced evidence; fresh resolution on another cache can
select different transitive versions. Compare its hash before claiming an identical environment.
`cmp baseline.json regenerated.json` is the positive artifact check; comparing the
scratch-mutated output to baseline is the deliberately failing drift check. No generator
or check command is added to Nemo's package scripts by this document.

```python
import hashlib, json, pathlib, re, subprocess, sys, tempfile
repo = pathlib.Path(sys.argv[1]).resolve()
probe = pathlib.Path(tempfile.mkdtemp(prefix='nemo-r09-schema-'))
(probe / 'src').mkdir()
def write(name, value):
    (probe / name).write_text(value)
def run(args, expected=0):
    p = subprocess.run(args, text=True, capture_output=True)
    if p.returncode != expected:
        raise RuntimeError(f'{args}: exit {p.returncode}\n{p.stdout}\n{p.stderr}')
    return p.stdout
source = (repo / 'vectorize-core/src/lib.rs').read_text()
part = source[source.index('#[derive(Deserialize, Default)]'):source.index('\nfn contour_from_element')]
assert part.count('pub struct ') == 4
part = re.sub(r'#\[derive\(([^)]+)\)\]', r'#[derive(\1, schemars::JsonSchema)]', part)
write('src/types.rs', 'use serde::{Deserialize, Serialize};\n' + part)
write('Cargo.toml', '''[package]
name = "nemo-schema-probe"
version = "0.0.0"
edition = "2021"
[dependencies]
schemars = "=0.8.22"
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = "=1.0.150"
''')
write('src/main.rs', '''include!("types.rs");
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("parse") {
        let input = std::fs::read_to_string(&args[2]).unwrap();
        match serde_json::from_str::<VectorizeConfigIn>(&input) {
            Ok(_) => println!("serde config PASS"),
            Err(e) => { eprintln!("serde config FAIL: {e}"); std::process::exit(1); }
        }
        return;
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "config": schemars::schema_for!(VectorizeConfigIn),
        "result": schemars::schema_for!(VectorizeResult)
    })).unwrap());
}
''')
run(['cargo', 'generate-lockfile', '--offline', '--manifest-path', str(probe / 'Cargo.toml')])
cargo = ['cargo', 'run', '--quiet', '--offline', '--locked', '--manifest-path', str(probe / 'Cargo.toml')]
write('baseline.json', run(cargo))
write('regenerated.json', run(cargo))
run(['cmp', str(probe / 'baseline.json'), str(probe / 'regenerated.json')])
print('schema regeneration: PASS (cmp exit 0)')
# Only this known JS producer expression is evaluated, with controlled input.
write('check.cjs', r'''
const fs = require('node:fs'), vm = require('node:vm');
const [schemaPath, repo, mode] = process.argv.slice(2);
const schema = JSON.parse(fs.readFileSync(schemaPath)).config;
const source = fs.readFileSync(repo + '/src/js/vectorize-bridge.js', 'utf8');
const expressions = [...source.matchAll(/var configJson = (JSON\.stringify\(\{[\s\S]*?\}\));/g)];
if (expressions.length !== 1) throw Error('producer changed; review extraction');
const cfg = {colorMode:'color', hierarchical:'cutout', filterSpeckle:4,
  colorPrecision:8, layerDifference:16, cornerThreshold:60};
const value = mode === 'overflow' ? {pathPrecision:4294967296} :
  JSON.parse(vm.runInNewContext(expressions[0][1], {cfg}, {timeout:1000}));
fs.writeFileSync(schemaPath + '.input.json', JSON.stringify(value));
// Deliberately narrow probe, not a general JSON Schema validator.
function valid(s, x) {
  const types = Array.isArray(s.type) ? s.type : [s.type];
  if (!types.some(t => t === 'null' ? x === null :
    t === 'object' ? x !== null && typeof x === 'object' && !Array.isArray(x) :
    t === 'integer' ? Number.isInteger(x) : t === 'number' ?
    typeof x === 'number' && Number.isFinite(x) : typeof x === t)) return false;
  if (x !== null && typeof x === 'number' && s.minimum !== undefined && x < s.minimum) return false;
  if (x !== null && typeof x === 'object') {
    if ((s.required || []).some(k => !(k in x))) return false;
    return Object.entries(s.properties || {}).every(([k,v]) => !(k in x) || valid(v,x[k]));
  }
  return true;
}
if (!valid(schema,value)) { console.error('JS producer/schema mismatch'); process.exit(1); }
console.log('JS producer/schema PASS');
''')
node = ['node', str(probe / 'check.cjs')]
run(node + [str(probe / 'baseline.json'), str(repo), 'normal'])
run(cargo + ['--', 'parse', str(probe / 'baseline.json.input.json')])
print('current JS payload: schema PASS; actual Serde PASS')
for label, value, status in [('missing', {}, 0), ('null', {'colorPrecision':None}, 0),
    ('unknown', {'futureField':True}, 0), ('wrong-type', {'colorPrecision':'eight'}, 1)]:
    write('case.json', json.dumps(value))
    run(cargo + ['--', 'parse', str(probe / 'case.json')], status)
    print(f'Serde {label}: expected exit {status}')
# Generate a config-only declaration from the observed schema; no TS compiler claimed.
def declaration(schema):
    rows = []
    for key, field in sorted(schema['properties'].items()):
        types = field['type'] if isinstance(field['type'], list) else [field['type']]
        ts = ' | '.join(dict.fromkeys('number' if t == 'integer' else t for t in types))
        optional = '' if key in schema.get('required', []) else '?'
        rows.append(f'  {key}{optional}: {ts};')
    return 'export interface VectorizeConfigIn {\n' + '\n'.join(rows) + '\n}\n'
schema = json.loads((probe / 'baseline.json').read_text())
write('baseline.d.ts', declaration(schema['config']))
# Demonstrate a real limitation: format uint32 alone does not constrain a JS number.
run(node + [str(probe / 'baseline.json'), str(repo), 'overflow'])
run(cargo + ['--', 'parse', str(probe / 'baseline.json.input.json')], 1)
print('uint32 overflow: narrow schema check PASS; Serde exit 1 (known gap)')
# Change only scratch Rust, never source; stale generated outputs must fail.
types = (probe / 'src/types.rs').read_text()
assert types.count('pub color_precision: Option<i32>') == 1
write('src/types.rs', types.replace('pub color_precision: Option<i32>', 'pub color_precision: Option<String>'))
write('drift.json', run(cargo))
write('drift.d.ts', declaration(json.loads((probe / 'drift.json').read_text())['config']))
run(['cmp', str(probe / 'baseline.json'), str(probe / 'drift.json')], 1)
run(['cmp', str(probe / 'baseline.d.ts'), str(probe / 'drift.d.ts')], 1)
run(node + [str(probe / 'drift.json'), str(repo), 'normal'], 1)
run(cargo + ['--', 'parse', str(probe / 'drift.json.input.json')], 1)
print('drift: schema cmp exit 1; declarations cmp exit 1; JS check exit 1; Serde exit 1')
for name in ['baseline.json', 'baseline.d.ts', 'Cargo.lock']:
    print(name, hashlib.sha256((probe / name).read_bytes()).hexdigest())
print('scratch:', probe)
```

Observed successful harness output (each negative result is asserted by the harness):

```text
schema regeneration: PASS (cmp exit 0)
current JS payload: schema PASS; actual Serde PASS
Serde missing: expected exit 0
Serde null: expected exit 0
Serde unknown: expected exit 0
Serde wrong-type: expected exit 1
uint32 overflow: narrow schema check PASS; Serde exit 1 (known gap)
drift: schema cmp exit 1; declarations cmp exit 1; JS check exit 1; Serde exit 1
```

Observed SHA-256 fingerprints:

| Temporary artifact | SHA-256 |
|---|---|
| baseline.json | `107054946cc33c9dc4f30cbcfd651b5a13e8c0970b53feeb326f1063da244f25` |
| baseline.d.ts | `58c444989bc5c77f6906af804f67ebe8e20e713c5eb7626ef03e157b1d3a2aff` |
| Cargo.lock | `fa8a35ffb152103a1c6ced13612e4908791d7186ea957c67836da9d370d2ec32` |

The generated config declaration contains `colorPrecision?: number | null`; the negative
control changes it to `colorPrecision?: string | null` by changing only the extracted Rust
field. Schema bytes, declaration bytes and existing JS producer compatibility therefore
all expose the drift. No negative control edits Nemo source.

Limitations: extraction is bounded text slicing, with assertions for the current source
shape; it is not a general Rust/JS parser. The JS check implements only the observed config
object/type/required/minimum subset and ignores numeric formats. It is **not** a general
Draft-07 validator. Result schema generation is observed, but result-payload validation,
TS compilation, renderer generation, actual image processing, native IPC, browser workers,
WASM compilation and live document round-trips are not exercised. Byte drift detects change,
not whether that change is backward compatible. No application tests are required for this
document-only change; its evidence gates are reproduction, source links and scoped diff.

## Provisional dependencies and next adoption gate

These were open candidates when inspected; do not treat their contents as merged contracts:

| Candidate | Inspected head | Use and limitation |
|---|---|---|
| [R03 #944](https://github.com/mysteropodes/nemo/pull/944) | `dd39e5215f000031c5f75821913cce2e6f971db9` | `engineering/inventory/SURFACES.md` supplies surface-to-consumer candidates; bounded static reachability is not handler or schema coverage. |
| [R03 #946](https://github.com/mysteropodes/nemo/pull/946) | `93d6d2ae76ffea04fcbd8e2fbe849f14469c6092` | `tests/fixtures/README.md` describes deterministic document fixtures and checks; useful inputs after integration, not proof all document workflows passed. |
| [R05 #948](https://github.com/mysteropodes/nemo/pull/948) | `33e248795a0b94b4c93dde210c1d29f3fdab5039` | `engineering/boundaries/profiles/app-surfaces.md` classifies source files and explicitly lacks final architectural layer assignment. It cannot choose DTO ownership automatically. |
| [Cross-contract #950](https://github.com/mysteropodes/nemo/pull/950) | `b53fccae6426f14b6541e927869a0abc1515c415` | Separate acceptance-evidence candidate, not modified or adopted by this lane. |

Buzzotron owns `engineering/contracts/evidence/inventory-contract-gaps.md`; coordinate any
later join through the integration owner. Original Fizz/Honey/Mochi proposal jobs remain
unresolved under their original, distinct path reservations. This lane does not replay,
rename, overwrite or claim those proposals. Root retains their exact job/path records and
sole board/integration ownership; nothing here closes R09 or its dependencies.

Next review decisions: choose the first DTO owner/module and schema dialect/version; select
and pin maintained JS validation/TS generation tools; define numeric/unknown-field/identity
policies; then authorize a narrowly scoped production generator with committed deterministic
artifacts and a check that runs in CI. Adoption must show old/current/future compatibility
fixtures, rejected stale artifacts and real native/WASM adapter parity before extending to
project persistence or the application command envelope. This document supplies preparation
evidence for that decision, not merge, release or issue-closure authority.
