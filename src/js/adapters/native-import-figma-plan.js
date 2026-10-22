/* Figma frame and typography preparation. Pure metadata only; the caller supplies
 * readonly catalog, family lookup and font parser ports. No document or cache writer. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportFigmaPlan=api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  // ---- verbatim baseline pure functions (figma-import.js) ----
  function matIdentity() { return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }; }
  function matMultiply(m1, m2) {
    return {
      a: m1.a * m2.a + m1.c * m2.b, b: m1.b * m2.a + m1.d * m2.b, c: m1.a * m2.c + m1.c * m2.d, d: m1.b * m2.c + m1.d * m2.d,
      e: m1.a * m2.e + m1.c * m2.f + m1.e, f: m1.b * m2.e + m1.d * m2.f + m1.f,
    };
  }
  function matInvert(m) {
    const det = m.a * m.d - m.b * m.c;
    if (!det) return matIdentity();
    return { a: m.d / det, b: -m.b / det, c: -m.c / det, d: m.a / det, e: (m.c * m.f - m.d * m.e) / det, f: (m.b * m.e - m.a * m.f) / det };
  }
  function nodeAbsMatrix(node) {
    const t = node.absoluteTransform;
    if (t && t.length === 2) return { a: t[0][0], c: t[0][1], e: t[0][2], b: t[1][0], d: t[1][1], f: t[1][2] };
    const bb = node.absoluteBoundingBox;
    return { a: 1, b: 0, c: 0, d: 1, e: bb ? bb.x : 0, f: bb ? bb.y : 0 };
  }
  function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }
  // Fills / strokes arrays are ordered bottom to top: the TOP-most visible SOLID (or IMAGE with a ref) wins.
  function firstSolidPaint(paints) {
    if (!paints) return null;
    for (let i = paints.length - 1; i >= 0; i--) { const p = paints[i]; if (p && p.visible !== false && p.type === 'SOLID') return p; }
    return null;
  }
  function firstImagePaint(paints) {
    if (!paints) return null;
    for (let i = paints.length - 1; i >= 0; i--) { const p = paints[i]; if (p && p.visible !== false && p.type === 'IMAGE' && p.imageRef) return p; }
    return null;
  }
  function paintToCss(paint, nodeOpacity) {
    if (!paint) return null;
    const c = paint.color || { r: 0, g: 0, b: 0, a: 1 };
    const a = clamp01((paint.opacity != null ? paint.opacity : 1) * (nodeOpacity != null ? nodeOpacity : 1) * (c.a != null ? c.a : 1));
    return 'rgba(' + Math.round(clamp01(c.r) * 255) + ',' + Math.round(clamp01(c.g) * 255) + ',' + Math.round(clamp01(c.b) * 255) + ',' + a + ')';
  }
  function geometryToPathTags(geomArr, cssColor, mat) {
    if (!geomArr || !geomArr.length || !cssColor) return [];
    const byRule = {};
    geomArr.forEach((g) => {
      if (!g || !g.path) return;
      const rule = g.windingRule === 'EVENODD' ? 'evenodd' : 'nonzero';
      byRule[rule] = (byRule[rule] || '') + ' ' + g.path;
    });
    const mtx = 'matrix(' + mat.a + ' ' + mat.b + ' ' + mat.c + ' ' + mat.d + ' ' + mat.e + ' ' + mat.f + ')';
    return Object.keys(byRule).map((rule) => '<path transform="' + mtx + '" d="' + byRule[rule].trim() + '" fill-rule="' + rule + '" fill="' + cssColor + '"/>');
  }
  const ALIGN_MAP = { LEFT: 'left', CENTER: 'center', RIGHT: 'right', JUSTIFIED: 'left' };
  // Walks one node and its descendants into FRAME-LOCAL space; geometry leaves stop the recursion, an image fill wins over fillGeometry
  // (and still emits its stroke), TEXT becomes a descriptor, a childless non-GROUP node is reported as skipped.
  function walkNode(node, frameInv, out) {
    if (!node || node.visible === false) return;
    const mat = matMultiply(frameInv, nodeAbsMatrix(node));
    const imagePaint = firstImagePaint(node.fills);
    if (imagePaint) {
      const bb = node.absoluteBoundingBox;
      out.images.push({ imageRef: imagePaint.imageRef, mat, w: bb ? bb.width : 0, h: bb ? bb.height : 0, opacity: node.opacity != null ? node.opacity : 1, name: node.name });
      if (node.strokeGeometry && node.strokeGeometry.length) {
        const imgStrokeCss = paintToCss(firstSolidPaint(node.strokes), node.opacity);
        if (imgStrokeCss) out.svgTags = out.svgTags.concat(geometryToPathTags(node.strokeGeometry, imgStrokeCss, mat));
      }
      return;
    }
    const hasFillGeo = node.fillGeometry && node.fillGeometry.length;
    const hasStrokeGeo = node.strokeGeometry && node.strokeGeometry.length;
    if (hasFillGeo || hasStrokeGeo) {
      if (hasFillGeo) out.svgTags = out.svgTags.concat(geometryToPathTags(node.fillGeometry, paintToCss(firstSolidPaint(node.fills), node.opacity), mat));
      if (hasStrokeGeo) out.svgTags = out.svgTags.concat(geometryToPathTags(node.strokeGeometry, paintToCss(firstSolidPaint(node.strokes), node.opacity), mat));
      if (!hasFillGeo && !hasStrokeGeo) out.skipped.push(node.type + ' (' + node.name + ', no paintable geometry)');
      return;
    }
    if (node.type === 'TEXT') {
      const tbb = node.absoluteBoundingBox;
      out.texts.push({ text: node.characters || '', node, x: mat.e, y: mat.f, w: tbb ? tbb.width : 200, style: node.style || {},
        color: paintToCss(firstSolidPaint(node.fills), node.opacity) || 'rgba(0,0,0,1)' });
      return;
    }
    if (node.children && node.children.length) { node.children.forEach((c) => walkNode(c, frameInv, out)); return; }
    if (node.type !== 'GROUP') out.skipped.push(node.type + ' (' + (node.name || '?') + ')');
  }
  function parseFileKey(urlOrKey) {
    if (!urlOrKey) return null;
    const s = urlOrKey.trim();
    const m = s.match(/figma\.com\/(?:file|design|proto)\/([a-zA-Z0-9]+)/);
    if (m) return m[1];
    if (/^[a-zA-Z0-9]+$/.test(s)) return s;
    return null;
  }
  function parseNodeId(urlOrKey) {
    if (!urlOrKey) return null;
    const m = String(urlOrKey).match(/[?&]node-id=([^&]+)/);
    if (!m) return null;
    let raw = decodeURIComponent(m[1]);
    if (raw.indexOf(':') < 0) raw = raw.replace('-', ':');
    return raw;
  }

  // ---- file JSON -> plan (convertFileJson without any side effect) ----
  const NO_ARTBOARD = 'La sélection Figma ne contient pas d’artboard (frame) — sélectionne un artboard, ou un groupe/section qui en contient, avant de copier le lien';
  function textRequest(t) {
    const size = t.style.fontSize || 24;
    const lhPx = t.style.lineHeightPx;
    return { name: t.node.name, text: t.text, family: t.style.fontFamily, size, color: t.color, align: ALIGN_MAP[t.style.textAlignHorizontal] || 'left',
      width: t.w || null, x: t.x, y: t.y, bold: (t.style.fontWeight || 400) >= 700, italic: !!t.style.italic,
      lineHeightMult: (lhPx && size) ? (lhPx / size) : null, letterSpacing: t.style.letterSpacing || 0 };
  }
  function convert(figmaJson, opts, canvas) {
    opts = opts || {};
    const plan = { frames: [], resize: null, skipped: [] };
    let frames;
    if (opts.scopedNode) {
      const sn = opts.scopedNode;
      frames = sn.type === 'FRAME' ? [sn] : (sn.children || []).filter((n) => n.type === 'FRAME');
      if (!frames.length) { plan.skipped.push(NO_ARTBOARD); return plan; }
    } else {
      const doc = figmaJson && figmaJson.document;
      if (!doc || !doc.children || !doc.children.length) { plan.skipped.push('empty document'); return plan; }
      const page = doc.children[opts.pageIndex || 0] || doc.children[0];
      frames = (page.children || []).filter((n) => n.type === 'FRAME');
      if (opts.frameNames && opts.frameNames.length) frames = frames.filter((f) => opts.frameNames.indexOf(f.name) >= 0);
      if (!frames.length) { plan.skipped.push('no FRAME node found on this page'); return plan; }
    }
    let canvasW = canvas.w, canvasH = canvas.h;
    for (let fi = 0; fi < frames.length; fi++) {
      const frame = frames[fi];
      if (frame.visible === false) continue;
      const frameInv = matInvert(nodeAbsMatrix(frame));
      const out = { svgTags: [], texts: [], images: [], skipped: [] };
      (frame.children || []).forEach((c) => walkNode(c, frameInv, out));
      const bb = frame.absoluteBoundingBox;
      // Same quirk as the baseline: the canvas follows the FIRST entry of the frame list, and only when that frame is visible and sized.
      if (fi === 0 && bb && bb.width > 0 && bb.height > 0) { plan.resize = [Math.round(bb.width), Math.round(bb.height)]; canvasW = plan.resize[0]; canvasH = plan.resize[1]; }
      const svgW = bb ? Math.round(bb.width) : canvasW, svgH = bb ? Math.round(bb.height) : canvasH;
      const images = [], skipped = out.skipped.slice();
      if (opts.imageMap) {
        out.images.forEach((img) => {
          const source = opts.imageMap[img.imageRef];
          if (!source) { skipped.push('IMAGE fill (' + img.name + '): no imageMap entry for ' + img.imageRef); return; }
          images.push({ name: img.name, imageRef: img.imageRef, source, width: Math.max(1, img.w), height: Math.max(1, img.h), x: img.mat.e + img.w / 2, y: img.mat.f + img.h / 2, opacity: img.opacity });
        });
      } else if (out.images.length) skipped.push(out.images.length + ' image fill(s) skipped — pass opts.imageMap (GET /v1/files/:key/images) to import them');
      plan.frames.push({
        index: fi, layerName: frame.name || ('Figma ' + (fi + 1)),
        svgText: out.svgTags.length ? '<svg xmlns="http://www.w3.org/2000/svg" width="' + svgW + '" height="' + svgH + '">' + out.svgTags.join('') + '</svg>' : null,
        texts: out.texts.filter((t) => t.text).map(textRequest), images, skipped,
      });
      plan.skipped = plan.skipped.concat(skipped);
    }
    return plan;
  }

  const MAX_FONT_BYTES = 16 * 1024 * 1024;
  function parsedFont(encoded, decode, parse) {
    if (typeof encoded !== 'string' || !encoded.length || encoded.length > Math.ceil(MAX_FONT_BYTES / 3) * 4) return false;
    try {
      const binary = decode(encoded);
      if (!binary.length || binary.length > MAX_FONT_BYTES) return false;
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      return !!parse(bytes.buffer);
    } catch (_) { return false; }
  }
  // Family discovery uses the existing readonly transport, never the font importer.
  // The batch later resolves/adopts its own bytes through the native owner.
  async function fontSource(tx, lookup, report, readCatalog) {
    const rows = readCatalog && readCatalog();
    if (!Array.isArray(rows)) return null;
    const fonts = Object.fromEntries(rows.map(row => [row.key, row]));
    const family = (tx.family || '').trim(), lower = family.toLowerCase();
    const keys = Object.keys(fonts);
    let key = !family ? 'Roboto-Regular' : keys.find((key) =>
      (fonts[key].label || '').toLowerCase().replace(/ bold$/, '') === lower);
    let discovered = null;
    if (!key) {
      const found = await lookup(family, 400, false);
      if (found === null) return null;
      if (found) { key = 'Google:' + family + '-Regular'; discovered = {kind:'google',family,weight:400,italic:false}; }
      else {
        report.skipped.push('Police "' + family + '" introuvable sur Google Fonts — police de repli utilisée');
        key = keys.find((key) => {
          const label = (fonts[key].label || '').toLowerCase();
          return label.indexOf(lower) === 0 || lower.indexOf(label) === 0;
        }) || 'Roboto-Regular';
      }
    }
    const source = discovered || fonts[key] && fonts[key].source;
    if (!source) return null;
    if (source.kind === 'google') return { key: key.replace(/-(Bold|Regular)$/, '') + (tx.bold ? '-Bold' : '-Regular'),
      source: { kind: 'google', family: source.family, weight: tx.bold ? 700 : 400, italic: false } };
    if (source.kind !== 'bundled') return null;
    const regular = source.fontKey.replace(/-Bold$/, '-Regular'), base = regular.replace(/-Regular$/, '');
    const resolved = tx.bold && Object.prototype.hasOwnProperty.call(fonts, base + '-Bold') ? base + '-Bold' : regular;
    const supplied = fonts[resolved] && fonts[resolved].source;
    return supplied && supplied.kind === 'bundled' ? { key: resolved, source: {kind:'bundled',fontKey:supplied.fontKey} } : null;
  }
  function textBlock(tx, assetId, Refusal) {
    const rgba = /^rgba\((\d+),(\d+),(\d+),([^,]+)\)$/.exec(tx.color);
    if (!rgba) throw new Refusal('text_color', tx.color);
    const hex = (n) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, '0');
    const alpha = Number(rgba[4]);
    if (!Number.isFinite(alpha) || alpha < 0 || alpha > 1) throw new Refusal('text_color', tx.color);
    const color = '#' + hex(rgba[1]) + hex(rgba[2]) + hex(rgba[3]) + (alpha < 1 ? hex(alpha * 255) : '');
    return { text: tx.text, font: { kind: 'asset', assetId }, size: tx.size, color, align: tx.align,
      fixedWidth: tx.width, fixedHeight: null, bold: tx.bold, italic: tx.italic, underline: false, strike: false,
      letterSpacing: tx.letterSpacing, wordSpacing: 0, lineHeightMult: tx.lineHeightMult || 1.25,
      textCase: 'none', anchorTopLeft: { x: tx.x, y: tx.y } };
  }

  return Object.freeze({convert,walkNode,parseFileKey,parseNodeId,textRequest,matIdentity,matMultiply,matInvert,
    nodeAbsMatrix,clamp01,firstSolidPaint,firstImagePaint,paintToCss,geometryToPathTags,NO_ARTBOARD,parsedFont,fontSource,textBlock});
}));
