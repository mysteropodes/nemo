'use strict';
// N25C2g complete native-input registration and caller/budget controls.
// Loaded in the original order by boundaries-rust.test.cjs; same tooling-test owner.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');
const ROOT = path.resolve(__dirname, '../..');
const read = (name) => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

test('N25C2g input child registration keeps full validation at every production caller', () => {
  const sources = ['src/native_request_input.rs', 'src/native_request_input_tests.rs'];
  for (const [profile, id] of [[read('rust.profile.json'), 'rust.mcp.transport'], [read('mcp-rust.profile.json'), 'mcp.transport']]) {
    const module = profile.modules.find(m => m.id === id);
    for (const source of sources) assert.ok(module.files.includes(source));
    assert.ok(module.publicApi.includes(sources[0]));
    assert.equal(profile.sizeProfiles[module.sizeProfile].hardMax, 500);
    const missing = structuredClone(profile);
    missing.modules.find(m => m.id === id).files = module.files.filter(f => !sources.includes(f));
    assert.equal(require('./lib/boundaries-coverage.cjs').checkSourceCoverage(missing, { root: ROOT, sourcePaths: sources.map(f => `nemo-mcp/${f}`) }).ok, false);
  }
  const contract = fs.readFileSync(path.join(ROOT, 'nemo-mcp/src/contract.rs'), 'utf8');
  assert.match(contract, /self\.validate_input\(\)\?;\s*capabilities::validate_native_operation\(&self\.operation\)/);
  // The physical clipboard envelope never selects a larger budget for unrelated operations.
  assert.match(contract, /pub const NATIVE_MAX_MESSAGE_BYTES: usize = 4096;/);
  const wire = fs.readFileSync(path.join(ROOT, 'nemo-mcp/src/wire.rs'), 'utf8');
  assert.match(wire, /pub fn decode_json_bounded<[\s\S]*?if bytes\.len\(\) > max_bytes[\s\S]*?raw_envelope::check_unique\(bytes\)[\s\S]*?native_request_bytes::check\(bytes\)\?;\s*Ok\(serde_json::from_slice\(bytes\)\?\)/);
  assert.match(wire, /pub fn native_request_byte_limit\(operation: &str\) -> usize \{\s*native_request_bytes::request_limit\(operation\)\s*\}/);
  const limits = fs.readFileSync(path.join(ROOT, 'nemo-mcp/src/native_request_bytes.rs'), 'utf8');
  assert.match(limits, /pub\(super\) const MAX_REQUEST_BYTES: usize = 65_536;/);
  // Exactly two operations carry vector geometry and get that budget: a vector record edit and a Fill Brush outline.
  assert.match(limits, /fn request_limit\(operation: &str\) -> usize \{\s*if super::native_clipboard_transport::owns\(operation\) \{\s*super::native_clipboard_transport::MESSAGE_BYTES\s*\} else if operation == "command\.document\.object\.vector\.edit"\s*\|\| operation == "command\.document\.object\.fill\.brush"\s*\{\s*MAX_REQUEST_BYTES\s*\} else \{\s*crate::contract::NATIVE_MAX_MESSAGE_BYTES\s*\}\s*\}/);
  assert.match(limits, /let limit = request_limit\(operation\.as_deref\(\)\.unwrap_or\(""\)\);\s*if request\.len\(\) > limit \{\s*return Err/);

  for (const file of ['nemo-mcp/src/server.rs', 'nemo-mcp/src/wire.rs', 'src-tauri/src/native_revision_sync.rs']) {
    const caller = fs.readFileSync(path.join(ROOT, file), 'utf8');
    assert.match(caller, file === 'nemo-mcp/src/wire.rs' ? /pub async fn call_native\([^{}]*\{\s*request\.validate\(\)\.map_err\(io::Error::other\)\?;/ : /request\.validate\(\)/);
    assert.equal((file === 'nemo-mcp/src/wire.rs' ? caller.replace(/pub fn decode_native_request\([^{}]*\{[\s\S]*?\n\}/, input => {
      assert.match(input, /decode_json_bounded\(bytes, native_clipboard_transport::MESSAGE_BYTES\)\?;[\s\S]*?Some\(&b'\{'\)[\s\S]*?request\.validate_input\(\)\.map_err\(io::Error::other\)\?;\s*Ok\(request\)/);
      assert.doesNotMatch(input, /TcpStream::connect|write_json\(/);
      return '';
    }) : caller).includes('validate_input('), false);
  }
  assert.equal(JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/application/capabilities-v2/native-object.json'), 'utf8')).availability.state, 'available');
  const policy = read('nemo-desktop.edges.json');
  assert.deepEqual(policy.externalCratePorts.nemo_mcp.allowedModules,
    ['rust.desktop.mcp.adapter', 'rust.desktop.native.application.contract', 'rust.desktop.native.application.commands',
      'rust.desktop.native.application.tests', 'rust.desktop.native.host.release.tests']);
  const denied = structuredClone(policy); denied.externalCratePorts.nemo_mcp.allowedModules.pop();
  assert.ok(R.checkRustCrate(read('rust.profile.json'), denied, { root: ROOT }).violations
    .some(v => v.rule === 'external-crate-violation' && v.file === 'src-tauri/src/native_object_host_read_tests.rs'));
});
