'use strict';
/* Registration facts of the lane-6 adapters and public declarations. Pure file reads: run on the composed candidate,
 * after the lead applied the patches of this leaf. It does not replace the repository boundary checks. */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(root, file), 'utf8');
const nonblank = (file) => read(file).split('\n').filter((line) => line.trim()).length;
const MODULES = [
  ['app.native.object.item.contract', 'domain', 'native-object-item-contract.js', 'Domain/application'],
  ['app.native.object.item.command', 'application', 'native-object-item-command.js', 'Domain/application'],
  ['app.native.object.raster.asset.adapter', 'adapters', 'native-object-raster-asset.js', 'Platform/engine adapter'],
  ['app.native.object.text.layout.adapter', 'adapters', 'native-object-text-layout.js', 'Platform/engine adapter'],
];

test('the four modules are registered in their layers and respect the layer direction', () => {
  const profile = JSON.parse(read('engineering/boundaries/profiles/app-js.profile.json'));
  for (const [id, layer, file, size] of MODULES) {
    const entries = profile.modules.filter((m) => m.id === id);
    assert.equal(entries.length, 1, id);
    assert.deepEqual([entries[0].layer, entries[0].dir, entries[0].files, entries[0].sizeProfile, entries[0].publicApi], [layer, 'src/js/adapters', [file], size, [file]], id);
    assert.ok(fs.existsSync(path.join(root, 'src/js/adapters', file)), file);
    assert.ok(nonblank('src/js/adapters/' + file) <= profile.sizeProfiles[size].hardMax, file + ' within its hard ceiling');
  }
  assert.ok(profile.layerRules.adapters.allowedLayers.includes('application') && profile.layerRules.adapters.allowedLayers.includes('domain'));
  assert.deepEqual(profile.layerRules.application.allowedLayers, ['domain']);
  assert.deepEqual(profile.layerRules.domain.allowedLayers, ['domain']);
  const requires = (file) => [...read('src/js/adapters/' + file).matchAll(/require\('\.\/([^']+)'\)/g)].map((m) => m[1]);
  assert.deepEqual(requires('native-object-item-contract.js'), []);
  assert.deepEqual(requires('native-object-item-command.js'), ['native-object-item-contract.js']);
  assert.deepEqual(requires('native-object-raster-asset.js').sort(), ['native-object-item-command.js', 'native-object-item-contract.js']);
  assert.deepEqual(requires('native-object-text-layout.js').sort(), ['native-object-item-command.js', 'native-object-item-contract.js']);
});

test('load order: contract, command, adapters, all before the common SDK that composes the contract', () => {
  const html = read('src/index.html');
  const at = (file) => {
    const hits = [...html.matchAll(new RegExp('<script src="js/adapters/' + file.replace('.', '\\.') + '"></script>', 'g'))];
    assert.equal(hits.length, 1, file);
    return hits[0].index;
  };
  const order = ['native-font-contract.js', 'native-object-item-contract.js', 'native-object-item-command.js', 'native-object-raster-asset.js', 'native-object-text-layout.js', 'native-application.js'].map(at);
  assert.deepEqual(order, order.slice().sort((a, b) => a - b));
});

test('the SDK seam composes the contract without growing the file and the session names the shared route', () => {
  const sdk = read('src/js/adapters/native-application.js');
  assert.match(sdk, /require\('\.\/native-object-item-contract\.js'\)/);
  assert.match(sdk, /root\.NemoNativeObjectItemContract/);
  assert.match(sdk, /item \? item\.compose\(base\) : base/);
  assert.ok(nonblank('src/js/adapters/native-application.js') <= 500);
  assert.match(read('src/js/adapters/native-object-project.js'), /runFontResource:runFontResource,runResourceCommand:runFontResource,/);
  assert.ok(nonblank('src/js/adapters/native-object-project.js') <= 500);
});

test('the MCP crate registers the validators, the descriptors and the shared response check', () => {
  const profile = JSON.parse(read('engineering/boundaries/profiles/mcp-rust.profile.json'));
  const files = profile.modules.flatMap((m) => m.files || []);
  for (const file of ['src/native_item_contract.rs', 'src/native_item_contract_tests.rs']) {
    assert.equal(files.filter((f) => f === file).length, 1, file);
    assert.ok(nonblank('nemo-mcp/' + file) <= 500, file);
  }
  // The whole-tree profile carries the same list in group rust.mcp.transport (the stricter MCP overlay must stay consistent with it).
  const whole = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json')).modules.filter((m) => m.id === 'rust.mcp.transport');
  assert.equal(whole.length, 1);
  const wholeFiles = whole[0].files;
  for (const file of files.filter((f) => f.startsWith('src/'))) assert.ok(wholeFiles.includes(file), 'rust.profile.json rust.mcp.transport lists ' + file);
  for (const file of ['src/native_item_contract.rs', 'src/native_item_contract_tests.rs']) assert.equal(wholeFiles.filter((f) => f === file).length, 1, file);
  assert.match(read('nemo-mcp/src/native_contract.rs'), /#\[path = "native_item_contract\.rs"\]\npub\(crate\) mod item;/);
  assert.match(read('nemo-mcp/src/native_contract.rs'), /item::ITEM \| item::ATTACH \| item::REMOVE => item::request\(operation, payload\)/);
  assert.match(read('nemo-mcp/src/native_contract.rs'), /op if item::owns\(op\) => \|o, _, p, r\| item::result\(o, p, r\),/);
  // CS1/#2341: nemo-mcp/build.rs embeds every capabilities-v2 file it discovers.
  const sources = require('../scripts/nemo/lib/capability-drift.cjs').nativeDescriptorFiles().map((file) => path.basename(file));
  for (const file of ['native-object-item.json', 'native-object-asset.json']) assert.ok(sources.includes(file), file);
  assert.match(read('nemo-mcp/src/wire.rs'), /crate::native_contract::item::response_matches\(request, response\)/);
  for (const op of ['ITEM', 'ATTACH', 'REMOVE']) assert.match(read('nemo-mcp/src/native_request_input.rs'), new RegExp('crate::native_contract::item::' + op));
  // The relative references of the request union resolve through the schema embedded in the advertised command schema.
  assert.match(read('nemo-mcp/src/server.rs'), /include_str!\(\s*"\.\.\/\.\.\/engineering\/application\/native-object-item-v1\.schema\.json"/);
  assert.match(read('nemo-mcp/src/server.rs'), /"nativeObjectItem": native_items\}/);
  assert.match(read('nemo-mcp/tests/stdio.rs'), /command_schema\["\$defs"\]\["nativeObjectItem"\], native_items/);
  assert.ok(read('nemo-mcp/src/server.rs').split('\n').length - 1 <= 500, 'server.rs stays within the 500-line hard maximum');
  assert.ok(read('engineering/application/native-transport-v2.schema.json').includes('native-object-item-v1.schema.json#/$defs/ItemEdit'));
});

test('the declaration forbids a fallback writer and an adapter-owned mirror in both descriptors', () => {
  for (const file of ['native-object-item.json', 'native-object-asset.json']) {
    const d = JSON.parse(read('engineering/application/capabilities-v2/' + file));
    assert.equal(d.authority.fallback, 'none', file);
    assert.equal(d.authority.adapterOwnsWritableMirror, false, file);
  }
});
