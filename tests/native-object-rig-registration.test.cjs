'use strict';
// L03-T ordinary registration of the public declaration and the adapters. Run only on the actually composed candidate.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
const nonblank = (text) => text.split('\n').filter((line) => line.trim()).length;
const FAMILIES = ['image-mesh', 'widget', 'rig'];
const RUST = ['src/native_object_rig_contract.rs', 'src/native_object_rig_contract_tests.rs', 'src/native_object_rig_receipt_tests.rs'];
const MODULES = [
  ['app.native.object.rig.contract', 'domain', 'native-object-rig-contract.js', 'Domain/application'],
  ['app.native.object.rig.seam', 'domain', 'native-object-rig-seam.js', 'Domain/application'],
  ['app.native.object.rig.command', 'application', 'native-object-rig-command.js', 'Domain/application'],
  ['app.native.object.image.mesh.adapter', 'adapters', 'native-object-image-mesh.js', 'Platform/engine adapter'],
  ['app.native.object.widget.adapter', 'adapters', 'native-object-widget.js', 'Platform/engine adapter'],
  ['app.native.object.rig.adapter', 'adapters', 'native-object-rig.js', 'Platform/engine adapter'],
];
const BEFORE_SDK = ['native-font-contract', 'native-font-resources', 'native-object-rig-contract', 'native-object-rig-seam', 'native-application'];
const AFTER_SDK = ['native-object-rig-command', 'native-object-image-mesh', 'native-object-widget', 'native-object-rig'];

function assertBootstrap(html) {
  // G41: the font contract tag stays directly followed by the font resources tag (scripts/nemo/boundaries-application.test.cjs).
  assert.ok(html.includes('<script src="js/adapters/native-font-contract.js"></script>\n<script src="js/adapters/native-font-resources.js"></script>'), 'font contract and resources stay adjacent');
  const scripts = [...html.matchAll(/<script\b([^>]*)>\s*<\/script>/gi)].map((match) => ({ tag: match[0], src: (match[1].match(/\bsrc=["']([^"']+)/) || [])[1] }));
  const positions = BEFORE_SDK.concat(AFTER_SDK).map((name) => {
    const source = 'js/adapters/' + name + '.js';
    const selected = scripts.filter((entry) => entry.src === source);
    assert.equal(selected.length, 1, source + ' must have exactly one registration');
    assert.equal(selected[0].tag, '<script src="' + source + '"></script>', source + ' must be a synchronous classic script');
    return scripts.indexOf(selected[0]);
  });
  assert.deepEqual(positions, positions.slice().sort((a, b) => a - b), 'font contract, resources, lane-3 contract, seam, SDK, command, adapters must load in that order');
  assert.equal(new Set(positions).size, positions.length);
}

test('the three capability descriptors are embedded exactly once and declare six unique operations', () => {
  // CS1/#2341: nemo-mcp/build.rs discovers capabilities-v2/*.json; read the same set.
  const sources = require('../scripts/nemo/lib/capability-drift.cjs').nativeDescriptorFiles().map((file) => '../../engineering/application/capabilities-v2/' + path.basename(file));
  for (const family of FAMILIES) assert.equal(sources.filter((source) => source === '../../engineering/application/capabilities-v2/native-object-' + family + '.json').length, 1, family);
  const descriptors = sources.map((source) => JSON.parse(fs.readFileSync(path.resolve(ROOT, 'nemo-mcp/src', source), 'utf8')));
  const operations = descriptors.flatMap((entry) => entry.operations);
  assert.equal(new Set(operations).size, operations.length, 'a native operation is declared by one descriptor only');
  assert.equal(new Set(descriptors.map((entry) => entry.id)).size, descriptors.length);
  assert.equal(operations.filter((operation) => /^command\.document\.object\.(?:image\.mesh\.edit|widget\.(?:create|write|settings)|rig\.(?:edit|pose))$/.test(operation)).length, 6);
  assert.deepEqual(descriptors.find((entry) => entry.id === 'native.layer-policy').operations, ['command.document.object.layer.policy.edit'], 'the F06 declaration is untouched');
});

test('the Rust contract and its two test children are members of the MCP transport module in both Rust profiles and exist', () => {
  const profile = JSON.parse(read('engineering/boundaries/profiles/mcp-rust.profile.json'));
  const transport = profile.modules.filter((module) => module.id === 'mcp.transport');
  assert.equal(transport.length, 1);
  assert.equal(transport[0].dir, 'nemo-mcp');
  assert.equal(transport[0].layer, 'adapters');
  for (const file of RUST.concat('src/native_wire_response_owner.rs')) {
    assert.equal(transport[0].files.filter((member) => member === file).length, 1, file);
    assert.ok(nonblank(read('nemo-mcp/' + file)) <= 500, file + ' stays under the hard maximum');
  }
  const census = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
  const censusTransport = census.modules.filter((module) => module.id === 'rust.mcp.transport');
  assert.equal(censusTransport.length, 1);
  assert.equal(censusTransport[0].dir, 'nemo-mcp');
  assert.equal(censusTransport[0].layer, 'adapters');
  for (const file of RUST.concat('src/native_wire_response_owner.rs')) {
    assert.equal(censusTransport[0].files.filter((member) => member === file).length, 1, 'rust.profile.json rust.mcp.transport lists ' + file);
    assert.equal(census.modules.filter((module) => module.files.includes(file)).length, 1, file + ' has one census declaration');
    assert.equal((censusTransport[0].publicApi || []).includes(file), false, file + ' stays private');
    assert.equal((transport[0].publicApi || []).includes(file), false, file + ' stays private in the MCP profile');
  }
  const production = read('nemo-mcp/src/native_object_rig_contract.rs');
  assert.ok(nonblank(production) <= 500);
  assert.equal(/\bsuper::/.test(production), false, 'the production contract imports through crate paths (ruling R5 style)');
  const contract = read('nemo-mcp/src/native_contract.rs');
  assert.equal(contract.split('pub(crate) mod object_rig;').length - 1, 1);
  assert.match(contract, /#\[path = "native_object_rig_contract\.rs"\]\s*pub\(crate\) mod object_rig;/);
  assert.match(contract, /op if object_rig::admitted\(op\) => object_rig::request\(op, payload\)/);
  assert.match(contract, /op if object_rig::admitted\(op\) => \|o, _, _, r\| object_rig::result\(o, r\),/);
  const wire = read('nemo-mcp/src/wire.rs');
  assert.match(wire, /object_rig::response_matches\(request, response\)/);
  assert.match(wire, /#\[path = "native_wire_response_owner\.rs"\]\s*mod response_owner;/);
  assert.equal(wire.split('let correlated_identity = response_owner::correlated(request, response);').length, 2);
  assert.match(wire, /let identity_is_valid = correlated_identity\s*&& response\.api_version == NATIVE_API_VERSION/);
  assert.match(wire, /if identity_is_valid && disposition_is_valid/);
  const responseOwner = read('nemo-mcp/src/native_wire_response_owner.rs');
  const correlatedOperation = responseOwner.match(/let correlated_operation = ([^;]+);/);
  assert.ok(correlatedOperation, 'the response owner declares the correlated operation set');
  assert.match(correlatedOperation[1], /(?:^|\|\|)\s*crate::native_contract::object_rig::admitted\(&request\.operation\)\s*(?:\|\||$)/);
  const correlatedIdentity = responseOwner.match(/let correlated_identity = ([\s\S]*?);\s*correlated_identity/);
  assert.ok(correlatedIdentity, 'the correlated operation set gates the returned identity');
  assert.match(correlatedIdentity[1], /^!correlated_operation\s*\|\| \(response\.request_id == request\.request_id\s*&&/);
  assert.match(correlatedIdentity[1], /\.unwrap_or\(\s*response\.instance_id == request\.instance_id\s*&& \(response\.document_id == request\.document_id/);
  assert.match(correlatedIdentity[1], /\|\| \(!response\.ok\s*&& response[\s\S]*?e\.code == "wrong_document"\)/);
  assert.match(correlatedIdentity[1], /&& serde_json::to_vec\(response\)\.is_ok_and\(\|b\|\s*\{\s*b\.len\(\) <= crate::wire::native_response_byte_limit\(&request\.operation\)\s*\}\)\)$/);
  const clipboardTransport = read('nemo-mcp/src/native_clipboard_transport.rs');
  const responseLimit = clipboardTransport.match(/pub\(crate\) fn response_limit\([^{}]*\{[\s\S]*?\n\}/);
  assert.ok(responseLimit, 'one operation-specific response budget helper exists');
  assert.deepEqual(responseLimit[0].match(/"command\.document\.object\.frames\.clipboard\.\w+"/g),
    ['"command.document.object.frames.clipboard.copy"', '"command.document.object.frames.clipboard.cut"']);
  assert.match(responseLimit[0], /MESSAGE_BYTES\s*\}\s*else\s*\{\s*4096/);
  const nativeCallStart = wire.indexOf('pub async fn call_native(');
  const nativeCallEnd = wire.indexOf('\npub fn validate_native_response(');
  assert.ok(nativeCallStart >= 0 && nativeCallEnd > nativeCallStart);
  const nativeCall = wire.slice(nativeCallStart, nativeCallEnd);
  assert.match(nativeCall, /validate_native_response\(&response, &response_contract\)\?;/);
  assert.match(nativeCall, /response\.request_id != request_id\s*\|\| response\.instance_id != endpoint\.instance_id/);
  assert.match(read('nemo-mcp/src/native_request_input.rs'), /native object rig command requires expectedRevision/);
});

test('the JS modules have ordinary, layered, size-bounded profile membership and keep the domain host-free', () => {
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
  layers.set('native-font-contract.js', profile.modules.find((module) => module.id === 'app.native.font.contract').layer);
  layers.set('native-application.js', profile.modules.find((module) => module.files.includes('native-application.js')).layer);
  // Composition (batch 8): the shared SDK also requires the contracts of the other lanes; admit every profiled module reachable from it.
  for (const queue = ['native-application.js']; queue.length;) for (const match of read('src/js/adapters/' + queue.pop()).matchAll(/require\('\.\/([^']+)'\)/g)) { const owner = profile.modules.find((module) => module.dir === 'src/js/adapters' && module.files.includes(match[1])); if (owner && !layers.has(match[1])) { layers.set(match[1], owner.layer); queue.push(match[1]); } }
  assert.equal(layers.get('native-object-rig-seam.js'), 'domain', 'the seam stays below the adapters-layer SDK');
  for (const [file, layer] of layers) {
    const text = read('src/js/adapters/' + file);
    for (const match of text.matchAll(/require\('\.\/([^']+)'\)/g)) {
      const dependency = layers.get(match[1]);
      assert.ok(dependency, file + ' requires an unprofiled module ' + match[1]);
      assert.ok(profile.layerRules[layer].allowedLayers.includes(dependency) || dependency === layer, `${file} (${layer}) may not require ${match[1]} (${dependency})`);
    }
    if (layer === 'domain') assert.equal(/(?<![\w.])(?:window|document|localStorage)\.|XMLHttpRequest|\bfetch\s*\(/.test(text), false, file + ' stays host-free');
  }
});

test('the SDK injects the lane-3 seam on one in-place line and stays at the 500-line maximum', () => {
  const sdk = read('src/js/adapters/native-application.js');
  assert.ok(/require\('\.\/native-object-rig-(?:seam|contract)\.js'\)/.test(sdk) || sdk.includes('NemoNativeObjectRigSeam') || sdk.includes('NemoNativeObjectRigContract'), 'the lane-3 contract is injected');
  assert.ok(nonblank(sdk) <= 500, 'native-application.js nonblank lines');
});

test('the load order is unique, synchronous and fail-closed', () => {
  const html = read('src/index.html');
  assertBootstrap(html);
  const tag = (name) => '<script src="js/adapters/' + name + '.js"></script>';
  for (const name of BEFORE_SDK.concat(AFTER_SDK)) {
    assert.throws(() => assertBootstrap(html.replace(tag(name), '')), assert.AssertionError, 'missing ' + name);
    assert.throws(() => assertBootstrap(html.replace(tag(name), tag(name) + '\n' + tag(name))), assert.AssertionError, 'duplicate ' + name);
    for (const attribute of ['async ', 'defer ', 'type="module" ']) {
      assert.throws(() => assertBootstrap(html.replace(tag(name), tag(name).replace('<script ', '<script ' + attribute))), assert.AssertionError, attribute + name);
    }
  }
  const seamAfterSdk = html.replace(tag('native-object-rig-seam') + '\n', '').replace(tag('native-application'), tag('native-application') + '\n' + tag('native-object-rig-seam'));
  assert.notEqual(seamAfterSdk, html);
  assert.throws(() => assertBootstrap(seamAfterSdk), assert.AssertionError, 'the seam must precede the SDK');
  assert.throws(() => assertBootstrap(html.replace(tag('native-object-rig-command') + '\n' + tag('native-object-image-mesh'), tag('native-object-image-mesh') + '\n' + tag('native-object-rig-command'))), assert.AssertionError);
});
