'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../..');
const read = file => fs.readFileSync(path.join(root, file), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const coverage = require('./lib/boundaries-coverage.cjs');
const files = ['native_revision_dispatch.rs', 'native_object_client.rs', 'native_object_client_tests.rs', 'native_object_client_admission_tests.rs'];
test('bounded common object client and controls are censused without policy relaxation', () => {
  const declared = profile.modules.flatMap(m => m.files.map(f => `${m.dir}/${f}`));
  for (const file of files) assert.ok(declared.includes(`src-tauri/src/${file}`));
  const without = structuredClone(profile);
  for (const m of without.modules) m.files = m.files.filter(f => !files.includes(f));
  const missing = coverage.checkSourceCoverage(without, {root, sourcePaths: files.map(f => `src-tauri/src/${f}`)});
  for (const file of files) assert.ok(missing.violations.some(v => v.rule === 'coverage-unprofiled-source' && v.file.endsWith(file)));
  const limits = profile.sizeProfiles['Rust production module'];
  assert.equal(limits.hardMax, 500);
  assert.equal(JSON.parse(read('engineering/application/capabilities-v2/native-object.json')).availability.state, 'available');
});
test('production raw entry stays behind main-window and common authority while typed entry validates availability', () => {
  const client = read('src-tauri/src/native_object_client.rs');
  assert.match(client, /wire::decode_json_bounded\(raw\.as_bytes\(\), MAX_BYTES\)/);
  assert.match(client, /state\.dispatch_object_client\(admit_raw\(raw\)\?\)/);
  assert.match(client, /query\.validate\(\)\.is_ok\(\) && visibility\.validate\(\)\.is_ok\(\)/);
  assert.match(client, /require_main\(&window\)\?;\s*dispatch_raw/);
  assert.doesNotMatch(client, /active_mut|\.dispatch\(/);
  const sync = read('src-tauri/src/native_revision_sync.rs');
  assert.match(sync, /request\.validate\(\)/);
  assert.match(sync, /admitted\.into_request\(\)/);
  const shell = read('src-tauri/src/lib.rs');
  assert.match(shell, /application_mcp::nemo_native_object_dispatch,/);
  assert.match(shell, /application_mcp::nemo_native_object_client_status,/);
  for (const file of ['nemo-mcp/src/server.rs', 'nemo-mcp/src/wire.rs']) assert.match(read(file), /request\.validate\(\)/);
});

test('N25D3/D4 command producer and independent consumers cannot lose source registration', () => {
  const without = structuredClone(profile);
  const sources = ['native-engine/src/object_command_dispatch.rs', 'native-engine/tests/object_command_dispatch.rs', 'src-tauri/src/native_object_host_command_tests.rs', 'native-engine/tests/object_history_dispatch.rs', 'src-tauri/src/native_object_host_history_tests.rs'];
  for (const module of without.modules) module.files = module.files.filter(file => !sources.includes(`${module.dir}/${file}`));
  const result = coverage.checkSourceCoverage(without, { root, sourcePaths: sources });
  assert.ok(sources.every(file => result.violations.some(v => v.rule === 'coverage-unprofiled-source' && v.file === file)));
  assert.deepEqual(JSON.parse(read('engineering/application/capabilities-v2/native-object.json')).operations,
    ['query.document.object', 'command.document.object.layer.visibility.set']);
});
