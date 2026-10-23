'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const R = require('./lib/boundaries-rust.cjs');
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
  const rawEntry = /pub\(crate\) fn nemo_native_object_dispatch\(\s*window: tauri::Window,\s*state: tauri::State<ApplicationMcp>,\s*request_json: String,\s*\) -> Result<NativeApplicationResponse, String> \{\s*require_main\(&window\)\?;\s*state\.dispatch_object_request_json\(&request_json\)\s*\}/;
  const requestWrapper = /pub\(crate\) fn dispatch_object_request_json\(\s*&self,\s*raw: &str,\s*\) -> Result<NativeApplicationResponse, String> \{\s*dispatch_raw\(self, raw\)\s*\}/;
  assert.match(client, rawEntry);
  assert.match(client, requestWrapper);
  const mainRemoved = client.replace('require_main(&window)?;\n    state.dispatch_object_request_json(&request_json)', 'state.dispatch_object_request_json(&request_json)');
  assert.notEqual(mainRemoved, client, 'the exact raw main-window guard mutation must occur');
  assert.doesNotMatch(mainRemoved, rawEntry, 'the raw route cannot omit its main-window guard');
  const admissionBypass = client.replace('state.dispatch_object_client(admit_raw(raw)?)', 'state.dispatch_object_client(raw)');
  assert.notEqual(admissionBypass, client, 'the bounded admission mutation must occur');
  assert.doesNotMatch(admissionBypass, /state\.dispatch_object_client\(admit_raw\(raw\)\?\)/);
  const wrapperBypass = client.replace('dispatch_raw(self, raw)', 'self.dispatch_object_client(raw)');
  assert.notEqual(wrapperBypass, client, 'the wrapper-to-admission mutation must occur');
  assert.doesNotMatch(wrapperBypass, requestWrapper);
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
    ['query.document.object', 'command.document.object.layer.visibility.set',
      'command.document.object.undo', 'command.document.object.redo']);
});

test('Main SVG export requires its exact dispatch grant and denies ordinary and raw borrowing', (t) => {
  const policy = JSON.parse(read('engineering/boundaries/profiles/nemo-desktop.edges.json'));
  const owner = 'rust.desktop.native.dispatch', item = 'export_job::export_object_svg_frames';
  const grants = policy.externalCratePorts.native_engine.moduleItems;
  assert.ok(grants[owner].includes(item));
  const denied = structuredClone(policy);
  denied.externalCratePorts.native_engine.moduleItems[owner] = grants[owner].filter(grant => grant !== item);
  assert.ok(denied.externalCratePorts.native_engine.moduleItems[owner].includes('export_job::export_object_frames_with_materializer'),
    'removing SVG must retain the PNG grant');
  const result = R.checkRustCrate(profile, denied, { root });
  assert.equal(result.ok, false);
  assert.ok(result.violations.some(v => v.rule === 'private-port-access' && v.module === owner &&
    v.file === 'src-tauri/src/native_object_export.rs' && v.detail.path === item),
    'actual SVG dispatch source requires the exact grant');
  const borrowers = [['rust.desktop.native.application', 'native_application.rs'],
    ['rust.desktop.native.application.tests', 'native_application_tests.rs'],
    ['rust.desktop.native.viewport', 'native_viewport.rs']];
  for (const raw of [false, true]) {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-svg-grant-'));
    t.after(() => fs.rmSync(fixture, { recursive: true, force: true }));
    fs.mkdirSync(path.join(fixture, 'src-tauri'), { recursive: true });
    fs.cpSync(path.join(root, 'src-tauri/src'), path.join(fixture, 'src-tauri/src'), { recursive: true });
    const prefix = raw ? 'r#' : '';
    for (const [module, source] of borrowers) {
      assert.equal(grants[module].includes(item), false);
      fs.appendFileSync(path.join(fixture, 'src-tauri/src', source),
        `\nuse ${prefix}native_engine::${prefix}export_job::${prefix}export_object_svg_frames;\n`);
    }
    const borrowed = R.checkRustCrate(profile, policy, { root: fixture });
    assert.equal(borrowed.ok, false);
    for (const [module, source] of borrowers) assert.ok(borrowed.violations.some(v =>
      v.rule === 'private-port-access' && v.module === module &&
      v.file === `src-tauri/src/${source}` && v.detail.path === item),
      `${module} cannot borrow the dispatch SVG grant (raw=${raw})`);
  }
});
