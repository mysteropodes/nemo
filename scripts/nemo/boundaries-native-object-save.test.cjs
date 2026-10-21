'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');
const C = require('./lib/boundaries-coverage.cjs');
const root = path.resolve(__dirname, '../..');
const read = f => fs.readFileSync(path.join(root, f), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const policy = JSON.parse(read('engineering/boundaries/profiles/nemo-desktop.edges.json'));
const source = 'src-tauri/src/native_object_save.rs';
const controls = 'src-tauri/src/native_object_save_tests.rs';
const failures = 'src-tauri/src/native_object_save_failure_tests.rs';

test('private object write consumer and real reopen controls have exact census', () => {
  const commands = profile.modules.find(m => m.id === 'rust.desktop.native.application.commands');
  assert.equal(commands.files.filter(f => f === 'native_object_save.rs').length, 1);
  assert.deepEqual(commands.publicApi, ['native_application_commands.rs',
    'native_reproduction_session.rs', 'native_object_bootstrap.rs']);
  const tests = profile.modules.find(m => m.id === 'rust.desktop.native.host.release.tests');
  assert.equal(tests.files.filter(f => f === 'native_object_save_tests.rs').length, 1);
  assert.equal(tests.files.filter(f => f === path.basename(failures)).length, 1);
  for (const file of [source, controls, failures]) {
    const omitted = structuredClone(profile);
    for (const m of omitted.modules) m.files = m.files.filter(f => f !== path.basename(file));
    assert.ok(C.checkSourceCoverage(omitted, { root, sourcePaths: [file] }).violations
      .some(v => v.rule === 'coverage-unprofiled-source' && v.file === file));
  }
  const tooling = JSON.parse(read('engineering/boundaries/profiles/scripts-nemo.profile.json'));
  const module = tooling.modules.find(m => m.id === 'nemo.test.boundariesRust');
  assert.equal(module.files.filter(f => f === 'boundaries-native-object-save.test.cjs').length, 1);
  assert.deepEqual(module.publicApi, ['boundaries-rust.test.cjs']);
});

test('production command preserves main scope, serializer authority and no-replace commit', () => {
  const producer = read(source), bootstrap = read('src-tauri/src/native_object_bootstrap.rs');
  assert.match(bootstrap, /#\[path = "native_object_save.rs"\]\s*mod save;/);
  assert.match(bootstrap, /#\[cfg\(test\)\]\s*#\[path = "native_object_save_tests.rs"\]/);
  assert.match(read('src-tauri/src/lib.rs'), /native_object_bootstrap::nemo_native_object_save_as,/);
  assert.match(producer, /require_main\(&window\)\?/);
  assert.match(producer, /try_fs_scope\(\)[\s\S]*scope.is_allowed\(path\)/);
  assert.match(producer, /serialize_object_file_snapshot\(&query.to_string\(\)\)/);
  assert.match(read('src-tauri/src/native_object_client.rs'), /fn serialize_object_file_snapshot[\s\S]*dispatch_raw\(self, raw\)/);
  assert.match(producer, /create_new\(true\)/);
  assert.match(producer, /file.sync_all\(\)\?/);
  assert.match(producer, /fs::hard_link\(&staged, &destination\)/);
  assert.doesNotMatch(producer, /native_engine::|active_mut|encode_project|fs::rename|truncate\(true\)/);
  assert.match(read(controls), /bootstrap_request\(&state/);
  assert.match(read(controls), /#\[path = "native_object_save_failure_tests.rs"\]/);
  assert.match(read(failures), /use crate::native_object_bootstrap::save_tests::\*;/);
  assert.match(read('src-tauri/src/native_object_client.rs'), /"saveAvailable":false/);
});

test('adopted desktop graph and actual forbidden host/engine import controls', () => {
  const result = R.checkRustCrate(profile, policy, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.equal(result.moduleCount, 17);
  assert.equal(result.edges.length, 44);
  assert.deepEqual(result.exceptionsApplied, []);
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-save-boundary-'));
  try {
    fs.cpSync(path.join(root, 'src-tauri/src'), path.join(scratch, 'src-tauri/src'), { recursive: true });
    const add = (file, text) => fs.appendFileSync(path.join(scratch, file), `\n${text}\n`);
    add(source, 'use native_engine::object_snapshot::ObjectSnapshot;');
    const privatePort = R.checkRustCrate(profile, policy, { root: scratch });
    assert.equal(privatePort.ok, false);
    assert.ok(privatePort.violations.some(v => v.rule === 'private-port-access' && v.file === source
      && v.detail.path === 'object_snapshot::ObjectSnapshot'));
    fs.writeFileSync(path.join(scratch, source), read(source));
    add('src-tauri/src/native_object_client.rs', 'use crate::native_application_contract::HostResult;');
    const wrongLayer = R.checkRustCrate(profile, policy, { root: scratch });
    assert.equal(wrongLayer.ok, false);
    assert.ok(wrongLayer.violations.some(v => v.rule === 'layer-violation'
      && v.file === 'src-tauri/src/native_object_client.rs'
      && v.detail.targetModule === 'rust.desktop.native.application.contract'));
  } finally { fs.rmSync(scratch, { recursive: true, force: true }); }
});
