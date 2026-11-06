/* The 3D Layer column cell of a native layer row (#2520/#2525): the baseline `.lico.motion-col-3d` hexagon icon of timeline.js@1065e9d8 with the
 * `off` class while the switch is off, titled "3D Layer". It owns no state and commits nothing: the row shows the read-model fact `threeD`, and a click
 * hands the opposite value to the `toggle` the presenter built (one `object.layer.policy.edit` of the threeD flag, one revision, one history entry). */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeLayerThreeD = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const ICO_3D = '<svg viewBox="0 0 24 24"><path d="M12 3 L20 7.5 L20 16.5 L12 21 L4 16.5 L4 7.5 Z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><path d="M12 3 L12 12 M20 7.5 L12 12 M4 7.5 L12 12 M12 12 L12 21" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>';
  const UNPLACED = new Set(['text', 'unknown', 'component', 'effector']);
  function cell(doc, layer, toggle) {
    if (!doc || !layer || typeof layer.layerUid !== 'string' || typeof toggle !== 'function') return null;
    const on = layer.threeD === true;
    // The engine takes the switch on every kind but Text, a component instance and an effector (typed unavailable, no revision): such a row shows no
    // cell, unless a stored switch is already on, which must stay switchable off.
    if (!on && UNPLACED.has(layer.kind)) return null;
    const node = doc.createElement('div');
    node.className = 'lico motion-col-3d' + (on ? '' : ' off');
    node.title = '3D Layer';
    node.innerHTML = ICO_3D;
    node.dataset.nativeLayerThreeD = String(on);
    node.addEventListener('click', (event) => {
      if (event && typeof event.stopPropagation === 'function') event.stopPropagation();
      return toggle(!on);
    });
    return node;
  }
  return Object.freeze({ cell, ICO_3D });
}));
