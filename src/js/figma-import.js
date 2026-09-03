// ---- Figma import (2026-08, feedback #138 — "importer des artboard figma") ----
// Audit outcome + scoped v1: Cyril asked for "un audit" ("il faudrait que tu
// vois comment faire") before a blind full build. Researched Figma's REST
// API (developers.figma.com/docs/rest-api), confirmed live (curl against
// api.figma.com) that unlike GitHub's API this app already proxies through
// a Tauri command / Cloudflare Worker (feedback-bridge.js, CLAUDE.md §6),
// Figma's REST API sends `access-control-allow-origin: *` on BOTH the
// actual GET and its OPTIONS preflight, explicitly allowing the
// `X-Figma-Token` header — a plain browser `fetch()` with a user-pasted
// personal access token works directly, identically on desktop (Tauri
// webview) and the plain web build. No Worker/Rust proxy needed here,
// simpler than the GitHub-feedback precedent this task was pointed at.
// The one real trust-boundary this app DOES enforce is its Tauri CSP
// (src-tauri/tauri.conf.json's `connect-src`) — `https://api.figma.com` was
// added there alongside this file so the desktop build isn't silently
// blocked; the web build has no CSP at all today so needed no change.
//
// Node-tree shape (confirmed via Figma's docs + community references,
// NOT live-tested end to end — no personal access token or real .fig file
// was available in this environment; see the PR description for exactly
// what that leaves unverified):
//   - GET /v1/files/:key?geometry=paths returns `document` -> pages
//     (CANVAS nodes) -> top-level FRAME nodes (artboards) -> arbitrarily
//     nested GROUP/FRAME/COMPONENT/INSTANCE/BOOLEAN_OPERATION/VECTOR/
//     RECTANGLE/ELLIPSE/LINE/REGULAR_POLYGON/STAR/TEXT children.
//   - `?geometry=paths` is REQUIRED to get `fillGeometry`/`strokeGeometry`
//     on each node: arrays of `{path, windingRule}` where `path` is already
//     an SVG path string (Figma's own docs: a subset of SVG path commands,
//     M/L/Q/C/Z) — so Figma vector geometry drops straight into an SVG
//     `<path d="...">` with ZERO reprojection of curve math. This makes
//     svg-import.js (Paper.js's own importSVG, already handling nested
//     transforms + CompoundPath/hole-merging) the natural reuse target
//     instead of writing a second geometry importer from scratch: this
//     file builds an in-memory SVG string per Figma FRAME and hands it to
//     `SMSvgImport.importString`, so every leaf lands as a real editable
//     Nemo Path via the SAME battle-tested insertion path SVG import
//     already uses (world-transform baking, hole slit-merging,
//     ensureStrokeId/tagOwner, saveActiveLayerFrame, CLAUDE.md §1's
//     consumer checklist) — zero new Paper.js item type, zero new
//     `layer.children` consumer to audit.
//   - TEXT nodes are NOT representable as SVG `<text>` in any editable way
//     Nemo understands (svg-import.js's own header notes `<text>` has
//     nowhere to go) — routed instead through `SMVectorText.
//     buildVectorTextGroup`, the SAME real-glyph-outline builder the
//     Typography tool uses, so an imported Figma text block stays
//     genuinely editable in place (its `text`/`vectorFont`/`size`/`color`
//     survive on `data.isTextRoot`, re-editable via the in-place text
//     editor) — NOT baked to dead geometry. This is the "conserver les
//     texts... non destructif" part of the ask, done for real rather than
//     flattening to shapes.
//   - IMAGE fills (a node's `fills[].type === 'IMAGE'`, referencing an
//     `imageRef` hash) need a SECOND request, `GET /v1/files/:key/images`,
//     which returns `{images: {<imageRef>: <S3 URL>}}` — NOT the same
//     endpoint as `/v1/images/:key` (that one renders/exports whole NODES
//     as PNG/SVG, a different feature: rasterizing a node you can't
//     otherwise decompose, not fetching a fill's source bitmap). Fetched
//     as a blob (not linked directly — the Tauri build's `img-src` CSP is
//     `'self' data: blob:`, no external host, and S3's signed-URL host
//     isn't a fixed domain worth allowlisting), then converted to a base64
//     data: URI (FileReader, 2026-08-29 persistence-audit fix — a blob: URL
//     doesn't survive past the current page's lifetime, but this value gets
//     written verbatim into the Raster's persisted data.src on save; the
//     first version of this file used URL.createObjectURL and any project
//     saved with a Figma-imported image would show it broken on the next
//     real reload) and inserted as a plain Raster, same self-contained
//     shape as psd-import-bridge.js's layer.canvas -> Raster step.
//
// Scope of THIS pass, stated honestly (mirrors svg-import.js's own
// "scope, stated honestly" section):
//   DONE, real and reusable independent of the fetch/token UI below:
//     `convertFileJson(figmaJson, opts)` — pure(ish) conversion given an
//     ALREADY-FETCHED Figma file JSON. One new Nemo layer per top-level
//     FRAME, vector geometry through SMSvgImport, text through
//     SMVectorText, image fills through Raster when `opts.imageMap` is
//     supplied. Verified against a hand-built fixture matching the
//     documented schema (see the PR description) — NOT against a live
//     Figma response, since no token was available here.
//   DONE, live network glue, CORS-confirmed but NOT live-tested (no
//   token/file available): `importFromToken(fileKeyOrUrl, token, opts)`.
//   NOT attempted, left for Cyril to decide (see PR description):
//     Auto Layout / constraints (Nemo has no live layout solver — a
//     Figma frame's children are placed at their exact absolute pixel
//     positions, which is the closest honest reading of "non destructif
//     niveau mise en page" without inventing a layout engine), gradient/
//     pattern fills (flattened to their first solid stop, same
//     "documented degrade" svg-import.js already accepts for gradients),
//     component INSTANCE overrides (an INSTANCE is walked like a plain
//     GROUP — its OWN children as returned by the API, not a live link to
//     the master component), effects (shadows/blurs — skipped, counted).
(function () {
  // ---- Affine 2x3 matrix helpers (a,b,c,d,e,f — SVG matrix() convention:
  // x' = a*x + c*y + e, y' = b*x + d*y + f). Hand-rolled rather than
  // reusing Paper.js's Matrix class here: this is the one piece of this
  // file's math that can't be exercised against a live Figma response in
  // this environment, so it's kept in a form that's easy to hand-verify
  // (see the PR description's worked example) instead of depending on
  // Paper.js's own append()/invert() compose-order semantics.
  function matIdentity() { return { a: 1, b: 0, c: 0, d: 1, e: 0, f: 0 }; }
  // m1 ∘ m2 — applying the result to a point equals applying m2 THEN m1.
  function matMultiply(m1, m2) {
    return {
      a: m1.a * m2.a + m1.c * m2.b,
      b: m1.b * m2.a + m1.d * m2.b,
      c: m1.a * m2.c + m1.c * m2.d,
      d: m1.b * m2.c + m1.d * m2.d,
      e: m1.a * m2.e + m1.c * m2.f + m1.e,
      f: m1.b * m2.e + m1.d * m2.f + m1.f,
    };
  }
  function matInvert(m) {
    var det = m.a * m.d - m.b * m.c;
    if (!det) return matIdentity();
    return {
      a: m.d / det, b: -m.b / det, c: -m.c / det, d: m.a / det,
      e: (m.c * m.f - m.d * m.e) / det, f: (m.b * m.e - m.a * m.f) / det,
    };
  }
  // Figma's `absoluteTransform` is documented as the top two rows of a 2D
  // matrix, [[a,c,e],[b,d,f]] — same convention as SVG matrix(). Falls back
  // to a pure translation from `absoluteBoundingBox` when absent (older/
  // partial JSON), which is enough for an unrotated node.
  function nodeAbsMatrix(node) {
    var t = node.absoluteTransform;
    if (t && t.length === 2) return { a: t[0][0], c: t[0][1], e: t[0][2], b: t[1][0], d: t[1][1], f: t[1][2] };
    var bb = node.absoluteBoundingBox;
    return { a: 1, b: 0, c: 0, d: 1, e: bb ? bb.x : 0, f: bb ? bb.y : 0 };
  }

  function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }
  // TOP-most visible SOLID paint in a fills/strokes array — gradients/
  // patterns/images are handled by their own callers (or skipped), matching
  // svg-import.js's "documented degrade, not a silent drop" contract.
  // 2026-08-29 fix (feedback #144, "problème de couleur"): Figma's own
  // Plugin API docs are explicit that `fills`/`strokes` arrays are ordered
  // BOTTOM to TOP — index 0 is the bottom-most paint, later entries render
  // OVER it. A node with two stacked solid fills (a base color + an
  // overlay tweak, a very common Figma pattern) therefore shows whichever
  // one is LAST in the array, not the first — walking from index 0 (as
  // this file originally did) silently picked the bottom, often-invisible
  // fill instead of what the design actually shows. Walking from the END
  // fixes that; a node with only one solid fill (the overwhelming common
  // case) is unaffected either way.
  function firstSolidPaint(paints) {
    if (!paints) return null;
    for (var i = paints.length - 1; i >= 0; i--) {
      var p = paints[i];
      if (p && p.visible !== false && p.type === 'SOLID') return p;
    }
    return null;
  }
  function paintToCss(paint, nodeOpacity) {
    if (!paint) return null;
    var c = paint.color || { r: 0, g: 0, b: 0, a: 1 };
    var a = clamp01((paint.opacity != null ? paint.opacity : 1) * (nodeOpacity != null ? nodeOpacity : 1) * (c.a != null ? c.a : 1));
    return 'rgba(' + Math.round(clamp01(c.r) * 255) + ',' + Math.round(clamp01(c.g) * 255) + ',' + Math.round(clamp01(c.b) * 255) + ',' + a + ')';
  }
  // Same bottom-to-top fix as firstSolidPaint above, for consistency — a
  // node with an image fill stacked under/over another paint should still
  // resolve to whichever is on TOP.
  function firstImagePaint(paints) {
    if (!paints) return null;
    for (var i = paints.length - 1; i >= 0; i--) {
      var p = paints[i];
      if (p && p.visible !== false && p.type === 'IMAGE' && p.imageRef) return p;
    }
    return null;
  }

  // Groups a node's fillGeometry/strokeGeometry path fragments by winding
  // rule (usually all the same rule, but kept correct if Figma ever mixes
  // them) and emits one `<path>` element per group, positioned by `mat`
  // (already frame-local — see walkNode below).
  function geometryToPathTags(geomArr, cssColor, mat) {
    if (!geomArr || !geomArr.length || !cssColor) return [];
    var byRule = {};
    geomArr.forEach(function (g) {
      if (!g || !g.path) return;
      var rule = g.windingRule === 'EVENODD' ? 'evenodd' : 'nonzero';
      byRule[rule] = (byRule[rule] || '') + ' ' + g.path;
    });
    var mtx = 'matrix(' + mat.a + ' ' + mat.b + ' ' + mat.c + ' ' + mat.d + ' ' + mat.e + ' ' + mat.f + ')';
    return Object.keys(byRule).map(function (rule) {
      return '<path transform="' + mtx + '" d="' + byRule[rule].trim() + '" fill-rule="' + rule + '" fill="' + cssColor + '"/>';
    });
  }

  var ALIGN_MAP = { LEFT: 'left', CENTER: 'center', RIGHT: 'right', JUSTIFIED: 'left' };
  // Font matching, in order (2026-08-29, v2 follow-up — Cyril's stated top
  // priority once the token/import loop itself was working): an exact match
  // against a bundled family needs no network; otherwise fetch the family
  // live from Google Fonts via SMVectorText.addGoogleFont (vector-text-
  // bridge.js) — the SAME catalog lookup the Typography panel's "+" field
  // already uses, so a Figma text node keeps its actual family (Poppins,
  // Manrope, Space Grotesk, ...) instead of silently landing on Roboto just
  // because it wasn't one of the 4 bundled TTFs. Works on both desktop
  // (Tauri's fetch_google_font) and the web build (worker/index.js's
  // /api/google-font proxy) — addGoogleFont itself picks the right path.
  // A family Google Fonts doesn't have (typo, or a paid/self-hosted font
  // Figma reports by its real name) degrades to the old loose bundled-name
  // match, then Roboto — reported in `report.skipped`, never a hard failure
  // for the whole import.
  async function resolveFontKey(figmaFamily, report) {
    var fam = (figmaFamily || '').trim();
    if (!fam) return 'Roboto-Regular';
    var famLc = fam.toLowerCase();
    var VF = window.SMVectorText ? window.SMVectorText.VECTOR_FONTS : {};
    var keys = Object.keys(VF);
    for (var i = 0; i < keys.length; i++) {
      var label = (VF[keys[i]].label || '').toLowerCase().replace(/ bold$/, '');
      if (label === famLc) return keys[i].replace(/-Bold$/, '-Regular');
    }
    if (window.SMVectorText && window.SMVectorText.addGoogleFont) {
      try {
        return await window.SMVectorText.addGoogleFont(fam);
      } catch (e) {
        if (report) report.skipped.push('Police "' + fam + '" introuvable sur Google Fonts — police de repli utilisée');
      }
    }
    for (var j = 0; j < keys.length; j++) {
      var label2 = (VF[keys[j]].label || '').toLowerCase();
      if (label2.indexOf(famLc) === 0 || famLc.indexOf(label2) === 0) return keys[j].replace(/-Bold$/, '-Regular');
    }
    return 'Roboto-Regular';
  }

  // Walks one Figma node (and its descendants) collecting SVG path tags,
  // text-node descriptors and image-fill descriptors — all already
  // transformed into FRAME-LOCAL space via `frameInv` (the inverse of the
  // owning frame's own absoluteTransform, computed once by the caller).
  // Geometry leaves (fillGeometry/strokeGeometry present) stop recursion —
  // BOOLEAN_OPERATION nodes carry their OWN resolved geometry, recursing
  // into their children too would double-import the same shape.
  function walkNode(node, frameInv, out) {
    if (!node || node.visible === false) return;
    var mat = matMultiply(frameInv, nodeAbsMatrix(node));
    // An image fill wins over fillGeometry (2026-08-30, feedback #144:
    // "l'import d'image échoue, pas d'image"). This used to require the
    // ABSENCE of fillGeometry — but every fetch in this file asks for
    // `geometry=paths`, and Figma then returns fillGeometry for every node
    // that has a fill AT ALL, image fills included. So the condition was
    // never true in practice and no image was ever imported.
    //
    // Worse, the node did not even land in `skipped`: it fell through to the
    // geometry branch below, where firstSolidPaint() returns null for an
    // image paint, so it emitted nothing and reported nothing. Measured on a
    // synthetic file shaped exactly as geometry=paths returns: 0 images,
    // 0 shapes, and an EMPTY skipped list — the node vanished in silence.
    // Without fillGeometry the same file imported the image correctly, which
    // is what isolates fillGeometry as the cause.
    //
    // A stroke on an image-filled node is still emitted, since the image
    // replaces only the FILL.
    var imagePaint = firstImagePaint(node.fills);
    if (imagePaint) {
      var bb = node.absoluteBoundingBox;
      out.images.push({ imageRef: imagePaint.imageRef, mat: mat, w: bb ? bb.width : 0, h: bb ? bb.height : 0, opacity: node.opacity != null ? node.opacity : 1, name: node.name });
      if (node.strokeGeometry && node.strokeGeometry.length) {
        var imgStrokeCss = paintToCss(firstSolidPaint(node.strokes), node.opacity);
        if (imgStrokeCss) out.svgTags = out.svgTags.concat(geometryToPathTags(node.strokeGeometry, imgStrokeCss, mat));
      }
      return;
    }
    var hasFillGeo = node.fillGeometry && node.fillGeometry.length;
    var hasStrokeGeo = node.strokeGeometry && node.strokeGeometry.length;
    if (hasFillGeo || hasStrokeGeo) {
      if (hasFillGeo) {
        var fillCss = paintToCss(firstSolidPaint(node.fills), node.opacity);
        out.svgTags = out.svgTags.concat(geometryToPathTags(node.fillGeometry, fillCss, mat));
      }
      if (hasStrokeGeo) {
        var strokeCss = paintToCss(firstSolidPaint(node.strokes), node.opacity);
        out.svgTags = out.svgTags.concat(geometryToPathTags(node.strokeGeometry, strokeCss, mat));
      }
      if (!hasFillGeo && !hasStrokeGeo) out.skipped.push(node.type + ' (' + node.name + ', no paintable geometry)');
      return;
    }
    if (node.type === 'TEXT') {
      var tbb = node.absoluteBoundingBox;
      out.texts.push({
        text: node.characters || '', node: node,
        x: mat.e, y: mat.f, w: tbb ? tbb.width : 200,
        style: node.style || {},
        color: paintToCss(firstSolidPaint(node.fills), node.opacity) || 'rgba(0,0,0,1)',
      });
      return;
    }
    if (node.children && node.children.length) {
      node.children.forEach(function (c) { walkNode(c, frameInv, out); });
      return;
    }
    if (node.type !== 'GROUP') out.skipped.push(node.type + ' (' + (node.name || '?') + ')');
  }

  // Fetches one image fill's bytes and returns a self-contained base64
  // data: URI — NOT a blob: URL (kept fixed 2026-08-29, persistence audit):
  // a blob: URL is only valid for the lifetime of the page/tab that created
  // it, but this value gets written verbatim into a Raster's persisted
  // data.src (serR, app.js) the moment the project is saved. A project
  // saved with a Figma-imported image, then reopened in any later session,
  // would try to load that stale blob: reference and show a broken image —
  // the exact "no data loss on reload" contract serR's own header comment
  // promises ("fully self-contained in the project JSON, no external file
  // dependency after import"), silently violated for this one import path.
  // psd-import-bridge.js's equivalent step (layer.canvas.toDataURL) already
  // produces a real data: URI; this mirrors that via FileReader since the
  // source here is an already-fetched Blob, not a canvas. Still avoids
  // pointing a Raster straight at the returned S3 URL (Tauri's img-src CSP
  // doesn't allowlist that host, and it's a signed URL that expires anyway)
  // — the fetch-as-blob step is unchanged, only what happens to the blob is.
  function fetchImageAsDataUrl(url) {
    return fetch(url).then(function (r) {
      if (!r.ok) throw new Error('HTTP ' + r.status);
      return r.blob();
    }).then(function (blob) {
      return new Promise(function (resolve, reject) {
        var reader = new FileReader();
        reader.onload = function () { resolve(reader.result); };
        reader.onerror = function () { reject(reader.error || new Error('FileReader failed')); };
        reader.readAsDataURL(blob);
      });
    });
  }

  // Converts an already-fetched Figma file JSON (GET /v1/files/:key?geometry=paths
  // response body) into real Nemo layers. `opts.imageMap` (optional):
  // {imageRef: sourceUrl} from GET /v1/files/:key/images, needed to bring
  // in image fills — omit it to skip images (counted, not silently lost).
  // `opts.frameNames` (optional array) restricts import to frames whose
  // name is in the list; default imports every top-level FRAME on the
  // first CANVAS (page). `opts.scopedNode` (optional, from a "Copy link to
  // selection" URL — see parseNodeId/importFromToken below): a single
  // already-resolved Figma node (GET /v1/files/:key/nodes response's
  // nodes[id].document) to import INSTEAD of walking the whole file —
  // takes priority over figmaJson/pageIndex/frameNames entirely.
  async function convertFileJson(figmaJson, opts) {
    opts = opts || {};
    var report = { framesImported: 0, shapesImported: 0, textsImported: 0, imagesImported: 0, skipped: [] };
    var frames;
    if (opts.scopedNode) {
      var sn = opts.scopedNode;
      // The selection IS the artboard, or a container (Section/Canvas/
      // Group) holding one or more artboards — never fall back to "every
      // frame in the whole file" here, that's exactly the bug being fixed.
      frames = sn.type === 'FRAME' ? [sn] : (sn.children || []).filter(function (n) { return n.type === 'FRAME'; });
      if (!frames.length) {
        report.skipped.push('La sélection Figma ne contient pas d’artboard (frame) — sélectionne un artboard, ou un groupe/section qui en contient, avant de copier le lien');
        return report;
      }
    } else {
      var doc = figmaJson && figmaJson.document;
      if (!doc || !doc.children || !doc.children.length) { report.skipped.push('empty document'); return report; }
      var page = doc.children[opts.pageIndex || 0] || doc.children[0];
      frames = (page.children || []).filter(function (n) { return n.type === 'FRAME'; });
      if (opts.frameNames && opts.frameNames.length) {
        frames = frames.filter(function (f) { return opts.frameNames.indexOf(f.name) >= 0; });
      }
      if (!frames.length) { report.skipped.push('no FRAME node found on this page'); return report; }
    }

    for (var fi = 0; fi < frames.length; fi++) {
      var frame = frames[fi];
      if (frame.visible === false) continue;
      var frameAbs = nodeAbsMatrix(frame);
      var frameInv = matInvert(frameAbs);
      var out = { svgTags: [], texts: [], images: [], skipped: [] };
      (frame.children || []).forEach(function (c) { walkNode(c, frameInv, out); });

      var idx = createUserLayer(frame.name || ('Figma ' + (fi + 1)));
      activateUL(idx);
      var layer = userLayers[idx];
      var bb = frame.absoluteBoundingBox;
      // Match canvas size to the first imported frame — same "trust the
      // source material's own dimensions" precedent psd-import-bridge.js
      // already set for PSD import, so a Figma artboard lands 1:1, not
      // squeezed/upscaled into whatever size the Nemo doc happened to be.
      if (fi === 0 && bb && bb.width > 0 && bb.height > 0 && window.SM && window.SM.setCanvasSize) {
        window.SM.setCanvasSize(Math.round(bb.width), Math.round(bb.height));
      }

      if (out.svgTags.length) {
        var svgW = bb ? Math.round(bb.width) : state.canvasW;
        var svgH = bb ? Math.round(bb.height) : state.canvasH;
        var svgText = '<svg xmlns="http://www.w3.org/2000/svg" width="' + svgW + '" height="' + svgH + '">' + out.svgTags.join('') + '</svg>';
        // Reuses svg-import.js as-is (see file header) — it targets
        // state.activeLayerIdx, already pointed at this frame's new layer
        // above, and handles its own pushUndo/ensureKeyframe/save/render.
        // {noFit:true} (added alongside this file, svg-import.js): our
        // paths already carry exact frame-relative absolute coordinates —
        // svg-import's own shrink+recenter step (built for a dropped-in
        // icon of unknown size) would otherwise shift the whole imported
        // group to the canvas center, destroying the original Figma
        // layout. Caught by actually running a synthetic fixture through
        // this file in a live browser (see PR description) — reading the
        // code alone would have missed it.
        var n = window.SMSvgImport ? window.SMSvgImport.importString(svgText, { noFit: true }) : 0;
        report.shapesImported += n || 0;
      }

      for (var ti = 0; ti < out.texts.length; ti++) {
        var t = out.texts[ti];
        if (!t.text) continue;
        var fontKey = await resolveFontKey(t.style.fontFamily, report);
        var size = t.style.fontSize || 24;
        var align = ALIGN_MAP[t.style.textAlignHorizontal] || 'left';
        try {
          // The text box's WIDTH, its line height and its letter spacing all
          // reached this point and were then dropped (2026-08-30, feedback
          // #144: "mise en page, calage de texte exact par rapport au
          // layout"). t.w was captured by walkNode and never passed; the
          // style's lineHeightPx and letterSpacing were never read at all, so
          // every imported block fell back to the 1.25 default line height and
          // zero tracking regardless of what the Figma file said.
          //
          // Passing the width also makes alignment mean what it means in
          // Figma — see buildVectorTextGroup's own note: without it, centring
          // happened inside the widest LINE, which for a single line is a
          // no-op, so a centred Figma label came in flush left.
          //
          // lineHeightPx is Figma's resolved value in px whatever unit the
          // designer picked (AUTO/PIXELS/PERCENT), so dividing by fontSize is
          // the multiplier this builder wants. Guarded because AUTO can report
          // 0 on some nodes, and a 0 multiplier would stack every line on one.
          var lhPx = t.style.lineHeightPx;
          var lhMult = (lhPx && size) ? (lhPx / size) : undefined;
          await window.SMVectorText.buildVectorTextGroup(
            t.text, fontKey, size, t.color, align, t.w || null,
            { x: t.x, y: t.y }, layer,
            {
              bold: (t.style.fontWeight || 400) >= 700,
              italic: !!t.style.italic,
              lineHeightMult: lhMult,
              letterSpacing: t.style.letterSpacing || 0,
            }
          );
          report.textsImported++;
        } catch (e) {
          out.skipped.push('TEXT (' + (t.node.name || '?') + '): ' + (e && e.message));
        }
      }

      if (opts.imageMap) {
        for (var ii = 0; ii < out.images.length; ii++) {
          var img = out.images[ii];
          var srcUrl = opts.imageMap[img.imageRef];
          if (!srcUrl) { out.skipped.push('IMAGE fill (' + img.name + '): no imageMap entry for ' + img.imageRef); continue; }
          try {
            var dataUrl = await fetchImageAsDataUrl(srcUrl);
            var prevActive = project.activeLayer; layer.activate();
            /* eslint-disable no-loop-func */
            await new Promise(function (resolve) {
              var r = new Raster(dataUrl);
              r.onLoad = function () {
                r.size = new Size(Math.max(1, img.w), Math.max(1, img.h));
                r.position = new Point(img.mat.e + img.w / 2, img.mat.f + img.h / 2);
                r.opacity = img.opacity;
                r.data.src = dataUrl;
                resolve();
              };
              r.onError = function () { resolve(); };
            });
            prevActive.activate();
            report.imagesImported++;
          } catch (e) {
            out.skipped.push('IMAGE fill (' + img.name + '): ' + (e && e.message));
          }
        }
      } else if (out.images.length) {
        out.skipped.push(out.images.length + ' image fill(s) skipped — pass opts.imageMap (GET /v1/files/:key/images) to import them');
      }

      saveActiveLayerFrame(); updateUI();
      if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
      report.framesImported++;
      report.skipped = report.skipped.concat(out.skipped);
    }
    return report;
  }

  // Accepts either a bare file key or any Figma file/design URL
  // (figma.com/file/:key/... or figma.com/design/:key/...).
  function parseFileKey(urlOrKey) {
    if (!urlOrKey) return null;
    var s = urlOrKey.trim();
    var m = s.match(/figma\.com\/(?:file|design|proto)\/([a-zA-Z0-9]+)/);
    if (m) return m[1];
    if (/^[a-zA-Z0-9]+$/.test(s)) return s;
    return null;
  }
  // Extracts `?node-id=...` from a "Copy link to selection" URL (feedback
  // #144: "quand j'importe via link of selection il import tous les
  // artboard" — the old code only ever read the file key and always fetched
  // the WHOLE file, silently ignoring any node-id, so a selection link
  // behaved exactly like a plain file link). Figma's shareable URL encodes
  // the node id's ':' separator as '-' (a real id "79:421" becomes
  // "79-421" in the query string) — only substitute the first '-' when the
  // value doesn't already contain a literal ':' (e.g. someone pasting a
  // bare id copied from elsewhere).
  function parseNodeId(urlOrKey) {
    if (!urlOrKey) return null;
    var m = String(urlOrKey).match(/[?&]node-id=([^&]+)/);
    if (!m) return null;
    var raw = decodeURIComponent(m[1]);
    if (raw.indexOf(':') < 0) raw = raw.replace('-', ':');
    return raw;
  }

  // Live network glue (CORS-confirmed, NOT live-tested — see file header).
  // `token` is the user's own personal access token (Settings -> Figma),
  // sent ONLY to api.figma.com, never persisted anywhere but this
  // machine's localStorage (same trust model as the GitHub feedback
  // token — see feedback-bridge.js / CLAUDE.md §6).
  async function importFromToken(fileKeyOrUrl, token, opts) {
    var key = parseFileKey(fileKeyOrUrl);
    if (!key) throw new Error('URL ou clé de fichier Figma invalide');
    if (!token) throw new Error('Token Figma manquant (Réglages > Figma)');
    var headers = { 'X-Figma-Token': token };
    var mergedOpts = Object.assign({}, opts);
    var figmaJson;
    var nodeId = parseNodeId(fileKeyOrUrl);
    if (nodeId) {
      // "Copy link to selection" (feedback #144) — fetch ONLY that node via
      // the /nodes endpoint instead of the whole file via /files, so the
      // import is scoped to the selection instead of every top-level frame.
      var nodeRes = await fetch('https://api.figma.com/v1/files/' + key + '/nodes?ids=' + encodeURIComponent(nodeId) + '&geometry=paths', { headers: headers });
      if (!nodeRes.ok) throw new Error('Figma API: HTTP ' + nodeRes.status + (nodeRes.status === 403 ? ' (token invalide ou sans accès à ce fichier)' : ''));
      var nodeJson = await nodeRes.json();
      var entry = nodeJson.nodes && nodeJson.nodes[nodeId];
      if (!entry || !entry.document) throw new Error('Nœud Figma introuvable (node-id ' + nodeId + ') — le lien de sélection ne correspond plus à un élément de ce fichier');
      mergedOpts.scopedNode = entry.document;
    } else {
      var fileRes = await fetch('https://api.figma.com/v1/files/' + key + '?geometry=paths', { headers: headers });
      if (!fileRes.ok) throw new Error('Figma API: HTTP ' + fileRes.status + (fileRes.status === 403 ? ' (token invalide ou sans accès à ce fichier)' : ''));
      figmaJson = await fileRes.json();
    }
    try {
      var imgRes = await fetch('https://api.figma.com/v1/files/' + key + '/images', { headers: headers });
      if (imgRes.ok) {
        var imgJson = await imgRes.json();
        mergedOpts.imageMap = imgJson.images || {};
      }
    } catch (e) { /* image fills degrade to "skipped", not a hard failure */ }
    return convertFileJson(figmaJson, mergedOpts);
  }

  window.SMFigmaImport = {
    convertFileJson: convertFileJson,
    importFromToken: importFromToken,
    parseFileKey: parseFileKey,
    parseNodeId: parseNodeId,
    // exposed for the fixture-based sanity check (see PR description) —
    // not part of the "public" surface other modules should call.
    _internal: { matMultiply: matMultiply, matInvert: matInvert, nodeAbsMatrix: nodeAbsMatrix, geometryToPathTags: geometryToPathTags },
  };

  // ---- Settings UI wiring (Réglages > Figma) ----
  var TOKEN_KEY = 'sm-figma-token';
  function init() {
    var tokenInput = document.getElementById('figma-token');
    var tokenSave = document.getElementById('figma-token-save');
    var urlInput = document.getElementById('figma-url');
    var importBtn = document.getElementById('figma-import-btn');
    if (!tokenInput || !importBtn) return;
    try { tokenInput.value = localStorage.getItem(TOKEN_KEY) || ''; } catch (e) {}
    if (tokenSave) tokenSave.addEventListener('click', function () {
      try { localStorage.setItem(TOKEN_KEY, tokenInput.value || ''); } catch (e) {}
      if (window.showToast) showToast('Token Figma enregistré');
    });
    importBtn.addEventListener('click', function () {
      var token = (function () { try { return localStorage.getItem(TOKEN_KEY) || tokenInput.value; } catch (e) { return tokenInput.value; } })();
      var url = urlInput ? urlInput.value : '';
      if (!token) { if (window.showToast) showToast('Colle ton token Figma personnel d\'abord'); return; }
      if (!url) { if (window.showToast) showToast(SM.t('hsFigmaPaste')); return; }
      if (window.showToast) showToast('Import Figma…');
      importFromToken(url, token).then(function (report) {
        var msg = report.framesImported + ' frame(s), ' + report.shapesImported + ' forme(s), ' + report.textsImported + ' texte(s), ' + report.imagesImported + ' image(s) importé(s)';
        if (report.skipped.length) msg += ' — ' + report.skipped.length + ' élément(s) ignoré(s) (voir console)';
        if (window.showToast) showToast(msg);
        if (report.skipped.length && window.console) console.warn('[figma-import] skipped:', report.skipped);
      }).catch(function (err) {
        console.error('[figma-import] failed', err);
        if (window.showToast) showToast('Import Figma échoué : ' + (err && err.message ? err.message : 'erreur inconnue'));
      });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
