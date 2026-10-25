'use strict';
/* Registration of the lane-7 brush resource modules and public declaration: pure file reads of the composed candidate.
 * Every check is a fact the repository's own gates would otherwise find late (load order, module profile, MCP profiles,
 * descriptor list, transport union, schema embed). */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const text = (file) => fs.readFileSync(path.join(root, file), 'utf8');
const json = (file) => JSON.parse(text(file));
const CONTRACT = 'native-bitmap-brush-resources-contract.js';
const ADAPTER = 'native-bitmap-brush-resources.js';
const RUST = ['src/native_brush_resource_contract.rs', 'src/native_brush_resource_contract_tests.rs'];
const count = (haystack, needle) => haystack.split(needle).length - 1;

test('index.html loads the contract, then the adapter, both before the SDK, each exactly once', () => {
  const html = text('src/index.html');
  const at = (name) => html.indexOf('<script src="js/adapters/' + name + '"></script>');
  for (const name of [CONTRACT, ADAPTER, 'native-application.js']) assert.equal(count(html, 'js/adapters/' + name + '"'), 1, name);
  assert.ok(at(CONTRACT) > 0 && at(CONTRACT) < at(ADAPTER), 'the adapter reads the contract global at load');
  assert.ok(at(ADAPTER) < at('native-application.js'), 'the contract must be in place before the SDK reads it');
  assert.ok(at('native-font-contract.js') < at('native-application.js'));
});

test('the module profile registers both modules with the right layers, sizes and public API', () => {
  const modules = json('engineering/boundaries/profiles/app-js.profile.json').modules;
  const find = (id) => modules.filter((m) => m.id === id);
  const contract = find('app.native.bitmap.brush.resources.contract');
  const adapter = find('app.native.bitmap.brush.resources.adapter');
  assert.equal(contract.length, 1);
  assert.equal(adapter.length, 1);
  assert.deepEqual([contract[0].layer, contract[0].dir, contract[0].files, contract[0].sizeProfile, contract[0].publicApi], ['domain', 'src/js/adapters', [CONTRACT], 'Domain/application', [CONTRACT]]);
  assert.deepEqual([adapter[0].layer, adapter[0].dir, adapter[0].files, adapter[0].sizeProfile, adapter[0].publicApi], ['adapters', 'src/js/adapters', [ADAPTER], 'Platform/engine adapter', [ADAPTER]]);
  assert.equal(modules.filter((m) => (m.files || []).includes(CONTRACT) || (m.files || []).includes(ADAPTER)).length, 2, 'no file is claimed twice');
  const lines = (file) => text(file).split('\n').length - 1;
  assert.ok(lines('src/js/adapters/' + CONTRACT) <= 400, 'Domain/application ceiling');
  assert.ok(lines('src/js/adapters/' + ADAPTER) <= 500, 'Platform/engine adapter ceiling');
});

test('layering: the adapter reaches only its own contract, and neither module touches the page, the engine or a writer', () => {
  const strip = (s) => s.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '').replace(/\s\/\/.*$/gm, '');
  const adapter = strip(text('src/js/adapters/' + ADAPTER)), contract = strip(text('src/js/adapters/' + CONTRACT));
  assert.deepEqual([...adapter.matchAll(/require\('([^']+)'\)/g)].map((m) => m[1]), ['./native-bitmap-brush-resources-contract.js']);
  assert.deepEqual([...contract.matchAll(/require\(/g)], []);
  for (const source of [adapter, contract]) {
    for (const name of ['window.', 'localStorage', '__TAURI__', 'fetch(', 'XMLHttpRequest', 'paper', 'SMBitmapBrush', 'SMEngineBridge', 'NemoImageBudget']) {
      assert.equal(source.includes(name), false, name);
    }
    assert.equal(/(?:^|[^\w.'])document\./.test(source), false, 'the page document (not an operation name)');
  }
  // The only globals read: the module registry names and the host passed in by the caller.
  assert.deepEqual([...new Set([...adapter.matchAll(/root\.(\w+)/g)].map((m) => m[1]))].sort(), ['NemoNativeBitmapBrushResources', 'NemoNativeBitmapBrushResourcesContract']);
});

test('the SDK seam composes the contract in front of the font contract without a second seam', () => {
  const sdk = text('src/js/adapters/native-application.js');
  assert.equal(count(sdk, 'native-bitmap-brush-resources-contract.js'), 1);
  assert.equal(count(sdk, 'NemoNativeBitmapBrushResourcesContract'), 1);
  assert.match(sdk, /\[[^\]]*require\('\.\/native-bitmap-brush-resources-contract\.js'\)[^\]]*\]\.reduce\(\(base, lane\) => \(lane \? lane\.compose\(base\) : base\)/, 'composed in the lane reduce of the single SDK line');
  assert.equal(count(sdk, 'const api = factory('), 1, 'one factory call');
  assert.ok(sdk.includes('.reduce((base, lane) => (lane ? lane.compose(base) : base), ((base, item) => item ? item.compose(base) : base)('), 'the font contract (or its lane-6 composition) stays the base');
});

test('the MCP crate registers the validators, the descriptor, the response check and the embedded schema', () => {
  for (const profile of ['engineering/boundaries/profiles/mcp-rust.profile.json', 'engineering/boundaries/profiles/rust.profile.json']) {
    const body = text(profile);
    for (const file of RUST) assert.equal(count(body, '"' + file + '"'), 1, profile + ' ' + file);
  }
  const mcp = json('engineering/boundaries/profiles/mcp-rust.profile.json');
  const transport = mcp.modules.find((m) => (m.files || []).includes('src/native_contract.rs'));
  for (const file of RUST) assert.ok(transport.files.includes(file), file);
  const whole = json('engineering/boundaries/profiles/rust.profile.json');
  const group = whole.modules.find((m) => m.id === 'rust.mcp.transport');
  for (const file of RUST) assert.ok(group.files.includes(file), file);
  for (const file of transport.files.filter((f) => f.startsWith('src/'))) assert.ok(group.files.includes(file), 'the stricter overlay and the whole-tree group agree on ' + file);
  const contract = text('nemo-mcp/src/native_contract.rs');
  assert.ok(contract.includes('#[path = "native_brush_resource_contract.rs"]\npub(crate) mod brush;'));
  assert.ok(contract.includes('brush::PUBLISH | brush::QUERY => brush::request(operation, payload)'));
  assert.ok(contract.includes('| brush::QUERY'), 'the read forbids expectedRevision');
  assert.ok(contract.includes('brush::result(o, p, r)'));
  assert.ok(text('nemo-mcp/src/wire.rs').includes('crate::native_contract::brush::response_matches(request, response)'));
  assert.ok(text('nemo-mcp/src/native_request_input.rs').includes('crate::native_contract::brush::PUBLISH'));
  const server = text('nemo-mcp/src/server.rs');
  assert.equal(count(server, 'native-bitmap-brush-resources-v1.schema.json'), 1);
  assert.equal(count(server, '"nativeBitmapBrushResources"'), 1);
  assert.ok(text('nemo-mcp/tests/stdio.rs').includes('command_schema["$defs"]["nativeBitmapBrushResources"]'));
});

test('the descriptor list names the new descriptor and the declared operations equal the request union operations', () => {
  // CS1/#2341: nemo-mcp/build.rs discovers capabilities-v2/*.json; read the same set.
  const sources = require('../scripts/nemo/lib/capability-drift.cjs').nativeDescriptorFiles().map((file) => '../../engineering/application/capabilities-v2/' + path.basename(file));
  assert.equal(sources.filter((s) => s.endsWith('capabilities-v2/native-bitmap-brush-resources.json')).length, 1);
  const descriptors = sources.map((s) => JSON.parse(fs.readFileSync(path.resolve(root, 'nemo-mcp/src', s), 'utf8')));
  const declared = descriptors.flatMap((d) => d.operations);
  assert.equal(new Set(declared).size, declared.length, 'no operation is declared twice');
  const transport = json('engineering/application/native-transport-v2.schema.json');
  const union = new Set();
  for (const branch of transport.$defs.Request.oneOf) {
    const operation = branch.allOf[1].properties.operation;
    for (const value of operation.enum || [operation.const]) union.add(value);
  }
  assert.deepEqual([...union].sort(), declared.slice().sort());
});

test('no draft file exceeds the size ceilings', () => {
  for (const file of ['tests/native-bitmap-brush-resources.test.cjs', 'tests/native-bitmap-brush-resources-scene.test.cjs', 'tests/native-bitmap-brush-resources-public.test.cjs',
    'tests/native-bitmap-brush-resources-registration.test.cjs', 'tests/native-bitmap-brush-resources-support.cjs', 'nemo-mcp/src/native_brush_resource_contract.rs',
    'nemo-mcp/src/native_brush_resource_contract_tests.rs']) {
    assert.ok(text(file).split('\n').length - 1 <= 500, file);
  }
});
