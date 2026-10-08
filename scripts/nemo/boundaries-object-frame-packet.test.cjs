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
const source = 'native-engine/src/object_frame_packet.rs';
const controls = 'native-engine/tests/object_frame_packet.rs';
const evaluation = 'rust.native.engine.evaluation';

function api(candidate) {
  assert.deepEqual(candidate.modules.find(m => m.id === evaluation).publicApi,
    ['evaluation.rs', 'animation_curve.rs', 'object_frame_packet.rs']);
}
function scratch(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-object-packet-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  fs.mkdirSync(path.join(dir, 'native-engine'), { recursive: true });
  fs.cpSync(path.join(root, 'native-engine/src'), path.join(dir, 'native-engine/src'), { recursive: true });
  return dir;
}

test('two new Rust files have exact census and one narrow internal API', () => {
  api(profile);
  const declared = profile.modules.flatMap(m => m.files.map(f => `${m.dir}/${f}`));
  for (const file of [source, controls]) assert.equal(declared.filter(f => f === file).length, 1);
  const missing = structuredClone(profile);
  for (const m of missing.modules) m.files = m.files.filter(f => f !== 'object_frame_packet.rs');
  const result = coverage.checkSourceCoverage(missing, { root, sourcePaths: [source, controls] });
  for (const file of [source, controls]) {
    assert.ok(result.violations.some(v => v.rule === 'coverage-unprofiled-source' && v.file === file));
  }
  for (const replacement of [[], ['evaluation.rs', 'animation_curve.rs', 'object_frame_packet.rs', 'private.rs']]) {
    const drift = structuredClone(profile);
    drift.modules.find(m => m.id === evaluation).publicApi = replacement;
    assert.throws(() => api(drift), assert.AssertionError);
  }
  assert.equal(profile.sizeProfiles['Rust production module'].hardMax, 500);
});

test('existing evaluation and application targets both register packet tests', () => {
  const lib = read('native-engine/src/lib.rs');
  assert.match(lib, /"evaluation" => evaluation, animation_curve, object_frame_packet;/);
  for (const target of ['evaluation', 'application']) {
    assert.match(lib, new RegExp(`"test-${target}" => object_frame_packet_${target}_tests = "../tests/object_frame_packet.rs";`));
    assert.match(read('native-engine/Cargo.toml'), new RegExp(`name = "${target}"[\\s\\S]*?required-features = \\["test-${target}"\\]`));
  }
  assert.match(read(controls), /#\[cfg\(feature = "history"\)\][\s\S]*fn actual_history_fill_undo_redo_and_reopen/);
  assert.equal(JSON.parse(read('engineering/application/capabilities-v2/native-object.json')).availability.state, 'unavailable');
});

test('producer stays in the existing eighteen-module forty-four-edge graph', () => {
  const result = R.checkRustCrate(profile, policy, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.equal(result.moduleCount, 18);
  assert.equal(result.edges.length, 44);
  assert.deepEqual(result.unsupported, []);
  assert.deepEqual(result.exceptionsApplied, []);
});

test('producer cannot acquire renderer/compositor or host dependencies', t => {
  const dir = scratch(t);
  fs.appendFileSync(path.join(dir, source), '\nuse crate::render_scene::RenderScene;\nuse crate::compositor::Compositor;\nuse tauri::Window;\nuse nemo_mcp::wire;\nuse nemo::Host;\n');
  const result = R.checkRustCrate(profile, policy, { root: dir });
  assert.equal(result.ok, false);
  for (const target of ['rust.native.engine.render-scene', 'rust.native.engine.compositor']) {
    assert.ok(result.violations.some(v => v.rule === 'layer-violation' && v.file === source && v.detail.targetModule === target));
  }
  for (const name of ['tauri', 'nemo_mcp', 'nemo']) {
    assert.ok(result.violations.some(v => v.rule === 'external-crate-violation' && v.file === source && v.detail.crate === name));
  }
});
