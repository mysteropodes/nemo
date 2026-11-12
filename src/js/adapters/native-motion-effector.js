/* Native Motion canvas: the effector handles of a duplicator layer (#2520/#2525), the pure geometry of motion.js@1065e9d8 (buildOverlayItemsInner effector
 * block, hitEffectorHandle, onDrag 'effector'). No document, DOM or native access. Each effector with an anchor is a crosshair in a circle plus a thin
 * falloff ring of its radius; the anchor is stored in the layer's own space and drawn through the ancestors' map (`outer`, the six numbers
 * [a, b, c, d, e, f] of the native `parentAffine`: x' = a x + c y + e, y' = b x + d y + f), so a drag maps the pointer back through its inverse.
 * An effector without an anchor has no handle (the baseline skips `!eff.pos`). The handle is a static duplicator setting, not a Motion property, so a
 * drag writes one `object.duplicator.config` `effectorPos` member, never a key. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeMotionEffector = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const IDENTITY = Object.freeze([1, 0, 0, 1, 0, 0]);
  const HIT_PX = 8, CROSS_PX = 8, DOT_PX = 5; // baseline: tolerance 8 / zoom, crosshair arms 8 / zoom, circle 5 / zoom
  const finite = (v) => typeof v === 'number' && Number.isFinite(v);
  const affine = (m) => (Array.isArray(m) && m.length === 6 && m.every(finite) ? m : IDENTITY);
  const apply = (m, p) => ({ x: m[0] * p.x + m[2] * p.y + m[4], y: m[1] * p.x + m[3] * p.y + m[5] });
  function invert(m, p) {
    const det = m[0] * m[3] - m[1] * m[2];
    if (!finite(det) || det === 0) return null;
    const dx = p.x - m[4], dy = p.y - m[5];
    return { x: (m[3] * dx - m[2] * dy) / det, y: (-m[1] * dx + m[0] * dy) / det };
  }
  /* The handles a duplicator view shows: [{ index, pos: {x, y}, radius }], in the view's order, anchors only. */
  function handles(view) {
    const list = view && Array.isArray(view.effectors) ? view.effectors : [];
    const out = [];
    list.forEach((item, index) => {
      if (item && item.pos && finite(item.pos.x) && finite(item.pos.y)) out.push({ index, pos: { x: item.pos.x, y: item.pos.y }, radius: finite(item.radius) ? Math.max(0, item.radius) : 0 });
    });
    return out;
  }
  /* The handle under the pointer, back to front so the last one added wins an overlapping click (baseline). `client` maps a world point to client pixels. */
  function hit(list, outer, client, pointer) {
    const m = affine(outer);
    for (let i = list.length - 1; i >= 0; i--) {
      const c = client(apply(m, list[i].pos));
      if (Math.hypot(pointer.x - c.x, pointer.y - c.y) < HIT_PX) return { type: 'effector', index: list[i].index };
    }
    return null;
  }
  /* The stored anchor a drag to the world point `to` means; null when the ancestors' map cannot be inverted or the point is not finite. */
  function move(index, outer, to) {
    if (!to || !finite(to.x) || !finite(to.y)) return null;
    const q = invert(affine(outer), to);
    if (!q || !finite(q.x) || !finite(q.y)) return null;
    return { index, x: Math.round(q.x * 1e6) / 1e6, y: Math.round(q.y * 1e6) / 1e6 };
  }
  /* What to draw for one handle, in client pixels: the centre, the crosshair arms, the dot radius and the falloff ring (a real document distance, so it
   * scales with the zoom through `pxPerUnit`, unlike the glyphs). */
  function glyph(handle, outer, client, pxPerUnit) {
    const c = client(apply(affine(outer), handle.pos));
    return { index: handle.index, c, arm: CROSS_PX, dot: DOT_PX, ring: handle.radius * pxPerUnit };
  }
  /* The list with one handle's anchor replaced (the drag preview). */
  function preview(list, moved) {
    return moved ? list.map((h) => (h.index === moved.index ? Object.assign({}, h, { pos: { x: moved.x, y: moved.y } }) : h)) : list;
  }
  return Object.freeze({ IDENTITY, handles, hit, move, glyph, preview, apply, invert });
}));
