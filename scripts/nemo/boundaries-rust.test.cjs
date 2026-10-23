'use strict';
// P12/#1014 — geometry engine module boundary through the normal gate.
//
// Analyzer unit tests (Rust `use` trees, comments/strings, inline test
// modules, cfg gates), negative controls on a synthetic crate (forbidden
// production import fails, allowed edge passes, cycle, undeclared feature,
// exported-port drift, unsupported shapes declared, exceptions with expiry),
// and the adopted policy against the real geometry-wasm crate at HEAD.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');

const ROOT = path.resolve(__dirname, '../..');
const read = (name) => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

// ---- analyzer ---------------------------------------------------------------

test('use trees: single, aliased, brace groups, nested groups, self, multi-line, pub use', () => {
  const src = `
use std::collections::HashMap;
use crate::engine::{build_bezpath, ItemIn};
use crate::{from_multipolygon, to_polygon as tp, PolygonIn};
use crate::a::{self, b::{c, d}};
pub use engine::VelloEngine;
use super::*;
use crate::multi::
    line::Thing;
use r#native_engine::{r#desktop_viewport::SurfacePort as r#Port, application::NativeApplication};
`;
  const a = R.analyzeRustSource(src);
  const paths = a.uses.map((u) => u.segments.join('::'));
  assert.deepEqual(paths, ['std::collections::HashMap', 'crate::engine::build_bezpath', 'crate::engine::ItemIn',
    'crate::from_multipolygon', 'crate::to_polygon', 'crate::PolygonIn', 'crate::a', 'crate::a::b::c', 'crate::a::b::d',
    'engine::VelloEngine', 'super::*', 'crate::multi::line::Thing',
    'native_engine::desktop_viewport::SurfacePort', 'native_engine::application::NativeApplication']);
  assert.deepEqual(a.uses.filter((u) => u.pub).map((u) => u.segments.join('::')), ['engine::VelloEngine']);
  assert.equal(a.uses[1].line, 3);
});

test('comments and string literals are not code; mod declarations and inline paths are found', () => {
  const src = `// use crate::not_real;
/* use crate::also_not; /* nested */ still comment */
const S: &str = "use crate::in_string;";
const R: &str = r#"crate::raw"#;
mod fill;
pub mod engine;
#[cfg(test)]
mod tests { use super::*; fn t() { let _ = super::helper(); } }
fn f() { crate::r#hit::r#hit_test_scene(1); let c = 'x'; }
`;
  const a = R.analyzeRustSource(src);
  assert.deepEqual(a.uses.map((u) => u.segments.join('::') + '@' + u.depth), ['super::*@1']);
  assert.deepEqual(a.mods.map((m) => m.name + (m.inlineBody ? '{}' : ';')), ['fill;', 'engine;', 'tests{}']);
  assert.deepEqual(a.inline.map((i) => i.segments.join('::') + '@' + i.depth), ['super::helper@1', 'crate::hit::hit_test_scene@0']);
  assert.deepEqual(a.cfgs.map((c) => c.expr), ['test']);
  assert.deepEqual(a.inlineMods, [{ name: 'tests', nested: false }]);
});

test('cfg predicates keep their string values (target_arch, feature) and nested inline modules are flagged', () => {
  const a = R.analyzeRustSource(`#[cfg(target_arch = "wasm32")]\nfn w() {}\n#[cfg(all(feature = "gpu", not(test)))]\nfn g() {}\nmod outer { mod inner { use super::*; } }\n`);
  assert.deepEqual(a.cfgs.map((c) => c.expr), ['target_arch = "wasm32"', 'all(feature = "gpu", not(test))']);
  assert.deepEqual(a.inlineMods, [{ name: 'outer', nested: true }]);
});

test('extern crate declarations retain canonical name, alias and line for bypass checks', () => {
  const a = R.analyzeRustSource('extern crate ext;\nextern crate native_engine as hidden;\nextern crate native_engine as r#raw_alias;\n');
  assert.deepEqual(a.externCrates, [
    { crate: 'ext', alias: null, line: 1 },
    { crate: 'native_engine', alias: 'hidden', line: 2 },
    { crate: 'native_engine', alias: 'raw_alias', line: 3 },
  ]);
});

// ---- synthetic crate ----------------------------------------------------------

const POLICY = {
  schemaVersion: 1, kind: 'rust-crate', policyId: 'nemo.rust.synth.edges', status: 'adopted',
  crate: 'synth', root: 'synth/src/lib.rs', sourceDir: 'synth/src',
  layerRules: { adapters: { allowedLayers: ['engine'] }, engine: { allowedLayers: ['engine'] } },
  exportedPort: { items: ['engine::render', 'shapes::rect'] },
  features: { declared: ['gpu'], allowedCfgs: ['test', 'target_arch = "wasm32"'] },
  exceptions: [],
};
const PROFILE = { modules: [
  { id: 'synth.api', layer: 'adapters', dir: 'synth/src', files: ['lib.rs'], publicApi: ['lib.rs'], sizeProfile: 'k' },
  { id: 'synth.engine', layer: 'engine', dir: 'synth/src', files: ['engine.rs'], publicApi: ['engine.rs'], sizeProfile: 'k' },
  { id: 'synth.shapes', layer: 'engine', dir: 'synth/src', files: ['shapes.rs', 'hit.rs'], publicApi: ['shapes.rs'], sizeProfile: 'k' },
  { id: 'synth.other.crate', layer: 'engine', dir: 'other/src', files: ['lib.rs'], publicApi: ['lib.rs'], sizeProfile: 'k' },
], sizeProfiles: { k: { warn: 1000, hardMax: 2000 } }, exceptions: [] };

function scaffold(t, files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-rust-edges-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const base = {
    'synth/src/lib.rs': 'mod engine;\nmod shapes;\nmod hit;\npub use engine::render;\npub use shapes::rect;\nuse wasm_bindgen::prelude::*;\n',
    'synth/src/engine.rs': 'use vello::Scene;\npub fn render() {}\npub struct SceneIn;\n#[cfg(test)]\nmod tests { use super::*; }\n',
    'synth/src/shapes.rs': 'use crate::engine::SceneIn;\npub fn rect() -> SceneIn { SceneIn }\n',
    'synth/src/hit.rs': 'use crate::engine::SceneIn;\npub fn hit(_s: &SceneIn) {}\n',
    'other/src/lib.rs': 'pub fn x() {}\n',
  };
  for (const [rel, text] of Object.entries({ ...base, ...files })) {
    const abs = path.join(root, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, text);
  }
  return root;
}
const run = (root, policy = POLICY, profile = PROFILE) => R.checkRustCrate(profile, policy, { root, now: new Date('2026-09-13T12:00:00Z') });

test('allowed edges pass: api → engine/shapes, shapes → engine; the other crate is outside the graph', (t) => {
  const r = run(scaffold(t, {}));
  assert.equal(r.ok, true, JSON.stringify(r.violations));
  assert.equal(r.moduleCount, 3);
  assert.deepEqual(r.edges.map((e) => `${e.from}->${e.to}`).sort(), ['synth.api->synth.engine', 'synth.api->synth.shapes', 'synth.shapes->synth.engine']);
  assert.deepEqual(r.exportedPort, ['engine::render', 'shapes::rect']);
  assert.deepEqual(r.unsupported, []);
});

test('one forbidden production import fails: an engine module reaching the API root', (t) => {
  const r = run(scaffold(t, { 'synth/src/engine.rs': 'use crate::helper;\npub fn render() { helper(); }\npub struct SceneIn;\n', 'synth/src/lib.rs': 'mod engine;\nmod shapes;\nmod hit;\npub use engine::render;\npub use shapes::rect;\npub fn helper() {}\n' }));
  assert.equal(r.ok, false);
  const layer = r.violations.filter((v) => v.rule === 'layer-violation');
  assert.deepEqual(layer.map((v) => [v.module, v.file, v.line, v.detail.toLayer, v.detail.path]), [['synth.engine', 'synth/src/engine.rs', 1, 'adapters', 'crate::helper']]);
  assert.ok(r.violations.some((v) => v.rule === 'cycle'), 'and the back-edge closes a cycle with lib.rs\'s mod declaration');
});

test('an inline crate:: path counts as an edge and a module cycle is reported once', (t) => {
  const r = run(scaffold(t, { 'synth/src/engine.rs': 'pub fn render() { crate::hit::hit(&SceneIn); }\npub struct SceneIn;\n' }));
  assert.equal(r.ok, false);
  assert.deepEqual(r.violations.map((v) => v.rule), ['cycle']);
  assert.deepEqual(r.violations[0].detail.path, ['synth.engine', 'synth.shapes', 'synth.engine']);
  assert.deepEqual(r.violations[0].detail.files.sort(), ['synth/src/engine.rs', 'synth/src/hit.rs', 'synth/src/shapes.rs']);
});

test('feature configuration: a declared feature passes, an undeclared feature or cfg predicate fails', (t) => {
  const ok = run(scaffold(t, { 'synth/src/shapes.rs': '#[cfg(feature = "gpu")]\nuse crate::engine::SceneIn;\n#[cfg(not(feature = "gpu"))]\npub struct SceneIn;\npub fn rect() -> SceneIn { SceneIn }\n' }));
  assert.equal(ok.ok, true, JSON.stringify(ok.violations));
  const bad = run(scaffold(t, { 'synth/src/shapes.rs': '#[cfg(feature = "cpu")]\nuse crate::engine::SceneIn;\n#[cfg(unix)]\npub fn rect() -> SceneIn { SceneIn }\n' }));
  assert.equal(bad.ok, false);
  assert.deepEqual(bad.violations.map((v) => [v.rule, v.line, v.detail.feature || v.detail.cfg]), [['undeclared-feature', 1, 'cpu'], ['undeclared-feature', 3, 'unix']]);
});

test('exported-port drift fails with the exact missing/extra re-exports', (t) => {
  const r = run(scaffold(t, { 'synth/src/lib.rs': 'mod engine;\nmod shapes;\nmod hit;\npub use engine::render;\npub use hit::hit;\n' }));
  assert.equal(r.ok, false);
  const v = r.violations.find((x) => x.rule === 'exported-port');
  assert.deepEqual(v.detail, { missing: ['shapes::rect'], extra: ['hit::hit'] });
});

test('unsupported module shapes are declared, not silently accepted', (t) => {
  const profile = structuredClone(PROFILE);
  profile.modules[2].files.push('deep/inner/leaf.rs');
  const r = run(scaffold(t, { 'synth/src/deep/inner/leaf.rs': 'use super::super::engine::SceneIn;\n' }), POLICY, profile);
  assert.equal(r.ok, false);
  assert.deepEqual(r.violations.map((v) => [v.rule, v.file]), [['unsupported', 'synth/src/deep/inner/leaf.rs']]);
  assert.match(r.violations[0].message, /not a supported module shape/);
});

test('super:: from a Rust-2018 x/y.rs shape resolves to a declared sibling x.rs, not x/mod.rs', (t) => {
  const profile = structuredClone(PROFILE);
  profile.modules.push({ id: 'synth.shapes.more', layer: 'engine', dir: 'synth/src/shapes', files: ['more.rs'], publicApi: ['more.rs'], sizeProfile: 'k' });
  const r = run(scaffold(t, { 'synth/src/shapes/more.rs': 'use super::*;\n' }), POLICY, profile);
  assert.equal(r.ok, true, JSON.stringify(r.violations));
  assert.deepEqual(r.unsupported, []);
  assert.ok(r.edges.some((e) => e.from === 'synth.shapes.more' && e.to === 'synth.shapes'));
});

const EXT_POLICY = { ...POLICY, externalCratePorts: { ext: { allowedModules: ['synth.shapes'], items: ['registry', 'wire'] } } };
const PER_MODULE_EXT_POLICY = { ...POLICY, externalCratePorts: { ext: { moduleItems: {
  'synth.shapes': ['registry'],
  'synth.engine': ['wire'],
} } } };

test('external-crate ports: allowed module + declared items pass, a disallowed module or undeclared item fails, a declared item covers deeper paths', (t) => {
  const ok = run(scaffold(t, { 'synth/src/shapes.rs': 'use crate::engine::SceneIn;\nuse ext::registry;\nuse ext::registry::Endpoint;\nuse ext::wire;\npub fn rect() -> SceneIn { SceneIn }\n' }), EXT_POLICY);
  assert.equal(ok.ok, true, JSON.stringify(ok.violations));

  const disallowedModule = run(scaffold(t, { 'synth/src/lib.rs': 'mod engine;\nmod shapes;\nmod hit;\npub use engine::render;\npub use shapes::rect;\nuse ext::registry;\n' }), EXT_POLICY);
  assert.equal(disallowedModule.ok, false);
  assert.deepEqual(disallowedModule.violations.map((v) => [v.rule, v.module]), [['external-crate-violation', 'synth.api']]);

  const privateUse = run(scaffold(t, { 'synth/src/shapes.rs': 'use crate::engine::SceneIn;\nuse ext::server::run;\npub fn rect() -> SceneIn { SceneIn }\n' }), EXT_POLICY);
  assert.equal(privateUse.ok, false);
  assert.deepEqual(privateUse.violations.map((v) => v.rule), ['private-port-access']);

  const privateInline = run(scaffold(t, { 'synth/src/hit.rs': 'use crate::engine::SceneIn;\npub fn hit(_s: &SceneIn) { let _ = ext::server::run(); }\n' }), EXT_POLICY);
  assert.equal(privateInline.ok, false);
  assert.deepEqual(privateInline.violations.map((v) => [v.rule, v.detail.path]), [['private-port-access', 'server::run']]);
});

test('per-module external ports reject an absent module and undeclared item without widening another module', (t) => {
  const ok = run(scaffold(t, {
    'synth/src/engine.rs': 'use ext::wire;\npub fn render() {}\npub struct SceneIn;\n',
    'synth/src/shapes.rs': 'use crate::engine::SceneIn;\nuse ext::registry::Endpoint;\npub fn rect() -> SceneIn { SceneIn }\n',
  }), PER_MODULE_EXT_POLICY);
  assert.equal(ok.ok, true, JSON.stringify(ok.violations));

  const absent = run(scaffold(t, {
    'synth/src/lib.rs': 'mod engine;\nmod shapes;\nmod hit;\npub use engine::render;\npub use shapes::rect;\nuse ext::registry;\n',
  }), PER_MODULE_EXT_POLICY);
  assert.deepEqual(absent.violations.map((v) => [v.rule, v.module]), [['external-crate-violation', 'synth.api']]);

  const privateUse = run(scaffold(t, {
    'synth/src/engine.rs': 'use ext::registry;\npub fn render() {}\npub struct SceneIn;\n',
  }), PER_MODULE_EXT_POLICY);
  assert.deepEqual(privateUse.violations.map((v) => [v.rule, v.detail.path]), [['private-port-access', 'registry']]);
});

test('governed extern-crate aliases are rejected instead of bypassing the port', (t) => {
  const r = run(scaffold(t, {
    'synth/src/shapes.rs': 'extern crate ext as hidden;\nuse crate::engine::SceneIn;\nuse hidden::registry;\npub fn rect() -> SceneIn { SceneIn }\n',
  }), PER_MODULE_EXT_POLICY);
  assert.equal(r.ok, false);
  assert.ok(r.violations.some((v) => v.rule === 'external-crate-violation'
    && v.detail.crate === 'ext' && v.detail.alias === 'hidden'));
});

test('unanalyzedModules must name real profile modules, and the ids are echoed back', (t) => {
  const root = scaffold(t, {});
  assert.throws(() => run(root, { ...POLICY, unanalyzedModules: ['synth.nonexistent'] }), /unanalyzedModules references unknown module "synth\.nonexistent"/);
  const r = run(root, { ...POLICY, unanalyzedModules: ['synth.other.crate'] });
  assert.equal(r.ok, true, JSON.stringify(r.violations));
  assert.deepEqual(r.unanalyzedModules, ['synth.other.crate']);
});

test('exceptions waive a named file+rule until they expire, then fail loudly', (t) => {
  const root = scaffold(t, { 'synth/src/engine.rs': 'pub fn render() { crate::hit::hit(&SceneIn); }\npub struct SceneIn;\n' });
  const waived = run(root, { ...POLICY, exceptions: [{ path: 'synth/src/engine.rs', rule: 'cycle', owner: 'o', issue: '1014', reason: 'known', expires: '2026-12-05' }] });
  assert.equal(waived.ok, true, JSON.stringify(waived.violations));
  assert.equal(waived.exceptionsApplied.length, 1);
  const expired = run(root, { ...POLICY, exceptions: [{ path: 'synth/src/engine.rs', rule: 'cycle', owner: 'o', issue: '1014', reason: 'known', expires: '2026-01-01' }] });
  assert.equal(expired.ok, false);
  assert.deepEqual(expired.violations.map((v) => v.rule).sort(), ['cycle', 'expired-exception']);
});

test('the policy is validated and the module set must match', (t) => {
  assert.throws(() => R.validateCratePolicy({ ...POLICY, kind: 'js' }), /kind/);
  assert.throws(() => R.validateCratePolicy({ ...POLICY, status: 'draft' }), /adopted/);
  assert.throws(() => R.validateCratePolicy({ ...POLICY, exceptions: [{ path: 'x', rule: 'size', owner: 'o', issue: '1', reason: 'r', expires: '2026-12-05' }] }), /not a crate rule/);
  assert.throws(() => run(scaffold(t, {}), { ...POLICY, profileModules: ['synth.api'] }), /policy declares synth\.api/);
  assert.throws(() => R.validateCratePolicy({ ...POLICY, externalCratePorts: { ext: {} } }), /exactly one/);
  assert.throws(() => R.validateCratePolicy({ ...POLICY, externalCratePorts: { ext: {
    allowedModules: ['synth.shapes'], items: ['registry'], moduleItems: { 'synth.shapes': ['registry'] },
  } } }), /exactly one/);
  assert.throws(() => run(scaffold(t, {}), { ...PER_MODULE_EXT_POLICY,
    externalCratePorts: { ext: { moduleItems: { 'synth.missing': ['registry'] } } },
  }), /references unknown module "synth\.missing"/);
  assert.throws(() => run(scaffold(t, {}), { ...PER_MODULE_EXT_POLICY,
    externalCratePorts: { ext: { moduleItems: { 'synth.other.crate': ['registry'] } } },
  }), /outside crate "synth"/);
});

// ---- the real crate ----------------------------------------------------------

function desktopFixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-desktop-edges-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'src-tauri'), { recursive: true });
  fs.cpSync(path.join(ROOT, 'src-tauri/src'), path.join(root, 'src-tauri/src'), { recursive: true });
  return root;
}

function appendSource(root, file, source) {
  fs.appendFileSync(path.join(root, file), `\n${source}\n`);
}

test('adopted geometry-wasm policy holds at HEAD: no violation, exactly the three recorded-debt exceptions, port matches lib.rs', () => {
  const policy = read('geometry-wasm.edges.json');
  const r = R.checkRustCrate(read('rust.profile.json'), policy, { root: ROOT });
  assert.equal(r.ok, true, JSON.stringify(r.violations));
  assert.equal(r.moduleCount, policy.profileModules.length);
  assert.deepEqual(r.exceptionsApplied.map((e) => `${path.basename(e.path)}:${e.rule}`).sort(),
    ['engine.rs:cycle', 'eraser.rs:cycle', 'eraser.rs:layer-violation']);
  assert.deepEqual(r.unsupported, []);
  // Named against the declared set, never pinned to a count. `=== 18` meant that
  // two branches each adding a `pub use` re-export both wrote `19`, and git
  // merged the line with no conflict — the counter class of #1316. It detected
  // nothing either: `ok` above already refuses a missing OR an extra re-export
  // (rule `exported-port`, mutation-covered by 'exported-port drift fails with
  // the exact missing/extra re-exports'), so the number only ever fired on the
  // legitimate case of adding an export and declaring it.
  assert.deepEqual(r.exportedPort.slice().sort(), policy.exportedPort.items.slice().sort());
  assert.ok(r.exportedPort.length > 0, 'the crate exports a non-empty port');
  assert.deepEqual(r.edges.map((e) => `${e.from}->${e.to}`).sort(), [
    'rust.geometry.api->rust.geometry.animation', 'rust.geometry.api->rust.geometry.engine', 'rust.geometry.api->rust.geometry.shapes',
    'rust.geometry.engine->rust.geometry.shapes', 'rust.geometry.shapes->rust.geometry.api', 'rust.geometry.shapes->rust.geometry.engine']);
  // No [features] table in Cargo.toml → the declared set is empty, and the crate uses only the two allowed predicates.
  assert.equal(/^\[features\]/m.test(fs.readFileSync(path.join(ROOT, 'geometry-wasm/Cargo.toml'), 'utf8')), false);
});

test('P37C1C4A CPU payload children stay private within revision and read-query layers', () => {
  const profile = read('rust.profile.json'), policy = read('native-engine.edges.json');
  const owner = (id) => profile.modules.find((entry) => entry.id === `rust.native.engine.${id}`);
  for (const [id, file] of [
    ['revision', 'image_resource_budget.rs'],
    ['read-queries', 'reference_image_resources.rs'],
  ]) {
    assert.ok(owner(id).files.includes(file));
    assert.equal(owner(id).publicApi.includes(file), false);
  }
  const result = R.checkRustCrate(profile, policy, { root: ROOT });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.deepEqual(result.unsupported, []);
  const pairs = new Set(result.edges.map((edge) => `${edge.from}->${edge.to}`));
  for (const forbidden of [
    'rust.native.engine.revision->rust.native.engine.codec',
    'rust.native.engine.read-queries->rust.native.engine.history',
    'rust.native.engine.codec->rust.native.engine.revision',
  ]) assert.equal(pairs.has(forbidden), false, `${forbidden} must stay denied`);
});


const EXPECTED_DESKTOP_PORT_BORROW_VIOLATIONS = [
  ['rust.desktop.native.application', 'commands::DispatchErrorCode'],
  ['rust.desktop.native.application', 'desktop_viewport::SurfacePort'],
  ['rust.desktop.native.application.tests', 'commands::DispatchErrorCode'],
  ['rust.desktop.native.application.tests', 'desktop_viewport::CssPoint'],
  ['rust.desktop.native.viewport', 'codec::decode_project'],
  ['rust.desktop.native.viewport', 'commands::DispatchErrorCode'],
];
test('desktop native-engine grants are module-local: host, host tests and viewport cannot borrow another module item', (t) => {
  const root = desktopFixture(t);
  appendSource(root, 'src-tauri/src/native_application.rs', 'use native_engine::desktop_viewport::SurfacePort;\nuse native_engine::commands::DispatchErrorCode;');
  appendSource(root, 'src-tauri/src/native_application_tests.rs', 'use native_engine::desktop_viewport::CssPoint;\nuse native_engine::commands::DispatchErrorCode;');
  appendSource(root, 'src-tauri/src/native_viewport.rs', 'use native_engine::codec::decode_project;\nuse native_engine::commands::DispatchErrorCode;');
  const policy = read('nemo-desktop.edges.json');
  const r = R.checkRustCrate(read('rust.profile.json'), policy, { root });
  assert.equal(r.ok, false);
  assert.deepEqual(r.violations.filter((v) => v.rule === 'private-port-access')
    .map((v) => [v.module, v.detail.path]).sort(), EXPECTED_DESKTOP_PORT_BORROW_VIOLATIONS);
});

test('raw identifiers cannot bypass desktop native-engine ports in the host, focused tests or viewport', (t) => {
  const root = desktopFixture(t);
  appendSource(root, 'src-tauri/src/native_application.rs', 'use r#native_engine::r#desktop_viewport::SurfacePort;\nuse r#native_engine::r#commands::r#DispatchErrorCode;');
  appendSource(root, 'src-tauri/src/native_application_tests.rs', 'use native_engine::r#desktop_viewport::r#CssPoint;\nuse r#native_engine::r#commands::r#DispatchErrorCode;');
  appendSource(root, 'src-tauri/src/native_viewport.rs',
    'fn raw_port_probe() { let _ = r#native_engine::r#codec::decode_project; let _: Option<r#native_engine::r#commands::r#DispatchErrorCode> = None; }');
  const r = R.checkRustCrate(read('rust.profile.json'), read('nemo-desktop.edges.json'), { root });
  assert.equal(r.ok, false);
  assert.deepEqual(r.violations.filter((v) => v.rule === 'private-port-access')
    .map((v) => [v.module, v.detail.path]).sort(), EXPECTED_DESKTOP_PORT_BORROW_VIOLATIONS);
});

test('desktop policy rejects a native_engine extern-crate alias before it can hide item access', (t) => {
  const root = desktopFixture(t);
  appendSource(root, 'src-tauri/src/native_application.rs', 'extern crate native_engine as hidden_engine;');
  const r = R.checkRustCrate(read('rust.profile.json'), read('nemo-desktop.edges.json'), { root });
  assert.equal(r.ok, false);
  assert.ok(r.violations.some((v) => v.rule === 'external-crate-violation'
    && v.module === 'rust.desktop.native.application'
    && v.detail.crate === 'native_engine' && v.detail.alias === 'hidden_engine'));
});


test('N25C2a/C2d private host registration and exact lifecycle grants fail closed', (t) => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const id = 'rust.desktop.native.host.release.tests', seam = 'rust.desktop.native.dispatch', file = 'src-tauri/src/native_application_dispatch_release_tests.rs';
  const module = profile.modules.find((m) => m.id === id);
  assert.deepEqual(module, { id, layer: 'host-release-tests', dir: 'src-tauri/src',
    files: [path.basename(file), 'native_object_host_tests.rs', 'native_media_host_tests.rs', 'native_object_bootstrap_tests.rs', 'native_object_host_read_tests.rs', 'native_object_host_command_tests.rs', 'native_object_host_history_tests.rs', 'native_object_client_tests.rs', 'native_object_timing_tests.rs', 'native_object_client_admission_tests.rs', 'native_object_serialization_tests.rs', 'native_object_reopen_tests.rs', 'native_object_project_replace_tests.rs', 'native_object_new_project_tests.rs', 'native_object_save_tests.rs', 'native_object_save_failure_tests.rs', 'native_object_v2_persistence_tests.rs', 'native_object_v2_refusal_tests.rs', 'native_object_preview_tests.rs', 'native_object_motion_tests.rs', 'native_object_motion_finalize_tests.rs', 'native_object_motion_finalize_diagnostics_tests.rs', 'native_object_exposure_tests.rs', 'native_object_preview_lifecycle_tests.rs', 'native_object_preview_route_tests.rs', 'native_object_preview_route_test_support.rs', 'native_object_desktop_preview_tests.rs', 'native_object_desktop_preview_test_support.rs', 'native_object_export_tests.rs', 'native_object_desktop_export_tests.rs', 'native_object_desktop_sequence_tests.rs', 'native_object_desktop_sequence_undo_tests.rs', 'native_object_svg_export_tests.rs', 'native_object_svg_desktop_export_tests.rs', 'native_object_desktop_preview_extent_tests.rs', 'native_object_desktop_preview_camera_return_tests.rs', 'native_object_desktop_export_extent_tests.rs', 'native_object_desktop_export_text_tests.rs', 'native_object_project_tests.rs', 'native_object_project_timing_tests.rs', 'native_object_reference_tests.rs', 'native_object_reference_preview_tests.rs', 'native_object_inactive_project_tests.rs', 'native_object_retained_tabs_tests.rs', 'native_project_settings_public_tests.rs', 'native_property_metadata_dispatch_tests.rs', 'native_property_track_dispatch_tests.rs', 'native_layer_policy_dispatch_tests.rs', 'native_property_read_dispatch_tests.rs', 'native_text_read_dispatch_tests.rs', 'native_property_edit_dispatch_tests.rs', 'native_property_bake_dispatch_tests.rs', 'native_image_mesh_binding_public_tests.rs', 'native_composition_dispatch_tests.rs', 'native_raster_image_dispatch_tests.rs', 'native_raster_image_preview_tests.rs', 'native_asset_remove_dispatch_tests.rs', 'native_frame_clipboard_transport_tests.rs', 'native_project_placement_public_tests.rs', 'native_project_placement_audio_fixture.rs', 'native_layer_playhead_cut_public_tests.rs', 'native_audio_track_public_tests.rs', 'native_shadow_dispatch_tests.rs', 'native_perspective_dispatch_tests.rs', 'native_perspective_read_dispatch_tests.rs', 'native_workspace_dispatch_tests.rs', 'native_custom_brush_dispatch_tests.rs', 'native_comments_dispatch_tests.rs', 'native_object_brush_preview_tests.rs', 'native_import_batch_dispatch_tests.rs'], publicApi: [], sizeProfile: 'Rust production module' });
  assert.equal(profile.sizeProfiles[module.sizeProfile].hardMax, 500);
  assert.deepEqual(policy.layerRules['host-release-tests'].allowedLayers, ['bootstrap', 'host-contract', 'host-seam', 'mcp-adapter', 'command-adapter', 'viewport-owner', 'host-ports', 'desktop-shell']);
  assert.equal(policy.layerRules.tests.allowedLayers.includes('host-seam'), false);
  for (const [owner, items] of [[seam, ['application::ApplicationReleaseReceipt', 'export_job::ReconciliationStage', 'resource_leases::WorkId',
      'history::NativeObjectHistory', 'object_document::ObjectDocument', 'object_snapshot::ObjectSnapshot', 'export_job::ExportReleaseReconciliation', 'commands::DispatchErrorCode']],
    ['rust.desktop.mcp.adapter', ['object_document::ObjectDocument']],
    [id, policy.externalCratePorts.native_engine.moduleItems[id]]]) for (const item of items) {
    const denied = structuredClone(policy);
    denied.externalCratePorts.native_engine.moduleItems[owner] = denied.externalCratePorts.native_engine.moduleItems[owner].filter((x) => x !== item);
    assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some((v) =>
      v.rule === 'private-port-access' && v.module === owner && v.detail.path === item), `${owner}: ${item}`);
  }
  const edgeDenied = structuredClone(policy);
  edgeDenied.layerRules['host-release-tests'].allowedLayers = edgeDenied.layerRules['host-release-tests'].allowedLayers.filter((x) => x !== 'mcp-adapter');
  assert.ok(R.checkRustCrate(profile, edgeDenied, { root: ROOT }).violations.some((v) => v.rule === 'layer-violation' && v.module === id && v.detail.targetModule === 'rust.desktop.mcp.adapter'));
  const removed = structuredClone(profile);
  removed.modules = removed.modules.filter((m) => m.id !== id);
  assert.ok(require('./lib/boundaries-coverage.cjs').checkSourceCoverage(removed, { root: ROOT, sourcePaths: [file] }).violations.some((v) => v.rule === 'coverage-unprofiled-source' && v.file === file));
  for (const source of ['native_object_host.rs', 'native_object_host_tests.rs', 'native_object_host_read_tests.rs', 'native_object_host_command_tests.rs', 'native_object_host_history_tests.rs', 'native_object_serialization_tests.rs', 'native_project_settings_public_tests.rs', 'native_asset_remove_dispatch_tests.rs']) {
    const candidate = structuredClone(profile), relative = `src-tauri/src/${source}`;
    for (const owner of candidate.modules) owner.files = owner.files.filter((x) => x !== source);
    assert.ok(require('./lib/boundaries-coverage.cjs').checkSourceCoverage(candidate, { root: ROOT, sourcePaths: [relative] }).violations.some((v) => v.rule === 'coverage-unprofiled-source' && v.file === relative));
  }
  const misclassified = structuredClone(profile);
  misclassified.modules.find((m) => m.id === id).layer = 'tests';
  assert.ok(R.checkRustCrate(misclassified, policy, { root: ROOT }).violations.some((v) =>
    v.rule === 'layer-violation' && v.module === id && v.detail.targetModule === seam));
  for (const raw of [false, true]) {
    const root = desktopFixture(t), prefix = raw ? 'r#' : '';
    appendSource(root, 'src-tauri/src/native_application_tests.rs', `use crate::${prefix}native_dispatch::NativeDispatch;
use ${prefix}native_engine::application::ApplicationReleaseReceipt;`);
    appendSource(root, file, `use crate::${prefix}application_mcp::ApplicationMcp;
use crate::${prefix}native_viewport::NativeViewport;
use ${prefix}native_engine::compositor::CompositorInstance;`);
    appendSource(root, 'src-tauri/src/native_object_host.rs', `use crate::${prefix}application_mcp::ApplicationMcp;`);
    appendSource(root, 'src-tauri/src/native_application_tests.rs', `use ${prefix}native_engine::history::NativeObjectHistory;`);
    appendSource(root, 'src-tauri/src/native_application_tests.rs', `use ${prefix}native_engine::commands::NativeOpacityApplication;`);
    const violations = R.checkRustCrate(profile, policy, { root }).violations;
    for (const target of ['rust.desktop.native.viewport']) {
      assert.ok(violations.some((v) => v.rule === 'layer-violation' && v.module === id && v.detail.targetModule === target));
    }
    assert.ok(violations.some((v) => v.rule === 'layer-violation' && v.module === seam && v.detail.targetModule === 'rust.desktop.mcp.adapter'));
    assert.ok(violations.some((v) => v.rule === 'cycle'), 'synthetic production back-edge must close and fail the MCP/host cycle');
    assert.ok(violations.some((v) => v.rule === 'private-port-access' && v.module === 'rust.desktop.native.application.tests' && v.detail.path === 'history::NativeObjectHistory'));
    assert.ok(violations.some((v) => v.rule === 'private-port-access' && v.module === 'rust.desktop.native.application.tests' && v.detail.path === 'commands::NativeOpacityApplication'));
    assert.ok(violations.some((v) => v.rule === 'layer-violation' && v.module === 'rust.desktop.native.application.tests' && v.detail.targetModule === seam));
    for (const [owner, item] of [[id, 'compositor::CompositorInstance'], ['rust.desktop.native.application.tests', 'application::ApplicationReleaseReceipt']]) {
      assert.ok(violations.some((v) => v.rule === 'private-port-access' && v.module === owner && v.detail.path === item));
    }
  }
});

test('N25F3D3B2C private preview route keeps exact registration and production-only GPU port', () => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const adapter = profile.modules.find((module) => module.id === 'rust.desktop.mcp.adapter');
  const tests = profile.modules.find((module) => module.id === 'rust.desktop.native.host.release.tests');
  assert.ok(adapter.files.includes('native_object_preview_route.rs'));
  assert.equal(adapter.publicApi.includes('native_object_preview_route.rs'), false);
  for (const file of ['native_object_preview_route_tests.rs', 'native_object_preview_route_test_support.rs']) {
    assert.ok(tests.files.includes(file));
  }
  const denied = structuredClone(policy);
  const grants = denied.externalCratePorts.native_engine.moduleItems['rust.desktop.mcp.adapter'];
  denied.externalCratePorts.native_engine.moduleItems['rust.desktop.mcp.adapter'] =
    grants.filter((item) => item !== 'compositor::Compositor');
  assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some((violation) =>
    violation.rule === 'private-port-access' && violation.module === 'rust.desktop.mcp.adapter' &&
    violation.detail.path === 'compositor::Compositor'));
});

test('N25F3D3B2D1 original viewport borrow has exact children and separate production/test ports', (t) => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const module = (id) => profile.modules.find((entry) => entry.id === id);
  assert.ok(module('rust.desktop.native.viewport').files.includes('native_viewport_appkit_binding.rs'));
  assert.ok(module('rust.desktop.native.application.viewport').files.includes('native_viewport_callbacks.rs'));
  assert.ok(module('rust.desktop.native.application.tests').files.includes('native_viewport_borrow_tests.rs'));
  const ports = policy.externalCratePorts.native_engine.moduleItems;
  assert.ok(ports['rust.desktop.native.viewport'].includes('desktop_viewport::CssPoint'));
  const testItems = ['AcquiredSurfaceFrame', 'DesktopViewportHost', 'SettledSurfaceFrame',
    'SurfaceAttempt', 'SurfacePort', 'SurfacePresentation', 'SurfaceRecoveryError', 'ViewportMapping'];
  for (const item of testItems) assert.ok(ports['rust.desktop.native.application.tests'].includes(`desktop_viewport::${item}`));
  const denied = structuredClone(policy);
  denied.externalCratePorts.native_engine.moduleItems['rust.desktop.native.viewport'] =
    ports['rust.desktop.native.viewport'].filter((item) => item !== 'desktop_viewport::CssPoint');
  assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some((v) =>
    v.rule === 'private-port-access' && v.module === 'rust.desktop.native.viewport' && v.detail.path === 'desktop_viewport::CssPoint'));
  const root = desktopFixture(t);
  appendSource(root, 'src-tauri/src/native_application.rs', 'use native_engine::desktop_viewport::SurfaceAttempt;');
  assert.ok(R.checkRustCrate(profile, policy, { root }).violations.some((v) =>
    v.rule === 'private-port-access' && v.module === 'rust.desktop.native.application' && v.detail.path === 'desktop_viewport::SurfaceAttempt'));
});

test('N25F3D3B2D2 original desktop preview registers only its private modules and exact ports', () => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const module = (id) => profile.modules.find((entry) => entry.id === id);
  const command = 'rust.desktop.native.application.commands';
  const owner = 'rust.desktop.native.application.viewport';
  const tests = 'rust.desktop.native.host.release.tests';
  const seam = 'rust.desktop.native.dispatch';
  for (const [id, file] of [[command, 'native_object_desktop_preview.rs'],
    [owner, 'native_object_viewport.rs'],
    [tests, 'native_object_desktop_preview_tests.rs'],
    [tests, 'native_object_desktop_preview_test_support.rs']]) {
    assert.ok(module(id).files.includes(file), `${id}: ${file}`);
    assert.equal(module(id).publicApi.includes(file), false);
  }
  const shell = fs.readFileSync(path.join(ROOT, 'src-tauri/src/lib.rs'), 'utf8');
  assert.match(shell, /mod native_object_desktop_preview;/);
  assert.match(shell, /native_object_desktop_preview::nemo_native_object_preview,/);
  const ports = policy.externalCratePorts.native_engine.moduleItems;
  const commandItems = ['desktop_viewport::DesktopViewportHost', 'desktop_viewport::ObjectPresentedFrame',
    'desktop_viewport::SurfacePort', 'object_document::FrameScopeKind',
    'object_frame_packet::ObjectFrameSelector', 'scheduler::OutputSpec'];
  for (const item of commandItems) assert.ok(ports[command].includes(item));
  assert.equal(ports[command].includes('desktop_viewport::PresentationReceipt'), false);
  assert.ok(ports[seam].includes('object_render_scene::ObjectPaintPlan'));
  for (const [id, item] of [[command, 'desktop_viewport::SurfacePort'],
    [command, 'desktop_viewport::ObjectPresentedFrame'],
    [seam, 'object_render_scene::ObjectPaintPlan']]) {
    const denied = structuredClone(policy);
    denied.externalCratePorts.native_engine.moduleItems[id] = ports[id].filter((entry) => entry !== item);
    assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some((v) =>
      v.rule === 'private-port-access' && v.module === id && v.detail.path === item), `${id}: ${item}`);
  }
  const edgeDenied = structuredClone(policy);
  edgeDenied.layerRules['host-release-tests'].allowedLayers =
    edgeDenied.layerRules['host-release-tests'].allowedLayers.filter((layer) => layer !== 'viewport-owner');
  assert.ok(R.checkRustCrate(profile, edgeDenied, { root: ROOT }).violations.some((v) =>
    v.rule === 'layer-violation' && v.module === tests && v.detail.targetModule === owner));
});

test('N25F3D3C2B object artifact bridge has exact private ports and test-only filesystem edge', () => {
  const profile = read('rust.profile.json');
  const desktop = read('nemo-desktop.edges.json');
  const engine = read('native-engine.edges.json');
  const module = (id) => profile.modules.find((entry) => entry.id === id);
  const seam = 'rust.desktop.native.dispatch';
  const tests = 'rust.desktop.native.host.release.tests';
  assert.ok(module('rust.native.engine.export-job').files.includes('object_export_host.rs'));
  assert.ok(module(seam).files.includes('native_object_export.rs'));
  assert.ok(module(tests).files.includes('native_object_export_tests.rs'));
  for (const item of ['export_job::export_object_frames_with_materializer', 'export_job::ObjectHostExportOutcome',
    'export_job::ObjectHostExportRequest', 'object_render_scene::prepare_object_composition']) {
    assert.ok(desktop.externalCratePorts.native_engine.moduleItems[seam].includes(item));
    const denied = structuredClone(desktop);
    denied.externalCratePorts.native_engine.moduleItems[seam] = denied.externalCratePorts.native_engine.moduleItems[seam].filter((entry) => entry !== item);
    assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some((v) =>
      v.rule === 'private-port-access' && v.module === seam && v.detail.path === item), item);
  }
  for (const item of ['export_job::ExternalEffectDisposition', 'export_job::JobStatus']) {
    assert.ok(desktop.externalCratePorts.native_engine.moduleItems[tests].includes(item));
  }
  const deniedPortEdge = structuredClone(desktop);
  deniedPortEdge.layerRules['host-release-tests'].allowedLayers =
    deniedPortEdge.layerRules['host-release-tests'].allowedLayers.filter((layer) => layer !== 'host-ports');
  assert.ok(R.checkRustCrate(profile, deniedPortEdge, { root: ROOT }).violations.some((v) =>
    v.rule === 'layer-violation' && v.module === tests && v.detail.targetModule === 'rust.desktop.native.application.ports'));
  const deniedCompositorEdge = structuredClone(engine);
  deniedCompositorEdge.layerRules['native-export-job'].allowedLayers =
    deniedCompositorEdge.layerRules['native-export-job'].allowedLayers.filter((layer) => layer !== 'native-compositor');
  assert.ok(R.checkRustCrate(profile, deniedCompositorEdge, { root: ROOT }).violations.some((v) =>
    v.rule === 'layer-violation' && v.module === 'rust.native.engine.export-job' && v.detail.targetModule === 'rust.native.engine.compositor'));
});

test('N25F3D3C3A desktop object export registers only the private route and exact lazy-port items', () => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const command = 'rust.desktop.native.application.commands';
  const tests = 'rust.desktop.native.host.release.tests';
  const module = (id) => profile.modules.find((entry) => entry.id === id);
  for (const file of ['native_object_desktop_export.rs', 'native_object_desktop_export_port.rs', 'native_object_desktop_sequence.rs']) {
    assert.ok(module(command).files.includes(file), file);
    assert.equal(module(command).publicApi.includes(file), false);
  }
  assert.ok(module(tests).files.includes('native_object_desktop_export_tests.rs'));
  assert.ok(module(tests).files.includes('native_object_desktop_sequence_tests.rs'));
  assert.ok(module(tests).files.includes('native_object_desktop_sequence_undo_tests.rs'));
  const shell = fs.readFileSync(path.join(ROOT, 'src-tauri/src/lib.rs'), 'utf8');
  assert.match(shell, /mod native_object_desktop_export;/);
  assert.match(shell, /native_object_desktop_export::nemo_native_object_export,/);
  const ports = policy.externalCratePorts.native_engine.moduleItems;
  for (const [id, item] of [
    ...['desktop_viewport::ViewportMapping', 'export_job::ExportArtifact',
      'export_job::ExternalEffectDisposition', 'export_job::JobStatus',
      'export_job::ObjectHostExportOutcome', 'export_job::StagedArtifactPort']
      .map((item) => [command, item]),
    [tests, 'export_job::StagedArtifactPort'],
  ]) {
    assert.ok(ports[id].includes(item), `${id}: ${item}`);
    const denied = structuredClone(policy);
    denied.externalCratePorts.native_engine.moduleItems[id] = ports[id].filter((entry) => entry !== item);
    assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some((v) =>
      v.rule === 'private-port-access' && v.module === id && v.detail.path === item), `${id}: ${item}`);
  }
});

test('N25C2e raw bootstrap is registered with exact codec and test-only edges', (t) => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const command = 'rust.desktop.native.application.commands', tests = 'rust.desktop.native.host.release.tests';
  const source = 'src-tauri/src/native_object_bootstrap.rs', controls = 'src-tauri/src/native_object_bootstrap_tests.rs';
  const owner = profile.modules.find((m) => m.id === command);
  assert.ok(owner.files.includes(path.basename(source)) && owner.publicApi.includes(path.basename(source)));
  assert.ok(profile.modules.find((m) => m.id === tests).files.includes(path.basename(controls)));
  const raw = fs.readFileSync(path.join(ROOT, source), 'utf8'), shell = fs.readFileSync(path.join(ROOT, 'src-tauri/src/lib.rs'), 'utf8');
  assert.match(shell, /mod native_object_bootstrap;/);
  assert.match(shell, /native_object_bootstrap::nemo_native_object_bootstrap,/);
  assert.match(raw, /request_json: String/);
  assert.match(raw, /require_main\(&window\)\?;\s*let admitted = if let Some\(request\) =[\s\S]*?admission::new_request\(&request_json\)\.filter\(\|request\| request\.replace\.is_some\(\)\)[\s\S]*?else if let Some\(request\) =[\s\S]*?reopen::file_request\(&request_json\)\.filter\(\|request\| request\.replace\.is_some\(\)\)/);
  assert.match(raw, /replace::prepare\(&state, request, \|path\|[\s\S]*?try_fs_scope\(\)[\s\S]*?scope\.is_allowed\(path\)/);
  assert.match(raw, /reopen::bootstrap_request\(&state, &request_json, \|path\|[\s\S]*?try_fs_scope\(\)[\s\S]*?scope\.is_allowed\(path\)/);
  const reopen = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_object_reopen.rs'), 'utf8');
  assert.match(reopen, /let resolved = Path::new\(&request\.source_path\)[\s\S]*?\.canonicalize\(\)/);
  assert.match(reopen, /allowed\(&resolved\)/);
  assert.match(reopen, /open_regular\(&resolved, \|path\| File::open\(path\)\)/);
  assert.equal(profile.sizeProfiles[owner.sizeProfile].hardMax, 500);
  assert.equal(policy.externalCratePorts.native_engine.moduleItems[command].filter((x) => x === 'object_codec::decode_storage_project').length, 1);
  const denied = structuredClone(policy);
  denied.externalCratePorts.native_engine.moduleItems[command] = denied.externalCratePorts.native_engine.moduleItems[command].filter((x) => x !== 'object_codec::decode_storage_project');
  assert.ok(R.checkRustCrate(profile, denied, { root: ROOT }).violations.some((v) => v.rule === 'private-port-access' && v.module === command && v.detail.path === 'object_codec::decode_storage_project'));
  const edgeDenied = structuredClone(policy);
  edgeDenied.layerRules['host-release-tests'].allowedLayers = edgeDenied.layerRules['host-release-tests'].allowedLayers.filter((x) => x !== 'command-adapter');
  assert.ok(R.checkRustCrate(profile, edgeDenied, { root: ROOT }).violations.some((v) => v.rule === 'layer-violation' && v.module === tests && v.detail.targetModule === command));
  for (const file of [source, controls, 'src-tauri/src/native_object_reopen.rs', 'src-tauri/src/native_object_reopen_tests.rs']) {
    const removed = structuredClone(profile);
    for (const module of removed.modules) module.files = module.files.filter((x) => x !== path.basename(file));
    assert.ok(require('./lib/boundaries-coverage.cjs').checkSourceCoverage(removed, { root: ROOT, sourcePaths: [file] }).violations.some((v) => v.rule === 'coverage-unprofiled-source' && v.file === file));
  }
  const root = desktopFixture(t);
  appendSource(root, 'src-tauri/src/native_application_tests.rs', 'use native_engine::object_codec::decode_project;');
  appendSource(root, 'src-tauri/src/application_mcp.rs', 'use crate::native_object_bootstrap::bootstrap_raw;');
  const violations = R.checkRustCrate(profile, policy, { root }).violations;
  assert.ok(violations.some((v) => v.rule === 'private-port-access' && v.module === 'rust.desktop.native.application.tests' && v.detail.path === 'object_codec::decode_project'));
  assert.ok(violations.some((v) => v.rule === 'layer-violation' && v.module === 'rust.desktop.mcp.adapter' && v.detail.targetModule === command));
  assert.ok(violations.some((v) => v.rule === 'cycle'), 'production back-edge must remain denied');
});


test('N25C2f immutable snapshot port stays within actual host seam and its tests', (t) => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const seam = 'rust.desktop.native.dispatch', item = 'object_snapshot::ObjectSnapshot';
  assert.deepEqual(Object.entries(policy.externalCratePorts.native_engine.moduleItems)
    .filter(([, ports]) => ports.includes(item)).map(([id]) => id),
    ['rust.desktop.native.host.release.tests', seam]);
  const host = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_object_host.rs'), 'utf8');
  assert.match(host, /let response = snapshot\s*\.dispatch\(request\.clone\(\)\)\s*\.map_err\(\|_\| "native object read correlation is invalid"\.to_string\(\)\)\?;/);
  assert.match(host, /let expression_state = request\.operation\(\) == "query\.document\.object\.property\.read"\s*&& serde_json::to_value\(&request\)\s*\.ok\(\)\s*\.is_some_and\(\|wire\| wire\["payload"\]\["view"\]\["kind"\] == "expressionState"\);/);
  assert.match(host, /if expression_state && response\.is_ok\(\) \{\s*self\.project_expression_state\(&request, response\)\s*\} else \{\s*Ok\(response\)\s*\}/);
  const projection = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_object_expression_state.rs'), 'utf8');
  assert.match(projection, /pub\(super\) fn project_expression_state\(\s*&self,/);
  assert.match(projection, /if !response\.is_ok\(\) \|\| request\.operation\(\) != "query\.document\.object\.property\.read" \{\s*return Ok\(response\);\s*\}/);
  assert.match(projection, /\.acquire_snapshot\(self\.content_revision\(\)\)/);
  assert.match(projection, /let Some\(diagnostic\) = self\.motion_diagnostic_for\(&snapshot, &target, property, frame\)\s*else \{\s*return Ok\(response\);\s*\};\s*snapshot\s*\.with_expression_state_evaluation\(request, response, diagnostic\)/);
  assert.match(host, /acquire_snapshot\(self\.content_revision\(\)\)/);
  const descriptor = JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object.json'), 'utf8'));
  assert.equal(descriptor.availability.state, 'available');
  for (const raw of [false, true]) {
    const root = desktopFixture(t), prefix = raw ? 'r#' : '';
    appendSource(root, 'src-tauri/src/native_object_bootstrap.rs', `use ${prefix}native_engine::object_snapshot::ObjectSnapshot;`);
    const result = R.checkRustCrate(profile, policy, { root });
    assert.ok(result.violations.some(v => v.rule === 'private-port-access'
      && v.module === 'rust.desktop.native.application.commands' && v.detail.path === item));
  }
});


require('./boundaries-native-input.test.cjs');

require('./boundaries-native-object-client.test.cjs');
