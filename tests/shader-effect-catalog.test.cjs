'use strict';
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const Catalog = require('../src/js/adapters/shader-effect-catalog.js');
const source = fs.readFileSync(path.join(__dirname, '../src/js/shader-effects-library.js'), 'utf8');

test('the complete authored shader catalog remains byte-exact after constructor separation', () => {
  const start = source.indexOf('  var effects = ['), end = source.indexOf('  catalog.install');
  assert(start >= 0 && end > start);
  assert.equal(crypto.createHash('sha256').update(source.slice(start, end)).digest('hex'),
    '587c565675400b709d39353178010ac9530df183d414c58dfa7b74564d63f1a5');
});

test('parameter construction preserves optional defaults, units and spatial metadata', () => {
  assert.deepEqual(Catalog.param('p1', 'Radius', 0, 100, 1, 'px', 0, true),
    { key: 'p1', label: 'Radius', min: 0, max: 100, step: 1, scale: 1, unit: 'px', defaultValue: 0, spatial: true });
  const absent = Catalog.param('p2', 'Optional', -1, 1, 0.1);
  assert.equal(absent.unit, '');
  assert.equal(Object.hasOwn(absent, 'defaultValue'), false);
  assert.equal(Object.hasOwn(absent, 'spatial'), false);
  assert.equal(Catalog.param('p2', 'Null', 0, 1, 1, '', null).defaultValue, null);
});

test('descriptor construction keeps parameter identity and exact WGSL line joining', () => {
  const params = [Catalog.param('p1', 'Gain', 0, 1, 0.1, '', 1)];
  const effect = Catalog.fx('test', 'Test', 'Color', params, ['let x = src;', 'return x;']);
  assert.equal(effect.params, params);
  assert.equal(effect.source, 'let x = src;\nreturn x;');
  assert.deepEqual(Object.keys(effect), ['id', 'name', 'category', 'params', 'source']);
});

test('ordinary classic bootstrap installs the real catalog and preserves public lookup identity', () => {
  const window = {};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../src/js/adapters/shader-effect-catalog.js'), 'utf8'),
    { window }, { filename: 'shader-effect-catalog.js' });
  assert.equal(typeof window.NemoShaderEffectCatalog.install, 'function');
  vm.runInNewContext(source, { window }, { filename: 'shader-effects-library.js' });
  const effects = window.SMSHADER_EFFECTS;
  assert(effects.length > 0);
  assert.equal(new Set(effects.map(effect => effect.id)).size, effects.length);
  for (const effect of effects) {
    assert.equal(window.SMShaderEffectDef(effect.id), effect);
    assert.equal(typeof effect.source, 'string');
    assert(effect.source.length > 0);
  }
  assert.equal(window.SMShaderEffectDef('missing'), null);
  assert.deepEqual(Array.from(window.SMSHADER_EFFECT_CATEGORIES),
    ['Color', 'Blur', 'Stylize', 'Generate', 'Distort', 'Keying', 'Particles', 'Channel']);
  const radius = window.SMShaderEffectDef('shader_light_sweep').params.find(param => param.key === 'p2');
  assert.equal(radius.spatial, true);
  assert.equal(radius.defaultValue, 0.18);
  assert.equal(window.SMShaderEffectDef('shader_exposure').params[0].defaultValue, 0);
  const added = { id: 'extension' };
  effects.push(added);
  assert.equal(window.SMShaderEffectDef('extension'), added, 'lookup retains the original live array contract');
});

test('catalog installation has no document, native or renderer access', () => {
  const root = {};
  for (const key of ['state', 'document', '__TAURI__', 'NemoNativeObjectProject']) {
    Object.defineProperty(root, key, { get() { throw Error('unexpected authority access'); } });
  }
  const effects = [{ id: 'one' }];
  Catalog.install(root, effects);
  assert.equal(root.SMSHADER_EFFECTS, effects);
  assert.equal(root.SMShaderEffectDef('one'), effects[0]);
  assert.equal(root.SMShaderEffectDef('missing'), null);
});
