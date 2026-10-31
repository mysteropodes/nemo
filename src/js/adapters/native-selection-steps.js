/* The batch steps of a native Select style edit (#2372, SEL-STYLE / SEL-STYLE2). Pure: it reads the selected presented paths (and, for Smooth,
 * a fresh read of their geometry) and names what the engine must do, it writes nothing. A `restyle` step per selected item carries only the
 * members that item takes, by the baseline rule (1065e9d8 timeline.js setters over `selectedPaths`):
 * - an image or a text block takes a colour and an opacity;
 * - a pressure ribbon is inked by its fill, so a stroke colour (or the stroke switch) is its ink, the fill switch and the plain stroke details
 *   (cap, join, miter, dash) do not reach it, and the paint order does;
 * - every other record takes the lot. A paint switch carries the colour it turns on with; an explicit colour of the same edit wins.
 * Smooth is geometry: the baseline re-simplified a plain path (Paper `simplify`) and a ribbon's centreline (`buildCenterSegmentsFromRawStroke`)
 * and the result goes out as the same `reshape` steps the sculpt sends. A brush preset is its own edit, as is a smooth. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectionSteps = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var ITEMS = { raster: true, text: true };
  var COLOUR = ['fill', 'stroke', 'width', 'opacity'], DETAIL = ['cap', 'join', 'miter', 'dashOffset', 'strokeStyle'];
  var HEX = /^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
  var CHOICES = { cap: ['butt', 'round', 'square'], join: ['miter', 'round', 'bevel'], paintOrder: ['fillFirst', 'strokeFirst'], strokeStyle: ['solid', 'dashed', 'dotted'] };
  // Every member a panel edit may name, and whether the edit stands alone (a preset and a smooth are not combined with other members).
  var MEMBERS = COLOUR.concat(['fillEnabled', 'strokeEnabled', 'fillWith', 'strokeWith', 'paintOrder', 'brushPreset', 'smooth', 'maskFeather'], DETAIL);
  var ALONE = ['brushPreset', 'smooth', 'maskFeather'];

  // The members of `style` that are well formed (a bad one is dropped by the caller as an invalid edit).
  function valid(style) {
    var out = {}, ok = true;
    MEMBERS.forEach(function (k) {
      var v = style && style[k];
      if (v === undefined || v === null) return;
      var good = ({ fill: HEX.test(v), stroke: HEX.test(v), fillWith: HEX.test(v), strokeWith: HEX.test(v), fillEnabled: typeof v === 'boolean', strokeEnabled: typeof v === 'boolean',
        width: typeof v === 'number' && v > 0 && v <= 10000, opacity: typeof v === 'number' && v >= 0 && v <= 1, miter: typeof v === 'number' && v >= 1 && v <= 1000,
        dashOffset: typeof v === 'number' && Math.abs(v) <= 1e6, smooth: typeof v === 'number' && v >= 0 && v <= 1000, maskFeather: typeof v === 'number' && v >= 0 && v <= 500,
        brushPreset: typeof v === 'string' && v.length > 0 && v.length <= 96 })[k];
      if (CHOICES[k]) good = CHOICES[k].indexOf(v) >= 0;
      if (good) out[k] = v; else ok = false;
    });
    return ok && Object.keys(out).length ? out : null;
  }
  // The member that makes the edit stand alone, or null.
  function alone(style) { return ALONE.filter(function (k) { return style[k] !== undefined; })[0] || null; }

  // The `restyle` style for one selected path, or null when the edit has nothing for it (the baseline skipped it).
  // `last` names the colour member written last in the edit ('fill' | 'stroke'): a ribbon is inked by one colour, so the later write wins.
  function styleFor(path, s, last) {
    var role = path.data.nativeRole, out = {}, copy = function (k) { if (s[k] !== undefined) out[k] = s[k]; };
    // A mask has the Mask panel's feather and nothing else: the layer's shared feather (SEL-MASK), no paint.
    if (s.maskFeather !== undefined) return role === 'mask' ? { maskFeather: s.maskFeather } : null;
    if (role === 'mask') return null;
    if (alone(s)) {
      // A preset restamps a plain stroke; a ribbon and an image are not eligible (the baseline filtered them out).
      return s.brushPreset !== undefined && role !== 'ribbon' && !ITEMS[role] ? { brushPreset: s.brushPreset } : null;
    }
    // An image has an opacity only; a text block a colour and its alpha.
    if (ITEMS[role]) { (role === 'text' ? ['fill', 'opacity'] : ['opacity']).forEach(copy); return Object.keys(out).length ? out : null; }
    COLOUR.forEach(copy);
    var ribbon = role === 'ribbon' || path.data.nativeRibbon === true;
    if (s.strokeEnabled !== undefined) {
      out.strokeEnabled = s.strokeEnabled;
      if (s.strokeEnabled) out.stroke = s.stroke !== undefined ? s.stroke : s.strokeWith;
    }
    if (!ribbon && s.fillEnabled !== undefined) {
      out.fillEnabled = s.fillEnabled;
      if (s.fillEnabled) out.fill = s.fill !== undefined ? s.fill : s.fillWith;
    }
    // A Stroke Style pick was the baseline's applyStrokeStyle, which ran only on a stroked path that is not a ribbon (isVectorBrush): its dash,
    // offset, miter and paint order reach nothing else. The record lists a stroke colour only while it has a real stroke.
    var pick = s.strokeStyle !== undefined && (ribbon || !(path.data.nativeStyle && path.data.nativeStyle.stroke) && s.strokeEnabled !== true);
    if (!pick) copy('paintOrder');
    if (ribbon) {
      // A ribbon's ink is its fill; of a fill and a stroke colour in one edit the later write wins (a swap writes the stroke after the fill).
      if (out.stroke !== undefined && out.fill !== undefined && last === 'fill') delete out.stroke;
      else if (out.stroke !== undefined) delete out.fill;
      delete out.fillEnabled;
    } else DETAIL.forEach(function (k) { if (!(pick && ['miter', 'dashOffset', 'strokeStyle'].indexOf(k) >= 0)) copy(k); });
    return Object.keys(out).length ? out : null;
  }
  function restyleSteps(paths, style, last) {
    return paths.map(function (p) {
      var st = styleFor(p, style, last);
      return st && { restyle: { stableTarget: p.data.nativeStableTarget, style: st } };
    }).filter(Boolean);
  }

  // The reshape steps that smooth `ids` by `amount` over a fresh read of their geometry (a ribbon with its centreline).
  async function smoothSteps(root, ids, layerIdx, amount) {
    var I = root.NemoSelectCanvasIntent, Point = root.Point || root.paper.Point, touched = [];
    var fresh = await I.layerPaths(root, function (item) { return ids.indexOf(item.stableTarget.strokeId) >= 0; }, true, layerIdx);
    if (!fresh) return null;
    fresh.paths.forEach(function (p) {
      var role = p.data.nativeRole, cs = p.data.centerSegments;
      if (ITEMS[role] || role === 'mask') return;
      if (p.data.isVectorBrush && cs && cs.length > 1) {
        var rebuilt = root.buildCenterSegmentsFromRawStroke(cs.map(function (s) { return new Point(s.point[0], s.point[1]); }), cs.map(function (s) { return s.width; }), amount);
        p.data.centerSegments = Array.prototype.slice.call(rebuilt).map(function (c) { return { point: c.point, handleIn: c.handleIn, handleOut: c.handleOut, width: c.width }; });
      } else if (role !== 'ribbon') p.simplify(amount);
      else return;
      touched.push(p);
    });
    return { layerUid: fresh.layerUid, frame: fresh.frame, steps: I.reshapeSteps(touched) };
  }
  return Object.freeze({ valid: valid, alone: alone, styleFor: styleFor, restyleSteps: restyleSteps, smoothSteps: smoothSteps });
}));
