'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createAssetTree } = require('../src/js/asset-tree.js');

function documentFixture() {
  const created = [];
  function createElement(tagName) {
    let classes = new Set();
    const listeners = new Map();
    const properties = new Map();
    const element = {
      tagName, children: [], textContent: '',
      get className() { return [...classes].join(' '); },
      set className(value) { classes = new Set(value.split(/\s+/).filter(Boolean)); },
      classList: {
        add(name) { classes.add(name); },
        contains(name) { return classes.has(name); },
        toggle(name) { if (classes.has(name)) { classes.delete(name); return false; } classes.add(name); return true; },
      },
      style: { setProperty(name, value) { properties.set(name, value); }, getPropertyValue(name) { return properties.get(name) || ''; } },
      appendChild(child) { this.children.push(child); return child; },
      addEventListener(name, handler) { listeners.set(name, handler); },
      click() { listeners.get('click')?.(); },
      listenerNames() { return [...listeners.keys()]; },
    };
    created.push(element);
    return element;
  }
  return { document: { createElement }, created };
}

test('production factory imports without a browser and preserves the exact public API', () => {
  const { document } = documentFixture();
  const api = createAssetTree(document, (_, fallback) => fallback);
  assert.deepEqual(Object.keys(api), [
    'FOLDER_COLORS', 'KIND_GROUP_LABEL', 'componentsLabel',
    'compositionsLabel', 'layersLabel', 'folderGroup',
  ]);
  assert.deepEqual(api.FOLDER_COLORS, {
    components: 'var(--purple)', compositions: 'var(--red)',
    image: 'var(--accent)', video: 'var(--orange)', audio: 'var(--green)',
  });
  assert.deepEqual([api.KIND_GROUP_LABEL.image, api.KIND_GROUP_LABEL.video,
    api.KIND_GROUP_LABEL.audio, api.componentsLabel(), api.compositionsLabel(), api.layersLabel()],
  ['Images', 'Vidéos', 'Audio', 'Composants', 'Compositions', 'Calques']);
});

test('caller owns rows; header click only toggles its own body and key has no persistence', () => {
  const { document, created } = documentFixture();
  const api = createAssetTree(document, (_, fallback) => fallback);
  const container = document.createElement('section');
  const body = api.folderGroup(container, { key: 'same', label: 'Images', count: 0, defaultCollapsed: true });
  const folder = container.children[0], header = folder.children[0];
  assert.equal(folder.className, 'asset-folder');
  assert.equal(header.className, 'asset-folder-hdr collapsed');
  assert.equal(header.children[0].textContent, '▾');
  assert.equal(header.children[1].style.getPropertyValue('--folder-color'), 'var(--text-dim)');
  assert.equal(header.children[2].textContent, 'Images');
  assert.equal(header.children[3].textContent, '0');
  assert.equal(body.className, 'asset-folder-body collapsed');
  const row = document.createElement('label'); body.appendChild(row);
  assert.deepEqual(header.listenerNames(), ['click']);
  assert.equal(body.listenerNames().length, 0);
  header.click();
  assert.equal(header.classList.contains('collapsed'), false);
  assert.equal(body.classList.contains('collapsed'), false);
  assert.equal(body.children[0], row);
  header.click();
  assert.equal(body.classList.contains('collapsed'), true);
  const next = api.folderGroup(container, { key: 'same', label: 'Other', color: '#123456', count: 3 });
  assert.equal(next.classList.contains('collapsed'), false, 'no persisted collapse state');
  assert.equal(container.children[1].children[0].children[1].style.getPropertyValue('--folder-color'), '#123456');
  assert.equal(container.children[1].children[0].children[3].textContent, '3');
  assert.equal(created.filter(element => element.listenerNames().length).length, 2);
});

test('browser facade uses the same implementation and reads current translation lazily', () => {
  const { document } = documentFixture();
  let language = 'fr';
  const translations = {
    fr: { assetGroupImages: 'Images FR', assetGroupComponents: 'Composants FR' },
    en: { assetGroupImages: 'Images EN', assetGroupComponents: 'Components EN' },
  };
  const window = { document, SM: { t: key => translations[language][key] } };
  Object.defineProperty(window, 'localStorage', { get() { throw new Error('storage must remain untouched'); } });
  const source = fs.readFileSync(path.join(__dirname, '../src/js/asset-tree.js'), 'utf8');
  vm.runInNewContext(source, { window });
  assert.deepEqual(Object.keys(window.SMAssetTree), Object.keys(createAssetTree(document, (_, fallback) => fallback)));
  assert.equal(window.SMAssetTree.KIND_GROUP_LABEL.image, 'Images FR');
  assert.equal(window.SMAssetTree.componentsLabel(), 'Composants FR');
  language = 'en';
  assert.equal(window.SMAssetTree.KIND_GROUP_LABEL.image, 'Images EN');
  assert.equal(window.SMAssetTree.componentsLabel(), 'Components EN');
  window.SM = null;
  assert.equal(window.SMAssetTree.layersLabel(), 'Calques');
});
