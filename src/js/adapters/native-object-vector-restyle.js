/* #2372 SEL-STYLE: the closed members of a `restyle` step of `object.vector.batch` (pure validation; the vector contract names the target).
 * A restyle changes only the style it names on a presented record or stored item: a hex `fill` and `stroke` (#rrggbb or #rrggbbaa, the alpha
 * kept), a `width` in (0, 10000] and an `opacity` in 0..1, at least one; an optional `frame` is the held cell the edit lands in.
 * Mirrors native-engine object_vector_batch_style.rs and nemo-mcp native_vector_batch_contract.rs. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectVectorRestyle = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const MEMBERS = ['fill', 'stroke', 'width', 'opacity'];
  const fail = (message) => { throw new TypeError(message); };
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const plain = (value) => value !== null && typeof value === 'object' && !Array.isArray(value) && (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  const hex = (value) => typeof value === 'string' && /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(value);
  // `op` is an already shape-checked restyle step; `where` names it in the message.
  function check(op, where) {
    const style = op.style;
    if (!plain(style) || !Object.keys(style).length || Object.keys(style).some((key) => !MEMBERS.includes(key))) fail(`${where}.style must name fill, stroke, width or opacity`);
    ['fill', 'stroke'].forEach((key) => { if (has(style, key) && !hex(style[key])) fail(`${where}.style.${key} must be #rrggbb or #rrggbbaa`); });
    if (has(style, 'width') && !(typeof style.width === 'number' && style.width > 0 && style.width <= 10000)) fail(`${where}.style.width must be within (0, 10000]`);
    if (has(style, 'opacity') && !(typeof style.opacity === 'number' && style.opacity >= 0 && style.opacity <= 1)) fail(`${where}.style.opacity must be within 0..1`);
    if (has(op, 'frame') && !(Number.isSafeInteger(op.frame) && op.frame >= 0 && op.frame <= 4294967295)) fail(`${where}.frame must be a frame`);
  }
  return Object.freeze({ check });
}));
