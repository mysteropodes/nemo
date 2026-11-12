/* D18-C (#2376): the native expression editor row (baseline buildExprEditorRow @1065e9d8): enable toggle, whip, Examples menu, line
 * gutter, code box, error line. It is a drawer under its property row, like the authored-keys drawer, and owns only DOM: the source
 * is the pinned `expressionSource` read, the diagnostics the `expressionState` read, every write one `object.property.edit`
 * (setExpression / setExpressionEnabled) sent with the pin the source was read at, so a stale revision cannot commit (the owner port
 * refuses it and the editor reloads). No legacy expressions writer, no second copy of the code. Height changes are announced through
 * ports.layoutChanged so the grid re-reserves the same height (CLAUDE.md section 11). Ports: document, readContext, readProperty,
 * editProperty, showContextMenu, showToast, translate, otherLayerNames(layerUid), loadControls(target), layoutChanged(reason). */
(function (root, factory) {
  const node = typeof module === 'object' && module.exports;
  const api = factory(node ? require('./native-motion-expression-snippets.js') : root.NemoNativeMotionExpressionSnippets,
    node ? require('./native-motion-expression-whip.js') : root.NemoNativeMotionExpressionWhip);
  if (node) module.exports = api;
  else root.NemoNativeMotionExpressionEditor = api;
}(typeof globalThis === 'object' ? globalThis : this, function (Snippets, Whip) {
  'use strict';
  const PIN = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const DIAG = { mustReturnNumber: ['exprErrorMustReturnNumber', 'Expression must return a number.'],
    mustReturnNumberOrXY: ['exprErrorMustReturnNumberOrXY', 'Expression must return a number or coordinates.'],
    tooDeep: ['exprErrorTooDeep', 'Too many chained expression references.'], timeout: ['exprErrorTimeout', 'Expression took too long.'] };

  function create(ports) {
    if (!ports || !ports.document || typeof ports.readContext !== 'function' || typeof ports.readProperty !== 'function' ||
        typeof ports.editProperty !== 'function') return null;
    const doc = ports.document;
    const tr = (key, fallback) => { let v; try { v = ports.translate ? ports.translate(key) : undefined; } catch (_) { v = undefined; } return typeof v === 'string' && v && v !== key ? v : fallback; };
    const make = (tag, cls, text) => { const n = doc.createElement(tag); if (cls) n.className = cls; if (text !== undefined) n.textContent = text; return n; };
    const toast = (m) => { if (typeof ports.showToast === 'function') ports.showToast(m); };
    const head = make('label', 'motion-expr-toggle'), cb = make('input'), whip = make('span', 'motion-expr-whip');
    const examples = make('button', 'motion-expr-glob', tr('btnExamplesEllipsis', 'Examples…'));
    const removeBtn = make('button', 'motion-expr-glob', tr('exprRemove', 'Remove'));
    const pane = make('div', 'motion-expr-pane'), gutter = make('div', 'motion-expr-gutter'), ta = make('textarea', 'motion-expr-code');
    const err = make('div', 'motion-expr-error');
    cb.type = 'checkbox'; ta.spellcheck = false; examples.type = 'button'; removeBtn.type = 'button'; err.hidden = true;
    head.appendChild(cb); head.appendChild(make('span', '', ' ' + tr('exprEnableLabel', 'Enable the expression'))); head.appendChild(whip);
    whip.title = tr('titleExprWhipHint', 'Drag onto a control row to insert its reference (Alt: clone its expression)');
    examples.title = tr('titleExamplesHint', 'Insert a common expression'); ta.title = tr('titleExprCodeHint', '');
    pane.appendChild(gutter); pane.appendChild(ta);
    let host = null, target = null, property = null, dimension = 1, serial = 0, loadedPin = null, source = null, errorLine = -1, busy = false, disposed = false;
    const listeners = [];
    const on = (n, type, fn) => { n.addEventListener(type, fn); listeners.push([n, type, fn]); };

    function paintGutter() {
      const count = String(ta.value).split('\n').length || 1;
      gutter.replaceChildren();
      for (let i = 1; i <= count; i += 1) gutter.appendChild(make('span', i === errorLine ? 'err' : '', String(i)));
    }
    function paintError(message, line) {
      errorLine = Number.isInteger(line) ? line : -1;
      err.textContent = message || ''; err.hidden = !message; err.dataset.errorLine = String(errorLine);
      paintGutter();
    }
    const stamp = (context) => { const pin = {}; PIN.forEach((k) => { pin[k] = context.pin[k]; }); return Object.freeze(pin); };
    const selectorFor = (pin, view) => ({ instanceId: pin.instanceId, documentId: pin.documentId, atRevision: pin.contentRevision, documentSnapshotId: pin.documentSnapshotId, view });

    async function load() {
      const ticket = ++serial;
      if (!target) return false;
      let context, pin;
      try { context = ports.readContext(); pin = stamp(context); } catch (_) { return unavailable(ticket); }
      try {
        const result = await ports.readProperty(selectorFor(pin, { kind: 'expressionSource', target, property }));
        if (ticket !== serial || disposed) return false;
        if (!result || result.kind !== 'expressionSource' || result.property !== property) throw new Error('foreign expression source');
        loadedPin = pin; source = result.expression;
      } catch (_) { return unavailable(ticket); }
      cb.checked = !!source && source.enabled === true; cb.disabled = !source; removeBtn.disabled = !source;
      if (doc.activeElement !== ta) ta.value = source ? source.code : '';
      paintGutter(); paintError('', -1);
      if (source) await diagnostics(ticket, pin, context.frame);
      return ticket === serial;
    }
    function unavailable(ticket) {
      if (ticket !== serial || disposed) return false;
      loadedPin = null; source = null; cb.disabled = true;
      paintError(tr('nativeExprUnavailable', 'Expression editing is unavailable.'), -1);
      return false;
    }
    async function diagnostics(ticket, pin, frame) {
      try {
        const result = await ports.readProperty(selectorFor(pin, { kind: 'expressionState', target, property, frame }));
        if (ticket !== serial || disposed || !result || result.kind !== 'expressionState') return;
        const d = result.evaluation && result.evaluation.state === 'evaluated' ? result.evaluation.diagnostic : null;
        if (!d) { paintError('', -1); return; }
        const known = DIAG[d.kind];
        paintError(known ? tr(known[0], known[1]) : (typeof d.message === 'string' ? d.message : ''), known ? -1 : d.line);
      } catch (_) { if (ticket === serial && !disposed) paintError(tr('nativeExprDiagUnavailable', 'Expression diagnostics are unavailable.'), -1); }
    }
    // One write at the loaded pin. A refusal (stale revision, owner busy, engine refusal) reloads and writes nothing.
    async function send(action) {
      if (busy || disposed || !loadedPin || !target) return false;
      const ticket = serial, pin = loadedPin;
      busy = true;
      try {
        const payload = { command: 'object.property.edit', target, property, action };
        const result = await ports.editProperty(payload, pin, () => !disposed && ticket === serial);
        return !!result && result.applied === true;
      } catch (error) {
        toast(tr('ctrlEditRefused', 'Edit refused: ') + String(error && error.message || error));
        return false;
      } finally { busy = false; if (!disposed) load(); }
    }
    async function commit() {
      const code = ta.value;
      if (source && code === source.code) return false;
      if (!source && code === '') return false;
      return send({ kind: 'setExpression', code });
    }
    function insert(code) { ta.value = code; paintGutter(); const done = commit(); if (typeof ta.focus === 'function') ta.focus(); return done; }

    on(cb, 'change', () => { send({ kind: 'setExpressionEnabled', enabled: cb.checked }); });
    on(removeBtn, 'click', (e) => { if (e && e.stopPropagation) e.stopPropagation(); if (source) send({ kind: 'removeExpression' }); });
    on(ta, 'input', paintGutter);
    on(ta, 'blur', () => { commit(); });
    on(ta, 'keydown', (e) => {
      if (e.stopPropagation) e.stopPropagation();
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { if (e.preventDefault) e.preventDefault(); commit(); if (ta.blur) ta.blur(); }
      if (e.key === 'Escape') { ta.value = source ? source.code : ''; paintGutter(); if (ta.blur) ta.blur(); }
      if (e.key === 'Tab') {
        if (e.preventDefault) e.preventDefault();
        const s = ta.selectionStart, en = ta.selectionEnd;
        ta.value = ta.value.slice(0, s) + '  ' + ta.value.slice(en); ta.selectionStart = ta.selectionEnd = s + 2; paintGutter();
      }
    });
    on(examples, 'click', async (e) => {
      if (e && e.stopPropagation) e.stopPropagation();
      if (e && e.preventDefault) e.preventDefault();
      if (typeof ports.showContextMenu !== 'function' || !target) return;
      let control = null;
      try { const all = typeof ports.loadControls === 'function' ? await ports.loadControls(target) : []; control = all.length ? { name: all[0].name, type: all[0].type } : null; } catch (_) { control = null; }
      const others = typeof ports.otherLayerNames === 'function' ? ports.otherLayerNames(target.layerUid) : [];
      const items = Snippets.list({ dimension, property, others, control }).map((s) => ({
        label: tr(s.labelKey, s.id) + s.suffix, action() { insert(s.code); } }));
      ports.showContextMenu(e.clientX || 0, e.clientY || 0, items);
    });
    on(whip, 'mousedown', (e) => {
      if (!target) return;
      Whip.start({ document: doc }, e, whip, async (pick, mods) => {
        // Alt clones the dropped property's own expression (any row but this one); a plain drop references an expression control.
        const self = !!pick && !!pick.target && pick.target.layerUid === target.layerUid && pick.property === property;
        const ref = mods.alt && !self ? { text: null } : Whip.reference({ layerUid: target.layerUid, property }, pick);
        if (ref.reject) { toast(tr('toastPropertyNotReferenceable', 'This property cannot be referenced')); return; }
        let text = ref.text;
        if (mods.alt) {
          let cloned = null;
          try { const r = await ports.readProperty(selectorFor(loadedPin || stamp(ports.readContext()), { kind: 'expressionSource', target: pick.target, property: pick.property })); cloned = r && r.expression; } catch (_) { cloned = null; }
          if (!cloned || !cloned.code) { toast(tr('toastNoExpressionToClone', 'Nothing to clone')); return; }
          text = cloned.code;
        }
        const s = ta.selectionStart != null ? ta.selectionStart : ta.value.length, en = ta.selectionEnd != null ? ta.selectionEnd : s;
        ta.value = ta.value.slice(0, s) + text + ta.value.slice(en); ta.selectionStart = ta.selectionEnd = s + text.length; paintGutter();
        if (await commit()) toast(tr(mods.alt ? 'toastExpressionCloned' : 'toastReferenceInserted', 'Reference inserted'));
      });
    });
    [head, examples, removeBtn, pane, err].forEach((n) => { n.hidden = false; });

    function announce(reason) { try { if (typeof ports.layoutChanged === 'function') ports.layoutChanged(reason); } catch (_) { /* presentation only */ } }
    return Object.freeze({
      mount(container) { host = container; },
      open(nextTarget, nextProperty, nextDimension) {
        target = nextTarget; property = nextProperty; dimension = nextDimension; source = null; loadedPin = null; ta.value = '';
        if (host) host.replaceChildren(head, examples, removeBtn, pane, err);
        ta.placeholder = dimension === 2 ? '[value[0] + wiggle(2, 10)[0], value[1] + wiggle(2, 10)[1]]' : 'value + wiggle(2, 10)';
        paintGutter(); announce('expression-open'); return load();
      },
      close() { ++serial; target = null; source = null; loadedPin = null; ta.value = ''; paintError('', -1); if (host) host.replaceChildren(); announce('expression-close'); },
      refresh() { return target ? load() : Promise.resolve(false); },
      isOpenFor(t, p) { return !!target && target.layerUid === t.layerUid && target.strokeId === t.strokeId && property === p; },
      read() { return Object.freeze({ code: ta.value, enabled: cb.checked, errorLine, error: err.textContent }); },
      dispose() { disposed = true; ++serial; listeners.forEach((l) => l[0].removeEventListener(l[1], l[2])); listeners.length = 0; if (host) host.replaceChildren(); },
    });
  }
  return Object.freeze({ create });
}));
