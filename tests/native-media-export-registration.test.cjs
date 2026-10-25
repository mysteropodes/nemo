'use strict';
// Composed-candidate test (needs patches 01-05): pure file reads of the registration chain. No transport, no process.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const root = path.join(__dirname, '..');
const read = (...p) => fs.readFileSync(path.join(root, ...p), 'utf8');
const exists = (...p) => fs.existsSync(path.join(root, ...p));
// Code only: comments and string / template literals removed (a message may say "document").
const strip = (source) => source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '')
  .replace(/'(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*"|`(?:[^`\\]|\\.)*`/g, "''").replace(/\/\/.*$/gm, '');

const CORE = 'native-media-export-core.js', CONTRACT = 'native-media-export-contract.js';
const VECTOR = 'native-vector-export-adapter.js', MEDIA = 'native-media-export-adapter.js';

test('both descriptors are registered once in the native catalog block, beside distinct ids and operations', () => {
  // CS1/#2341: nemo-mcp/build.rs discovers capabilities-v2/*.json; read the same set.
  const files = require('../scripts/nemo/lib/capability-drift.cjs').nativeDescriptorFiles().map((file) => '../../engineering/application/capabilities-v2/' + path.basename(file));
  for (const name of ['native-vector-export.json', 'native-media-export.json']) {
    assert.equal(files.filter((f) => f.endsWith('/capabilities-v2/' + name)).length, 1, name);
  }
  const descriptors = files.map((f) => JSON.parse(fs.readFileSync(path.resolve(root, 'nemo-mcp', 'src', f), 'utf8')));
  assert.equal(new Set(descriptors.map((d) => d.id)).size, descriptors.length, 'descriptor ids are unique');
  const operations = descriptors.flatMap((d) => d.operations);
  assert.equal(new Set(operations).size, operations.length, 'every operation is declared once');
  const schema = JSON.parse(read('engineering', 'application', 'native-transport-v2.schema.json'));
  const union = schema.$defs.Request.oneOf.flatMap((b) => { const o = b.allOf[1].properties.operation; return o.const ? [o.const] : o.enum; });
  assert.deepEqual([...operations].sort(), [...union].sort(), 'declared operations equal the v2 union operations');
});

test('script order: core and contract before the SDK, adapters after the opacity export adapter', () => {
  const html = read('src', 'index.html');
  const order = [...html.matchAll(/<script\b[^>]*\bsrc="([^"]+)"/g)].map((m) => m[1]);
  const at = (name) => { const hits = order.filter((s) => s === 'js/adapters/' + name); assert.equal(hits.length, 1, name); return order.indexOf('js/adapters/' + name); };
  assert.equal(at('native-font-resources.js'), at('native-font-contract.js') + 1, 'G41 keeps the font contract / resources pair adjacent: nothing is inserted between them');
  assert.ok(at('native-font-resources.js') < at(CORE));
  assert.ok(at(CORE) < at(CONTRACT));
  assert.ok(at(CONTRACT) < at('native-application.js'), 'the contract precedes the SDK that reads it');
  assert.ok(at('native-opacity-export.js') < at(VECTOR));
  assert.ok(at(VECTOR) < at(MEDIA));
  assert.ok(at('native-application.js') < at(VECTOR));
});

test('the app-js profile registers the four files in exactly two layers', () => {
  const profile = JSON.parse(read('engineering', 'boundaries', 'profiles', 'app-js.profile.json'));
  const modules = profile.modules || profile.moduleProfiles || Object.values(profile).find((v) => Array.isArray(v) && v.some((m) => m && m.id));
  const owner = (file) => modules.filter((m) => m.dir === 'src/js/adapters' && (m.files || []).includes(file));
  for (const file of [CORE, CONTRACT, VECTOR, MEDIA]) { assert.equal(owner(file).length, 1, file); assert.ok(exists('src', 'js', 'adapters', file), file); }
  assert.equal(owner(CORE)[0], owner(CONTRACT)[0], 'the contract imports the core: one module');
  assert.equal(owner(CORE)[0].layer, 'domain');
  assert.equal(owner(CORE)[0].id, 'app.native.media.export.contract');
  assert.equal(owner(VECTOR)[0].layer, 'adapters');
  assert.equal(owner(MEDIA)[0].layer, 'adapters');
  assert.deepEqual(owner(VECTOR)[0].publicApi, [VECTOR]);
  assert.equal(new Set(modules.map((m) => m.id)).size, modules.length, 'module ids stay unique');
});

test('sources are pure: no document, storage, network, timer, random, engine or legacy writer is reachable', () => {
  const forbidden = ['window', 'document', 'localStorage', 'sessionStorage', 'indexedDB', 'fetch', 'XMLHttpRequest', '__TAURI__', 'paper', 'Paper', 'SMEngineBridge', 'SMProject', 'NemoExportSvg',
    'exportFrameSVGString', 'exportMP4ToPath', 'exportSVGSequence', 'setTimeout', 'setInterval', 'requestAnimationFrame', 'Math.random', 'Date.now', 'new Date', 'userLayers', 'ffmpeg', 'Worker'];
  for (const file of [CORE, CONTRACT, VECTOR, MEDIA]) {
    const source = strip(read('src', 'js', 'adapters', file));
    for (const word of forbidden) assert.equal(new RegExp('(^|[^A-Za-z0-9_$.])' + word + '(?![A-Za-z0-9_$])').test(source), false, `${file} must not mention ${word}`);
    assert.doesNotMatch(source, /\beval\b|new Function|import\(/, file);
    assert.doesNotMatch(source, /(^|[^A-Za-z0-9_$.])state\.[A-Za-z_$]/, file + ' must not read the legacy global state (state.fps, state.waIn ...)');
  }
  for (const file of [CORE, CONTRACT, VECTOR, MEDIA]) assert.ok(read('src', 'js', 'adapters', file).split('\n').length <= 400, file + ' stays under the domain ceiling');
});

test('the SDK seam is the single composition point and replaces one line', () => {
  const sdk = read('src', 'js', 'adapters', 'native-application.js');
  assert.equal((sdk.match(/native-media-export-contract\.js/g) || []).length, 1);
  assert.equal((sdk.match(/NemoNativeMediaExportContract/g) || []).length, 1);
  assert.match(sdk, /\[[^\]]*require\('\.\/native-media-export-contract\.js'\)[^\]]*\]\.reduce\(\(base, lane\) => \(lane \? lane\.compose\(base\) : base\)/, 'composed in the lane reduce of the single SDK line');
  const lines = sdk.split('\n');
  assert.match(lines[2], /^  const vectorContract = typeof module === 'object' && module\.exports \? require\('\.\/native-object-vector-contract\.js'\) : root\.NemoNativeObjectVectorContract, fontContract = \[/);
  const lanes = lines[2].match(/fontContract = \[([^\]]+)\]\.reduce\(\(base, lane\) => \(lane \? lane\.compose\(base\) : base\)/);
  assert.ok(lanes, 'the media lane is composed before the frame-edit join');
  assert.match(lanes[1], /require\('\.\/native-media-export-contract\.js'\) : root\.NemoNativeMediaExportContract/);
  assert.match(lanes[1], /, vectorContract$/, 'the vector lane is the last composed lane');
  assert.match(lines[2], /require\('\.\/native-object-edit-seam\.js'\) : root\.NemoNativeObjectEditSeam, typeof module === 'object' && module\.exports \? require\('\.\/native-object-item-contract\.js'\) : root\.NemoNativeObjectItemContract\)\);$/);
  const fontSeam = read('src', 'js', 'adapters', 'native-object-edit-seam.js');
  assert.match(fontSeam, /const font = commonjs \? require\('\.\/native-font-contract\.js'\) : root\.NemoNativeFontContract;/);
  assert.match(fontSeam, /return edit\.compose\(font\);/, 'the base seam retains the font contract');
  assert.equal(lines[3], "  const propertyContract = typeof module === 'object' && module.exports ? require('./native-property-contract.js') : root.NemoNativePropertyContract;");
  assert.equal(lines[4], "  const api = factory((typeof module === 'object' && module.exports ? require('./native-frame-edit-contract.js') : root.NemoNativeFrameEditContract || { join: (base) => base }).join(fontContract), vectorContract, propertyContract);");
  assert.match(sdk, /function \(fontContract, vectorContract, propertyContract\) \{/);
  assert.match(sdk, /const property = propertyContract && propertyContract\.create\(/);
});

test('the MCP validators are listed once in both Rust profiles (mcp.transport / rust.mcp.transport) and dispatched by native_contract.rs', () => {
  const files = ['src/native_media_export_contract.rs', 'src/native_media_export_contract_tests.rs'];
  const find = (node, id) => {
    if (Array.isArray(node)) { for (const item of node) { const hit = find(item, id); if (hit) return hit; } return null; }
    if (node && typeof node === 'object') { if (node.id === id) return node; for (const value of Object.values(node)) { const hit = find(value, id); if (hit) return hit; } }
    return null;
  };
  for (const [profile, moduleId] of [['mcp-rust.profile.json', 'mcp.transport'], ['rust.profile.json', 'rust.mcp.transport']]) {
    const module = find(JSON.parse(read('engineering', 'boundaries', 'profiles', profile)), moduleId);
    assert.ok(module, profile);
    for (const file of files) assert.equal(module.files.filter((f) => f === file).length, 1, `${profile} ${file}`);
    assert.equal(module.dir, 'nemo-mcp');
    assert.equal(module.layer, 'adapters');
    const policy = JSON.parse(read('engineering', 'boundaries', 'profiles', profile));
    for (const file of files) {
      assert.equal(policy.modules.filter((owner) => owner.dir === 'nemo-mcp' && owner.files.includes(file)).length, 1, `${file} has exactly one owner`);
      assert.equal(module.publicApi.includes(file), false, `${file} stays private`);
    }
  }
  const dispatcher = read('nemo-mcp', 'src', 'native_contract.rs');
  assert.equal((dispatcher.match(/native_media_export_contract\.rs/g) || []).length, 1);
  assert.match(dispatcher, /operation if export::owns\(operation\) => export::request\(operation, payload\),/);
  assert.ok(dispatcher.indexOf('mod export;') < dispatcher.indexOf('pub(crate) mod font;'));
  const rust = read('nemo-mcp', 'src', 'native_media_export_contract.rs');
  assert.ok(rust.split('\n').length <= 500 && read('nemo-mcp', 'src', 'native_media_export_contract_tests.rs').split('\n').length <= 500);
  assert.match(rust, /#\[path = "native_media_export_contract_tests\.rs"\]/);
});
