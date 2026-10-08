'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');
const coverage = require('./lib/boundaries-coverage.cjs');
const root = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const policy = JSON.parse(read('engineering/boundaries/profiles/native-engine.edges.json'));
const source = 'native-engine/src/object_render_scene.rs';
const controls = 'native-engine/tests/object_render_scene.rs';
const sceneModule = 'rust.native.engine.render-scene';

function api(candidate) {
  assert.deepEqual(candidate.modules.find(m => m.id === sceneModule).publicApi,
    ['render_scene.rs', 'render_geometry.rs', 'object_render_scene.rs']);
}
function harnesses(lib) {
  assert.match(lib, /"compositor" => compositor, render_scene, render_geometry, object_render_scene;/);
  for (const target of ['compositor', 'application']) {
    assert.match(lib, new RegExp(`"test-${target}" => object_render_scene_${target}_tests = "../tests/object_render_scene.rs";`));
  }
}

test('contour consumer has exact Rust census, narrow API and tooling census', () => {
  api(profile);
  const declared = profile.modules.flatMap(m => m.files.map(f => `${m.dir}/${f}`));
  for (const file of [source, controls]) assert.equal(declared.filter(f => f === file).length, 1);
  const missing = structuredClone(profile);
  for (const m of missing.modules) m.files = m.files.filter(f => f !== 'object_render_scene.rs');
  const result = coverage.checkSourceCoverage(missing, { root, sourcePaths: [source, controls] });
  for (const file of [source, controls]) {
    assert.ok(result.violations.some(v => v.rule === 'coverage-unprofiled-source' && v.file === file));
  }
  for (const replacement of [['render_scene.rs', 'render_geometry.rs'],
    ['render_scene.rs', 'render_geometry.rs', 'object_render_scene.rs', 'private.rs']]) {
    const drift = structuredClone(profile);
    drift.modules.find(m => m.id === sceneModule).publicApi = replacement;
    assert.throws(() => api(drift), assert.AssertionError);
  }
  const tooling = JSON.parse(read('engineering/boundaries/profiles/scripts-nemo.profile.json'));
  const module = tooling.modules.find(m => m.id === 'nemo.test.boundariesRust');
  assert.equal(module.files.filter(f => f === 'boundaries-object-render-scene.test.cjs').length, 1);
  assert.deepEqual(module.publicApi, ['boundaries-rust.test.cjs']);
  assert.equal(profile.sizeProfiles['Rust production module'].hardMax, 500);
});

test('both existing targets register contours and actual-history source controls', () => {
  const lib = read('native-engine/src/lib.rs');
  harnesses(lib);
  for (const target of ['compositor', 'application']) {
    const entry = `    "test-${target}" => object_render_scene_${target}_tests = "../tests/object_render_scene.rs";`;
    assert.throws(() => harnesses(lib.replace(entry, '')), assert.AssertionError);
    assert.match(read('native-engine/Cargo.toml'), new RegExp(`name = "${target}"[\\s\\S]*?required-features = \\["test-${target}"\\]`));
  }
  assert.match(read(controls), /#\[cfg\(feature = "history"\)\][\s\S]*fn actual_history_fill_undo_redo_and_reopen/);
  assert.equal(JSON.parse(read('engineering/application/capabilities-v2/native-object.json')).availability.state, 'unavailable');
});

test('real contour consumer retains the existing eighteen-module forty-four-edge graph', () => {
  const result = R.checkRustCrate(profile, policy, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.equal(result.moduleCount, 18);
  assert.equal(result.edges.length, 44);
  assert.deepEqual(result.unsupported, []);
  assert.deepEqual(result.exceptionsApplied, []);
});

test('contour consumer cannot acquire direct document/revision/history/compositor or host dependencies', t => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-object-contours-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  fs.mkdirSync(path.join(dir, 'native-engine'), { recursive: true });
  fs.cpSync(path.join(root, 'native-engine/src'), path.join(dir, 'native-engine/src'), { recursive: true });
  fs.appendFileSync(path.join(dir, source), '\nuse crate::object_document::ObjectRecord;\nuse crate::revision::ObjectSnapshot;\nuse crate::history::NativeObjectHistory;\nuse crate::compositor::Compositor;\nuse tauri::Window;\nuse nemo_mcp::wire;\nuse nemo::Host;\n');
  const result = R.checkRustCrate(profile, policy, { root: dir });
  assert.equal(result.ok, false);
  for (const target of ['document', 'revision', 'history', 'compositor']) {
    assert.ok(result.violations.some(v => v.rule === 'layer-violation' && v.file === source && v.detail.targetModule === `rust.native.engine.${target}`));
  }
  for (const name of ['tauri', 'nemo_mcp', 'nemo']) {
    assert.ok(result.violations.some(v => v.rule === 'external-crate-violation' && v.file === source && v.detail.crate === name));
  }
});
