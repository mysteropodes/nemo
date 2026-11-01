/* D18-C (#2376): the "+" next to a widget axis (#p-widget-xadd / #p-widget-yadd), baseline openAddLinkMenu of rig-widget.js @1065e9d8.
 * Two entries, each a menu of every layer property: "Link a property" writes `layerControl("uid", "Axis")`, "Drive a pose" writes
 * `self.at(layerControl("uid", "Axis"))` (CLAUDE.md section 13) into that property's expression, as one native `object.property.edit`
 * setExpression at the pin the lists were read at. The axis IS an expression control of the widget layer, addressed by its name.
 * Ports: readContext, query, editProperty, layers() -> [{layerUid, name}], contextId, showContextMenu, showToast, translate. A path
 * that cannot be proven current ends in a typed toast; there is no legacy expressions or exprControls writer here. */
(function (root, factory) {
  const node = typeof module === 'object' && module.exports;
  const api = factory(node ? require('./native-motion-property-view.js') : root.NemoNativeMotionPropertyView);
  if (node) module.exports = api;
  else root.NemoNativeObjectWidgetLink = api;
}(typeof globalThis === 'object' ? globalThis : this, function (View) {
  'use strict';
  const refText = (layerUid, name) => 'layerControl(' + JSON.stringify(layerUid) + ', ' + JSON.stringify(name) + ')';
  const label = (item) => (typeof item.controlName === 'string' ? item.controlName : item.property.charAt(0).toUpperCase() + item.property.slice(1));

  function create(ports) {
    const tr = (key, fallback) => { let v; try { v = ports.translate ? ports.translate(key) : undefined; } catch (_) { v = undefined; } return typeof v === 'string' && v && v !== key ? v : fallback; };
    const toast = (m) => { if (typeof ports.showToast === 'function') ports.showToast(m); };
    // Every available property of one layer, page by page from the pinned metadata.
    async function properties(layerUid) {
      const consumer = View.createConsumer({ readContext: ports.readContext, query: ports.query }), out = [];
      try {
        let view = { kind: 'properties', target: { contextId: ports.contextId, layerUid }, offset: 0, limit: 8 };
        for (let pages = 0; pages < 64; pages += 1) {
          const page = await consumer.load(view);
          if (!page || page.available !== true) throw new Error('native properties are unavailable');
          page.items.forEach((item) => { if (item.availability.applicability === 'available' && item.property !== 'order') out.push(item); });
          view = View.continuation(page);
          if (!view) return out;
        }
        throw new Error('native properties exceeded the page bound');
      } finally { consumer.dispose(); }
    }
    // source: {layerUid, name, key?} of the widget axis. mode: 'link' | 'pose'.
    async function open(x, y, source, mode) {
      if (typeof ports.showContextMenu !== 'function' || typeof ports.editProperty !== 'function') { toast(tr('nativeWidgetLinkUnavailable', 'Native widget linking is unavailable.')); return false; }
      const ref = refText(source.layerUid, source.name), code = mode === 'pose' ? 'self.at(' + ref + ')' : ref;
      let pin, choices = [];
      try {
        pin = Object.freeze(Object.assign({}, ports.readContext().pin));
        for (const layer of ports.layers()) {
          (await properties(layer.layerUid)).forEach((item) => {
            // The axis itself would read its own value into itself: a cycle, never offered.
            if (layer.layerUid === source.layerUid && (item.property === source.key || item.controlName === source.name)) return;
            choices.push({ layerUid: layer.layerUid, label: layer.name + ' · ' + label(item), property: item.property });
          });
        }
      } catch (error) { toast(tr('nativeWidgetLinkUnavailable', 'Native widget linking is unavailable.') + ' ' + String(error && error.message || error)); return false; }
      if (!choices.length) return false;
      ports.showContextMenu(x + 8, y + 8, choices.map((c) => ({ label: c.label, async action() {
        try {
          const payload = { command: 'object.property.edit', target: { contextId: ports.contextId, layerUid: c.layerUid }, property: c.property, action: { kind: 'setExpression', code } };
          const result = await ports.editProperty(payload, pin, () => true);
          if (result && result.applied === true) toast(tr(mode === 'pose' ? 'toastPoseDrivenByWidget' : 'toastLinkedToWidget', 'Linked'));
        } catch (error) { toast(tr('ctrlEditRefused', 'Edit refused: ') + String(error && error.message || error)); }
      } })));
      return true;
    }
    // The two baseline entries; each opens the property menu in its mode.
    function openMenu(x, y, source) {
      if (typeof ports.showContextMenu !== 'function') { toast(tr('nativeWidgetLinkUnavailable', 'Native widget linking is unavailable.')); return false; }
      ports.showContextMenu(x, y, [
        { label: tr('ctxWidgetAddLinkPropertyEllipsis', 'Link a property…'), action() { open(x, y, source, 'link'); } },
        { label: tr('ctxWidgetAddDrivePoseEllipsis', 'Drive a pose…'), action() { open(x, y, source, 'pose'); } },
      ]);
      return true;
    }
    return Object.freeze({ openMenu, open });
  }
  return Object.freeze({ create, refText });
}));
