# Application-surface source classification (R05 adoption increment)

Status: **coverage classification + one candidate legacy-size profile**, not an R05-closing
adoption and not a claim that any application file has been architecturally reviewed. Produced
against `scripts/nemo/lib/boundaries.cjs`/`boundaries-ratchet.cjs` as they exist on `origin/main`
(commit `35f0f5f`, the merge of #943), without modifying that checker, `scripts/nemo/lib/jobs.cjs`,
`package.json`, or `engineering/inventory/**` (R03's generated inventory, PR #944, still open —
used here only as read-only context, never edited, and never waited on: see "Relationship to
R03 #944" below).

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
owner" field. Reviewer of this classification: the agent session that authored it, pending
maintainer/integration review (Codexitron, per this packet's task thread).

## Relationship to R01/#897 and R03/#944

`README.md`'s "Integration contract for R01/R03 (pending)" states application-wide adoption
needs R01 (real domain/application/features/ports/adapters/shared layer assignment per file)
and R03 (a reviewed module inventory with real public APIs). Neither has landed for
application source: R01/#897 has not produced a layer assignment for `src/js/**`, and R03's
first reviewable increment (#944, open, unmerged) inventories **UI surfaces** (DOM controls,
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

143 `.js` files (105 at `src/js/*.js`, 38 at `src/js/labs/*.js`), counted by `find src/js -name
'*.js' | wc -l`. **140 of these 143** load as classic global `<script>` tags (of the 132
`<script>` tags in `src/index.html`, 130 point at a `src/js/**` file; the other 2 are an inline
script and `paper-full.min.js`) — confirmed by `grep -l "^import \|require(" src/js/*.js
src/js/labs/*.js` matching only one file (`opentype.min.js`, vendor, itself excluded below) for
*lexical* `import`/`require` syntax, and by `04_MODULARITY_POLICY.md`'s own "Legacy migration"
step 4 ("Migrate from classic global script order toward ESM..."), which is still open work, not
done. **The other 3 are an execution-mode exception the lexical grep above cannot see**:
`geometry-wasm-loader.js` and `vectorize-wasm-loader.js` load via `<script type="module"
src="js/...">` (`src/index.html:2429–2430`), and `vectorize-worker.js` is not a `<script>` tag at
all — `vectorize-wasm-loader.js:29` spawns it as a module Web Worker (`new Worker(new
URL('vectorize-worker.js', import.meta.url), { type: 'module' })`). Neither loader matches the
grep above because both reach the network via a dynamic `import()` call inside module scope, not
a static top-of-file `import` statement — lexical import/require syntax and script execution mode
are two different axes, and "every application file uses classic `<script>` tags" is the
inaccurate conflation of the two this packet is correcting here. This has a direct, measured
consequence for every rule below.

### 1a. Lexically unsupported — 63 of 143 files, excluded from the candidate profile

Running the checker's own `extractImports` (unmodified, read-only) against every file found
**63 files (44%) throw the documented "ambiguous slash after `)`" tokenizer failure**
(`scripts/nemo/lib/boundaries.cjs`'s literal v1 scope cut, same failure mode already recorded
for `scripts/nemo/lib/receipt.cjs` in `scripts-nemo.md`'s "Coverage limits") — a run-level
failure (exit 2 per `README.md`), not a per-file violation, so any file in this list **must** be
left out of `modules[].files` entirely or the checker cannot run at all:

- `src/js/app.js`, `audio-bridge.js`, `bitmap-brush.js`, `bpm-grid.js`, `brush-editor.js`,
  `brush-menu-bridge.js`, `camera.js`, `color-picker.js`, `draw-bridge.js`, `effects-panel.js`,
  `engine-bridge.js`, `eraser-bridge.js`, `export.js`, `expr-bake.js`, `figma-import.js`,
  `gradient-bridge.js`, `image-mesh.js`, `images.js`, `layer-inout.js`, `lipsync.js`,
  `lottie-preview.js`, `markers.js`, `motion-graph.js`, `motion.js`, `native-video-bridge.js`,
  `path-fx.js`, `project.js`, `reference-bridge.js`, `render-manager.js`, `rig-bridge.js`,
  `rig-widget.js`, `rulers-bridge.js`, `second-viewer.js`, `select-bridge.js`, `shape-bridge.js`,
  `shapes-panel.js`, `storyboard.js`, `stroke-modeler.js`, `symmetry-bridge.js`,
  `text-animator.js`, `text-selector.js`, `timeline-zoom.js`, `timeline.js`, `tools.js`,
  `tracker-panel.js`, `tracker.js`, `tweens.js`, `ui.js`, `vector-text-bridge.js`,
  `vectorize-bridge.js`, `viewtools-bridge.js`
- `src/js/labs/canvas-grid.js`, `french-curve.js`, `predictive-stroke.js`, `reference-3d.js`,
  `rig-deform.js`, `speed-lines.js`, `storyboard-mode.js`, `timelapse.js`, `vector-sculpt.js`
- `src/js/delaunator.vendor.js`, `mp4box.all.min.js`, `opentype.min.js` (also vendor — see
  Category 4; excluded twice-over)

**This is the concrete checker gap this packet reports to root**: the excluded 63 files carry
**80,688 of the tree's 105,591 nonblank lines (76%)** — including the six largest and most
central application files (`motion.js` 13,897 lines, `timeline.js` 11,870, `tools.js` 8,618,
`app.js` 5,401, `tweens.js` 5,270, `engine-bridge.js` 4,670). A profile built only from the
files the checker can currently parse covers **less than a quarter of application JS by volume**
and misses the files most likely to carry real cycles or forbidden edges if this codebase ever
adopts ESM.

**Two populations are in play here and should not be read as one**: the 63/80,688/105,591 figures
above count all 143 `.js` files, including the 3 vendored files already named for exclusion on
provenance grounds in Category 4 (`delaunator.vendor.js`, `mp4box.all.min.js`, `opentype.min.js`
— 736 nonblank lines combined, `wc -l` per file after stripping blanks). Restricting to the
**140 handwritten files** this packet actually owns review of, the same gap is **60 files /
79,952 of 104,855 nonblank lines — still 76%** omitted. The vendor files were never candidates
for the lexical parse regardless of the tokenizer bug (Category 4 excludes them on provenance,
not on parse failure), so their presence in the all-`.js` count above should not be read as the
tokenizer gap being larger than it is; both figures are reported so neither population is
implied by the other.

Closing this gap requires either an AST-based inventory (the checker's own
`README.md` already says so — "Use a parsed R03 inventory before broader adoption") or
extending the tokenizer past its documented v1 cut; both are checker-implementation changes
explicitly out of this packet's owned paths (`engineering/boundaries/**`, not
`scripts/nemo/**`). Reproduce (both directories, 63 lines of output): `node -e "const
b=require('./scripts/nemo/lib/boundaries.cjs'); const fs=require('fs'); for (const d of
['src/js','src/js/labs']) fs.readdirSync(d).filter(f=>f.endsWith('.js')).forEach(f=>{try{
b.extractImports(fs.readFileSync(d+'/'+f,'utf8'))}catch(e){console.log(d+'/'+f, e.message)}})"`.

### 1b. Candidate profile — 80 files, [`app-js.profile.json`](./app-js.profile.json)

The remaining 80 files (24,903 nonblank lines) parse without error. For these, real
`require`/`import` edges are near-zero — `extractImports` finds **exactly one** local edge in
the entire 80-file set (`src/js/psd-import-bridge.js:30`, a dynamic `import('./ag-psd.vendor.mjs')`
to an un-declared vendor `.mjs` file, correctly left as an unresolved relative reference per
the checker's own documented scope). That target is itself a two-hop chain, not a leaf: the
esm.sh-bundled `ag-psd.vendor.mjs` has its own static import at line 2,
`import { Buffer as __Buffer$ } from "./node-buffer-shim.vendor.mjs"` — so the real edge is
`psd-import-bridge.js:30 → ag-psd.vendor.mjs → node-buffer-shim.vendor.mjs`. Both targets are
`.mjs`, not `.js`, so neither was ever inside the 143-file population §1/§1a count — they are
named explicitly in Category 4 below so this chain is not a silent omission. This is not a
modeling gap this packet introduced — it is
the accurate, mechanical shape of an app built on global `<script>` load order: **cycle,
private-import and layer-violation are structurally vacuous here because there is close to no
static import graph to violate**, not because the code is clean of coupling (CLAUDE.md's own
`window.SM*` bridge pattern is exactly the real, unmodeled coupling — see 1c).

Given that, and given R01 has not assigned real layers to application files, `app-js.profile.json`
declares:

- **One flat, honestly-named layer, `app-legacy`**, `allowedLayers: ["app-legacy"]` (permissive
  only to itself) — not a claim that these 80 files share one real architectural layer, a
  disclosure that no real layer classification exists yet for them.
- **`publicApi` = the file's own basename** for every module — not a claim that every export is
  intentionally public, a disclosure that no per-file API review has happened, so nothing is
  asserted private (asserting privacy without review would fabricate `private-import` findings
  no one has verified).
- **One size profile, `"App JS unclassified (pre-R01)"` (warn 350 / hard max 500)** — the more
  permissive of `04_MODULARITY_POLICY.md`'s two applicable JS rows ("Feature UI JS or TS"),
  chosen because these files are predominantly UI-facing (`*-panel.js`, `*-bridge.js` naming
  throughout), used uniformly rather than guessing per-file which of "Domain/application" or
  "Feature UI" each belongs to — that split is R01's job.
- **11 exact-path size exceptions** (owner `ivg-design`, issue `901`, expiry `2026-12-05` as a
  review checkpoint, not a remediation deadline) for the 11 files already over 500 nonblank
  lines, each ceiling set to the file's exact current count — the "exact-path legacy no-growth
  ceiling" the task names: `feedback-bridge.js` (581), `group-bridge.js` (842), `i18n.js`
  (4241 — see 1c below for why this one gets a distinct reason), `linked-media.js` (748),
  `media-library.js` (884), `nemo-script.js` (667), `rive-export.js` (963),
  `shader-effects-library.js` (1312), `subselect-bridge.js` (539), `tutorial.js` (1864),
  `labs/labs-float-panel.js` (607). None of these ceilings permit growth beyond today's
  measured count; they only prevent an immediate false failure on files nobody has reviewed yet.

[`app-js.baseline.json`](./app-js.baseline.json) is a byte-identical copy of the candidate,
checked in as the first ratchet checkpoint (no prior reviewed application-JS profile exists to
diff against) — same convention as `scripts-nemo.baseline.json`.

### 1c. `i18n.js` — translation table, not logic (4,241 nonblank lines)

`src/js/i18n.js` is the one file in this set whose bulk is data, not control flow — a
translation table embedded in hand-written JS (there is no separate locale-data format in this
codebase; confirmed `find . -iname '*locale*' -o -iname '*i18n*'` matches only this one file).
`04_MODULARITY_POLICY.md`'s Exceptions section says explicitly: "do not split a shader catalog
or translation table merely to satisfy an application-code line budget." Its exception above
carries that distinct reason rather than the generic "legacy pre-R01" text used for the other
ten, so a future reviewer does not mistake a translation table for an oversized logic file that
needs decomposing.

### 1d. Real signal the candidate profile already finds

Running `node scripts/nemo/boundaries.cjs engineering/boundaries/profiles/app-js.profile.json
--json` against the real, unmodified tree (validated on this branch): **exit 1, 80 modules, 0
cycle / 0 private-import / 0 layer-violation (vacuous per 1b) / 0 size violations (11 ceilings
absorb today's count) / 7 size warnings** (files between 350–500 lines) **/ 2 `unsupported-import`
violations** (`geometry-wasm-loader.js:23`, `vectorize-worker.js:18` — both a non-literal
`import()`/`require()` target, reported per-rule rather than crashing the run) **/ 161
`global-state` violations across 71 of the 80 modules** (one diagnostic per **distinct**
`window.SM*` global accessed per module, not one per raw access — verified directly:
`ae-camera-export.js` has 5 raw `window\.SM[A-Za-z]` occurrences across 3 distinct globals
(`SMKitsu`, `SMCamera`, `SMExport`) and the checker reports exactly 3 violations for it, one per
global, each pointing at that global's first accessed line. `app-legacy` is not the checker's
hardcoded `"adapters"`/`"bootstrap"` exemption — the same naming-mismatch limitation
`scripts-nemo.md` already flagged, but with real teeth here: it is not overridden by fabricating
a layer named `adapters`, since that would misrepresent every one of these files as an adapter).
**161 is a lower bound on two independent axes, not the real count or the real access volume**:
it already excludes the 63 files in §1a entirely, and even within these 80 files it is
deduplicated per distinct-global-per-module rather than counting every raw access (above) — the
six largest files in 1a (`motion.js`, `timeline.js`, `tools.js`, `app.js`, `engine-bridge.js`,
`select-bridge.js`) are exactly the ones a `grep -c "window\.SM[A-Za-z]"` shows access it most
(117, 101, 53, and more occurrences respectively) and none of them are visible to this profile.
161 should therefore be read as "at least 71 modules touch at least one undeclared global each,"
not as an inventory of every `window.SM*` access or as any kind of reviewed architectural-debt
allowance — it is a raw, mechanical diagnostic count.
This packet does not add 71 fabricated per-occurrence exceptions to force a clean run — per
`README.md`, an active exception "applies only to its exact file/rule," and manufacturing 161 of
them for intentional, undocumented-by-R01 architecture would be exactly the "blanket
grandfathering" acceptance criterion 1 forbids. The candidate is reported here as a true,
reproducible, currently-failing result — not something this packet is scoped to turn green.

**Deliberate-violation evidence** (temporary scratch copy, not committed, reproducible): copying
`src/js/` to a scratch root, appending 250 lines to `abr-import.js` (no existing exception, 278 →
528 nonblank lines) and one line to `i18n.js` (past its 4241-line exception ceiling) and
re-running against `--root <scratch>` produces exactly two new `size` violations —
`"528 nonblank lines exceeds hard maximum 500"` and `"4242 nonblank lines exceeds excepted
ceiling 4241"` — confirming both the ordinary hard-max path and the exact-path exception ceiling
actually block growth, not just the illustrative fixture in `scripts-nemo.fixture/`. The
`--baseline app-js.baseline.json` ratchet checkpoint against the real tree reports
`{"ok": true, "baselinePathCount": 80, "candidatePathCount": 80, "violations": [], "reductions":
[], "removals": []}` — self-consistent as the first checkpoint, independent of the ordinary
check's `global-state`/`unsupported-import` findings above (ratchet only compares policy
ceilings between the two profiles, not live source). **This `{"ok": true}` is a new-seed
self-comparison, not regression evidence against a protected branch**: `app-js.baseline.json`
is a file this same packet introduces — neither this candidate's own parent commit nor
`origin/main` contains a prior copy to diff against. The ratchet only starts doing its job
(catching a *later* commit silently raising a ceiling) once that later commit is checked
against this seed; it has not caught anything yet, and none is claimed here.

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
tags (130 classic, 2 `type="module"` — see Category 1) that IS the application's real
load-order dependency graph — but that graph lives in HTML attribute order, a shape this
checker's `resolveSpecifier` has no concept of. `src/css/style.css` (2,850 lines) and
`src/css/tutorial.css` (169 lines) are stylesheets; `04_MODULARITY_POLICY.md` already names a
dedicated "Stylesheet" size profile (250/350 — both files already exceed it, `style.css` by a
wide margin), reserved for whatever future dedicated tool actually parses CSS; not retrofitted
onto a JS-lexical checker here.

| Path | Lines | Note |
|---|---:|---|
| `src/index.html` | 2455 | 132 `<script>` tags = real load-order graph, unparsed by this tool |
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
| `src/js/delaunator.vendor.js` | vendored (filename says so), also in the §1a lexical-failure list |
| `src/js/mp4box.all.min.js` | vendored, minified, also in the §1a lexical-failure list |
| `src/js/opentype.min.js` | vendored, minified, also in the §1a lexical-failure list |
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

- `node scripts/nemo/boundaries.cjs engineering/boundaries/profiles/scripts-nemo.profile.json`
  and `... --baseline engineering/boundaries/profiles/scripts-nemo.baseline.json --json` —
  re-run unmodified on the rebased branch (`35f0f5f` base) to confirm the prior accepted
  increment still holds: exit 0, `23 module(s), 0 violation(s), 3 warning(s)`; ratchet
  `{"ok": true, "baselinePathCount": 23, "candidatePathCount": 23}` — unchanged from
  `scripts-nemo.md`'s own recorded evidence, confirming no regression from the rebase onto
  `#943`/`#901`'s merged correction.
- `node scripts/nemo/boundaries.cjs engineering/boundaries/profiles/app-js.profile.json --json`
  → exit 1 (real findings, not a crash) — see §1d for the full breakdown.
- `node scripts/nemo/boundaries.cjs engineering/boundaries/profiles/app-js.profile.json
  --baseline engineering/boundaries/profiles/app-js.baseline.json --json` → ratchet
  `{"ok": true, "baselinePathCount": 80, "candidatePathCount": 80, "violations": [], "reductions":
  [], "removals": []}`.
- `validateProfile` (invoked internally before any source read) accepts both
  `app-js.profile.json` and `app-js.baseline.json` without error.
- Deliberate-violation scratch test in §1d: two forced-growth cases (`abr-import.js` past
  hard max, `i18n.js` past its exact-path exception ceiling) each produce exactly the expected
  new `size` violation and nothing else changes.
- All counts above (143/63/80/24,903/80,688/105,591 lines, 140/60 handwritten-only per §1a; 16
  Rust files; 6 shaders; 10 generated; 4 vendor `.js` + 2 vendor `.mjs`) are reproducible
  directly from the commands quoted inline in each section — none are asserted without a
  command.

## Coverage limits (explicit, per task instructions)

- **This is not a reviewed architecture profile.** `app-legacy` is one honest, flat,
  non-claim layer. No file above has been assigned its real domain/application/features/ports/
  adapters/shared layer — that is R01/#897's job, still open. Treat `app-js.profile.json` as a
  size + global-state instrument only, not evidence any file's coupling has been reviewed.
- **`private-import`/`cycle`/`layer-violation` are vacuous for the 80-file candidate today**,
  not passing — there is close to no static import graph in this codebase to check (§1b). This
  will only become a meaningful check once ESM migration (`04_MODULARITY_POLICY.md`'s own
  pending "Legacy migration" step 4) gives these files real edges to model.
- **76% of application JS by volume (63 files, 80,688 lines) is entirely outside this or any
  boundaries profile** because the checker's tokenizer cannot parse it — see §1a for the exact
  list and the reproduction command. This is the single largest concrete gap this packet found;
  it is a checker-implementation limitation, not something a profile author can work around
  from `engineering/boundaries/**` alone.
- **161 real `global-state` violations are reported, not fixed or excepted** — see §1d. Fixing
  application source is explicitly out of this packet's owned paths.
- **No `package.json` script, no `scripts/nemo/lib/jobs.cjs` registration, no CI wiring** for
  either `app-js.profile.json` or the Category 2–4 classifications above — matching this
  packet's task framing ("adoption increment, not R05 closure") and `scripts-nemo.md`'s own
  precedent. The remaining adoption gate is explicit: R01 must assign real layers before
  `layer-violation`/`private-import` mean anything for application code; the 63-file lexical gap
  must close (checker change, not owned here) before any profile can claim to cover application
  JS by volume, not just by file count; and CI/`jobs.cjs` wiring for any of this remains a
  separate, later packet exactly as `scripts-nemo.md` deferred it for tooling.
- **No `CODEOWNERS`, no per-module reviewer sign-off tooling** exists in this repository —
  same fact as `scripts-nemo.md`; owner attribution above is the issue-level accountable owner,
  not a verified per-file reviewer.
- **`app-js.baseline.json`'s ratchet `{"ok": true}` is a new-seed self-comparison, not
  regression evidence against a protected branch** — see §1d. No prior commit on `origin/main`
  or on this candidate's own parent contains this baseline file; the ratchet gate starts
  protecting against silent ceiling growth only from the *next* commit checked against this
  seed onward, not from this one.

## Checker-implementation blockers (out of this packet's owned paths — not fixed here)

An independent review of this PR's head reported four `scripts/nemo/lib/boundaries.cjs` defects
beyond the already-documented "ambiguous slash" tokenizer cut (§1a). Each repro below was
independently re-read against current source before being recorded here; none is fixed in this
packet — `scripts/nemo/**` is not an owned path — and none is worked around by loosening
`app-js.profile.json`'s rules or ceilings.

1. **Parser — ordinary division after `)`/`}` is rejected, not just genuinely ambiguous slash.**
   `boundaries.cjs:163`'s "ambiguous slash" guard fires on any `/` immediately following `)` or
   `}`, which includes plain arithmetic division. Verified division-after-`)` in files already
   excluded by §1a: `app.js` (`Math.round(v*1000)/1000`), `motion.js`
   (`(next.x - prev.x) / 2`), `timeline.js` (`Math.floor((now-playClock)/frameMs)`), `tools.js`
   (`Math.atan2(dy,dx)/step`), `camera.js` (division inside its bisection loop). Declaring any
   of these files in a profile aborts the entire `checkProfile` run (uncaught throw inside the
   per-file loop, `boundaries.cjs:388`) with no JSON report at all, not a per-file violation —
   this is why §1a's 63 files cannot appear in `modules[].files` today and is the largest
   concrete reason this profile has no size/global-state signal for most of application JS by
   volume.
2. **Global/dependency binding gaps.** `analyzeSource` (`boundaries.cjs:231-238` for globals,
   `260-273`/`405-409` for imports) only recognizes the literal pattern `window.SM<Name>`: it
   misses a bare `state` global (`src/js/feedback-bridge.js:73`, `src/js/transplant.js:221`,
   both read `state.*` without a `window.` prefix), a non-`SM`-prefixed `window.*` assignment
   (`window.GeometryWasm = {...}` at `src/js/geometry-wasm-loader.js:10`), and a bare-alias
   *read* of a real `window.SM*` provider: `src/js/asset-tree.js:40` declares
   `window.SMAssetTree = {...}` and `src/js/transplant.js:145-154` is a real consumer
   (`SMAssetTree.folderGroup(...)`, `SMAssetTree.componentsLabel()`) written without the
   `window.` prefix — a genuine provider/consumer coupling invisible to the scanner in either
   direction. Any layer/dependency rule built on this scanner today would silently miss all of
   the above.
3. **Loader-resolution gaps.** `resolveSpecifier` (`boundaries.cjs:260-273`) only resolves a
   literal relative specifier that exists verbatim on disk; a cache-busted specifier (e.g.
   `import('./mod.js?v=1')`) or one pointing outside any declared module both silently resolve
   to `null` ("external, or not found") and are dropped rather than reported as unresolved. This
   document's own §1b/§4 already lean on that exact behavior to correctly treat
   `psd-import-bridge.js:30`'s dynamic import as an intentionally-undeclared vendor reference —
   the same mechanism means a genuinely forbidden cross-module import written with a
   cache-busting suffix would pass silently today.
4. **Baseline/CI adoption gap.** No CI lane currently runs `boundaries.cjs` against a
   protected-branch baseline (see the ratchet-seed caveat above and in §1d). Until one exists,
   raising both a candidate's and a baseline's ceiling together, renaming a source file out of a
   profile instead of removing it, or extending an exception's `expires` date all pass the
   ratchet comparator without any independent review gate — a process/CI gap, not something
   `engineering/boundaries/profiles/**` content can close on its own.

These are handed to the checker-owning lead as concrete, reproduced findings rather than
summarized as "the checker has limitations" — per task instructions, checker fixes remain a
separate lead action and are not represented as resolved or worked around here.
