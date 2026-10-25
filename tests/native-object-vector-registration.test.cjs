'use strict';
// L01-J ordinary registration of the public declaration and the four adapters. Pure file reads: run only on the actually
// composed candidate (the patches applied), where it fails closed on a missing, duplicated or reordered registration.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
const nonblank = (text) => text.split('\n').filter((line) => line.trim()).length;
const OPERATIONS = ['command.document.object.vector.edit', 'command.document.object.parametric.edit', 'command.document.object.path.effects.edit'];
const RUST = ['src/native_vector_contract.rs', 'src/native_vector_contract_shapes.rs', 'src/native_vector_contract_tests.rs'];
const MODULES = [
  ['app.native.object.vector.contract', 'domain', 'native-object-vector-contract.js', 'Domain/application'],
  ['app.native.object.vector.stroke', 'domain', 'native-object-vector-stroke.js', 'Domain/application'],
  ['app.native.object.vector.command', 'application', 'native-object-vector-command.js', 'Domain/application'],
  ['app.native.object.vector.adapter', 'adapters', 'native-object-vector.js', 'Platform/engine adapter'],
];
const ORDER = ['native-font-contract', 'native-font-resources', 'native-object-vector-contract', 'native-object-vector-stroke', 'native-object-vector-command',
  'native-object-vector', 'native-application'];
const BEFORE_SDK = ['native-object-vector-contract'];

function assertBootstrap(html) {
  const scripts = [...html.matchAll(/<script\b([^>]*)>\s*<\/script>/gi)].map((match) => ({ tag: match[0], src: (match[1].match(/\bsrc=["']([^"']+)/) || [])[1] }));
  const at = (name) => {
    const source = 'js/adapters/' + name + '.js';
    const selected = scripts.filter((entry) => entry.src === source);
    assert.equal(selected.length, 1, source + ' must have exactly one registration');
    assert.equal(selected[0].tag, '<script src="' + source + '"></script>', source + ' must be a synchronous classic script');
    return scripts.indexOf(selected[0]);
  };
  // the font contract stays directly followed by the font resources tag (scripts/nemo/boundaries-application.test.cjs)
  assert.ok(html.includes('<script src="js/adapters/native-font-contract.js"></script>\n<script src="js/adapters/native-font-resources.js"></script>'));
  const positions = Object.fromEntries(ORDER.map((name) => [name, at(name)]));
  // dependencies first: the contract before the stroke mapper, the command and the SDK that reads it; the intents last of the four
  assert.ok(positions['native-object-vector-contract'] < positions['native-object-vector-stroke']);
  assert.ok(positions['native-object-vector-stroke'] < positions['native-object-vector-command']);
  assert.ok(positions['native-object-vector-command'] < positions['native-object-vector']);
  for (const name of BEFORE_SDK) assert.ok(positions[name] < positions['native-application'], name + ' must precede the SDK');
  assert.ok(positions['native-font-resources'] < positions['native-object-vector-contract']);
}

test('the descriptor is embedded exactly once and declares the three operations, none of which another descriptor declares', () => {
  // CS1/#2341: nemo-mcp/build.rs discovers capabilities-v2/*.json; read the same set.
  const sources = require('../scripts/nemo/lib/capability-drift.cjs').nativeDescriptorFiles().map((file) => '../../engineering/application/capabilities-v2/' + path.basename(file));
  assert.equal(sources.filter((s) => s === '../../engineering/application/capabilities-v2/native-object-vector.json').length, 1);
  const descriptors = sources.map((source) => JSON.parse(fs.readFileSync(path.resolve(ROOT, 'nemo-mcp/src', source), 'utf8')));
  const operations = descriptors.flatMap((entry) => entry.operations);
  assert.equal(new Set(operations).size, operations.length, 'a native operation is declared by one descriptor only');
  assert.equal(new Set(descriptors.map((entry) => entry.id)).size, descriptors.length);
  const vector = descriptors.find((entry) => entry.id === 'native.object.vector');
  assert.deepEqual(vector.operations, OPERATIONS);
  assert.equal(vector.availability.state, 'available');
  assert.equal(vector.authority.fallback, 'none'); assert.equal(vector.authority.adapterOwnsWritableMirror, false);
  assert.equal(vector.resourceBoundary.maxRequestBytes, 65536);
  assert.deepEqual(vector.resourceBoundary.requestBytesByOperation, { [OPERATIONS[0]]: 65536, [OPERATIONS[1]]: 4096, [OPERATIONS[2]]: 4096 });
  assert.equal(vector.resourceBoundary.maxReadResponseBytes, 4096);
  assert.equal(vector.examples.length, 7);
  assert.deepEqual(descriptors.find((entry) => entry.id === 'native.layer-policy').operations, ['command.document.object.layer.policy.edit'], 'the F06 declaration is untouched');
});

test('the shared schema carries the definitions, one Request branch per operation, the receipt and the widened object read', () => {
  const schema = JSON.parse(read('engineering/application/native-transport-v2.schema.json'));
  assert.equal(JSON.stringify(schema, null, 2) + '\n', read('engineering/application/native-transport-v2.schema.json'), 'canonical two-space JSON');
  for (const name of ['ObjectVectorRecord', 'ObjectVectorCreateRecord', 'ObjectVectorEditPayload', 'ObjectParametricPayload', 'ObjectPathEffectsPayload', 'ObjectVectorEditResult']) {
    assert.ok(schema.$defs[name], name);
  }
  const branches = schema.$defs.Request.oneOf.map((b) => b.allOf && b.allOf[1] && b.allOf[1].properties);
  const payloads = { [OPERATIONS[0]]: 'ObjectVectorEditPayload', [OPERATIONS[1]]: 'ObjectParametricPayload', [OPERATIONS[2]]: 'ObjectPathEffectsPayload' };
  for (const operation of OPERATIONS) {
    const own = branches.filter((properties) => properties && properties.operation && properties.operation.const === operation);
    assert.equal(own.length, 1, operation);
    assert.equal(own[0].payload.$ref, '#/$defs/' + payloads[operation]);
    assert.ok(schema.$defs.Request.oneOf.some((b) => b.allOf[0].$ref === '#/$defs/RequiredRevisionRequest' && b.allOf[1].properties.operation.const === operation), 'revisioned');
  }
  const results = schema.$defs.ResponseBase.properties.result.oneOf.map((entry) => entry.$ref);
  assert.equal(results.filter((ref) => ref === '#/$defs/ObjectVectorEditResult').length, 1);
  assert.deepEqual(schema.$defs.ObjectQueryResult.properties.object.oneOf.map((entry) => entry.$ref), ['#/$defs/ObjectRecord', '#/$defs/ObjectVectorReadRecord']);
  assert.equal(schema.$defs.ObjectVectorReadRecord.properties.vectorExt.$ref, '#/$defs/ObjectVectorReadExt');
  assert.equal(schema.$defs.ObjectVectorReadExt.properties.bitmapBrush.$ref, '#/$defs/ObjectClipboardBitmapBrush');
  assert.equal(schema.$defs.ObjectVectorRecord.properties.vectorExt.$ref, '#/$defs/ObjectVectorExt');
  assert.equal(Object.hasOwn(schema.$defs.ObjectVectorExt.properties, 'bitmapBrush'), false, 'generic vector writers cannot admit bitmap anchors');
  assert.equal(schema.$defs.ObjectRecord.properties.family.const, 'closed-solid-fill-cubic-path', 'the legacy family is untouched');
});

test('the Rust contract and its test child are members of the MCP transport module in both Rust profiles and exist', () => {
  const profile = JSON.parse(read('engineering/boundaries/profiles/mcp-rust.profile.json'));
  const transport = profile.modules.filter((module) => module.id === 'mcp.transport');
  assert.equal(transport.length, 1);
  const census = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
  const censusTransport = census.modules.filter((module) => module.id === 'rust.mcp.transport');
  assert.equal(censusTransport.length, 1);
  for (const file of RUST) {
    assert.equal(transport[0].files.filter((member) => member === file).length, 1, file);
    assert.equal(censusTransport[0].files.filter((member) => member === file).length, 1, 'rust.profile.json rust.mcp.transport lists ' + file);
    assert.equal(census.modules.filter((module) => module.files.includes(file)).length, 1, file + ' has one census declaration');
    assert.ok(nonblank(read('nemo-mcp/' + file)) <= 500, file + ' stays under the hard maximum');
    assert.equal((transport[0].publicApi || []).includes(file) || (censusTransport[0].publicApi || []).includes(file), false, file + ' stays private');
  }
  const production = read('nemo-mcp/src/native_vector_contract.rs');
  assert.ok(nonblank(production) <= 350, 'production contract below the warning size');
  assert.equal(/\bsuper::/.test(production), false, 'the production contract imports through crate paths');
  assert.match(production, /#\[path = "native_vector_contract_shapes\.rs"\]\s*mod shapes;/);
  const shapes = read('nemo-mcp/src/native_vector_contract_shapes.rs');
  assert.ok(nonblank(shapes) <= 350, 'private shape admission below the warning size');
  assert.doesNotMatch(shapes, /\bsuper::/, 'private shape admission imports through crate paths');
  assert.match(production, /use shapes::\{record_shape, shape\};/);
  const contract = read('nemo-mcp/src/native_contract.rs');
  assert.equal(contract.split('pub(crate) mod vector;').length - 1, 1);
  assert.match(contract, /vector::VECTOR \| vector::PARAMETRIC \| vector::EFFECTS => \{\s+vector::request\(operation, payload\)/);
  assert.match(contract, /op if vector::owns\(op\) => \|o, _, p, r\| vector::result\(o, p, r\),/);
  assert.match(read('nemo-mcp/src/wire.rs'), /vector::response_matches\(request, response\)/);
  assert.match(read('nemo-mcp/src/native_request_input.rs'), /crate::native_contract::vector::VECTOR[\s\S]*crate::native_contract::vector::EFFECTS/);
  assert.match(read('nemo-mcp/src/native_object_contract.rs'), /crate::native_contract::vector::FAMILY/);
  for (const file of ['native_contract.rs', 'wire.rs', 'capabilities.rs', 'native_request_input.rs', 'native_object_contract.rs']) assert.ok(nonblank(read('nemo-mcp/src/' + file)) <= 500, file);
  assert.match(read('engineering/boundaries/profiles/rust.coverage.json'), /"nativeVectorPublicContractRegistration": "L01-J\/#2261/);
});

test('the four adapters have ordinary, layered, size-bounded profile membership', () => {
  const profile = JSON.parse(read('engineering/boundaries/profiles/app-js.profile.json'));
  const layers = new Map();
  for (const [id, layer, file, sizeProfile] of MODULES) {
    const members = profile.modules.filter((module) => module.id === id);
    assert.equal(members.length, 1, id);
    assert.deepEqual(members[0], { id, layer, dir: 'src/js/adapters', files: [file], sizeProfile, publicApi: [file] });
    assert.equal(profile.modules.filter((module) => module.files.includes(file)).length, 1, file + ' has one declaration');
    layers.set(file, layer);
    assert.ok(nonblank(read('src/js/adapters/' + file)) <= profile.sizeProfiles[sizeProfile].hardMax, file);
  }
  for (const [file, layer] of layers) {
    const text = read('src/js/adapters/' + file);
    for (const match of text.matchAll(/require\('\.\/([^']+)'\)/g)) {
      const dependency = layers.get(match[1]);
      assert.ok(dependency, file + ' requires an unprofiled module ' + match[1]);
      assert.ok(profile.layerRules[layer].allowedLayers.includes(dependency) || dependency === layer, `${file} (${layer}) may not require ${match[1]} (${dependency})`);
    }
    if (layer === 'domain' || layer === 'application') assert.equal(/(?<![\w.])(?:window|document|localStorage)\.|XMLHttpRequest|\bfetch\s*\(/.test(text), false, file + ' stays host-free');
    // no legacy writer is even named by an adapter
    assert.equal(/\b(?:paper|Paper|serP|desP|shapeGeom|commitShape|SMPathFx|applyParamShape\w*|insertBooleanResult)\b/.test(text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '')), false, file + ' names no legacy writer');
  }
  const sdk = profile.modules.find((module) => module.files.includes('native-application.js'));
  assert.ok(profile.layerRules[sdk.layer].allowedLayers.includes('domain'), 'the SDK may read the domain contract');
});

test('the SDK composes the vector contract on its one contract line and stays at its nonblank-line ceiling', () => {
  const sdk = read('src/js/adapters/native-application.js');
  assert.equal(sdk.split("require('./native-object-vector-contract.js') : root.NemoNativeObjectVectorContract").length - 1, 1);
  assert.equal((sdk.match(/native-object-edit-seam\.js/g) || []).length, 1, 'the edit seam is still the one injected base');
  assert.equal((sdk.match(/native-object-item-contract\.js/g) || []).length, 1);
  assert.ok(nonblank(sdk) <= 500, 'native-application.js nonblank lines');
  assert.match(sdk, /vectorContract\.readRecord\(value\)/);
  assert.match(sdk, /item \? item\.compose\(base\) : base/, 'the lane-6 composition expression is kept verbatim (native-object-item-registration.test.cjs)');
  assert.match(sdk, /, vectorContract\]\.reduce\(\(base, lane\) => \(lane \? lane\.compose\(base\) : base\), /, 'the vector contract is the last lane of the composed reduce');
});

test('the load order is unique, synchronous and fail-closed', () => {
  const html = read('src/index.html');
  assertBootstrap(html);
  const tag = (name) => '<script src="js/adapters/' + name + '.js"></script>';
  for (const name of ORDER) {
    assert.throws(() => assertBootstrap(html.replace(tag(name), '')), assert.AssertionError, 'missing ' + name);
    assert.throws(() => assertBootstrap(html.replace(tag(name), tag(name) + '\n' + tag(name))), assert.AssertionError, 'duplicate ' + name);
    for (const attribute of ['async ', 'defer ', 'type="module" ']) {
      assert.throws(() => assertBootstrap(html.replace(tag(name), tag(name).replace('<script ', '<script ' + attribute))), assert.AssertionError, attribute + name);
    }
  }
  const contractAfterSdk = html.replace(tag('native-object-vector-contract') + '\n', '').replace(tag('native-application'), tag('native-application') + '\n' + tag('native-object-vector-contract'));
  assert.notEqual(contractAfterSdk, html);
  assert.throws(() => assertBootstrap(contractAfterSdk), assert.AssertionError, 'the contract must precede the SDK');
  const swapped = html.replace(tag('native-object-vector-command') + '\n' + tag('native-object-vector'), tag('native-object-vector') + '\n' + tag('native-object-vector-command'));
  assert.notEqual(swapped, html);
  assert.throws(() => assertBootstrap(swapped), assert.AssertionError, 'the command precedes the intents');
});
