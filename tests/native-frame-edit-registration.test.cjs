'use strict';
// L05-K ordinary registration: load order, module membership, the SDK / session / MCP wiring and the coverage note.
// Run only on the actually composed candidate; each membership check has a real missing-membership control.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { checkSourceCoverage } = require('../scripts/nemo/lib/boundaries-coverage.cjs');
const ROOT = path.resolve(__dirname, '..');
const text = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
const json = (file) => JSON.parse(text(file));
const count = (haystack, needle) => haystack.split(needle).length - 1;
const JS = ['src/js/adapters/native-frame-edit-contract.js', 'src/js/adapters/native-frame-edit-intent.js'];
const RUST = ['nemo-mcp/src/native_frame_edit_contract.rs', 'nemo-mcp/src/native_frame_edit_contract_tests.rs'];

test('the contract loads synchronously before the session and the shared client, the intent after it', () => {
  const html = text('src/index.html');
  const scripts = [...html.matchAll(/<script\b([^>]*)>\s*<\/script>/gi)].map((m) => ({ tag: m[0], src: (m[1].match(/\bsrc=["']([^"']+)/) || [])[1] }));
  const at = (name) => {
    const source = 'js/adapters/' + name + '.js', hits = scripts.filter((s) => s.src === source);
    assert.equal(hits.length, 1, source + ' must be registered once');
    assert.equal(hits[0].tag, '<script src="' + source + '"></script>', source + ' must be synchronous classic');
    return scripts.indexOf(hits[0]);
  };
  const contract = at('native-frame-edit-contract'), intent = at('native-frame-edit-intent');
  assert.ok(contract < at('native-viewport-underlay'), 'before the F06 underlay/parser/Project/ProjectFile/UI run');
  assert.equal(contract + 1, at('native-viewport-underlay'));
  assert.ok(contract < at('native-object-project') && contract < at('native-application') && contract < intent);
  assert.ok(at('native-font-contract') < at('native-application'), 'the font contract stays where the client reads it');
});

test('both JS files have ordinary module membership with real missing and duplicate controls', () => {
  const profile = json('engineering/boundaries/profiles/app-js.profile.json');
  const shapes = [
    { id: 'app.native.frame-edit.contract', layer: 'domain', dir: 'src/js/adapters', files: ['native-frame-edit-contract.js'], sizeProfile: 'Domain/application', publicApi: ['native-frame-edit-contract.js'] },
    { id: 'app.native.frame-edit.intent.adapter', layer: 'adapters', dir: 'src/js/adapters', files: ['native-frame-edit-intent.js'], sizeProfile: 'Platform/engine adapter', publicApi: ['native-frame-edit-intent.js'] }
  ];
  const members = shapes.map((shape) => {
    const found = profile.modules.filter((m) => m.id === shape.id);
    assert.equal(found.length, 1, shape.id); assert.deepEqual(found[0], shape);
    return found[0];
  });
  const options = { root: ROOT, sourcePaths: JS, exclusions: [] };
  assert.equal(checkSourceCoverage({ modules: members }, options).ok, true);
  for (const file of JS) {
    const only = { modules: members.filter((m) => path.posix.join(m.dir, m.files[0]) !== file) };
    assert.deepEqual(checkSourceCoverage(only, options).violations.map(({ rule, file: f }) => ({ rule, file: f })), [{ rule: 'coverage-unprofiled-source', file }]);
  }
  const dup = checkSourceCoverage({ modules: [...members, { ...members[0], id: members[0].id + '.duplicate' }] }, options);
  assert.deepEqual(dup.violations.map(({ rule, file }) => ({ rule, file })), [{ rule: 'coverage-duplicate-declaration', file: JS[0] }]);
});

test('the two Rust files are private members of the nemo-mcp transport module in both profiles', () => {
  const owners = [['engineering/boundaries/profiles/mcp-rust.profile.json', 'mcp.transport'], ['engineering/boundaries/profiles/rust.profile.json', 'rust.mcp.transport']];
  for (const [file, id] of owners) {
    const module = json(file).modules.find((m) => m.id === id);
    assert.equal(module.dir, 'nemo-mcp');
    for (const rust of RUST) {
      const name = rust.slice('nemo-mcp/'.length);
      assert.equal(module.files.filter((entry) => entry === name).length, 1, file + ' ' + name);
      assert.equal(module.publicApi.includes(name), false, name + ' stays private');
    }
    const options = { root: ROOT, sourcePaths: RUST, exclusions: [] };
    const mini = { modules: [{ id, dir: module.dir, files: RUST.map((r) => r.slice('nemo-mcp/'.length)) }] };
    assert.equal(checkSourceCoverage(mini, options).ok, true);
    const missing = { modules: [{ id, dir: module.dir, files: [RUST[0].slice('nemo-mcp/'.length)] }] };
    assert.deepEqual(checkSourceCoverage(missing, options).violations.map(({ rule, file: f }) => ({ rule, file: f })), [{ rule: 'coverage-unprofiled-source', file: RUST[1] }]);
  }
});

test('the SDK, the session and the MCP contract are wired to the one declaration', () => {
  const sdk = text('src/js/adapters/native-application.js');
  assert.equal(count(sdk, "require('./native-frame-edit-contract.js')"), 1);
  assert.match(sdk, /factory\(\(typeof module === 'object' && module\.exports \? require\('\.\/native-frame-edit-contract\.js'\) : root\.NemoNativeFrameEditContract \|\| \{ join: \(base\) => base \}\)\.join\(fontContract\), vectorContract, propertyContract\);/);
  assert.ok(sdk.split('\n').filter((line) => line.trim()).length <= 500, 'native-application.js stays within its 500 nonblank-line hard maximum');
  const project = text('src/js/adapters/native-object-project.js');
  assert.equal(count(project, "require('./native-frame-edit-contract.js')"), 1);
  assert.match(project, /function\(Metadata,Desktop,Properties,FrameEdit,Exposure\)\{/);
  assert.ok(project.split('\n').filter((line) => line.trim()).length <= 500, 'native-object-project.js stays within its 500 nonblank-line hard maximum');
  assert.match(project, /get editFrames\(\)\{return FrameEdit&&typeof ports\.dispatchApplication==='function'&&subscription/);
  assert.match(project, /'nemo_native_dispatch'\);\}\n\s*function editProjectSettings/, 'the frame edit goes through the public application client only');
  assert.equal(count(text('nemo-mcp/src/capabilities.rs'), 'capabilities-v2/native-frame-edit.json'), 1);
  const contract = text('nemo-mcp/src/native_contract.rs');
  assert.match(contract, /#\[path = "native_frame_edit_contract\.rs"\]\s*pub\(crate\) mod frame_edit;/);
  assert.ok(contract.indexOf('native_frame_edit_contract.rs') < contract.indexOf('native_layer_policy_contract.rs'), 'frame_edit precedes layer_policy (rustfmt order)');
  assert.match(contract, /op if frame_edit::declares\(op\) => frame_edit::request\(op, payload\),/);
  assert.match(contract, /if frame_edit::declares\(operation\) \{\s*return frame_edit::result_for_operation\(operation, result\);/);
  assert.match(text('nemo-mcp/src/native_request_input.rs'), /frame_edit::declares\(&self\.operation\)[\s\S]{0,200}requires expectedRevision/);
  const wire = text('nemo-mcp/src/wire.rs');
  assert.match(wire, /#\[path = "native_wire_response_owner\.rs"\]\s*mod response_owner;/);
  assert.equal(count(wire, 'let correlated_identity = response_owner::correlated(request, response);'), 1);
  assert.match(wire, /let identity_is_valid = correlated_identity\s*&& response\.api_version == NATIVE_API_VERSION/);
  const responseOwner = text('nemo-mcp/src/native_wire_response_owner.rs');
  assert.match(responseOwner, /let frame_edit_command = crate::native_contract::frame_edit::declares\(&request\.operation\);/);
  const correlatedOperation = responseOwner.match(/let correlated_operation = ([^;]+);/);
  assert.ok(correlatedOperation, 'the response owner declares its correlated operation set');
  assert.match(correlatedOperation[1], /(?:^|\|\|)\s*frame_edit_command\s*(?:\|\||$)/);
  assert.match(correlatedOperation[1], /(?:^|\|\|)\s*request\.operation == crate::native_contract::metadata::OP\s*(?:\|\||$)/);
  assert.match(correlatedOperation[1], /(?:^|\|\|)\s*policy_command\s*(?:\|\||$)/);
  assert.match(responseOwner, /let correlated_identity = !correlated_operation\s*\|\| \(response\.request_id == request\.request_id\s*&&/);
  assert.match(wire, /frame_edit::revision_matches_for_operation\(\s*&request\.operation,\s*request\.expected_revision,\s*response\.content_revision,/);
  const rust = text('nemo-mcp/src/native_frame_edit_contract.rs');
  assert.doesNotMatch(rust.split('#[cfg(test)]')[0], /\bsuper::/, 'no relative import in production code');
  assert.match(rust, /#\[cfg\(test\)\]\s*#\[path = "native_frame_edit_contract_tests\.rs"\]\s*mod tests;/);
});

test('the frozen classic-startup sequence of the opacity cutover test names both scripts in load order', () => {
  const cutover = text('tests/native-opacity-cutover.test.cjs');
  const at = (name) => cutover.indexOf("'js/adapters/" + name + ".js'");
  assert.ok(at('native-frame-edit-contract') > 0 && at('native-frame-edit-contract') < at('native-viewport-underlay'));
  assert.ok(at('native-frame-edit-intent') > at('native-project-settings-desktop') && at('native-frame-edit-intent') < at('native-object-reference'));
});

test('the coverage policy records the registration and no host file widens the raw client list', () => {
  const policy = json('engineering/boundaries/profiles/rust.coverage.json');
  const note = policy.nativeFrameEditPublicRegistration || (policy.provenance && policy.provenance.nativeFrameEditPublicRegistration);
  assert.match(note, /L05-K\/#2248/);
  assert.deepEqual(policy.exclusions, []);
  // The public route (nemo_native_dispatch) admits through the catalog; the bounded raw client list is unchanged.
  assert.doesNotMatch(text('src-tauri/src/native_object_client.rs'), /frames\.insert|layer\.key\.promote/);
});
