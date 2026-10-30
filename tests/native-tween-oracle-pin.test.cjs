'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const crypto = require('node:crypto');
const vm = require('node:vm');
const root = path.resolve(__dirname, '..');
const baseline = '46442ea96c09e6bfaadf956eb8586e273a658085';
const fixture = JSON.parse(fs.readFileSync(path.join(root, 'native-engine/tests/fixtures/tween-baseline/oracle.json')));
const source = file => cp.execFileSync('git', ['show', `${baseline}:${file}`], { cwd: root, maxBuffer: 32 << 20 });

test('CS8 tween oracle pins every independently executed baseline source', () => {
  assert.equal(fixture.baseline, baseline);
  cp.execFileSync('git', ['merge-base', '--is-ancestor', baseline, 'HEAD'], { cwd: root });
  const files = {
    sourceSha256: 'src/js/tweens.js', toolsSha256: 'src/js/tools.js', uiSha256: 'src/js/ui.js',
    bitmapSha256: 'src/js/bitmap-brush.js', appSha256: 'src/js/app.js', groupSha256: 'src/js/group-bridge.js',
    wasmJsSha256: 'src/wasm/geometry_wasm.js', wasmBinarySha256: 'src/wasm/geometry_wasm_bg.wasm',
    paperSha256: 'src/paper-full.min.js',
  };
  for (const [key, file] of Object.entries(files)) {
    assert.equal(crypto.createHash('sha256').update(source(file)).digest('hex'), fixture[key], file);
  }
});

test('CS8 native brush preset catalog exactly matches the independent baseline catalog', () => {
  const tools = source('src/js/tools.js').toString('utf8');
  const context = vm.createContext({ window: {} });
  vm.runInContext(tools.slice(tools.indexOf('var BRUSH_PRESETS='), tools.indexOf('// Builds the real, layer-inserted texture')), context);
  const native = JSON.parse(fs.readFileSync(path.join(root, 'native-engine/src/tween_brush_presets.json')));
  assert.deepEqual(native, JSON.parse(JSON.stringify(context.BRUSH_PRESETS)));
});
