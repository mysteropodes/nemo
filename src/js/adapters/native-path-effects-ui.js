/* #2372: the path-effects section of the Effects panel (baseline src/js/effects-panel.js @1065e9d8, "Effets de TRACE") over the native
 * document. The baseline list is one stack, ld.pathFx, edited by four controls: add (the select), the enable checkbox, the delete
 * button and one number field per parameter. Each of them sends the layer's COMPLETE resulting stack as ONE native edit
 * (NemoNativeObjectVector.setPathEffects, object.path.effects.set, one history entry). The current stack is read from the pinned
 * project info at the moment the edit runs (layer row member `pathFx`, `pathFxOversize` when past the row bound) and the edit is
 * pinned to that revision, so a queued action never rewrites a stale stack; actions run strictly one after the other.
 * The adapter holds no document state and has no fallback writer: without a native project `on()` is false and the caller keeps
 * its legacy code; with one, a missing route, an unknown effect type or a refusal is a toast, never a write to ld.pathFx.
 * A parameter drag commits once on release (data-native-opacity-scrub, the scrub contract of ui.js), a typed value on change. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativePathEffectsUi = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const PIN_KEYS = ['instanceId', 'documentId', 'contentRevision', 'documentSnapshotId'];
  const MESSAGES = Object.freeze({
    unavailable: 'Native path-effect command is unavailable.', oversize: 'This path-effect stack is too large to edit natively.',
    stale: 'The project changed under this edit; try again.', refused: 'Native path-effect command was refused.',
    layer: 'Select a layer first.',
  });
  const copy = (value) => JSON.parse(JSON.stringify(value));
  const panels = new WeakMap();

  function panel(host) {
    if (panels.has(host)) return panels.get(host);
    let tail = Promise.resolve();
    const session = () => { try { const mod = host.NemoNativeObjectProject, s = mod && mod.desktop(host); return s && s.blocksLegacy() ? s : null; } catch (_) { return null; } };
    const vector = () => { try { const mod = host.NemoNativeObjectVector; return mod ? mod.desktop(host) : null; } catch (_) { return null; } };
    const on = () => session() !== null && vector() !== null;
    const rows = () => { const s = session(), info = s && s.info(); return info && Array.isArray(info.layers) ? info.layers : []; };
    const toast = (message) => { if (typeof host.showToast === 'function') host.showToast(message); };
    const redraw = () => { try { if (typeof host.updatePathFxPanel === 'function') host.updatePathFxPanel(); } catch (_) { /* presentation only */ } };
    const layerRow = () => rows()[host.state.activeLayerIdx] || null;
    // The stored stack of a row: [] when none, null when the row names it oversize (never guess at a stack).
    const stackOfRow = (row) => (!row || row.pathFxOversize === true ? null : Array.isArray(row.pathFx) ? copy(row.pathFx) : []);
    const defaults = (type) => (host.SMPathFx && typeof host.SMPathFx.defaultsFor === 'function' ? host.SMPathFx.defaultsFor(type) : { type, enabled: true });

    function report(result) {
      const status = result && result.status;
      if (status === 'applied' || status === 'noop') return;
      if (status === 'unmapped') {
        const typed = result.code === 'unsupported_effect';
        toast(typed ? `Path effect not available natively: ${result.reason}` : `Native path-effect stack was not accepted: ${result.reason || result.code}`);
      } else if (status === 'stale') toast(MESSAGES.stale);
      else if (status === 'unavailable') toast(MESSAGES.unavailable);
      else toast(MESSAGES.refused);
    }
    // One action: `uid` is the layer the control was drawn for, `mutate(stack)` the baseline's change of a copy of its stack.
    function edit(uid, mutate) {
      tail = tail.then(() => {
        const client = vector(), s = session(), info = s && s.info();
        if (!client || !info) { toast(MESSAGES.unavailable); return null; }
        const row = rows().find((candidate) => candidate.layerUid === uid);
        if (!row) { toast(MESSAGES.layer); return null; }
        const stack = stackOfRow(row);
        if (!stack) { toast(MESSAGES.oversize); return null; }
        const next = mutate(stack);
        if (!Array.isArray(next)) return null;
        const origin = PIN_KEYS.reduce((acc, key) => { acc[key] = info[key]; return acc; }, {});
        return client.setPathEffects(uid, next, { origin }).then(report, () => toast(MESSAGES.refused));
      }).catch(() => toast(MESSAGES.refused)).then(redraw);
      return tail;
    }

    function addType(type) {
      const row = layerRow();
      if (!row) { toast(MESSAGES.layer); return; }
      edit(row.layerUid, (stack) => stack.concat([defaults(type)]));
    }
    const label = (key) => (host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : key);
    function paramRow(uid, idx, fx, p) {
      const doc = host.document;
      const pr = doc.createElement('div');
      pr.className = 'pr'; pr.style.cssText = 'margin-top:4px;';
      const lbl = doc.createElement('span'); lbl.className = 'pl'; lbl.textContent = label(p.label);
      const inp = doc.createElement('input');
      inp.type = 'number'; inp.className = 'pi scrub';
      inp.min = p.min; inp.max = p.max; inp.step = p.step; inp.dataset.step = String(p.step);
      // ui.js: a dragged field skips the live input/change events and fires ONE change on release (one native history entry).
      inp.dataset.nativeOpacityScrub = 'static';
      inp.value = host.SMPathFx.paramValue(fx, p.key);
      inp.addEventListener('change', () => { const value = parseFloat(inp.value) || 0; edit(uid, (stack) => { if (!stack[idx]) return null; stack[idx][p.key] = value; return stack; }); });
      pr.appendChild(lbl); pr.appendChild(inp);
      return pr;
    }
    function entryRow(uid, idx, fx, def) {
      const doc = host.document;
      const el = doc.createElement('div');
      el.style.cssText = 'border:1px solid var(--border);border-radius:5px;padding:5px 6px;margin-bottom:4px;';
      const head = doc.createElement('div');
      head.style.cssText = 'display:flex;align-items:center;gap:6px;font-size:10px;';
      const eye = doc.createElement('input');
      eye.type = 'checkbox'; eye.checked = fx.enabled !== false;
      eye.style.accentColor = 'var(--accent)';
      eye.addEventListener('change', () => { const enabled = eye.checked; edit(uid, (stack) => { if (!stack[idx]) return null; stack[idx].enabled = enabled; return stack; }); });
      const name = doc.createElement('span');
      name.textContent = label(def.label); name.style.flex = '1';
      const del = doc.createElement('button');
      del.className = 'pbtn'; del.textContent = '×'; del.style.cssText = 'padding:1px 6px;font-size:11px;';
      del.addEventListener('click', () => edit(uid, (stack) => { if (!stack[idx]) return null; stack.splice(idx, 1); return stack; }));
      head.appendChild(eye); head.appendChild(name); head.appendChild(del);
      el.appendChild(head);
      def.params.forEach((p) => el.appendChild(paramRow(uid, idx, fx, p)));
      return el;
    }
    // Draws the section from the pinned row. Answers false when no native project is active (the caller draws its own).
    function render() {
      if (!on()) return false;
      const doc = host.document, list = doc.getElementById('path-fx-list');
      if (!list || !host.SMPathFx) return true;
      const row = layerRow(), wrap = doc.getElementById('path-fx-sec');
      if (wrap) wrap.style.display = row && row.kind !== 'null' ? '' : 'none';
      if (!row || row.kind === 'null') return true;
      list.innerHTML = '';
      const stack = stackOfRow(row);
      if (!stack) {
        const note = doc.createElement('div');
        note.style.cssText = 'font-size:10px;opacity:.7;padding:4px 0;'; note.textContent = MESSAGES.oversize;
        list.appendChild(note);
      } else {
        stack.forEach((fx, idx) => { const def = host.SMPathFx.typeDef(fx.type); if (def) list.appendChild(entryRow(row.layerUid, idx, fx, def)); });
      }
      const add = doc.getElementById('path-fx-add');
      if (add) {
        if (!add._nativePathFx) {
          add._nativePathFx = true;
          add.addEventListener('change', () => { const type = add.value; if (!type) return; add.value = ''; if (on()) addType(type); });
        }
        const known = Array.prototype.slice.call(add.options || add.children || []).map((o) => o.value);
        host.SMPathFx.TYPES.forEach((t) => {
          if (known.indexOf(t.id) >= 0) return;
          const o = doc.createElement('option'); o.value = t.id; o.textContent = label(t.label); add.appendChild(o);
        });
      }
      return true;
    }
    const api = Object.freeze({ on, render, stack: () => stackOfRow(layerRow()), addType, MESSAGES });
    panels.set(host, api);
    return api;
  }
  return Object.freeze({ panel });
}));
