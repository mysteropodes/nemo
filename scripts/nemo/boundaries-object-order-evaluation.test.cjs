'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');
const C = require('./lib/boundaries-coverage.cjs');
const root = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const policy = JSON.parse(read('engineering/boundaries/profiles/native-engine.edges.json'));
const source = 'native-engine/src/object_order_evaluation.rs';
const controls = 'native-engine/tests/object_order_evaluation.rs';

test('numeric order private children have exact census without another API file or feature', () => {
  const declared = profile.modules.flatMap(m => m.files.map(f => `${m.dir}/${f}`));
  for (const file of [source, controls]) {
    assert.equal(declared.filter(f => f === file).length, 1);
    const omitted = structuredClone(profile);
    for (const m of omitted.modules) m.files = m.files.filter(f => `${m.dir}/${f}` !== file);
    assert.ok(C.checkSourceCoverage(omitted, { root, sourcePaths: [file] }).violations
      .some(v => v.rule === 'coverage-unprofiled-source' && v.file === file));
  }
  assert.deepEqual(profile.modules.find(m => m.id === 'rust.native.engine.evaluation').publicApi,
    ['evaluation.rs', 'animation_curve.rs', 'object_frame_packet.rs']);
  assert.match(read('native-engine/src/object_frame_packet.rs'), /#\[path = "object_order_evaluation.rs"\]\s*mod order;/);
  assert.match(read('native-engine/tests/object_frame_packet.rs'), /#\[path = "object_order_evaluation.rs"\]\s*mod numeric_order_tests;/);
  assert.match(read('native-engine/src/lib.rs'), /"evaluation" => evaluation, animation_curve, object_frame_packet;/);
  const owner = JSON.parse(read('engineering/boundaries/profiles/scripts-nemo.profile.json'))
    .modules.find(m => m.id === 'nemo.test.boundariesRust');
  assert.equal(owner.files.filter(f => f === path.basename(__filename)).length, 1);
  assert.deepEqual(owner.publicApi, ['boundaries-rust.test.cjs']);
  assert.equal(profile.sizeProfiles['Rust production module'].hardMax, 500);
});

test('numeric order uses the adopted eighteen-module forty-nine-edge engine policy', () => {
  const result = R.checkRustCrate(profile, policy, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.equal(result.moduleCount, 18); assert.equal(result.edges.length, 49);
  assert.deepEqual(result.unsupported, []); assert.deepEqual(result.exceptionsApplied, []);
});

test('private numeric producer cannot take writable history or render/application authority', t => {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-order-producer-boundary-'));
  t.after(() => fs.rmSync(scratch, { recursive: true, force: true }));
  fs.cpSync(path.join(root, 'native-engine/src'), path.join(scratch, 'native-engine/src'), { recursive: true });
  fs.appendFileSync(path.join(scratch, source), '\nuse crate::history::NativeObjectHistory;\nuse crate::application::NativeApplication;\nuse crate::render_scene::RenderScene;\nuse tauri::Window;\n');
  const refused = R.checkRustCrate(profile, policy, { root: scratch });
  assert.equal(refused.ok, false);
  for (const target of ['history', 'application', 'render-scene']) {
    assert.ok(refused.violations.some(v => v.rule === 'layer-violation' && v.file === source
      && v.detail.targetModule === `rust.native.engine.${target}`));
  }
  assert.ok(refused.violations.some(v => v.rule === 'external-crate-violation'
    && v.file === source && v.detail.crate === 'tauri'));
});

// The helper's narrow input is the scalability boundary: no document object
// slice/snapshot is available while resolving each ordered element.
function indexedLookup(candidate) {
  const helper = candidate.slice(candidate.indexOf('fn elements('), candidate.indexOf('pub(crate) fn prepare('));
  assert.doesNotMatch(helper, /ObjectSnapshot|ObjectRecord|\.objects\(|\.position\(/);
  assert.match(helper, /sources:[\s\S]*SourceIndices/);
  assert.match(helper, /sources[\s\S]*\.get\(/);
  const indexed = candidate.slice(candidate.indexOf('pub(super) fn prepare_context_with_resolver'),
    candidate.indexOf('fn natural_component('));
  assert.equal((indexed.match(/\.objects\(\)/g) || []).length, 1);
  assert.match(indexed, /let records = selected\.objects\(\);/);
  assert.equal((indexed.match(/records\.iter\(\)/g) || []).length, 1);
  const natural = candidate.slice(candidate.indexOf('fn natural_component('));
  assert.equal((natural.match(/\.objects\(\)/g) || []).length, 1);
  assert.match(natural, /for \(source_object_index, record\) in context\.objects\(\)\.iter\(\)\.enumerate\(\)/);
  const layers = natural.slice(natural.indexOf('let layers = context'));
  assert.doesNotMatch(layers, /\.objects\(|\.position\(/);
  assert.match(layers, /std::mem::take\(&mut layer_elements\[index\]\)/);
}

test('element lookup cannot regress to a document scan per element', () => {
  const producer = read(source);
  indexedLookup(producer);
  assert.throws(() => indexedLookup(producer.replace('sources: &SourceIndices',
    'snapshot: &ObjectSnapshot, sources: &SourceIndices')), assert.AssertionError);
  assert.throws(() => indexedLookup(producer.replace('let index = sources',
    'let index = snapshot.document().objects().iter().position(|_| true); let index = sources')),
  assert.AssertionError);
  assert.throws(() => indexedLookup(producer.replace('let elements = std::mem::take',
    'let _rescan = context.objects().iter().filter(|_| true).count(); let elements = std::mem::take')),
  assert.AssertionError);
});
