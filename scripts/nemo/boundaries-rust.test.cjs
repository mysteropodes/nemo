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

test('adopted nemo-desktop policy holds at HEAD: exact edges, no debt, MCP and native-engine ports pinned', () => {
  const policy = read('nemo-desktop.edges.json');
  const r = R.checkRustCrate(read('rust.profile.json'), policy, { root: ROOT });
  assert.equal(r.ok, true, JSON.stringify(r.violations));
  assert.equal(r.moduleCount, policy.profileModules.length);
  assert.deepEqual(r.unsupported, []);
  assert.deepEqual(r.exceptionsApplied, []);
  assert.deepEqual(r.exportedPort, []);
  assert.deepEqual(r.edges.map((e) => `${e.from}->${e.to}`).sort(), [
    'rust.desktop.mcp.adapter->rust.desktop.native.dispatch',
    'rust.desktop.mcp.adapter.tests->rust.desktop.mcp.adapter',
    'rust.desktop.native.application->rust.desktop.native.application.contract',
    'rust.desktop.native.application->rust.desktop.native.application.ports',
    'rust.desktop.native.application->rust.desktop.native.dispatch',
    'rust.desktop.native.application.commands->rust.desktop.mcp.adapter',
    'rust.desktop.native.application.commands->rust.desktop.native.application',
    'rust.desktop.native.application.commands->rust.desktop.native.application.contract',
    'rust.desktop.native.application.commands->rust.desktop.native.application.ports',
    'rust.desktop.native.application.commands->rust.desktop.native.application.viewport',
    'rust.desktop.native.application.commands->rust.desktop.native.dispatch',
    'rust.desktop.native.application.contract->rust.desktop.native.application.ports',
    'rust.desktop.native.application.ports.tests->rust.desktop.native.application.ports',
    'rust.desktop.native.application.tests->rust.desktop.native.application',
    'rust.desktop.native.application.tests->rust.desktop.native.application.contract',
    'rust.desktop.native.application.tests->rust.desktop.native.application.ports',
    'rust.desktop.native.application.viewport->rust.desktop.native.application.contract',
    'rust.desktop.native.application.viewport->rust.desktop.native.viewport',
    'rust.desktop.shell->rust.desktop.mcp.adapter', 'rust.desktop.shell->rust.desktop.media',
    'rust.desktop.shell->rust.desktop.native.application',
    'rust.desktop.shell->rust.desktop.native.application.commands',
    'rust.desktop.shell->rust.desktop.native.application.contract',
    'rust.desktop.shell->rust.desktop.native.application.ports',
    'rust.desktop.shell->rust.desktop.native.application.viewport',
    'rust.desktop.shell->rust.desktop.native.dispatch',
    'rust.desktop.shell->rust.desktop.native.viewport', 'rust.desktop.shell->rust.desktop.tasks',
    'rust.desktop.tasks.tests->rust.desktop.tasks']);
  assert.deepEqual(r.unanalyzedModules.slice().sort(),
    ['rust.desktop.build', 'rust.mcp.transport', 'rust.native.engine']);

  const application = fs.readFileSync(path.join(ROOT, 'src-tauri/src/application_mcp.rs'), 'utf8');
  const applicationTests = fs.readFileSync(path.join(ROOT, 'src-tauri/src/application_mcp_tests.rs'), 'utf8');
  const viewport = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_viewport.rs'), 'utf8');
  const ports = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_application_ports.rs'), 'utf8');
  const bootstrap = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_application.rs'), 'utf8');
  assert.match(application, /use nemo_mcp::\{/);
  assert.match(application, /registry::\{self, Endpoint, Registration\}/);
  assert.match(application, /\bwire,/);
  assert.match(application, /nemo_mcp::BUILD_SOURCE_ID/);
  for (const symbol of ['ApplicationRequest', 'ApplicationResponse', 'NativeApplicationError',
    'NativeApplicationRequest', 'NativeApplicationResponse', 'NativeHostStatus', 'NativeStatusRequest',
    'NATIVE_API_VERSION', 'Operation']) assert.match(application, new RegExp(`\\b${symbol}\\b`));
  assert.deepEqual(policy.externalCratePorts.nemo_mcp.items.slice().sort(),
    ['BUILD_SOURCE_ID', 'contract::ApplicationRequest', 'contract::ApplicationResponse',
      'contract::NativeApplicationError', 'contract::NativeApplicationRequest',
      'contract::NativeApplicationResponse', 'contract::NativeHostStatus',
      'contract::NativeStatusRequest', 'contract::NATIVE_API_VERSION', 'contract::Operation',
      'registry', 'wire'].sort());
  assert.deepEqual(policy.externalCratePorts.nemo_mcp.allowedModules.slice().sort(), [
    'rust.desktop.mcp.adapter', 'rust.desktop.native.application.commands',
    'rust.desktop.native.application.contract', 'rust.desktop.native.application.tests',
  ]);

  assert.match(applicationTests, /use crate::application_mcp::\*/);
  assert.match(application, /use native_engine::\{/);
  assert.match(applicationTests, /use native_engine::\{/);
  assert.match(viewport, /use native_engine::compositor::\{/);
  assert.match(viewport, /use native_engine::desktop_viewport::\{/);
  assert.match(ports, /use native_engine::application::\{/);
  assert.match(ports, /use native_engine::png_output::\{/);
  assert.match(bootstrap, /use native_engine::\{/);
  assert.equal(policy.externalCratePorts.native_engine.allowedModules, undefined);
  assert.equal(policy.externalCratePorts.native_engine.items, undefined);
  assert.deepEqual(policy.externalCratePorts.native_engine.moduleItems, {
    'rust.desktop.mcp.adapter': [
      'application::ExportResourceResolver', 'application::NativeApplication',
      'commands::OpacityRequest', 'document::OpacityDocument',
      'export_job::ExportCompositor', 'export_job::JobReceipt', 'export_job::StagedArtifactPort',
      'protocol::OP_JOB_EXPORT_PNG_BEGIN'],
    'rust.desktop.mcp.adapter.tests': [
      'application::ResourceResolutionError', 'application::ResourceResolutionErrorKind',
      'codec::decode_project', 'export_job::ExportArtifact', 'export_job::ExportReadback',
      'protocol::OpaqueResourceHandle', 'render_scene::GeometryPaintInput', 'render_scene::RenderScene'],
    'rust.desktop.native.viewport': [
      'compositor::CompositionResult', 'compositor::Compositor', 'compositor::CompositorError',
      'compositor::CompositorInstance', 'desktop_viewport::AcquiredSurfaceFrame',
      'desktop_viewport::DesktopViewportHost', 'desktop_viewport::PresentationReceipt',
      'desktop_viewport::SettledSurfaceFrame', 'desktop_viewport::SurfaceAttempt',
      'desktop_viewport::SurfacePort', 'desktop_viewport::SurfacePresentation',
      'desktop_viewport::SurfaceRecoveryError', 'desktop_viewport::ViewportError',
      'desktop_viewport::ViewportMapping', 'render_scene::ScheduledFrameIdentity', 'scheduler::WorkId'],
    'rust.desktop.native.application.ports': [
      'application::ExportResourceResolver', 'application::ResourceResolutionError',
      'application::ResourceResolutionErrorKind', 'compositor::CompositionResult', 'compositor::Compositor',
      'png_output::ExportArtifact', 'png_output::ExportCompositor', 'png_output::ExportReadback',
      'png_output::StagedArtifactPort', 'protocol::OpaqueResourceHandle',
      'render_scene::GeometryPaintInput', 'render_scene::RenderScene'],
    'rust.desktop.native.application.contract': [
      'codec::decode_project', 'desktop_viewport::CssBounds', 'desktop_viewport::PhysicalExtent',
      'desktop_viewport::ViewportMapping', 'document::OpacityDocument', 'export_job::CleanupStatus',
      'export_job::ExternalEffectDisposition', 'export_job::JobReceipt', 'export_job::JobStatus',
      'render_scene::GeometryPaintInput', 'render_scene::LayerGeometry', 'render_scene::OpaqueSrgbPaint',
      'resource_leases::WorkId', 'scheduler::OutputSpec'],
    'rust.desktop.native.application.viewport': [
      'compositor::CompositionResult', 'compositor::Compositor', 'desktop_viewport::DeferredAction',
      'desktop_viewport::FatalAction', 'desktop_viewport::ViewportMapping', 'desktop_viewport::ViewportStatus',
      'render_scene::ScheduledFrameIdentity', 'resource_leases::WorkId'],
    'rust.desktop.native.application': [
      'application::NativeApplication', 'commands::OpacityRequest', 'commands::ResponseEnvelope',
      'compositor::CompositionResult', 'document::OpacityDocument',
      'export_job::JobReceipt', 'export_job::PendingFrame', 'render_scene',
      'resource_leases::FrameFailure', 'resource_leases::FrameFailureKind', 'resource_leases::WorkId',
      'scheduler::EvaluationKey', 'scheduler::FrameScheduler'],
    'rust.desktop.native.application.commands': ['compositor::Compositor'],
    'rust.desktop.native.application.tests': ['compositor::Compositor'],
    'rust.desktop.native.dispatch': [
      'application::ExportResourceResolver', 'application::NativeApplication',
      'commands::OpacityRequest', 'commands::ResponseEnvelope', 'document::OpacityDocument',
      'export_job::ExportCompositor', 'export_job::JobReceipt', 'export_job::JobStatus',
      'export_job::PendingFrame', 'export_job::StagedArtifactPort'],
  });
  const rustProfile = read('rust.profile.json');
  for (const [moduleId, items] of Object.entries(policy.externalCratePorts.native_engine.moduleItems)) {
    const module = rustProfile.modules.find((entry) => entry.id === moduleId);
    const refs = module.files.flatMap((file) => {
      const source = fs.readFileSync(path.join(ROOT, module.dir, file), 'utf8');
      const analysis = R.analyzeRustSource(source, { externalCrates: ['native_engine'] });
      return analysis.uses.concat(analysis.inline)
        .filter((entry) => entry.segments[0] === 'native_engine')
        .map((entry) => entry.segments.slice(1).join('::'));
    });
    for (const item of items) assert.ok(refs.some((ref) => ref === item || ref.startsWith(`${item}::`)),
      `${moduleId} grants unused native_engine item ${item}`);
  }
});

test('desktop native-engine grants are module-local: host, host tests and viewport cannot borrow another module item', (t) => {
  const root = desktopFixture(t);
  appendSource(root, 'src-tauri/src/native_application.rs', 'use native_engine::desktop_viewport::SurfacePort;');
  appendSource(root, 'src-tauri/src/native_application_tests.rs', 'use native_engine::desktop_viewport::SurfacePort;');
  appendSource(root, 'src-tauri/src/native_viewport.rs', 'use native_engine::codec::decode_project;');
  const policy = read('nemo-desktop.edges.json');
  const r = R.checkRustCrate(read('rust.profile.json'), policy, { root });
  assert.equal(r.ok, false);
  assert.deepEqual(r.violations.filter((v) => v.rule === 'private-port-access')
    .map((v) => [v.module, v.detail.path]).sort(), [
    ['rust.desktop.native.application', 'desktop_viewport::SurfacePort'],
    ['rust.desktop.native.application.tests', 'desktop_viewport::SurfacePort'],
    ['rust.desktop.native.viewport', 'codec::decode_project'],
  ]);
});

test('raw identifiers cannot bypass desktop native-engine ports in the host, focused tests or viewport', (t) => {
  const root = desktopFixture(t);
  appendSource(root, 'src-tauri/src/native_application.rs', 'use r#native_engine::r#desktop_viewport::SurfacePort;');
  appendSource(root, 'src-tauri/src/native_application_tests.rs', 'use native_engine::r#desktop_viewport::r#SurfacePort;');
  appendSource(root, 'src-tauri/src/native_viewport.rs',
    'fn raw_port_probe() { let _ = r#native_engine::r#codec::decode_project; }');
  const r = R.checkRustCrate(read('rust.profile.json'), read('nemo-desktop.edges.json'), { root });
  assert.equal(r.ok, false);
  assert.deepEqual(r.violations.filter((v) => v.rule === 'private-port-access')
    .map((v) => [v.module, v.detail.path]).sort(), [
    ['rust.desktop.native.application', 'desktop_viewport::SurfacePort'],
    ['rust.desktop.native.application.tests', 'desktop_viewport::SurfacePort'],
    ['rust.desktop.native.viewport', 'codec::decode_project'],
  ]);
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
