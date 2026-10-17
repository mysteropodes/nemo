'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkRustSourceCoverage, validateVendorManifest } = require('../scripts/nemo/lib/boundaries-rust-vendor.cjs');
const { checkSourceSizes } = require('../scripts/nemo/lib/boundaries-size.cjs');
const { compareSizeBaseline } = require('../scripts/nemo/lib/boundaries-ratchet.cjs');
const ROOT = path.resolve(__dirname, '..');
const PREFIX = 'native-engine/third_party/rquickjs-sys-0.14.0';
const POLICY = 'engineering/boundaries/profiles/rust.vendor.json';
const manifestBytes = fs.readFileSync(path.join(ROOT, POLICY));
const manifest = JSON.parse(manifestBytes);
const rust = manifest.files.filter((record) => record.path.endsWith('.rs')).map((record) => `${PREFIX}/${record.path}`);
const app = 'native-engine/src/vendor_policy_control.rs';
function profile() {
  return { modules: [{ id: 'fixture.native', layer: 'engine', dir: 'native-engine/src', files: ['vendor_policy_control.rs'],
    publicApi: [], sizeProfile: 'ordinary' }], sizeProfiles: { ordinary: { warn: 400, hardMax: 500 } }, exceptions: [] };
}
function fixture(t) {
  // Keep scratch under the test root when requested privately; ordinary adoption
  // uses the platform temp directory. No source tree is ever edited.
  const tempBase = process.env.NEMO_VENDOR_TEST_TMP || os.tmpdir();
  const root = fs.mkdtempSync(path.join(tempBase, 'nemo-vendor-control-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, path.dirname(PREFIX)), { recursive: true });
  fs.cpSync(path.join(ROOT, PREFIX), path.join(root, PREFIX), { recursive: true });
  for (const file of [POLICY, `${PREFIX}.quickjs.patch`, `${PREFIX}.PROVENANCE.md`]) {
    fs.mkdirSync(path.join(root, path.dirname(file)), { recursive: true });
    fs.copyFileSync(path.join(ROOT, file), path.join(root, file));
  }
  fs.mkdirSync(path.join(root, path.dirname(app)), { recursive: true });
  fs.writeFileSync(path.join(root, app), 'pub fn ordinary() {}\n');
  return { root, run: (candidate = profile(), selected = [app, ...rust], exclusions = []) =>
    checkRustSourceCoverage(candidate, { root, sourcePaths: selected, exclusions }) };
}
function change(root, file) { fs.appendFileSync(path.join(root, file), '\nchanged\n'); }

test('complete real 306-file vendor packet is counted separately from handwritten Rust', (t) => {
  const f = fixture(t), result = f.run();
  assert.equal(result.ok, true);
  assert.equal(result.sourcePathCount, 26);
  assert.equal(result.handwrittenSourcePathCount, 1);
  assert.equal(result.declaredPathCount, 1);
  assert.equal(result.vendorSourcePathCount, 25);
  assert.equal(result.vendorIntegrity.preservedFileCount, 306);
  assert.equal(result.vendorIntegrity.upstreamIdenticalRustCount, 25);
  assert.deepEqual(result.vendorSourcePaths, rust);
  assert.equal(result.excludedPathCount, 0);
  assert.equal(result.sourcePathCount, result.declaredPathCount + result.vendorSourcePathCount);
  assert.equal(checkSourceSizes(profile(), { root: f.root }).ok, true);
});
for (const [label, file, error] of [
  ['upstream Rust change', rust[0], /vendor bytes changed/],
  ['patched C change', `${PREFIX}/quickjs/quickjs.c`, /vendor bytes changed/],
  ['patched header change', `${PREFIX}/quickjs/quickjs.h`, /vendor bytes changed/],
  ['other preserved asset change', `${PREFIX}/Cargo.toml`, /vendor bytes changed/],
  ['literal patch change', `${PREFIX}.quickjs.patch`, /patch bytes changed/],
  ['provenance change', `${PREFIX}.PROVENANCE.md`, /provenance bytes changed/],
  ['manifest change', POLICY, /manifest raw hash changed/],
]) test(`rejects ${label}`, (t) => { const f = fixture(t); change(f.root, file); assert.throws(() => f.run(), error); });
for (const label of ['missing', 'extra']) test(`rejects ${label} vendor file`, (t) => {
  const f = fixture(t);
  if (label === 'missing') fs.unlinkSync(path.join(f.root, rust[0]));
  else fs.writeFileSync(path.join(f.root, PREFIX, 'unlisted.rs'), 'pub fn extra() {}');
  assert.throws(() => f.run(), /inventory has missing or extra/);
});
test('rejects Rust source omitted from independent census', (t) => {
  const f = fixture(t); assert.throws(() => f.run(profile(), [app, ...rust.slice(1)]), /missing from independent census/);
});
test('rejects duplicate selected paths', (t) => {
  const f = fixture(t); assert.throws(() => f.run(profile(), [app, ...rust, rust[0]]), /duplicate selected path/);
});
test('rejects vendor ownership overlap with ordinary modules', (t) => {
  const f = fixture(t), p = profile();
  p.modules.push({ ...p.modules[0], id: 'laundered', dir: PREFIX, files: [manifest.files.find((r) => r.path.endsWith('.rs')).path] });
  assert.throws(() => f.run(p), /overlaps handwritten module/);
});
test('refuses exclusions rather than changing the empty Rust policy', (t) => {
  const f = fixture(t); assert.throws(() => f.run(profile(), [app, ...rust], [app]), /exclusions must stay empty/);
});
test('arbitrary unregistered application Rust remains unprofiled', (t) => {
  const f = fixture(t), extra = 'native-engine/src/unregistered.rs';
  fs.writeFileSync(path.join(f.root, extra), 'pub fn extra() {}');
  const result = f.run(profile(), [app, extra, ...rust]);
  assert.equal(result.ok, false);
  assert.deepEqual(result.violations.map((v) => [v.rule, v.file]), [['coverage-unprofiled-source', extra]]);
});
test('cannot launder an application path through an altered manifest', (t) => {
  const f = fixture(t), changed = structuredClone(manifest);
  changed.root = 'native-engine/src';
  fs.writeFileSync(path.join(f.root, POLICY), JSON.stringify(changed));
  assert.throws(() => f.run(), /manifest raw hash changed/);
});
test('ordinary handwritten 501-line source still fails the unchanged size ceiling and ratchet', (t) => {
  const f = fixture(t), p = profile();
  fs.writeFileSync(path.join(f.root, app), Array.from({ length: 501 }, (_, i) => `// ${i}`).join('\n'));
  assert.equal(f.run(p).ok, true, 'coverage is not a size waiver');
  assert.equal(checkSourceSizes(p, { root: f.root }).ok, false);
  const raised = structuredClone(p); raised.sizeProfiles.ordinary.hardMax = 501;
  assert.equal(compareSizeBaseline(p, raised, { root: f.root }).ok, false);
});
test('rejects vendor symlink and app-to-vendor hardlink alias', (t) => {
  const f = fixture(t), file = path.join(f.root, rust[0]), backup = file + '.original';
  fs.renameSync(file, backup); fs.symlinkSync(backup, file);
  assert.throws(() => f.run(), /non-regular vendor entry/);
  fs.unlinkSync(file); fs.renameSync(backup, file);
  fs.unlinkSync(path.join(f.root, app)); fs.linkSync(file, path.join(f.root, app));
  assert.throws(() => f.run(), /handwritten source aliases vendor/);
});
for (const badPath of ['../escape.rs', '/absolute.rs', 'bad\\path.rs', 'a//b.rs', 'a/./b.rs', 'a\u0000b.rs']) {
  test(`rejects invalid manifest path ${JSON.stringify(badPath)}`, () => {
    const bad = structuredClone(manifest); bad.files[0].path = badPath;
    assert.throws(() => validateVendorManifest(bad), /normalized and repository-relative/);
  });
}
test('rejects duplicate manifest paths', () => {
  const bad = structuredClone(manifest); bad.files[1].path = bad.files[0].path;
  assert.throws(() => validateVendorManifest(bad), /unique and sorted/);
});
test('rejects changed Rust hash even when both hashes remain well-formed', () => {
  const bad = structuredClone(manifest); bad.files.find((r) => r.path.endsWith('.rs')).sha256 = '0'.repeat(64);
  assert.throws(() => validateVendorManifest(bad), /Rust vendor source differs/);
});
test('manifest input remains unchanged after every control', () => {
  assert.deepEqual(fs.readFileSync(path.join(ROOT, POLICY)), manifestBytes);
});
