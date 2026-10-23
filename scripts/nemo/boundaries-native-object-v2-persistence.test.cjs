'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const R = require('./lib/boundaries-rust.cjs');
const C = require('./lib/boundaries-coverage.cjs');
const root = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const policyPath = 'engineering/boundaries/profiles/nemo-desktop.edges.json';
const policy = JSON.parse(read(policyPath));
const command = 'rust.desktop.native.application.commands';
const flow = 'src-tauri/src/native_object_v2_persistence_tests.rs';
const failure = 'src-tauri/src/native_object_v2_refusal_tests.rs';

function admissionRoutes(bootstrap, reopen) {
  const raw = bootstrap.slice(bootstrap.indexOf('pub(crate) fn bootstrap_raw('),
    bootstrap.indexOf('pub(super) fn bootstrap_file_bytes('));
  assert.match(raw, /bootstrap_file_bytes\(state, request.request_id, request.document_json.as_bytes\(\)\)/);
  const file = bootstrap.slice(bootstrap.indexOf('pub(super) fn bootstrap_file_bytes('),
    bootstrap.indexOf('#[tauri::command]'));
  assert.match(file, /if format.format == "nemo.native-project-document"\s*\{\s*state\s*\.install_native_project_file\(bytes\)/);
  assert.match(file, /else\s*\{[\s\S]*let document = decode_storage_project\(bytes\)[\s\S]*\.install_native_object\(document\)/);
  assert.match(reopen, /let bytes = read_file\(state, &request, allowed\)\?;\s*return bootstrap_file_bytes\(state, request.request_id, &bytes\);/);
  assert.match(reopen, /\}\s*bootstrap_raw\(state, raw\)/);
}

test('v2 host/file controls have exact census and preserve original-byte admission routes', () => {
  for (const file of [flow, failure]) {
    assert.equal(profile.modules.flatMap(m => m.files.map(f => `${m.dir}/${f}`)).filter(f => f === file).length, 1);
    const omitted = structuredClone(profile);
    for (const module of omitted.modules) module.files = module.files.filter(f => `${module.dir}/${f}` !== file);
    assert.ok(C.checkSourceCoverage(omitted, { root, sourcePaths: [file] }).violations
      .some(v => v.rule === 'coverage-unprofiled-source' && v.file === file));
  }
  const bootstrap = read('src-tauri/src/native_object_bootstrap.rs');
  const reopen = read('src-tauri/src/native_object_reopen.rs');
  admissionRoutes(bootstrap, reopen);
  assert.throws(() => admissionRoutes(bootstrap.replace('decode_storage_project(bytes)',
    'decode_project(bytes)'), reopen), assert.AssertionError);
  assert.throws(() => admissionRoutes(bootstrap.replace('.install_native_object(document)',
    '.install_native_object(unvalidated)'), reopen), assert.AssertionError);
  assert.throws(() => admissionRoutes(bootstrap, reopen.replace(
    'bootstrap_file_bytes(state, request.request_id, &bytes)', 'bootstrap_raw(state, raw)')),
  assert.AssertionError);
  assert.match(bootstrap, /require_main\(&window\)\?/);
  assert.match(read('src-tauri/src/native_object_save.rs'), /serialize_object_file_snapshot\(&query.to_string\(\)\)/);
  assert.match(read('src-tauri/src/native_object_client.rs'), /"saveAvailable":false/);
  const owner = JSON.parse(read('engineering/boundaries/profiles/scripts-nemo.profile.json')).modules
    .find(m => m.id === 'nemo.test.boundariesRust');
  assert.equal(owner.files.filter(f => f === path.basename(__filename)).length, 1);
  assert.deepEqual(owner.publicApi, ['boundaries-rust.test.cjs']);
});

test('storage decoder remains confined to command and actual host-release tests', () => {
  const items = policy.externalCratePorts.native_engine.moduleItems[command];
  assert.equal(items.filter(item => item === 'object_codec::decode_storage_project').length, 1);
  assert.ok(!items.includes('object_codec::decode_project'));
  for (const [owner, allowed] of Object.entries(policy.externalCratePorts.native_engine.moduleItems)) {
    if (owner !== command && owner !== 'rust.desktop.native.host.release.tests') {
      assert.ok(!allowed.includes('object_codec::decode_storage_project'), owner);
    }
  }
  assert.ok(policy.externalCratePorts.native_engine.moduleItems['rust.desktop.native.host.release.tests']
    .includes('object_codec::decode_project'));
  assert.ok(policy.externalCratePorts.native_engine.moduleItems['rust.desktop.native.host.release.tests']
    .includes('object_codec::decode_storage_project'));
});

test('actual desktop graph admits new command port, denies old command and cross-adapter decoders', () => {
  const result = R.checkRustCrate(profile, policy, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
  assert.equal(result.moduleCount, 17); assert.equal(result.edges.length, 45);
  assert.deepEqual(result.unsupported, []); assert.deepEqual(result.exceptionsApplied, []);
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-v2-host-port-'));
  try {
    fs.cpSync(path.join(root, 'src-tauri/src'), path.join(scratch, 'src-tauri/src'), { recursive: true });
    const bootstrap = 'src-tauri/src/native_object_bootstrap.rs';
    fs.writeFileSync(path.join(scratch, bootstrap), read(bootstrap).replace('object_codec::decode_storage_project', 'object_codec::decode_project'));
    const denied = R.checkRustCrate(profile, policy, { root: scratch });
    assert.ok(denied.violations.some(v => v.rule === 'private-port-access' && v.file === bootstrap
      && v.detail.path === 'object_codec::decode_project'));
    fs.writeFileSync(path.join(scratch, bootstrap), read(bootstrap));
    for (const file of ['src-tauri/src/application_mcp.rs', 'src-tauri/src/native_dispatch.rs']) {
      fs.appendFileSync(path.join(scratch, file), '\nuse native_engine::object_codec::decode_storage_project;\n');
      const refused = R.checkRustCrate(profile, policy, { root: scratch });
      assert.equal(refused.ok, false);
      assert.ok(refused.violations.some(v => v.rule === 'private-port-access' && v.file === file
        && v.detail.path === 'object_codec::decode_storage_project'));
      fs.writeFileSync(path.join(scratch, file), read(file));
    }
  } finally { fs.rmSync(scratch, { recursive: true, force: true }); }
});
