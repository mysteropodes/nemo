'use strict';
// #2372 PAL-APPLY: palette swatches and Swap paint through the app's SM colour setters (which hand a native selection to the engine); a palette that is
// not shown, or a commit in flight, refuses with the one notice and writes nothing.
const test = require('node:test'), assert = require('node:assert/strict');
const path = require.resolve('../src/js/adapters/native-palette-desktop.js');
function node(tag = 'div') {
  const listeners = new Map(); let text = '';
  const el = { tagName: tag.toUpperCase(), children: [], dataset: {}, className: '', disabled: false, style: { setProperty() {} },
    setAttribute() {}, appendChild(c) { this.children.push(c); return c; },
    addEventListener(k, f) { if (!listeners.has(k)) listeners.set(k, []); listeners.get(k).push(f); },
    fire(k, extra = {}) { const e = { preventDefault() {}, clientX: 0, clientY: 0, dataTransfer: {}, ...extra }; (listeners.get(k) || []).forEach(f => f(e)); } };
  Object.defineProperty(el, 'textContent', { get: () => text, set(v) { text = v; el.children = []; } });
  return el;
}
function fixture(colors = ['#AbC', '#112233', '#44556677'], fillEnabled = true) {
  const origin = { palette: { palettes: [{ ordinal: 1, name: 'P' }], active: { summary: { ordinal: 1, name: 'P' }, colors: colors.map((color, i) => ({ index: i + 1, color })) } } };
  let shown = origin, settled = Promise.resolve(null);
  require.cache[path] = { id: path, filename: path, loaded: true, exports: { create: () => ({ read: () => shown, readSettled: () => settled,
    edit: async () => ({ status: 'applied-presented' }), refresh: async () => shown, invalidate() {}, dispose() {} }) } };
  delete require.cache[require.resolve('../src/js/adapters/native-palette-ui.js')];
  const UI = require('../src/js/adapters/native-palette-ui.js');
  const elements = Object.fromEntries(['palette-grid', 'palette-tabs', 'btn-palette-new', 'btn-palette-add-fill', 'btn-palette-add-stroke', 'btn-palette-replace', 'btn-palette-swap', 'palette-replace-scope'].map(id => [id, node()]));
  const calls = [], notices = [], menus = [];
  const info = { instanceId: 'i' }, session = { info: () => info, isActive: () => true, readSubscriptionGeneration: () => 1,
    readPresentedObjectSession: () => ({ ...info, presentation: { owner: 'native', status: 'presented', ...info } }) };
  const root = { __TAURI__: { core: {} }, state: { fillEnabled }, document: Object.assign(node('document'), { createElement: node, getElementById: id => elements[id] || null }),
    NemoNativeObjectProject: { desktop: () => session }, NemoNativeObjectProjectFile: { projectionReadyForUI: () => true },
    SM: { setFillColor: v => calls.push(['fill', v]), setStrokeColor: v => calls.push(['stroke', v]), setFillEnabled: v => calls.push(['fillEnabled', v]), swapStrokeFill: () => calls.push(['swap']) },
    showToast: t => notices.push(t), showContextMenu: (x, y, items) => menus.push(items), addEventListener() {} };
  const controller = UI.install(root); controller.render();
  return { root, elements, calls, notices, menus, controller, swatches: () => elements['palette-grid'].children,
    hide() { shown = null; controller.render(); }, flipOrigin() { shown = { ...origin }; } };
}
test('click paints Fill and enables it when off; Shift+click paints Stroke; hex is passed through (shorthand widened)', () => {
  const h = fixture(undefined, false);
  h.swatches()[1].fire('click'); assert.deepEqual(h.calls, [['fill', '#112233'], ['fillEnabled', true]]);
  h.calls.length = 0; h.swatches()[2].fire('click', { shiftKey: true }); assert.deepEqual(h.calls, [['stroke', '#44556677']]);
  h.calls.length = 0; h.swatches()[0].fire('click'); assert.deepEqual(h.calls, [['fill', '#AAbbCC'], ['fillEnabled', true]]);
  const on = fixture(); on.swatches()[1].fire('click'); assert.deepEqual(on.calls, [['fill', '#112233']], 'fill already on: no second write');
  assert.deepEqual(h.notices, []);
});
test('swatch menu offers Fill and Stroke, then Replace in layer (#2372 PAL-REPLACE), before Retirer', () => {
  const h = fixture(); h.swatches()[1].fire('contextmenu');
  const items = h.menus.at(-1).filter(i => !i.sep); assert.deepEqual(items.map(i => i.label), ['Utiliser comme Fill', 'Utiliser comme Stroke', 'Remplacer dans le calque…', 'Retirer de la palette']);
  items[0].action(); items[1].action(); assert.deepEqual(h.calls, [['fill', '#112233'], ['stroke', '#112233']]);
});
test('Swap is enabled with a shown palette, replace is on (PAL-REPLACE), and the controller swaps once through SM.swapStrokeFill', () => {
  const h = fixture(); assert.equal(h.elements['btn-palette-swap'].disabled, false); assert.equal(h.elements['btn-palette-replace'].disabled, false);
  assert.equal(h.controller.swap(), true); assert.deepEqual(h.calls, [['swap']]);
});
test('a palette that is not shown, a replaced read or an invalid colour refuses with one notice and writes nothing', () => {
  const h = fixture(['#zzz']); h.swatches()[0].fire('click'); assert.deepEqual(h.calls, []); assert.equal(h.notices.length, 1);
  const stale = fixture(); const [swatch] = stale.swatches(); stale.flipOrigin(); swatch.fire('click'); swatch.fire('click', { shiftKey: true });
  assert.equal(stale.controller.swap(), true, 'swap reads the live origin');
  assert.deepEqual(stale.calls, [['swap']]); assert.equal(stale.notices.length, 2, 'both stale swatch clicks refused');
  const gone = fixture(); const [kept] = gone.swatches(); gone.hide(); kept.fire('click'); gone.swatches(); assert.equal(gone.controller.swap(), false);
  assert.deepEqual(gone.calls, []); assert.equal(gone.notices.length, 2); assert.equal(gone.elements['btn-palette-swap'].disabled, true);
});
