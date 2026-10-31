'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const Panel = require('../src/js/adapters/native-font-panel.js');
const UI = require('../src/js/adapters/native-object-ui.js');
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return {promise, resolve, reject}; };
const turn = () => new Promise(setImmediate);
class Element {
  constructor(tag) { this.tagName = tag; this.children = []; this.attributes = {}; this.listeners = {}; this.style = {}; this.dataset = {}; this.value = ''; this.hidden = false; this.disabled = false; this.textContent = ''; this.classes = new Set();
    this.classList = {add: x => this.classes.add(x), remove: x => this.classes.delete(x), contains: x => this.classes.has(x), toggle: (x, on) => { if (on) this.classes.add(x); else this.classes.delete(x); }};
  }
  appendChild(n) { if (n.parentNode) n.parentNode.removeChild(n); this.children.push(n); n.parentNode = this; return n; }
  removeChild(n) { this.children.splice(this.children.indexOf(n), 1); n.parentNode = null; }
  replaceChildren(...nodes) { for (const n of [...this.children]) this.removeChild(n); nodes.forEach(n => this.appendChild(n)); }
  setAttribute(k, v) { this.attributes[k] = String(v); }
  getAttribute(k) { return this.attributes[k]; }
  addEventListener(k, fn) { this.listeners[k] = fn; }
  removeEventListener(k) { delete this.listeners[k]; }
  click() { if (!this.disabled) return this.listeners.click?.({target: this}); }
  remove() { if (this.parentNode) this.parentNode.removeChild(this); }
  insertBefore(n, before) { if (!before) return this.appendChild(n); if (n.parentNode) n.parentNode.removeChild(n); this.children.splice(this.children.indexOf(before), 0, n); n.parentNode = this; }
  get nextSibling() { if (!this.parentNode) return null; return this.parentNode.children[this.parentNode.children.indexOf(this) + 1] || null; }
}
function document() {
  const body = new Element('body');
  function find(n, id) { if (n.id === id) return n; for (const child of n.children) { const hit = find(child, id); if (hit) return hit; } return null; }
  return {body, createElement: tag => new Element(tag), createComment: () => new Element('comment'), getElementById: id => find(body, id)};
}
const row = (id, name = 'Same family', available = true) => available ? {assetId: id, name, available, sourceKind: 'embedded', mediaType: 'font/ttf', encodedBytes: 42, encodedSha256: 'a'.repeat(64)} :
  {assetId: id, name, available: false, sourceKind: 'linked', reason: 'linked_font_unavailable'};
function fixture(initial = []) {
  const doc = document(), host = doc.createElement('div'); doc.body.appendChild(host);
  let current = {instanceId: 'instance', documentId: 'document', lifecycleGeneration: 1, contentRevision: 0, documentSnapshotId: 's0'};
  let rows = initial, listImpl = async () => rows, importImpl, loadImpl = async () => ({font: true}); const calls = [];
  const api = {
    list: () => { calls.push(['list']); return listImpl(); },
    load: id => { calls.push(['load', id]); return loadImpl(id); },
    importSource: (source, signal) => { calls.push(['import', {...source}, signal]); return importImpl ? importImpl(source, signal) : Promise.resolve({assetId: 'native / returned:雪', font: {}}); }
  };
  let available = true;
  const ports = {document: doc, readInfo: () => current, resources: () => available ? api : null};
  const panel = Panel.create(host, ports), get = id => doc.getElementById('native-fonts-' + id);
  return {doc, host, panel, ports, api, calls, get, open: () => get('toggle').click(),
    choose: id => get('list').children.find(n => n.getAttribute('data-asset-id') === id).children.find(n => n.tagName === 'button').click(),
    info: () => current, setInfo: p => { current = p; }, rows: p => { rows = p; }, list: f => { listImpl = f; }, import: f => { importImpl = f; }, load: f => { loadImpl = f; }, unavailable: () => { available = false; }};
}
test('ordinary controls list, select and load exact persisted opaque IDs without importing', async () => {
  const id = ' native ID/雪 :7 ', f = fixture([row('other'), row(id)]);
  assert.equal(f.calls.length, 0); await f.open(); assert.deepEqual(f.calls, [['list']]);
  f.choose(id); assert.deepEqual(f.calls, [['list']]); await f.get('load').click();
  assert.deepEqual(f.calls, [['list'], ['load', id]]); assert.match(f.get('status').textContent, /Loaded resource/);
  assert.equal(f.get('list').children[1].children.find(n => n.tagName === 'button').getAttribute('aria-pressed'), 'true');
});
test('all eight bundled choices submit explicit original source keys and preserve the returned ID', async () => {
  const f = fixture(); await f.open();
  const expected = ['Roboto-Regular', 'Roboto-Bold', 'DejaVuSans-Regular', 'DejaVuSans-Bold', 'DejaVuSerif-Regular', 'DejaVuSerif-Bold', 'Manrope-Regular', 'Inter-Regular'];
  assert.deepEqual(f.get('bundled').children.map(n => n.value), expected);
  for (const key of expected) {
    const id = 'id:' + key; f.import(async () => { f.rows([row(id, key)]); f.setInfo({...f.info(), contentRevision: f.info().contentRevision + 1, documentSnapshotId: id}); return {assetId: id, font: {}}; });
    f.get('bundled').value = key; await f.get('import-bundled').click();
    assert.equal(f.calls.filter(c => c[0] === 'import').at(-1)[1].fontKey, key);
    assert.match(f.get('status').textContent, /^Imported resource: /); assert.doesNotMatch(f.get('status').textContent, new RegExp(id)); await f.get('load').click(); assert.deepEqual(f.calls.at(-1), ['load', id]);
  }
});
test('Google controls request exactly Regular400 and Bold700, never warm up or infer an ID from the family', async () => {
  const f = fixture(); await f.open(); f.get('google-family').value = ' Poppins ';
  for (const w of ['400', '700']) {
    const id = 'opaque selected/' + w; f.import(async () => { f.rows([row(id, 'Poppins')]); return {assetId: id, font: {}}; });
    f.get('google-weight').value = w; await f.get('import-google').click(); await f.get('load').click();
    assert.deepEqual(f.calls.at(-1), ['load', id]);
  }
  assert.deepEqual(f.calls.filter(c => c[0] === 'import').map(c => c[1]), [
    {kind: 'google', family: 'Poppins', weight: 400, italic: false}, {kind: 'google', family: 'Poppins', weight: 700, italic: false}]);
});
test('a reopened document rediscovers and loads persisted identity with zero import calls', async () => {
  const f = fixture([row('persisted/source/42')]); await f.open(); f.choose('persisted/source/42'); await f.get('load').click();
  f.setInfo({...f.info(), documentId: 'reopened', lifecycleGeneration: 2, documentSnapshotId: 'reopened0'});
  await f.panel.refresh(); assert.equal(f.get('load').disabled, true); f.choose('persisted/source/42'); await f.get('load').click();
  assert.equal(f.calls.filter(c => c[0] === 'import').length, 0); assert.equal(f.calls.filter(c => c[0] === 'load').length, 2);
});
test('frame-only refresh neither refetches nor rebuilds focused resource row controls', async () => {
  const f = fixture([row('id')]); await f.open(); const first = f.get('list').children[0];
  for (let frame = 0; frame < 20; frame++) await f.panel.refresh();
  assert.equal(f.get('list').children[0], first); assert.equal(f.calls.length, 1);
  f.setInfo({...f.info(), contentRevision: 1, documentSnapshotId: 's1'}); await f.panel.refresh(); assert.equal(f.calls.length, 2);
});
test('owner replacement aborts pending selection UI and cannot publish the prior imported ID', async () => {
  const f = fixture(), hold = deferred(); await f.open(); let signal;
  f.import((_, s) => { signal = s; return hold.promise; }); const pending = f.get('import-bundled').click();
  assert.equal(f.get('import-google').disabled, true);
  f.setInfo({...f.info(), documentId: 'other', lifecycleGeneration: 2, documentSnapshotId: 'other0'}); await f.panel.refresh();
  assert.equal(signal.aborted, true); assert.equal(f.get('load').disabled, true);
  hold.resolve({assetId: 'prior-owned-font', font: {}}); await pending;
  assert.doesNotMatch(f.get('status').textContent, /prior-owned-font/); assert.equal(f.get('load').disabled, true);
});
test('old catalog replies are discarded on both revision advance and owner replacement', async () => {
  const f = fixture(), hold = deferred(); f.list(() => hold.promise); const old = f.open();
  f.setInfo({...f.info(), contentRevision: 1, documentSnapshotId: 'new'}); f.list(async () => [row('current')]); await f.panel.refresh();
  hold.resolve([row('stale')]); await old; assert.equal(f.get('list').children[0].getAttribute('data-asset-id'), 'current');
  const next = deferred(); f.list(() => next.promise); const previous = f.get('refresh').click();
  f.setInfo({...f.info(), documentId: 'new-owner', lifecycleGeneration: 3}); f.list(async () => []); await f.panel.refresh();
  next.resolve([row('wrong-owner')]); await previous; assert.equal(f.get('list').children.length, 0);
});
test('a stale visible action cannot import or load into a newly replaced owner before its repaint', async () => {
  const f = fixture([row('old-id')]); await f.open(); f.choose('old-id');
  f.setInfo({...f.info(), documentId: 'new-document', lifecycleGeneration: 2, documentSnapshotId: 'new0'});
  await f.get('load').click(); assert.equal(f.calls.filter(c => c[0] === 'load').length, 0);
  f.setInfo({...f.info(), contentRevision: 1, documentSnapshotId: 'new1'});
  await f.get('import-bundled').click(); assert.equal(f.calls.filter(c => c[0] === 'import').length, 0);
});
test('selecting a stale revision row first refreshes membership instead of naming an old resource', async () => {
  const f = fixture([row('removed')]); await f.open();
  f.rows([]); f.setInfo({...f.info(), contentRevision: 1, documentSnapshotId: 'after-undo'});
  await f.choose('removed'); assert.equal(f.get('list').children.length, 0); assert.equal(f.get('load').disabled, true);
  assert.equal(f.calls.filter(c => c[0] === 'load' || c[0] === 'import').length, 0);
});
test('linked resources display the native unavailability reason and never load or resolve a source', async () => {
  const f = fixture([row('linked exact ID', 'Unavailable font', false)]); await f.open(); f.choose('linked exact ID');
  assert.match(f.get('list').children[0].children[1].textContent, /linked_font_unavailable/); assert.equal(f.get('load').disabled, true);
  await f.get('load').click(); assert.deepEqual(f.calls, [['list']]);
  await f.get('load').listeners.click(); assert.deepEqual(f.calls, [['list']]);
});
test('lifecycle-only replacement clears selection and retirement even when the document ID is unchanged', async () => {
  const f = fixture([row('persisted')]); await f.open(); f.choose('persisted');
  f.setInfo({...f.info(), lifecycleGeneration: 2}); await f.panel.refresh();
  assert.equal(f.get('load').disabled, true); assert.equal(f.calls.filter(c => c[0] === 'import').length, 0);
});
test('malformed duplicate catalog identities fail without publishing a partial list', async () => {
  const f = fixture([row('same'), row('same', 'Different name')]); await f.open();
  assert.match(f.get('status').textContent, /catalog is invalid/); assert.equal(f.get('list').children.length, 0);
  assert.equal(f.get('load').disabled, true); assert.equal(f.calls.length, 1);
});
test('an indeterminate import blocks repeat imports even after catalog refresh and failed reads', async () => {
  const f = fixture([row('existing')]); await f.open();
  f.import(async () => { throw Object.assign(Error('lost receipt'), {code: 'font_import_indeterminate'}); });
  await f.get('import-bundled').click(); assert.equal(f.get('import-bundled').disabled, true); await f.get('refresh').click();
  assert.match(f.get('status').textContent, /outcome unknown/); f.choose('existing'); f.load(async () => { throw Error('read refused'); }); await f.get('load').click();
  assert.equal(f.get('import-google').disabled, true); assert.equal(f.calls.filter(c => c[0] === 'import').length, 1);
  f.setInfo({...f.info(), documentId: 'other', lifecycleGeneration: 2}); await f.panel.refresh(); assert.equal(f.get('import-google').disabled, false);
});
test('provider/read/import failures never invent rows, IDs or automatic retries', async () => {
  const f = fixture(); await f.open(); f.import(async () => { throw Error('source unavailable'); });
  await f.get('import-bundled').click(); assert.match(f.get('status').textContent, /source unavailable/);
  assert.equal(f.get('list').children.length, 0); assert.equal(f.calls.filter(c => c[0] === 'import').length, 1);
  f.unavailable(); await f.get('refresh').click(); assert.match(f.get('status').textContent, /unavailable/); assert.equal(f.get('import-google').disabled, true);
});
test('pending parse/load cannot report success against a newer revision', async () => {
  const f = fixture([row('id')]), hold = deferred(); await f.open(); f.choose('id'); f.load(() => hold.promise);
  const pending = f.get('load').click(); f.setInfo({...f.info(), contentRevision: 1, documentSnapshotId: 'new'}); await f.panel.refresh();
  hold.resolve({font: true}); await pending; assert.doesNotMatch(f.get('status').textContent, /Loaded resource/);
});
test('retiring the panel cancels pending UI without disposing its shared native resource service', async () => {
  const f = fixture(), hold = deferred(); await f.open(); let signal; const button = f.get('import-bundled');
  f.import((_, s) => { signal = s; return hold.promise; }); const pending = button.click(); f.panel.retire(); assert.equal(signal.aborted, true);
  hold.resolve({assetId: 'not visible', font: {}}); await pending; await button.click();
  assert.equal(f.calls.filter(c => c[0] === 'import').length, 1); assert.equal(f.get('toggle'), null);
});
test('actual native Document inspector mounts ordinary Fonts controls and retires them on restore', async () => {
  const f = fixture([row('native asset ID')]); f.panel.retire();
  const panel = f.doc.createElement('div'); panel.id = 'props-panel'; f.doc.body.appendChild(panel);
  const legacy = f.doc.createElement('div'); legacy.id = 'text-props-sec'; panel.appendChild(legacy);
  for (const id of ['layer-list', 'frame-hdr', 'frame-grid']) { const n = f.doc.createElement('div'); n.id = id; f.doc.body.appendChild(n); }
  const metadata = {...f.info(), totalFrames: 1, layers: [], canvasW: 320, canvasH: 180, fps: 24};
  f.setInfo(metadata);
  const root = {document: f.doc, state: {currentFrame: 0, activeLayerIdx: 0}, _layerSel: [], NemoNativeFontPanel: Panel};
  const ports = {projectionReady: () => true, currentSession: () => ({isActive: () => true, info: f.info}),
    selectLayer: () => assert.fail('no layer writer'), repaint() {}, fontResources: () => f.api};
  UI.render(root, metadata, false, ports); assert.equal(legacy.hidden, true);
  await f.get('toggle').click(); assert.equal(f.get('list').children[0].getAttribute('data-asset-id'), 'native asset ID');
  UI.render(root, metadata, true, ports); assert.equal(f.calls.length, 1);
  UI.restoreInspector(root); assert.equal(f.get('toggle'), null); assert.equal(legacy.hidden, false);
});

test('#2267 panel shows the human family/style name, never the raw native-font-<hash> id (kept as tooltip and value)', async () => {
  const id = 'native-font-4e7c7544b3d9f8b8363226b33e445b01', f = fixture();
  await f.open(); f.import(async () => { f.rows([row(id, 'Manrope-Regular')]); f.setInfo({...f.info(), contentRevision: f.info().contentRevision + 1, documentSnapshotId: 'after-import'}); return {assetId: id, font: {}}; });
  f.get('bundled').value = 'Manrope-Regular'; await f.get('import-bundled').click();
  assert.equal(f.get('status').textContent, 'Imported resource: Manrope Regular');
  const item = f.get('list').children[0];
  assert.equal(item.getAttribute('data-asset-id'), id); assert.equal(item.title, id);
  assert.equal(item.children[0].textContent, 'Manrope Regular'); assert.doesNotMatch(item.children[0].textContent, /native-font-/);
  f.choose(id); assert.equal(f.get('status').textContent, 'Selected resource: Manrope Regular');
  await f.get('load').click(); assert.equal(f.get('status').textContent, 'Loaded resource: Manrope Regular'); assert.deepEqual(f.calls.at(-1), ['load', id]);
});
test('#2267 font labels: known keys, file names and raw ids', () => {
  const {label} = Panel;
  assert.equal(label('DejaVuSerif-Bold'), 'DejaVu Serif Bold'); assert.equal(label('Inter-Regular'), 'Inter Regular');
  assert.equal(label('Poppins'), 'Poppins'); assert.equal(label('My_Font-Italic.ttf'), 'My Font Italic');
  assert.equal(label('native-font-4e7c7544b3d9f8b8'), 'Imported font'); assert.equal(label(''), 'Imported font');
});
