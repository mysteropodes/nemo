'use strict';
// L02-J ordinary registration of the public declaration and the adapters. Run only on the actually composed candidate.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
const nonblank = (text) => text.split('\n').filter((line) => line.trim()).length;
const FAMILIES = ['layer-lifecycle', 'group', 'selection-edit', 'boolean', 'fill'];
const RUST = ['src/native_object_edit_contract.rs', 'src/native_object_edit_contract_tests.rs', 'src/native_object_edit_receipt_tests.rs'];
const MODULES = [
  ['app.native.object.edit.contract', 'domain', 'native-object-edit-contract.js', 'Domain/application'],
  ['app.native.object.edit.intent.core', 'domain', 'native-object-edit-intent-core.js', 'Domain/application'],
  ['app.native.object.edit.seam', 'domain', 'native-object-edit-seam.js', 'Domain/application'],
  ['app.native.object.group.intent.adapter', 'adapters', 'native-object-group-intent.js', 'Platform/engine adapter'],
  ['app.native.object.layer-lifecycle.intent.adapter', 'adapters', 'native-object-layer-lifecycle-intent.js', 'Platform/engine adapter'],
];
const ORDER = ['native-font-contract', 'native-font-resources', 'native-object-edit-contract', 'native-object-edit-seam', 'native-application',
  'native-object-edit-intent-core', 'native-object-group-intent', 'native-object-layer-lifecycle-intent'];

function assertBootstrap(html) {
  // G41: the font contract tag stays directly followed by the font resources tag (scripts/nemo/boundaries-application.test.cjs).
  assert.ok(html.includes('<script src="js/adapters/native-font-contract.js"></script>\n<script src="js/adapters/native-font-resources.js"></script>'), 'font contract and resources stay adjacent');
  const scripts = [...html.matchAll(/<script\b([^>]*)>\s*<\/script>/gi)].map((match) => ({ tag: match[0], src: (match[1].match(/\bsrc=["']([^"']+)/) || [])[1] }));
  const positions = ORDER.map((name) => {
    const source = 'js/adapters/' + name + '.js';
    const selected = scripts.filter((entry) => entry.src === source);
    assert.equal(selected.length, 1, source + ' must have exactly one registration');
    assert.equal(selected[0].tag, '<script src="' + source + '"></script>', source + ' must be a synchronous classic script');
    return scripts.indexOf(selected[0]);
  });
  assert.deepEqual(positions, positions.slice().sort((a, b) => a - b), 'font contract, font resources, edit contract, seam, SDK, core, intents must load in that order');
  assert.equal(new Set(positions).size, ORDER.length);
}

test('the five capability descriptors are embedded exactly once and declare 18 unique operations', () => {
  const rust = read('nemo-mcp/src/capabilities.rs');
  const start = rust.indexOf('pub const NATIVE_CAPABILITY_SOURCES:');
  assert.ok(start >= 0);
  const block = rust.slice(start, rust.indexOf('];', start));
  const sources = [...block.matchAll(/include_str!\s*\(\s*"([^"]+)"\s*\)/g)].map((match) => match[1]);
  for (const family of FAMILIES) {
    const relative = '../../engineering/application/capabilities-v2/native-object-' + family + '.json';
    assert.equal(sources.filter((source) => source === relative).length, 1, family);
  }
  const descriptors = sources.map((source) => JSON.parse(fs.readFileSync(path.resolve(ROOT, 'nemo-mcp/src', source), 'utf8')));
  const operations = descriptors.flatMap((entry) => entry.operations);
  assert.equal(new Set(operations).size, operations.length, 'a native operation is declared by one descriptor only');
  assert.equal(new Set(descriptors.map((entry) => entry.id)).size, descriptors.length);
  assert.equal(operations.filter((operation) => /^command\.document\.object\.(?:layer\.(?:delete|duplicate|rename|reorder|split)|group\.|selection\.|clipboard\.paste|boolean\.|fill\.trace)/.test(operation)).length, 18);
  const policy = descriptors.find((entry) => entry.id === 'native.layer-policy');
  assert.deepEqual(policy.operations, ['command.document.object.layer.policy.edit'], 'the F06 declaration is untouched');
});

test('the Rust contract and its two test children are members of the MCP transport module in both Rust profiles and exist', () => {
  const profile = JSON.parse(read('engineering/boundaries/profiles/mcp-rust.profile.json'));
  const transport = profile.modules.filter((module) => module.id === 'mcp.transport');
  assert.equal(transport.length, 1);
  for (const file of RUST) {
    assert.equal(transport[0].files.filter((member) => member === file).length, 1, file);
    const text = read('nemo-mcp/' + file);
    assert.ok(nonblank(text) <= 500, file + ' stays under the hard maximum');
  }
  // The repo-wide Rust census profile lists every nemo-mcp source file too: an unlisted .rs is `coverage-unprofiled-source`.
  const census = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
  const censusTransport = census.modules.filter((module) => module.id === 'rust.mcp.transport');
  assert.equal(censusTransport.length, 1);
  for (const file of RUST) {
    assert.equal(censusTransport[0].files.filter((member) => member === file).length, 1, 'rust.profile.json rust.mcp.transport lists ' + file);
    assert.equal(census.modules.filter((module) => module.files.includes(file)).length, 1, file + ' has one census declaration');
    assert.equal((censusTransport[0].publicApi || []).includes(file), false, file + ' stays private');
    assert.equal((transport[0].publicApi || []).includes(file), false, file + ' stays private in the MCP profile');
  }
  const production = read('nemo-mcp/src/native_object_edit_contract.rs');
  assert.ok(nonblank(production) <= 350, 'production contract below the warning size');
  assert.equal(/\bsuper::/.test(production), false, 'the production contract imports through crate paths (ruling R5 style)');
  const contract = read('nemo-mcp/src/native_contract.rs');
  assert.equal(contract.split('pub(crate) mod object_edit;').length - 1, 1);
  assert.match(contract, /op if object_edit::admitted\(op\) => object_edit::request\(op, payload\)/);
  assert.match(contract, /if object_edit::admitted\(operation\) \{\s+return object_edit::result\(operation, result\);/);
  assert.match(read('nemo-mcp/src/wire.rs'), /object_edit::response_matches\(request, response\)/);
  assert.match(read('nemo-mcp/src/native_request_input.rs'), /native object edit command requires expectedRevision/);
});

test('the JS contract, core, seam and two intents have ordinary, layered, size-bounded profile membership', () => {
  const profile = JSON.parse(read('engineering/boundaries/profiles/app-js.profile.json'));
  const layers = new Map();
  for (const [id, layer, file, sizeProfile] of MODULES) {
    const members = profile.modules.filter((module) => module.id === id);
    assert.equal(members.length, 1, id);
    assert.deepEqual(members[0], { id, layer, dir: 'src/js/adapters', files: [file], sizeProfile, publicApi: [file] });
    assert.equal(profile.modules.filter((module) => module.files.includes(file)).length, 1, file + ' has one declaration');
    layers.set(file, layer);
    const text = read('src/js/adapters/' + file);
    assert.ok(nonblank(text) <= profile.sizeProfiles[sizeProfile].hardMax, file);
  }
  const font = profile.modules.find((module) => module.id === 'app.native.font.contract');
  layers.set('native-font-contract.js', font.layer);
  // The common SDK is the consumer of the seam: its adapters-to-seam edge must be allowed by the layer rules.
  const sdk = profile.modules.find((module) => module.files.includes('native-application.js'));
  layers.set('native-application.js', sdk.layer);
  // Composition with L06-M: its item contract composes in front of the seam on the same SDK line (a profiled domain module).
  const itemContract = profile.modules.find((module) => module.id === 'app.native.object.item.contract');
  if (itemContract) layers.set(itemContract.files[0], itemContract.layer);
  // Composition (batch 8): the shared SDK also requires the contracts of the other lanes; admit every profiled module reachable from it.
  for (const queue = ['native-application.js']; queue.length;) for (const match of read('src/js/adapters/' + queue.pop()).matchAll(/require\('\.\/([^']+)'\)/g)) { const owner = profile.modules.find((module) => module.dir === 'src/js/adapters' && module.files.includes(match[1])); if (owner && !layers.has(match[1])) { layers.set(match[1], owner.layer); queue.push(match[1]); } }
  assert.equal(layers.get('native-object-edit-seam.js'), 'domain', 'the seam stays below the adapters-layer SDK');
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

test('the SDK injects the composed seam on one in-place line and stays at the 500-line maximum', () => {
  const sdk = read('src/js/adapters/native-application.js');
  assert.equal(sdk.split("require('./native-object-edit-seam.js') : root.NemoNativeObjectEditSeam").length - 1, 1);
  assert.equal(sdk.includes('native-object-edit-contract'), false, 'only the seam is injected');
  assert.ok(nonblank(sdk) <= 500, 'native-application.js nonblank lines');
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
  const seamAfterSdk = html.replace(tag('native-object-edit-seam') + '\n', '').replace(tag('native-application'), tag('native-application') + '\n' + tag('native-object-edit-seam'));
  assert.notEqual(seamAfterSdk, html);
  assert.throws(() => assertBootstrap(seamAfterSdk), assert.AssertionError, 'the seam must precede the SDK');
  assert.throws(() => assertBootstrap(html.replace(tag('native-object-edit-intent-core') + '\n' + tag('native-object-group-intent'), tag('native-object-group-intent') + '\n' + tag('native-object-edit-intent-core'))), assert.AssertionError);
});
