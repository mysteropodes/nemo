# Application-surface source classification (R05 adoption increment)

Status: **adopted source coverage + legacy no-growth size profile**, not a claim that
application files have been assigned reviewed target layers or public APIs. The original
80-file classification was authored against `35f0f5f` (#943). Candidate `c9ab634` combines
that profile work from #948 with the #957 tokenizer correction (`d677989`) and expands the
profile to all **140 handwritten `src/js/**` files**. The R05 integration adopts those
reviewed profile bytes and the exact 12-file vendor/generated policy in the standard boundary
lane. It changes no application source or `engineering/inventory/**` content.


Current R06 integration adds two handwritten modules: the pure project validation
helper [`project-document.js`](../../../src/js/project-document.js), loaded before
`timeline.js`, and the presentation adapter
[`project-entry.js`](../../../src/js/project-entry.js), loaded before `project.js`.
The validator preserves the existing frame-only migration and rejects malformed
layer/frame structures before document replacement. The adapter schedules Open/Resume
repaint after canvas resize, with native-renderer and Paper fallbacks. Both enter the
142-file profile with ordinary 400/500-line budgets; existing legacy ceilings remain
unchanged. The counts and file table below are historical evidence for `c9ab634`.
Current membership and bootstrap pins are in `app-js.coverage.json`; complete
application architecture enforcement remains open.

Scope: the application source the previous `scripts/nemo/**` increment
([`scripts-nemo.md`](./scripts-nemo.md)) explicitly excluded — `src/**`, `src-tauri/src/**`,
`geometry-wasm/src/**`. This packet classifies every file in that scope into one of four
buckets named by the task: **application JS**, **Rust**, **UI/style/bootstrap**, and
**generated/vendor/shader/translation** — with a justified exclusion, an exact-path legacy
no-growth ceiling, or both, per path. No path is silently grandfathered: every path below is
named, not summarized away.

Owner/reviewer: no per-path `CODEOWNERS` exists in this repository (same fact recorded in
`scripts-nemo.md`). Accountable human owner for all paths below is Ilya (`ivg-design`), per
issue [#901](https://github.com/mysteropodes/nemo/issues/901)'s "Accountable human / integration
owner" field. The adopted mechanical classification remains distinct from the outstanding
maintainer review of target layers, public APIs and provider/consumer boundaries.

## Relationship to R01/#897 and R03/#944

R01/#897 adopted the remediation handbook but did not produce a real
domain/application/features/ports/adapters/shared layer assignment for `src/js/**`. R03's
reviewable increment (#944, open at this adoption) inventories **UI surfaces** (DOM controls,
menu items, shortcuts, Labs registrations, the scripting/plugin API — `engineering/inventory/
surfaces.json`), not a module/dependency graph with per-file public APIs. It cannot supply this
packet's `modules[]`/`layerRules` even once merged — it answers "what user-facing capability
does this control reach," not "what does this file import and export." Its own PR body states
plainly that `scripts/nemo/inventory.cjs` itself is not yet declared in any boundaries profile.

This is the "false-mapping limitation" the task names: treating #944's surface→consumer rows as
if they were a module dependency graph would misattribute UI-reachability edges as import edges,
which they are not. This packet therefore does **not** wait for #944's acceptance and does not
derive `modules[]`/`layerRules`/`publicApi` from it — it builds the one thing that does not
require either R01 or R03: a **mechanical, per-path inventory** (real file list, real lexical
parse result, real line counts) plus an **honest, single flat layer** for the JS subset the
checker can run against at all, explicitly not claiming the real architectural layers R01 would
assign. Full graph-accurate adoption (cycle/private-import/layer-violation with real semantics)
remains blocked on R01 exactly as documented; this packet does not close that gate and does not
claim to.

## Category 1 — Application JS (`src/js/**`)

There are **143 `.js` files** (105 directly in `src/js/`, 38 in `src/js/labs/`): **140
handwritten files** with 104,855 nonblank lines, plus three vendored `.js` files excluded on
provenance grounds (Category 4). File membership and execution mode are different counts.
The 143-file population divides as follows in the source at `c9ab634`:

| Execution/source population | Files | Evidence |
|---|---:|---|
| Classic startup script tags pointing into `src/js/**` | 128 | `src/index.html`; includes the three vendor `.js` files, so 125 are handwritten |
| Module startup scripts | 2 | `geometry-wasm-loader.js`, `vectorize-wasm-loader.js`, both `type="module"` |
| Module worker, with no startup script tag | 1 | `vectorize-worker.js`, constructed by the vectorize launcher |
| Labs files with no startup script tag | 12 | Exact filenames below; tag absence alone does not establish runtime reachability |

`src/index.html` has **132 script tags** altogether: the 130 file tags into `src/js/**`
above, one inline script, and `paper-full.min.js`. It therefore does not load all 140
handwritten files as classic scripts. The twelve Labs files without startup tags are
`src/js/labs/{auto-actions.js,clip-mask-bake.js,command-palette.js,layer-effects-bake.js,
lipsync-assistant.js,oca-export.js,rig-deform.js,screentone-bake.js,shader-effects-bake.js,
storyboard-mode.js,timelapse.js,timeline-zoom.js}`.

The module paths have different responsibilities:

- [`geometry-wasm-loader.js`](../../../src/js/geometry-wasm-loader.js) dynamically imports
  the generated geometry glue (`../wasm/geometry_wasm.js?v=` plus its cache token).
- [`vectorize-wasm-loader.js`](../../../src/js/vectorize-wasm-loader.js) constructs
  `new Worker(new URL('vectorize-worker.js', import.meta.url), { type: 'module' })` on demand.
  The launcher does **not** dynamically import the vectorize WASM glue.
- [`vectorize-worker.js`](../../../src/js/vectorize-worker.js) performs that dynamic import
  inside the worker (`../wasm-vectorize/vectorize_wasm.js?v=` plus its cache token).

Static top-level import syntax, dynamic imports, script-tag mode, and worker construction are
separate source facts; searching only for top-level `import`/`require` cannot classify them.

### 1a. Lexical coverage — the former 60-file handwritten gap is closed on this candidate

Before #957, the tokenizer rejected ordinary division after `)`/`}`. The original candidate
could therefore declare only 80 handwritten files (24,903 nonblank lines), omitting 60
handwritten files (79,952 lines). Counting the three vendor `.js` files as well gave the older
63-of-143 / 80,688-of-105,591-line figures. Those were two different populations and are now
**historical exclusion counts**, not the current profile's coverage.

The tokenizer correction in `d677989` closes that specific division/control-header-regex
failure. `c9ab634` adds the previously omitted 60 handwritten files, including `motion.js`,
`timeline.js`, `tools.js`, `app.js`, `tweens.js`, and `engine-bridge.js`. All 140 handwritten
files now have one module entry each. The three vendored `.js` files and two vendored `.mjs`
files remain excluded for provenance, regardless of whether the tokenizer can parse them.

This is lexical and per-path coverage. It does not supply reviewed layers, a complete
provider/consumer graph, or runtime acceptance. The tokenizer remains a lexical scanner;
remaining unsupported forms such as escaped identifiers, legacy numeric string escapes, and
ambiguous bare-`}` slash contexts remain outside its documented scope.

### 1b. Adopted legacy profile — 140 files, [`app-js.profile.json`](./app-js.profile.json)

The adopted profile retains the deliberately conservative legacy policy:

- **One flat layer, `app-legacy`**, with `allowedLayers: ["app-legacy"]`. This records that
  real domain/application/features/ports/adapters/shared assignments remain unreviewed.
- **`publicApi` = the file's own basename** for every module. This exposes the mechanical
  inventory without pretending that per-file public/private API review has occurred.
- **One size profile, `"App JS unclassified (pre-R01)"`**, warn 350 / hard maximum 500.
  It uses the policy's Feature UI ceiling provisionally rather than guessing each real layer.
- **36 exact-path size exceptions**, comprising the original 11 plus 25 for newly covered
  files. Each names owner `ivg-design`, issue `901`, expiry `2026-12-05` as a review checkpoint,
  and the file's current nonblank line count as its ceiling. The JSON enumerates every path;
  none of these ceilings permits growth beyond the measured source.

The import-only graph still cannot describe the application's coupling. For example,
`psd-import-bridge.js:30` dynamically imports `./ag-psd.vendor.mjs`, whose line 2 statically
imports `./node-buffer-shim.vendor.mjs`. Both targets are vendor `.mjs` files outside the
143-file `.js` count and outside this profile. The integrated checker reports that edge as an
`unprofiled-local-import`; the coverage policy separately verifies the excluded vendor bytes
and provenance. The two generated WASM imports above have computed targets and are reported as
unsupported imports. Worker construction and HTML startup order are not import edges modeled
by this checker.

**Zero cycle/private-import/layer-violation findings are structurally vacuous for this
140-file candidate, not evidence of clean coupling.** Meaningful checks require reviewed
per-file layers/APIs and dependency evidence. ESM migration can provide explicit import
edges, but it is **not a prerequisite** to modeling existing classic scripts: a reviewed
global/provider-consumer inventory, reconciled with startup and worker relationships, can
also supply those edges. That route remains to be designed, reviewed, and integrated; this
profile does not implement it or equate R03's UI-surface rows with module dependencies.

[`app-js.baseline.json`](./app-js.baseline.json) remains byte-identical to the candidate as the
reviewed first-adoption seed. The standard CI lane requires that byte identity only while the
protected base lacks an application profile. After adoption, it materializes the prior profile
from the protected base commit and rejects ceiling growth independently of this seed.

### 1c. `i18n.js` — translation table, not logic (4,241 nonblank lines)

`src/js/i18n.js` embeds a translation table in handwritten JS. It remains in the 140-file
profile with a distinct size-exception reason. `04_MODULARITY_POLICY.md` explicitly excludes
splitting translation tables merely to satisfy an application-code line budget. The other
35 size exceptions retain their own exact paths and legacy rationale; this classification
does not approve decomposition or exempt any file from future ownership review.

### 1d. Recorded signal from the expanded candidate

The `c9ab634` commit records this command against its unmodified source:

```sh
node scripts/nemo/boundaries.cjs engineering/boundaries/profiles/app-js.profile.json --baseline engineering/boundaries/profiles/app-js.baseline.json --json
```

Its recorded result is **exit 1, 140 modules, 437 `global-state` violations, 2
`unsupported-import` violations, 4 `unsupported-global` violations, 18 size warnings,
0 size violations, and 36 applied size exceptions**. The two unsupported imports are the
computed generated-glue imports in `geometry-wasm-loader.js:23` and `vectorize-worker.js:18`.
The four unsupported globals are computed `window[...]` accesses. These diagnostics are
retained as reported findings; none is fixed or excepted by the profile expansion or this
documentation correction.

The global count is deduplicated by distinct recognized `window.SM*` property per module,
not by raw access. It also misses bare globals, non-`SM` window properties, and bare-alias
consumers (see the remaining blockers below). It is therefore a mechanical diagnostic count,
not a complete coupling inventory or an accepted debt allowance. No adapter-layer fiction
or blanket global exception is used to manufacture a passing result.

The recorded ratchet result is `{"ok": true, "baselinePathCount": 140,
"candidatePathCount": 140, "violations": [], "reductions": [], "removals": []}`. This compares
the expanded candidate to its own updated seed. The parent contains the earlier 80-file
baseline; this result does **not** compare against that parent or a protected-branch baseline.
That result supplied first-adoption evidence only. The integrated standard lane now protects
later changes with the application profile materialized from the pull request's base commit.

The original 80-file packet separately recorded two scratch size-growth failures:
`abr-import.js` grew from 278 to 528 nonblank lines (hard maximum 500), and `i18n.js` from
4241 to 4242 (exception ceiling 4241). That historical evidence covers those two retained
paths; it is not a fresh validation of every new exception. This documentation-only pass
checks artifact/source consistency and preserves the JSON bytes; it does not rerun application
tests or promote the recorded candidate results to full R05 acceptance.

## Category 2 — Rust (`src-tauri/src/**`, `geometry-wasm/src/**`)

**Excluded entirely — the checker cannot parse Rust.** `scripts/nemo/lib/boundaries.cjs`'s
`extractImports`/tokenizer targets JS `require`/`import`/dynamic-`import()` syntax only; it has
no `use`/`mod`/`pub` grammar. This is not a per-file judgment call, it is the tool's documented
scope (`README.md`'s "Limitations" section never claims JS-family coverage beyond `.cjs`/`.js`/
`.mjs`). `04_MODULARITY_POLICY.md`'s own "Enforcement" list names the real tools for this:
"rustfmt, Clippy, module visibility and Cargo metadata enforce Rust boundaries" — a separate
toolchain, not `boundaries.cjs`, and standing that up is out of this packet's owned paths.

16 files, listed exactly (line counts via `wc -l`, physical not nonblank — no checker-compatible
counter exists for Rust here):

| Path | Lines |
|---|---:|
| `src-tauri/src/video_decode.rs` | 2301 |
| `src-tauri/src/lib.rs` | 254 |
| `src-tauri/src/vectorize.rs` | 26 |
| `src-tauri/src/main.rs` | 5 |
| `geometry-wasm/src/engine.rs` | 4235 |
| `geometry-wasm/src/tweenmatch.rs` | 1115 |
| `geometry-wasm/src/fill.rs` | 1037 |
| `geometry-wasm/src/track.rs` | 432 |
| `geometry-wasm/src/interp.rs` | 404 |
| `geometry-wasm/src/strokemodeler.rs` | 315 |
| `geometry-wasm/src/tween.rs` | 164 |
| `geometry-wasm/src/eraser.rs` | 153 |
| `geometry-wasm/src/lib.rs` | 134 |
| `geometry-wasm/src/hit.rs` | 104 |
| `geometry-wasm/src/timeline.rs` | 88 |
| `geometry-wasm/src/shapes.rs` | 57 |

`04_MODULARITY_POLICY.md`'s size table does name a "Rust production module" profile (350/500) —
recorded here as the target ceiling for whichever tool eventually enforces it (several files
above already exceed it, most sharply `engine.rs` at 4235 and `video_decode.rs` at 2301; both
already carry their own documented single-body-of-composition rationale in `CLAUDE.md` §3/§5,
not re-litigated here). `src-tauri/{Cargo.toml,Cargo.lock,tauri.conf.json,
.tauri_private_key.pub}` are Rust/Tauri project config, not source modules; out of scope for
both this checker and the "Rust production module" profile, and the `.pub` suffix confirms the
committed key is the public half, not a secret.

## Category 3 — UI/style/bootstrap (`src/index.html`, `src/css/**`)

**Excluded entirely — neither HTML nor CSS is JS.** `boundaries.cjs` tokenizes JS import syntax;
it has no HTML tag parser and no CSS `@import` grammar. `src/index.html` (2,455 lines) is this
codebase's literal bootstrap: it is not just markup, it is the ordered list of 132 `<script>`
tags: 128 classic file tags into `src/js/**`, 2 module file tags, one inline script, and
`paper-full.min.js` (Category 1). Their ordering records startup relationships, but cannot
by itself describe every global/provider-consumer or worker edge. The checker does not parse
that HTML order. `src/css/style.css` (2,850 lines) and
`src/css/tutorial.css` (169 lines) are stylesheets; `04_MODULARITY_POLICY.md` already names a
dedicated "Stylesheet" size profile (250/350 — both files already exceed it, `style.css` by a
wide margin), reserved for whatever future dedicated tool actually parses CSS; not retrofitted
onto a JS-lexical checker here.

| Path | Lines | Note |
|---|---:|---|
| `src/index.html` | 2455 | 132 script tags; startup order is unparsed by this tool |
| `src/css/style.css` | 2850 | exceeds policy's Stylesheet hard max (350) today |
| `src/css/tutorial.css` | 169 | within policy's Stylesheet hard max |

## Category 4 — Generated / vendor / shader / translation

Per `04_MODULARITY_POLICY.md`: "Vendored/generated/minified/data assets use a separate
provenance and integrity policy; do not split a shader catalog or translation table merely to
satisfy an application-code line budget." None of the paths below get a `boundaries.cjs` module
entry; each is named with its exclusion reason so none is a silent omission.

**Vendor (third-party JS/ESM, unmodified upstream, not owned/authored here):**

| Path | Reason |
|---|---|
| `src/paper-full.min.js` | Paper.js build, minified; also unparseable (single-line minified body) |
| `src/js/delaunator.vendor.js` | vendored; excluded on provenance, independent of lexical support |
| `src/js/mp4box.all.min.js` | vendored, minified; excluded on provenance |
| `src/js/opentype.min.js` | vendored, minified; excluded on provenance |
| `src/js/ag-psd.vendor.mjs` | vendored esm.sh bundle (`ag-psd@31.0.2`), `.mjs` — outside the 143 `.js`-file count in Category 1; reached only via `psd-import-bridge.js:30`'s dynamic `import()` (§1b) |
| `src/js/node-buffer-shim.vendor.mjs` | vendored Node `Buffer` shim, `.mjs` — outside the 143 `.js`-file count; reached only via `ag-psd.vendor.mjs:2`'s static `import`, not referenced directly by any application file (§1b) |

**Generated (wasm-bindgen output from the Rust crates in Category 2, not hand-edited):**

`src/wasm/{geometry_wasm.js, geometry_wasm.d.ts, geometry_wasm_bg.wasm,
geometry_wasm_bg.wasm.d.ts, package.json}` and `src/wasm-vectorize/{vectorize_wasm.js,
vectorize_wasm.d.ts, vectorize_wasm_bg.wasm, vectorize_wasm_bg.wasm.d.ts, package.json}` — 10
files, provenance is the Rust build (`geometry-wasm`/`vectorize-wasm` crates), not a JS source
review; regenerated by the wasm build job, not hand-authored.

**Shader (WGSL, GPU pipeline source for the vello/WebGPU engine):**

`geometry-wasm/src/{simple_fx.wgsl (348 lines), blend.wgsl (182), blur.wgsl (98), matte.wgsl
(69), color_adjust.wgsl (53), vignette.wgsl (51)}` — 6 files, 801 lines total. Excluded for two
independent reasons: WGSL is not JS (same tokenizer gap as Category 2/3), and
`04_MODULARITY_POLICY.md` explicitly names "shader catalog" as an asset class this checker's
line budget is not meant to reach into.

**Translation:** `src/js/i18n.js` — covered in §1c above as part of the JS candidate profile
(it IS lexically valid JS, unlike the other three categories here), with its own distinct
exception reason rather than a blanket exclusion.

**Other binary/data assets, named for completeness, no line-count concept applies:**
`src/fonts/*.ttf` (8 third-party font files), `src/{logo-64,logo-128,favicon-32,favicon-64}.png`
(4 project image assets), `src/assets/reference-3d/{AvatarSample_D.vrm, CC0_Rigged_Arms.obj,
CC0_Rigged_Arms_Texture.png, ATTRIBUTION.md}` (4 files, already self-documented provenance via
their own `ATTRIBUTION.md`), `src/_headers` (1 file, Cloudflare static-header config, not
application source).

## Validation evidence

- **Source/artifact consistency:** direct `.js` enumeration and HTML script-attribute parsing
  reproduce Category 1's 143 total / 140 handwritten files, 104,855 handwritten nonblank
  lines, 128 classic startup file tags, 2 module file tags, 1 module worker, and 12 Labs files
  without startup tags. Reading the loader/worker source confirms who constructs the worker
  and who imports each generated WASM glue module.
- **Adopted profile bytes:** both app-js JSON artifacts remain exactly as committed in `c9ab634`,
  each declaring 140 modules and 36 exact-path size exceptions; their bytes are identical to
  each other. The recorded full-checker signal remains in §1d; the integrated lane freshly
  reruns source coverage, current size/expiry enforcement and protected-base ceiling comparison.
- **Integrated tooling profile:** every current `scripts/nemo/**/*.cjs` file is declared and
  fresh discovery rejects an unprofiled addition. Its protected-base ratchet preserves all
  prior ceilings while allowing newly reviewed modules within existing size profiles.
- **Documentation reconciliation:** the profile history remains here while the boundary README
  describes the integrated command and enforcement contract.

## Coverage limits (explicit, per task instructions)

- **No reviewed architecture profile.** `app-legacy` remains a flat provisional layer, and
  each own-basename public API remains unreviewed. R01/#897 layer assignment and the reviewed
  module/provider-consumer inventory are still required.
- **Lexical coverage is not graph coverage.** All 140 handwritten files are declared, but
  the zero import-graph findings do not establish absence of cycles, private access, or
  forbidden layer edges. Reviewed global/provider-consumer modeling is a valid route for
  current classic scripts; ESM migration is another route, not the sole prerequisite.
- **437 global-state, 2 unsupported-import, 4 unsupported-global, and 1 expected
  unprofiled-vendor-import finding remain.** Neither the flat profile nor this documentation
  treats those diagnostics as an accepted clean graph.
- **Application graph rules remain gated.** The standard boundary lane enforces exact source
  coverage, exclusion provenance and protected-base size ceilings. It does not run the flat
  legacy profile as proof of clean cycles, private imports, layers or globals; the recorded
  findings above remain unresolved evidence for the architecture review.
- **No per-file architecture sign-off.** The reviewed mechanical profile and its exact
  no-growth ceilings do not establish target-layer, public-API or dependency ownership.
- **The committed baseline is the first-adoption seed.** On later changes, the CI gate ignores
  candidate-controlled seed updates and reads the prior application profile from the protected
  base commit.

## Checker findings and remaining adoption gates

The original #948 review recorded four findings. The integrated candidate includes the parser,
resolver, source-discovery, coverage and protected-baseline corrections described below while
preserving the unresolved architecture limits explicitly.

1. **Original division-parser gap corrected in the selected candidate.** #957 (`d677989`)
   removes the division/control-header-regex failure that excluded the 60 handwritten files.
   Its remaining documented lexical cuts are still limits; 140-file profile coverage does
   not make this an AST or binding-aware scanner.
2. **Global/provider-consumer binding gaps remain.** `analyzeSource` recognizes qualified
   `window.SM*` properties, including supported literal-bracket/optional forms, but does not
   resolve bare globals or identify complete provider/consumer relationships. Examples in
   the selected source are bare `state.*` reads (`feedback-bridge.js:73`, `transplant.js:221`),
   the non-`SM` provider `window.GeometryWasm` (`geometry-wasm-loader.js:10`), and
   `window.SMAssetTree` (`asset-tree.js:40`) consumed through bare `SMAssetTree` calls
   (`transplant.js:147–154`). Detecting a nearby qualified guard does not model those bare
   calls or prove the dependency's direction. A reviewed global/provider-consumer graph can
   address this without requiring ESM conversion; it is not implemented by this profile.
3. **Literal local resolution is integrated; runtime relationships remain open.** The checker
   now resolves literal ESM query/fragment URLs and Node `require` targets and fails unresolved,
   unsupported or unprofiled local dependencies explicitly. The application's actual computed
   WASM imports, worker construction, classic global providers/consumers and HTML readiness
   order still require parsed inventory and runtime evidence.
4. **Protected source-coverage and size adoption is integrated.** The standard boundary lane
   independently discovers application sources, validates exact exclusion provenance, and
   compares candidate ceilings with the application profile from the protected base commit.

These findings preserve the remaining target-layer, public-API, global-modeling and runtime
readiness gates. Source coverage and size enforcement are adopted; they do not establish a
clean application graph or product acceptance.
