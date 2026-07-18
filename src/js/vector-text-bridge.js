// ---- Vector Text (2026-07) ----
// Real vector glyph rendering, as a follow-up to the raster-baked text tool:
// opentype.js parses a bundled TTF and returns bezier-curve glyph outlines
// per character, converted here into ordinary Paper.js Path items — a hole
// (the counter of an 'o', 'e', 'a', ...) is merged into its exterior contour
// via the SAME slit-merge helper (_mergeHoleIntoExterior, tools.js) hand-
// drawn boolean-op results already use, so a glyph stays a single plain Path
// with straight/curved segments, never a CompoundPath. This means vector
// text needs ZERO new item type and ZERO Rust/engine changes: every
// consumer of layer.children (buildSceneJson, saveActiveLayerFrame,
// tween matching, export.js, serP/desP) already treats these as ordinary
// Paths — the exact same "family of bug n°1" risk this session's earlier
// audit flagged for a real distinct item type is sidestepped entirely.
//
// Bonus this buys for free: each character is ALREADY its own Path (or
// small set of Paths for disconnected glyphs like ':'/'%'), so per-
// character Motion animation/expressions work the instant vector text is
// placed — no separate "Découper par caractère" bake step needed, unlike
// the raster text tool's per-character split.
var VECTOR_FONTS = {
  'Roboto-Regular': { url: 'fonts/Roboto-Regular.ttf', label: 'Roboto' },
  'Roboto-Bold': { url: 'fonts/Roboto-Bold.ttf', label: 'Roboto Bold' },
};
var _vecFontCache = {};
function loadVectorFont(key) {
  if (_vecFontCache[key]) return _vecFontCache[key];
  var spec = VECTOR_FONTS[key];
  if (!spec) return Promise.reject(new Error('unknown vector font ' + key));
  _vecFontCache[key] = fetch(spec.url).then(function (r) { return r.arrayBuffer(); }).then(function (buf) { return opentype.parse(buf); });
  return _vecFontCache[key];
}
// Pre-warm both bundled weights on script load — Regular is the common
// case, warmed first, so the FIRST vector-text placement doesn't stall on
// a network fetch.
loadVectorFont('Roboto-Regular');
loadVectorFont('Roboto-Bold');

// opentype.js glyph paths carry command types M/L/Q/C/Z with coordinates
// ALREADY in final pixel space (glyph.getPath(x,y,fontSize) applies
// unitsPerEm scaling and the font-units-are-y-up -> canvas-is-y-down flip
// internally) — so no extra scale/flip is needed here, only a quadratic-
// >cubic conversion (Paper.js Segments are cubic-only) and converting
// absolute control points into the handleIn/handleOut OFFSETS Paper.js
// Segments expect (confirmed via serP's existing handleIn/handleOut usage
// elsewhere in this codebase — these are relative to their own segment's
// point, not absolute coordinates).
function otPathToContours(otPath) {
  var contours = []; var cur = null;
  otPath.commands.forEach(function (c) {
    if (c.type === 'M') {
      cur = [{ point: [c.x, c.y], handleIn: [0, 0], handleOut: [0, 0] }];
      contours.push(cur);
    } else if (c.type === 'L') {
      cur.push({ point: [c.x, c.y], handleIn: [0, 0], handleOut: [0, 0] });
    } else if (c.type === 'Q') {
      var p0 = cur[cur.length - 1].point;
      var c1x = p0[0] + 2 / 3 * (c.x1 - p0[0]), c1y = p0[1] + 2 / 3 * (c.y1 - p0[1]);
      var c2x = c.x + 2 / 3 * (c.x1 - c.x), c2y = c.y + 2 / 3 * (c.y1 - c.y);
      cur[cur.length - 1].handleOut = [c1x - p0[0], c1y - p0[1]];
      cur.push({ point: [c.x, c.y], handleIn: [c2x - c.x, c2y - c.y], handleOut: [0, 0] });
    } else if (c.type === 'C') {
      var p0c = cur[cur.length - 1].point;
      cur[cur.length - 1].handleOut = [c.x1 - p0c[0], c.y1 - p0c[1]];
      cur.push({ point: [c.x, c.y], handleIn: [c.x2 - c.x, c.y2 - c.y], handleOut: [0, 0] });
    } else if (c.type === 'Z') {
      // A well-formed glyph contour's last on-curve point already coincides
      // with its own start (M) — drop that duplicate (Paper.js's own
      // `closed=true` draws the implicit closing segment already) so the
      // slit-merge's closest-pair search below never picks a zero-length
      // spurious match at the seam.
      if (cur && cur.length > 1) {
        var first = cur[0].point, last = cur[cur.length - 1].point;
        if (Math.abs(first[0] - last[0]) < 0.01 && Math.abs(first[1] - last[1]) < 0.01) {
          cur[0].handleIn = cur[cur.length - 1].handleIn;
          cur.pop();
        }
      }
    }
  });
  return contours;
}
function contourBBox(segs) {
  var minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  segs.forEach(function (s) {
    var p = s.point;
    if (p[0] < minX) minX = p[0]; if (p[0] > maxX) maxX = p[0];
    if (p[1] < minY) minY = p[1]; if (p[1] > maxY) maxY = p[1];
  });
  return { minX: minX, minY: minY, maxX: maxX, maxY: maxY };
}
function bboxContains(a, b) { return a.minX <= b.minX && a.minY <= b.minY && a.maxX >= b.maxX && a.maxY >= b.maxY; }
function bboxArea(b) { return Math.max(0, b.maxX - b.minX) * Math.max(0, b.maxY - b.minY); }
// Classifies each contour as an exterior or a hole of its tightest-fitting
// container (bbox-containment, same heuristic insertBooleanResult already
// relies on for boolean-op results elsewhere in this codebase — winding-
// direction-agnostic, so it doesn't depend on opentype.js's exact contour
// orientation convention), then merges each hole into its exterior via
// _mergeHoleIntoExterior (tools.js) — same "keyhole slit" technique that
// function already uses for boolean-op results, reused as-is here since it
// only reorders segment dicts, never touches their handle values.
function contoursToMergedPaths(contours) {
  var withBBox = contours.map(function (c) { return { segs: c, bbox: contourBBox(c), container: -1 }; });
  withBBox.forEach(function (c, i) {
    withBBox.forEach(function (o, j) {
      if (i === j) return;
      if (bboxContains(o.bbox, c.bbox)) {
        if (c.container === -1 || bboxArea(withBBox[c.container].bbox) > bboxArea(o.bbox)) c.container = j;
      }
    });
  });
  var out = [];
  withBBox.forEach(function (c, i) {
    if (c.container !== -1) return; // handled as a hole below, from its container's pass
    var extSegs = c.segs.slice();
    withBBox.forEach(function (h) { if (h.container === i) extSegs = _mergeHoleIntoExterior(extSegs, h.segs); });
    out.push(extSegs);
  });
  return out;
}
function buildGlyphPaths(otPath, color) {
  var contours = otPathToContours(otPath);
  if (!contours.length) return [];
  var merged = contoursToMergedPaths(contours);
  return merged.map(function (segs) {
    var p = new Path({ insert: true });
    segs.forEach(function (s) {
      p.add(new Segment(new Point(s.point[0], s.point[1]), new Point(s.handleIn[0], s.handleIn[1]), new Point(s.handleOut[0], s.handleOut[1])));
    });
    p.closed = true;
    p.fillColor = color; p.strokeColor = null;
    return p;
  });
}
// Same greedy word-wrap contract as timeline.js's computeTextLayout (raster
// path) — kept intentionally parallel so switching a text block between
// raster and vector modes wraps identically, not a second slightly-
// different implementation of the same behavior.
function wrapVectorLines(font, text, size, fixedWidthWorld) {
  var lines = text.split('\n');
  if (!fixedWidthWorld) return lines;
  var wrapped = [];
  lines.forEach(function (l) {
    if (l === '') { wrapped.push(''); return; }
    var words = l.split(' '); var cur = '';
    words.forEach(function (w) {
      var test = cur ? cur + ' ' + w : w;
      if (cur && font.getAdvanceWidth(test, size) > fixedWidthWorld) { wrapped.push(cur); cur = w; }
      else cur = test;
    });
    wrapped.push(cur);
  });
  return wrapped;
}
// Builds a placed vector-text block as flat Path items sharing a groupId
// (same "stable id groups members, not a new item type" pattern as Cmd+G
// groups) into `layer`, top-left anchored at `topLeftWorld`. Returns
// {paths, groupId, width, height} — paths[0] carries the FULL metadata
// (isTextRoot) needed to find/re-edit/re-wrap this block later.
function buildVectorTextGroup(text, fontKey, size, color, align, fixedWidthWorld, topLeftWorld, layer) {
  return loadVectorFont(fontKey).then(function (font) {
    var lineHeight = size * 1.25; // matches the raster bake's own line-height convention
    var wrapped = wrapVectorLines(font, text, size, fixedWidthWorld);
    var maxW = 0;
    wrapped.forEach(function (l) { maxW = Math.max(maxW, font.getAdvanceWidth(l, size)); });
    var groupId = 'vtxt' + Date.now().toString(36) + '_' + Math.floor(Math.random() * 1e6);
    var allPaths = [];
    var prevActive = project.activeLayer; layer.activate();
    wrapped.forEach(function (line, li) {
      var lineWidth = font.getAdvanceWidth(line, size);
      var startX = topLeftWorld.x;
      if (align === 'center') startX = topLeftWorld.x + (maxW - lineWidth) / 2;
      else if (align === 'right') startX = topLeftWorld.x + (maxW - lineWidth);
      var cursorX = startX;
      // Ascent ≈ size, matching the raster bake's textBaseline='top' anchor
      // closely enough for the two modes to feel consistent when switching.
      var baselineY = topLeftWorld.y + li * lineHeight + size * 0.8;
      line.split('').forEach(function (ch) {
        var glyph = font.charToGlyph(ch);
        if (ch.trim() !== '') {
          var otPath = glyph.getPath(cursorX, baselineY, size);
          var built = buildGlyphPaths(otPath, color);
          built.forEach(function (p) { p.data.isVectorText = true; p.data.groupId = groupId; p.data.vectorChar = ch; });
          allPaths = allPaths.concat(built);
        }
        cursorX += glyph.advanceWidth * (size / font.unitsPerEm);
      });
    });
    prevActive.activate();
    if (allPaths.length) {
      var root = allPaths[0];
      root.data.isText = true; root.data.isTextRoot = true;
      root.data.text = text; root.data.vectorFont = fontKey; root.data.size = size;
      root.data.color = color; root.data.align = align; root.data.fixedWidth = fixedWidthWorld || null;
    }
    allPaths.forEach(function (p) { if (window.tagOwner) tagOwner(p); });
    return { paths: allPaths, groupId: groupId, width: maxW, height: wrapped.length * lineHeight };
  });
}
// Every Path sharing `raster`'s (well, root Path's) groupId — used both to
// remove the old block before a re-edit rebuild and, in principle, by any
// future tooling that needs "every glyph of this text block".
function vectorTextGroupMembers(root) {
  if (!root || !root.data || !root.data.groupId || !root.parent) return [root].filter(Boolean);
  var gid = root.data.groupId;
  return root.parent.children.filter(function (c) { return c.data && c.data.groupId === gid; });
}
window.SMVectorText = {
  VECTOR_FONTS: VECTOR_FONTS,
  loadVectorFont: loadVectorFont,
  buildVectorTextGroup: buildVectorTextGroup,
  vectorTextGroupMembers: vectorTextGroupMembers,
};
