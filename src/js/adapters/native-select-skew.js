/* Native Select skew zones and handle cursors (#2372 A-select-skew-anchor-lasso). Baseline (1065e9d8): select-bridge.js skewZoneHitTest (:640-665),
 * the xform-skew drag (:1134-1150, onMove ~2368) and the cursor map (:1874); engine-bridge.js buildTransformBoxItems drew the tick marks.
 * Two small zones flank each edge midpoint, running ALONG the edge (9 to 20 screen px from the midpoint, on an edge of at least 48 px); they are
 * tried only after the 8 scale handles and the rotate ring, so a resize handle always wins a tie. Dragging a zone shears the box along that edge's
 * axis about the OPPOSITE edge midpoint (which stays fixed): the top and bottom edges shear x as a function of y, the left and right edges shear y
 * as a function of x, by the drag distance over the box's perpendicular dimension (so a drag of N px always skews by the same angle whatever the box
 * size); Ctrl lets the other axis shear too. The result is ONE world affine the gesture commits like a move or a scale (native-select-gesture.js
 * commitMatrix): a plain stroke, a ribbon and a fill by their geometry, an image or a text block by their stored `skew`.
 * This module also owns the cursor of every handle under the idle pointer (scale: the resize cursors, rotate: the turn arrow, skew: ew / ns) and the
 * hovered edge whose tick marks the overlay colours orange. It holds no document state and writes nothing. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeSelectSkew = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var MIN_EDGE_PX = 48, INNER_PX = 9, OUTER_PX = 20, TICK = 'rgb(74,158,255)', HOVER = 'rgb(255,159,10)';
  // Box corners are [nw, ne, se, sw]; handles [nw, n, ne, e, se, s, sw, w]. An edge: its two corners and its midpoint handle.
  var ENDS = { n: [0, 1], s: [3, 2], w: [0, 3], e: [1, 2] }, MID = { n: 1, e: 3, s: 5, w: 7 }, OPPOSITE = { n: 's', s: 'n', e: 'w', w: 'e' };
  var SCALE_CURSORS = ['nwse-resize', 'ns-resize', 'nesw-resize', 'ew-resize', 'nwse-resize', 'ns-resize', 'nesw-resize', 'ew-resize'];
  // No native CSS rotate cursor exists: the baseline's curved arrow, white on black, hotspot at its centre.
  var ROTATE_CURSOR = "url(\"data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='22' height='22' viewBox='0 0 22 22'><path d='M4 11a7 7 0 1 1 2.1 5' fill='none' stroke='black' stroke-width='3.2' stroke-linecap='round'/><path d='M4 11a7 7 0 1 1 2.1 5' fill='none' stroke='white' stroke-width='1.6' stroke-linecap='round'/><path d='M3 15.5 4 11l4 2.2Z' fill='black'/><path d='M3.6 15 4.3 11.6l3 1.7Z' fill='white'/></svg>\") 11 11, grab";
  var states = new WeakMap();
  function state(root) { var s = states.get(root); if (!s) states.set(root, s = { edge: null }); return s; }

  // The edge whose skew zone the world point `p` is in (`wpp` world units per screen pixel), or null.
  function hit(box, p, wpp) {
    var found = null;
    Object.keys(ENDS).forEach(function (k) {
      if (found) return;
      var a = box.corners[ENDS[k][0]], b = box.corners[ENDS[k][1]], ex = b[0] - a[0], ey = b[1] - a[1], len = Math.hypot(ex, ey);
      if (!(len / wpp >= MIN_EDGE_PX)) return;
      var dx = ex / len, dy = ey / len, mid = box.handles[MID[k]];
      [-1, 1].forEach(function (side) {
        if (found) return;
        var cx = mid[0] + dx * side * (INNER_PX + OUTER_PX) / 2 * wpp, cy = mid[1] + dy * side * (INNER_PX + OUTER_PX) / 2 * wpp;
        var rx = p.x - cx, ry = p.y - cy, along = rx * dx + ry * dy, perp = Math.hypot(rx - dx * along, ry - dy * along);
        if (Math.abs(along) <= (OUTER_PX - INNER_PX) / 2 * wpp && perp <= INNER_PX * wpp) found = { type: 'skew', edge: k };
      });
    });
    return found;
  }
  // The drag's fixed data: the opposite edge midpoint it shears about, the grabbed midpoint, and the perpendicular dimension that normalises it.
  function start(box, edge) {
    var vertical = edge === 'n' || edge === 's';
    return { anchor: box.handles[MID[OPPOSITE[edge]]].slice(), grab: box.handles[MID[edge]].slice(), perp: Math.abs(vertical ? box.height : box.width) || 1 };
  }
  // The world affine of the drag at the world point `at` (the total from the press, never a per-tick delta), identity when it would flatten the selection.
  function matrix(g, at, ctrl) {
    var sign = g.edge === 'n' || g.edge === 'w' ? -1 : 1, dx = at.x - g.grab[0], dy = at.y - g.grab[1], shx = 0, shy = 0;
    if (g.edge === 'n' || g.edge === 's') { shx = sign * dx / g.perp; if (ctrl) shy = sign * dy / g.perp; }
    else { shy = sign * dy / g.perp; if (ctrl) shx = sign * dx / g.perp; }
    if (!Number.isFinite(shx) || !Number.isFinite(shy) || Math.abs(1 - shx * shy) < 0.05) return [1, 0, 0, 1, 0, 0];
    return [1, shy, shx, 1, -shx * g.anchor[1], -shy * g.anchor[0]];
  }
  function cursorOf(edge) { return edge === 'n' || edge === 's' ? 'ew-resize' : 'ns-resize'; }

  // The two tick marks flanking each edge midpoint, running along the edge, drawn into the overlay's svg. `c` is the overlay's capture (V, pin, bounds).
  function draw(root, svg, c, box) {
    var hovered = state(root).edge, k = c.pin.canvasW / c.bounds.width, V = c.V;
    var at = function (p) { return [V[0] * p[0] + V[2] * p[1] + V[4], V[1] * p[0] + V[3] * p[1] + V[5]]; };
    Object.keys(ENDS).forEach(function (edge) {
      var a = at(box.corners[ENDS[edge][0]]), b = at(box.corners[ENDS[edge][1]]), len = Math.hypot(b[0] - a[0], b[1] - a[1]);
      if (!(len / k >= MIN_EDGE_PX)) return;
      var dx = (b[0] - a[0]) / len, dy = (b[1] - a[1]) / len, mid = at(box.handles[MID[edge]]);
      [-1, 1].forEach(function (side) {
        var n = root.document.createElementNS('http://www.w3.org/2000/svg', 'line');
        [['x1', mid[0] + dx * side * INNER_PX * k], ['y1', mid[1] + dy * side * INNER_PX * k], ['x2', mid[0] + dx * side * OUTER_PX * k], ['y2', mid[1] + dy * side * OUTER_PX * k],
          ['stroke', hovered === edge ? HOVER : TICK], ['stroke-width', 1.6], ['stroke-linecap', 'butt'], ['vector-effect', 'non-scaling-stroke']].forEach(function (a2) { n.setAttribute(a2[0], a2[1]); });
        svg.appendChild(n);
      });
    });
  }

  // The idle pointer over the selection's handles: the cursor of the one under it, and the hovered skew edge for the tick colour. Passive: it reads
  // the overlay's box and the current presentation and writes nothing in the document. Alt shows no handle cursor (it moves the pivot or lassos).
  function hover(root, e) {
    var I = root.NemoSelectCanvasIntent, O = root.NemoNativeSelectOverlay, G = root.NemoNativeSelectGesture, P = root.NemoNativeSelectPivot;
    var canvas = root.document && root.document.getElementById && root.document.getElementById('drawing-canvas'), s = state(root);
    var sel = (root.selectedPaths || []).filter(function (p) { return p && p.data && p.data.nativeStableTarget; }), cursor = '', edge = null;
    try {
      var shown = I && sel.length && !e.altKey && root.state && root.state.tool === 'select' && I.presentedState(root);
      var box = shown && O && O.selectionContext(root) === I.pickContext(root) ? O.boxOf(root, sel) : null;
      if (box) {
        var world = function (x, y) { return I.worldPoint(root, x, y, shown.pin, shown.presentation); };
        var p = world(e.clientX, e.clientY), q = world(e.clientX + 1, e.clientY), wpp = Math.hypot(q.x - p.x, q.y - p.y) || p.perPixel;
        var h = G && typeof G.hitHandle === 'function' ? G.hitHandle(box, p, wpp, P ? P.pointOf(root, box) : null) : null, z = h ? null : hit(box, p, wpp);
        if (h && h.type === 'rotate') cursor = ROTATE_CURSOR;
        else if (h) cursor = SCALE_CURSORS[h.index] || '';
        else if (z) { cursor = cursorOf(z.edge); edge = z.edge; }
      }
    } catch (_) { cursor = ''; edge = null; }
    var next = cursor || 'default';
    if (canvas && canvas.style && (canvas.dataset ? canvas.dataset.xformCursor : undefined) !== next) {
      canvas.style.cursor = next;
      if (canvas.dataset) canvas.dataset.xformCursor = next;
    }
    if (edge !== s.edge) { s.edge = edge; if (O && typeof O.refresh === 'function') O.refresh(root); }
  }
  return Object.freeze({ hit: hit, start: start, matrix: matrix, draw: draw, hover: hover, cursorOf: cursorOf, hoveredEdge: function (root) { return state(root).edge; } });
}));
