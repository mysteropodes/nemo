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
`;
  const a = R.analyzeRustSource(src);
  const paths = a.uses.map((u) => u.segments.join('::'));
  assert.deepEqual(paths, ['std::collections::HashMap', 'crate::engine::build_bezpath', 'crate::engine::ItemIn',
    'crate::from_multipolygon', 'crate::to_polygon', 'crate::PolygonIn', 'crate::a', 'crate::a::b::c', 'crate::a::b::d',
    'engine::VelloEngine', 'super::*', 'crate::multi::line::Thing']);
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
fn f() { crate::hit::hit_test_scene(1); let c = 'x'; }
`;
  const a = R.analyzeRustSource(src);
  assert.deepEqual(a.uses.map((u) => u.segments.join('::') + '@' + u.depth), ['super::*@1']);
  assert.deepEqual(a.mods.map((m) => m.name + (m.inlineBody ? '{}' : ';')), ['fill;', 'engine;', 'tests{}']);
  assert.deepEqual(a.inline.map((i) => i.segments.join('::') + '@' + i.depth), ['super::helper@1', 'crate::hit@0']);
  assert.deepEqual(a.cfgs.map((c) => c.expr), ['test']);
  assert.deepEqual(a.inlineMods, [{ name: 'tests', nested: false }]);
});

test('cfg predicates keep their string values (target_arch, feature) and nested inline modules are flagged', () => {
  const a = R.analyzeRustSource(`#[cfg(target_arch = "wasm32")]\nfn w() {}\n#[cfg(all(feature = "gpu", not(test)))]\nfn g() {}\nmod outer { mod inner { use super::*; } }\n`);
  assert.deepEqual(a.cfgs.map((c) => c.expr), ['target_arch = "wasm32"', 'all(feature = "gpu", not(test))']);
  assert.deepEqual(a.inlineMods, [{ name: 'outer', nested: true }]);
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
});

// ---- the real crate ----------------------------------------------------------

test('adopted geometry-wasm policy holds at HEAD: no violation, exactly the three recorded-debt exceptions, port matches lib.rs', () => {
  const r = R.checkRustCrate(read('rust.profile.json'), read('geometry-wasm.edges.json'), { root: ROOT });
  assert.equal(r.ok, true, JSON.stringify(r.violations));
  assert.equal(r.moduleCount, 4);
  assert.deepEqual(r.exceptionsApplied.map((e) => `${path.basename(e.path)}:${e.rule}`).sort(),
    ['engine.rs:cycle', 'eraser.rs:cycle', 'eraser.rs:layer-violation']);
  assert.deepEqual(r.unsupported, []);
  assert.equal(r.exportedPort.length, 18);
  assert.deepEqual(r.edges.map((e) => `${e.from}->${e.to}`).sort(), [
    'rust.geometry.api->rust.geometry.animation', 'rust.geometry.api->rust.geometry.engine', 'rust.geometry.api->rust.geometry.shapes',
    'rust.geometry.engine->rust.geometry.shapes', 'rust.geometry.shapes->rust.geometry.api', 'rust.geometry.shapes->rust.geometry.engine']);
  // No [features] table in Cargo.toml → the declared set is empty, and the crate uses only the two allowed predicates.
  assert.equal(/^\[features\]/m.test(fs.readFileSync(path.join(ROOT, 'geometry-wasm/Cargo.toml'), 'utf8')), false);
});
