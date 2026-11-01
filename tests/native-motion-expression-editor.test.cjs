'use strict';
// D18-C (#2376): the native expression editor drawer and its pickwhip against a fake native owner that applies the public expression
// actions with the engine's rules and validates every payload and read selector with the real public contract. The legacy writers throw.
const test = require('node:test');
const assert = require('node:assert/strict');
const Editor = require('../src/js/adapters/native-motion-expression-editor.js');
const Whip = require('../src/js/adapters/native-motion-expression-whip.js');
const Contract = require('../src/js/adapters/native-property-contract.js');

const settle = async () => { for (let i = 0; i < 12; i += 1) await new Promise((resolve) => setImmediate(resolve)); };
function element(tagName) {
  const classes = new Set(), listeners = {}, kids = [];
  const el = { tagName, style: {}, dataset: {}, textContent: '', type: '', value: '', title: '', hidden: false, checked: false, disabled: false, parentNode: null,
    selectionStart: null, selectionEnd: null, classList: { add: (x) => classes.add(x), remove: (x) => classes.delete(x), contains: (x) => classes.has(x) },
    addEventListener(k, f) { listeners[k] = f; }, removeEventListener(k) { delete listeners[k]; }, setAttribute() {}, focus() {}, blur() { el.blurred = true; },
    replaceChildren(...next) { kids.splice(0).forEach((k) => { k.parentNode = null; }); next.forEach((k) => this.appendChild(k)); },
    appendChild(k) { if (k.parentNode) k.parentNode.removeChild(k); kids.push(k); k.parentNode = this; return k; },
    removeChild(k) { kids.splice(kids.indexOf(k), 1); k.parentNode = null; }, remove() { if (el.parentNode) el.parentNode.removeChild(el); },
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 4, height: 4 }), closest(sel) { return sel === '.motion-prop-row' && el.pick ? el : null; },
    fire(type, extra) { return listeners[type] && listeners[type](Object.assign({ target: el, preventDefault() {}, stopPropagation() {} }, extra)); } };
  Object.defineProperty(el, 'className', { get: () => [...classes].join(' '), set(v) { classes.clear(); String(v).split(/\s+/).filter(Boolean).forEach((x) => classes.add(x)); } });
  Object.defineProperty(el, 'children', { get: () => kids.slice() });
  return el;
}
const walk = (n) => [n, ...n.children.flatMap(walk)];
const named = (n, cls) => walk(n).find((x) => x.className.split(' ').includes(cls));

const TARGET = { contextId: 'scene-root', layerUid: 'L-a' };
function harness(options = {}) {
  const sent = [], toasts = [], menus = [], layout = [], reads = [];
  const state = { revision: 4, expr: options.expr === undefined ? { enabled: true, code: 'value' } : options.expr, diag: options.diag || null, others: options.others || {} };
  const pinNow = () => ({ instanceId: 'instance-A', documentId: 'document-A', lifecycleGeneration: 1, contentRevision: state.revision, documentSnapshotId: 'snap-' + state.revision });
  const validate = Contract.create({ exact: (v, req, opt) => { for (const k of req) if (!(k in v)) throw new TypeError('missing ' + k); for (const k of Object.keys(v)) if (!req.includes(k) && !(opt || []).includes(k)) throw new TypeError('extra ' + k); },
    revision() {}, objectUid() {}, frame() {}, id() {}, plain: (v) => !!v && typeof v === 'object' && !Array.isArray(v), has: (v, k) => Object.prototype.hasOwnProperty.call(v, k) });
  const context = () => { const pin = pinNow(); return { pin, frame: 3, viewGeneration: 1, presented: { available: true, ...pin, presentation: { ...pin, owner: 'native', status: 'presented', frame: 3 } } }; };
  const ports = {
    document: { createElement: element },
    readContext: context,
    async readProperty(selector) {
      reads.push(selector); validate.readPayload({ atRevision: selector.atRevision, documentSnapshotId: selector.documentSnapshotId, view: selector.view });
      const base = { instanceId: selector.instanceId, documentId: selector.documentId, atRevision: selector.atRevision, documentSnapshotId: selector.documentSnapshotId,
        kind: selector.view.kind, target: selector.view.target, property: selector.view.property, dimension: 1 };
      if (selector.view.kind === 'expressionSource') {
        const own = selector.view.target.layerUid === TARGET.layerUid ? state.expr : state.others[selector.view.target.layerUid] || null;
        return { ...base, expression: own ? { ...own } : null };
      }
      return { ...base, frame: selector.view.frame, expression: { present: !!state.expr, enabled: !!(state.expr && state.expr.enabled) },
        evaluation: state.expr ? { state: 'evaluated', diagnostic: state.diag } : { state: 'evaluated', diagnostic: null } };
    },
    async editProperty(payload, expectedPin, isCurrent) {
      assert.equal(isCurrent(), true);
      validate.editPayload(payload);
      sent.push(structuredClone(payload));
      if (JSON.stringify(expectedPin) !== JSON.stringify(pinNow())) throw new Error('stale_revision');
      const a = payload.action;
      if (a.kind === 'setExpression') state.expr = { enabled: true, code: a.code };
      else if (a.kind === 'setExpressionEnabled') state.expr = state.expr && { ...state.expr, enabled: a.enabled };
      else if (a.kind === 'removeExpression') state.expr = null;
      state.revision += 1;
      return { applied: true, historyEntriesAdded: 1 };
    },
    showContextMenu: (x, y, items) => menus.push({ x, y, items }), showToast: (m) => toasts.push(m),
    translate: (k) => ({ exprEnableLabel: 'Enable the expression', btnExamplesEllipsis: 'Examples…', exprExWiggle: 'Wiggle', exprExValue: 'Value', exprErrorMustReturnNumber: 'Expression must return a number.', exprRemove: 'Remove' }[k] || 'T:' + k),
    otherLayerNames: () => ['Back', 'Front'], loadControls: async () => (options.controls || []), layoutChanged: (r) => layout.push(r),
  };
  return { ports, state, sent, toasts, menus, layout, reads };
}
async function mounted(h, dimension = 1, property = 'rotation') {
  const editor = Editor.create(h.ports), host = element('div');
  editor.mount(host);
  await editor.open(TARGET, property, dimension);
  return { editor, host };
}

test('open reads the pinned expressionSource, shows enabled/code/gutter, announces the layout change and reads the diagnostics', async () => {
  const h = harness({ expr: { enabled: true, code: 'a\nb\nc' } });
  const { editor, host } = await mounted(h);
  assert.equal(named(host, 'motion-expr-code').value, 'a\nb\nc');
  assert.equal(walk(host).find((n) => n.type === 'checkbox').checked, true);
  assert.equal(named(host, 'motion-expr-gutter').children.length, 3);
  assert.deepEqual(h.layout, ['expression-open']);
  assert.deepEqual(h.reads.map((r) => r.view.kind), ['expressionSource', 'expressionState']);
  editor.close();
  assert.deepEqual(h.layout, ['expression-open', 'expression-close']);
  assert.equal(host.children.length, 0, 'a closed drawer leaves nothing in its slot');
});

test('runtime diagnostics show the message and mark the failing gutter line; a typed kind shows its translated text', async () => {
  const h = harness({ expr: { enabled: true, code: 'x\ny' }, diag: { kind: 'runtime', message: 'boom is not defined', line: 2 } });
  const { host } = await mounted(h);
  const err = named(host, 'motion-expr-error');
  assert.equal(err.textContent, 'boom is not defined'); assert.equal(err.hidden, false);
  assert.equal(named(host, 'motion-expr-gutter').children[1].className, 'err');
  h.state.diag = { kind: 'mustReturnNumber' };
  const again = await mounted(h);
  assert.equal(named(again.host, 'motion-expr-error').textContent, 'Expression must return a number.');
});

test('commit sends exactly one setExpression at the loaded pin; unchanged text, Escape and an empty new box write nothing', async () => {
  const h = harness();
  const { host } = await mounted(h);
  const box = named(host, 'motion-expr-code');
  box.fire('blur'); await settle(); assert.deepEqual(h.sent, [], 'unchanged text is not a write');
  box.value = 'zzz'; box.fire('keydown', { key: 'Escape' }); assert.equal(box.value, 'value');
  box.value = 'value + wiggle(2, 10)'; box.fire('keydown', { key: 'Enter', metaKey: true }); await settle();
  assert.deepEqual(h.sent.map((p) => p.action), [{ kind: 'setExpression', code: 'value + wiggle(2, 10)' }]);
  assert.deepEqual(h.sent[0].target, TARGET); assert.equal(h.sent[0].property, 'rotation');
  assert.equal(h.state.expr.code, 'value + wiggle(2, 10)');
  const none = harness({ expr: null }); const m = await mounted(none);
  named(m.host, 'motion-expr-code').fire('blur'); await settle(); assert.deepEqual(none.sent, [], 'no expression and no text: nothing to write');
});

test('the enable toggle and Remove are the public setExpressionEnabled and removeExpression intents', async () => {
  const h = harness();
  const { host } = await mounted(h);
  const toggle = walk(host).find((n) => n.type === 'checkbox'); toggle.checked = false; toggle.fire('change'); await settle();
  assert.deepEqual(h.sent[0].action, { kind: 'setExpressionEnabled', enabled: false }); assert.equal(h.state.expr.enabled, false);
  const remove = walk(host).find((n) => n.tagName === 'button' && n.textContent === 'Remove'); remove.fire('click'); await settle();
  assert.deepEqual(h.sent[1].action, { kind: 'removeExpression' }); assert.equal(h.state.expr, null);
  assert.equal(walk(host).find((n) => n.type === 'checkbox').disabled, true, 'nothing to enable once removed');
});

test('a stale revision cannot commit: the owner refuses, the user is told, the source is untouched and the editor reloads', async () => {
  const h = harness();
  const { host } = await mounted(h);
  h.state.revision += 1; // another writer moved the document after the source was read
  const box = named(host, 'motion-expr-code'); box.value = 'stale text'; box.fire('keydown', { key: 'Enter', ctrlKey: true }); await settle();
  assert.equal(h.sent.length, 1); assert.equal(h.state.expr.code, 'value', 'the refused edit changed nothing');
  assert.match(h.toasts[0], /stale_revision/);
  assert.equal(h.reads.filter((r) => r.view.kind === 'expressionSource').length, 2, 'reloaded at the new pin');
  box.value = 'fresh'; box.fire('keydown', { key: 'Enter', ctrlKey: true }); await settle();
  assert.equal(h.state.expr.code, 'fresh', 'the reloaded pin commits');
});

test('Examples menu: baseline labels, real other layers, and a click inserts the exact text and commits it', async () => {
  const h = harness({ controls: [{ key: 'xc_1', name: 'Turn', type: 'number' }] });
  const { host } = await mounted(h, 2, 'position');
  named(host, 'motion-expr-glob').fire('click', { clientX: 5, clientY: 6 }); await settle();
  const menu = h.menus.at(-1);
  assert.deepEqual(menu.items.slice(0, 3).map((i) => i.label), ['Value', 'Wiggle', 'T:exprExOscillate']);
  assert.equal(menu.items.some((i) => i.label === 'T:exprExAngleTo'), false);
  assert.equal(menu.items.at(-1).label, 'T:exprExControl · Turn');
  menu.items[1].action(); await settle();
  assert.equal(h.sent.at(-1).action.code, '[value[0] + wiggle(2, 10)[0], value[1] + wiggle(2, 10)[1]]');
  menu.items.at(-1).action(); await settle();
  assert.equal(h.sent.at(-1).action.code, 'const c = self.control("Turn");\nreturn [value[0] + c, value[1] + c];');
  menu.items.find((i) => i.label === 'T:exprExFollowLayer · position').action(); await settle();
  assert.equal(h.sent.at(-1).action.code, "layer('Back').position");
});

test('pickwhip reference: a control row gives layerControl("uid", "Name"); a non-control row or the editing row is rejected', () => {
  const source = { layerUid: 'L-a', property: 'rotation' };
  assert.deepEqual(Whip.reference(source, { target: { layerUid: 'L-b' }, property: 'xc_1', controlType: 'number', controlName: 'Tur"n' }), { text: 'layerControl("L-b", "Tur\\"n")' });
  assert.deepEqual(Whip.reference(source, { target: { layerUid: 'L-b' }, property: 'position', controlType: null, controlName: null }), { reject: 'notReferenceable' });
  assert.deepEqual(Whip.reference(source, { target: { layerUid: 'L-a' }, property: 'rotation', controlType: null, controlName: null }), { reject: 'cycle' });
  assert.deepEqual(Whip.reference(source, null), { reject: 'notReferenceable' });
});

function gesture(h, host, row, mods = {}) {
  const doc = h.ports.document, listeners = {};
  doc.body = element('body'); doc.addEventListener = (k, f) => { listeners[k] = f; }; doc.removeEventListener = (k) => { delete listeners[k]; };
  doc.elementFromPoint = () => row;
  named(host, 'motion-expr-whip').fire('mousedown', { clientX: 1, clientY: 1 });
  listeners.mousemove({ clientX: 8, clientY: 9 });
  listeners.mouseup({ clientX: 8, clientY: 9, altKey: !!mods.alt });
}
const rowOf = (pick) => { const r = element('div'); r.pick = true; r._nemoPick = pick; return r; };

test('pickwhip drop: control row inserts at the caret and commits; a non-control or self drop toasts and writes no history entry; Alt clones', async () => {
  const h = harness({ expr: { enabled: true, code: 'value + ' }, others: { 'L-b': { enabled: true, code: 'cloned()' } } });
  const { host } = await mounted(h);
  const box = named(host, 'motion-expr-code'); box.selectionStart = box.selectionEnd = box.value.length;
  gesture(h, host, rowOf({ target: { contextId: 'scene-root', layerUid: 'L-b' }, property: 'xc_1', controlType: 'number', controlName: 'Turn' })); await settle();
  assert.equal(h.sent.at(-1).action.code, 'value + layerControl("L-b", "Turn")'); assert.equal(h.toasts.at(-1), 'T:toastReferenceInserted');
  const writes = h.sent.length, revision = h.state.revision;
  gesture(h, host, rowOf({ target: { contextId: 'scene-root', layerUid: 'L-b' }, property: 'position', controlType: null, controlName: null })); await settle();
  gesture(h, host, rowOf({ target: TARGET, property: 'rotation', controlType: null, controlName: null })); await settle();
  assert.equal(h.sent.length, writes, 'rejected drops never reach the owner'); assert.equal(h.state.revision, revision, 'no history entry');
  assert.deepEqual(h.toasts.slice(-2), ['T:toastPropertyNotReferenceable', 'T:toastPropertyNotReferenceable']);
  gesture(h, host, rowOf({ target: { contextId: 'scene-root', layerUid: 'L-b' }, property: 'position', controlType: null, controlName: null }), { alt: true }); await settle();
  assert.match(h.sent.at(-1).action.code, /cloned\(\)/); assert.equal(h.toasts.at(-1), 'T:toastExpressionCloned');
});

test('unavailable owner: a failed read is a typed in-drawer message, never a write', async () => {
  const h = harness();
  h.ports.readProperty = async () => { throw new Error('boom'); };
  const { host } = await mounted(h);
  assert.equal(named(host, 'motion-expr-error').textContent, 'T:nativeExprUnavailable');
  named(host, 'motion-expr-code').fire('blur'); await settle(); assert.deepEqual(h.sent, []);
});
