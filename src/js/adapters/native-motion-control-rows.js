/* D18-B (#2376): the "Expression controls…" entry of the native Motion inspector, for the single active layer.
 * Baseline: openExprControlsMenu / addExprControl / renameExprControl / removeExprControl / moveExprControl of src/js/motion.js
 * @1065e9d8 (nested menu: add by type, then the existing controls with rename, move up, move down, delete; the same labels and toasts).
 * Every write is the native `object.property.edit` control declaration edit (addControl, renameControl, removeControl, moveControl)
 * through the inspector's own pinned editProperty port: no legacy exprControls writer, no second state. The control list is read from
 * the native property metadata (controlType/controlName), never cached across opens. window.prompt() is silently ignored by Tauri's
 * WKWebView, so the name is typed in an inline field of the row instead. The owner's refusals are typed, so a clash is pre-checked
 * here only to give the baseline toast; the engine stays the authority. */
(function (root, factory) {
  const commonjs = typeof module === 'object' && module.exports;
  const api = factory(commonjs ? require('./native-motion-property-view.js') : root.NemoNativeMotionPropertyView);
  if (commonjs) module.exports = api;
  else root.NemoNativeMotionControlRows = api;
}(typeof globalThis === 'object' ? globalThis : this, function (View) {
  'use strict';
  const TYPES = ['number', 'checkbox', 'angle', 'point', 'color'];
  const norm = (name) => String(name == null ? '' : name).trim().toLowerCase();
  const genKey = () => 'xc_' + Date.now().toString(36) + '_' + Math.random().toString(36).slice(2, 6);
  const tr = (host, key, fallback) => { const value = host.SM && typeof host.SM.t === 'function' ? host.SM.t(key) : null; return typeof value === 'string' && value && value !== key ? value : fallback; };
  const make = (host, tag, className, text) => { const node = host.document.createElement(tag); if (className) node.className = className; if (text !== undefined) node.textContent = text; return node; };

  // The declared controls of one layer, read page by page from the native property metadata (never cached across calls).
  async function loadControls(ports, target) {
    const consumer = View.createConsumer({ readContext: ports.readContext, query: ports.query }), found = [];
    try {
      let view = { kind: 'properties', target: { contextId: target.contextId, layerUid: target.layerUid }, offset: 0, limit: 8 };
      for (let pages = 0; pages < 64; pages += 1) {
        const page = await consumer.load(view);
        if (!page || page.available !== true) throw new Error('native controls are unavailable');
        page.items.forEach((item) => { if (item.controlType !== null) found.push({ key: item.property, name: item.controlName, type: item.controlType }); });
        view = View.continuation(page);
        if (!view) return found;
      }
      throw new Error('native controls exceeded the page bound');
    } finally { consumer.dispose(); }
  }

  // `ports`: readContext() -> {pin, ...}, query(selector), editProperty(payload, pin, isCurrent). `target`: {contextId, layerUid}.
  function build(host, ports, target) {
    if (!host || !host.document || !ports || typeof ports.readContext !== 'function' || typeof ports.query !== 'function' ||
        typeof ports.editProperty !== 'function' || !target || target.strokeId !== undefined || typeof target.layerUid !== 'string') return null;
    const node = make(host, 'div', 'native-motion-control-rows'), row = make(host, 'div', 'lrow motion-prop-row');
    const button = make(host, 'button', 'pbtn native-motion-controls-button', tr(host, 'ctxExprControlsEllipsis', 'Expression controls…'));
    const editor = make(host, 'div', 'lrow motion-prop-row native-motion-control-editor');
    let disposed = false, busy = false;
    button.type = 'button'; editor.style.display = 'none';
    row.appendChild(button); node.appendChild(row); node.appendChild(editor);
    const toast = (message) => { if (typeof host.showToast === 'function') host.showToast(message); };

    const load = () => loadControls({ readContext: ports.readContext, query: ports.query }, target);
    async function send(property, action) {
      if (busy) return false;
      busy = true;
      try {
        const context = ports.readContext();
        if (!context || !context.pin) throw new Error('native context is unavailable');
        const payload = { command: 'object.property.edit', target: { contextId: target.contextId, layerUid: target.layerUid }, property, action };
        const result = await ports.editProperty(payload, context.pin, () => !disposed);
        return !!result && result.applied === true;
      } catch (error) {
        toast(tr(host, 'ctrlEditRefused', 'Control edit refused: ') + String(error && error.message || error));
        return false;
      } finally { busy = false; }
    }
    function typeLabel(type) { return tr(host, 'ctrlType_' + type, type); }
    function closeEditor() { editor.replaceChildren(); editor.style.display = 'none'; }
    // The inline name field replaces the baseline prompt(): Enter or OK commits, Escape or Cancel leaves everything untouched.
    function openEditor(label, initial, commit) {
      const title = make(host, 'span', '', label), input = make(host, 'input', 'pi'), ok = make(host, 'button', 'pbtn', 'OK'), cancel = make(host, 'button', 'pbtn', tr(host, 'btnCancel', 'Cancel'));
      input.type = 'text'; input.value = initial; input.setAttribute('aria-label', label); ok.type = 'button'; cancel.type = 'button';
      const done = async () => { const name = String(input.value).trim(); closeEditor(); if (name) await commit(name); };
      input.addEventListener('keydown', (e) => {
        if (e.stopPropagation) e.stopPropagation();
        if (e.key === 'Enter') { if (e.preventDefault) e.preventDefault(); done(); } else if (e.key === 'Escape') closeEditor();
      });
      ok.addEventListener('click', (e) => { if (e && e.stopPropagation) e.stopPropagation(); done(); });
      cancel.addEventListener('click', (e) => { if (e && e.stopPropagation) e.stopPropagation(); closeEditor(); });
      editor.replaceChildren(title, input, ok, cancel); editor.style.display = '';
      if (typeof input.focus === 'function') input.focus();
    }
    const taken = (controls, name, except) => controls.some((c) => c.key !== except && norm(c.name) === norm(name));

    function menuItems(controls, x, y) {
      const items = [{ label: tr(host, 'ctrlMenuAddEllipsis', 'Add a control…'), action() {
        host.showContextMenu(x + 8, y + 8, TYPES.map((type) => ({ label: typeLabel(type), action() {
          openEditor(tr(host, 'promptControlName', 'Name of the control:'), typeLabel(type), async (name) => {
            if (taken(controls, name, null)) { toast(tr(host, 'toastControlNameTaken', 'A control with that name already exists on this layer')); return; }
            if (await send(genKey(), { kind: 'addControl', controlType: type, name })) {
              toast(tr(host, 'toastControlAddedPrefix', 'Control “') + name + tr(host, 'toastControlAddedSuffix', '” added'));
            }
          });
        } })));
      } }];
      if (!controls.length) return items;
      items.push({ sep: true }, { label: tr(host, 'ctrlMenuExistingColon', 'Existing controls:'), disabled: true, action() {} });
      controls.forEach((c, index) => {
        items.push({ label: c.name + '  ·  ' + typeLabel(c.type), action() {
          host.showContextMenu(x + 8, y + 8, [
            { label: tr(host, 'ctrlMenuRenameEllipsis', 'Rename…'), action() {
              openEditor(tr(host, 'promptControlRename', 'New name for this control:'), c.name, async (name) => {
                if (taken(controls, name, c.key)) { toast(tr(host, 'toastControlNameTaken', 'A control with that name already exists on this layer')); return; }
                await send(c.key, { kind: 'renameControl', name });
              });
            } },
            { label: tr(host, 'ctrlMenuMoveUp', 'Move up'), disabled: index === 0, action() { send(c.key, { kind: 'moveControl', direction: -1 }); } },
            { label: tr(host, 'ctrlMenuMoveDown', 'Move down'), disabled: index === controls.length - 1, action() { send(c.key, { kind: 'moveControl', direction: 1 }); } },
            { label: tr(host, 'ctrlMenuDelete', 'Delete'), action() {
              send(c.key, { kind: 'removeControl' }).then((applied) => {
                if (applied) toast(tr(host, 'toastControlDeletedPrefix', 'Control “') + c.name + tr(host, 'toastControlDeletedSuffix', '” deleted'));
              });
            } },
          ]);
        } });
      });
      return items;
    }
    button.addEventListener('click', async (e) => {
      if (e && e.stopPropagation) e.stopPropagation();
      if (disposed || typeof host.showContextMenu !== 'function') return;
      const box = typeof button.getBoundingClientRect === 'function' ? button.getBoundingClientRect() : { left: 0, bottom: 0 };
      let controls;
      try { controls = await load(); } catch (error) { toast(tr(host, 'ctrlListUnavailable', 'Controls are unavailable: ') + String(error && error.message || error)); return; }
      if (!disposed) host.showContextMenu(box.left, box.bottom + 2, menuItems(controls, box.left, box.bottom + 2));
    });
    return Object.freeze({ node, refresh: () => !disposed, dispose() { disposed = true; closeEditor(); row.replaceChildren(); } });
  }

  // Both entries (link rows and control row) are one panel row object: the panel owns a single refresh/dispose.
  function compose(host, first, second) {
    const parts = [first, second].filter(Boolean);
    if (parts.length < 2) return parts[0] || null;
    const node = make(host, 'div', 'native-motion-layer-rows');
    parts.forEach((part) => node.appendChild(part.node));
    return Object.freeze({ node, refresh: () => parts.map((p) => (typeof p.refresh === 'function' ? p.refresh() : true)).every(Boolean),
      dispose() { parts.forEach((p) => { try { p.dispose(); } catch (_) { /* a disposed part must not block the others */ } }); } });
  }

  return Object.freeze({ build, compose, loadControls });
}));
