// ---- IMAGE MESH — deform an imported image through an editable mesh ----
//
// Cyril's ask (2026-08-30): deform and animate an imported image via an
// editable mesh, with a masking system, BOTH living on the image layer
// itself rather than as separate layers — and explicitly, "the mask outline
// is also the mesh boundary": the mesh is triangulated INSIDE the mask.
// That single confirmed rule is what makes this one feature instead of two:
// there is no separate image-mask code path anywhere, because the outline
// IS the silhouette the engine clips to (see engine.rs's
// paint_layer_items -> mesh branch: one outer push_layer on the DEFORMED
// outline, per-triangle clips inside it).
//
// ---- WHY THE STORE IS PROJECT-LEVEL AND NOT ON THE STROKE ----
//
// A still image's stroke dict is written into EVERY frame of its layer as a
// separate object literal (images.js's importStandalone loop — see its own
// comment: saveAllLayerFrames only ever re-serializes the CURRENT frame, so
// every other frame keeps verbatim whatever that loop wrote). Hanging the
// mesh (outline + rest vertices + triangle indices) off the stroke dict
// would therefore duplicate the whole topology once per frame: a 120-frame
// still with a 15x15 mesh would carry 120 copies of ~450 numbers. Measured
// on a 120-frame layer, mesh-on-dict vs mesh-in-store: 903 KB vs 8.7 KB of
// project JSON for the SAME single mesh.
//
// So topology lives ONCE in `state.imageMeshes[meshId]` — same category and
// same persistence slot as state.symbols/state.trackRoles (project-level
// registry keyed by a stable id, exported/imported in timeline.js) — and the
// stroke dict carries only `meshId`, a ~20-byte string. That also gives the
// semantics a still image actually wants: one mesh for the image across the
// whole layer, not an independent mesh per frame.
//
// ---- SPACE ----
//
// Everything in a mesh is in NORMALIZED space over the raster's own display
// rect: (0,0) = top-left corner, (1,1) = bottom-right, BEFORE any Motion /
// parenting / 3D transform. Deliberately not world coordinates:
//   - moving, scaling or rotating the image must carry the deformation with
//     it rather than leaving it behind,
//   - engine-bridge.js already resolves the whole Motion chain down to ONE
//     final rect (x/y/width/height/rotation) per image item, so mapping
//     normalized -> world at scene-build time reuses that resolved rect
//     instead of re-deriving the transform chain a second time (CLAUDE.md
//     §3's "two copies of the same math drift silently" rule).
//
// `verts` holds REST positions. `offsets` holds the static per-vertex
// deformation (same normalized units). Deformed = rest + offset. Animation
// (PR3) layers keyframed per-vertex offsets on top of this static pose,
// exactly like motion.js's existing applyPathVertexOffsets does for a Path's
// segments — the reason `offsets` is a separate array rather than baked into
// `verts` is precisely so a keyed offset can replace it without destroying
// the rest pose the mesh was triangulated from.
//
// INVARIANT worth keeping: verts[0 .. outline.length-1] ARE the outline
// points, in order. Interior points follow. So an outline vertex is
// addressable by its own index in both arrays, and PR2/PR3 can key a vertex
// by a stable integer without a second lookup table.
(function () {
  'use strict';

  var NS = {};

  // ---- store ----------------------------------------------------------
  function store() {
    if (!state.imageMeshes) state.imageMeshes = {};
    return state.imageMeshes;
  }
  var _seq = 0;
  function newId() {
    var s = store(), id;
    do { _seq++; id = 'im_' + _seq; } while (s[id]);
    return id;
  }
  function get(meshId) {
    if (!meshId) return null;
    var m = store()[meshId];
    return m || null;
  }

  // ---- geometry helpers -----------------------------------------------

  // Standard crossing-number test. Points exactly ON an edge are "inside or
  // out" arbitrarily, which is fine for the only two callers: filtering
  // Delaunay triangles by their CENTROID (never on an edge for a
  // non-degenerate triangle) and dropping grid points that land outside a
  // custom outline.
  function pointInPoly(poly, x, y) {
    var inside = false;
    for (var i = 0, j = poly.length - 1; i < poly.length; j = i++) {
      var xi = poly[i][0], yi = poly[i][1], xj = poly[j][0], yj = poly[j][1];
      if (((yi > y) !== (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi)) inside = !inside;
    }
    return inside;
  }

  // Signed area x2 of a polygon (positive = counter-clockwise in a y-down
  // screen space it reads as clockwise; only its SIGN is used, to normalize
  // outline winding so the engine's non-zero fill of the outline path never
  // depends on which direction the user happened to draw it).
  function signedArea2(poly) {
    var a = 0;
    for (var i = 0, j = poly.length - 1; i < poly.length; j = i++) {
      a += (poly[j][0] * poly[i][1]) - (poly[i][0] * poly[j][1]);
    }
    return a;
  }

  // ---- triangulation --------------------------------------------------
  //
  // Delaunator (vendored, ISC — see delaunator.vendor.js) over the outline
  // points PLUS the interior grid points, then every triangle whose centroid
  // falls outside the outline is dropped. Delaunay of a point set is the
  // CONVEX HULL's triangulation, so for a convex outline (including the
  // default rectangle) the filter removes nothing and the result covers the
  // outline exactly. For a concave outline the filter carves the hull back
  // to the shape.
  //
  // KNOWN v1 LIMIT, stated rather than hidden: this is unconstrained
  // Delaunay, so a deeply concave outline (a thin spiral, a narrow fjord)
  // can lose a sliver of coverage near the concavity where a triangle
  // straddles the boundary — its centroid is outside so it is dropped, and
  // nothing replaces it. Mitigated by densifying long outline edges below
  // (more boundary points = the hull hugs the outline more tightly), which
  // is enough for the shapes this is actually for (a character part, a
  // cut-out prop). A constrained triangulation would remove the limit
  // entirely and is the natural v2 if a real outline ever hits it.
  function triangulate(pts, outlineCount) {
    var out = [];
    if (!window.Delaunator || pts.length < 3) return out;
    var d;
    try { d = window.Delaunator.from(pts, function (p) { return p[0]; }, function (p) { return p[1]; }); }
    catch (e) { return out; }
    var poly = pts.slice(0, outlineCount);
    var t = d.triangles;
    for (var k = 0; k < t.length; k += 3) {
      var a = t[k], b = t[k + 1], c = t[k + 2];
      var cx = (pts[a][0] + pts[b][0] + pts[c][0]) / 3;
      var cy = (pts[a][1] + pts[b][1] + pts[c][1]) / 3;
      if (!pointInPoly(poly, cx, cy)) continue;
      out.push(a, b, c);
    }
    return out;
  }

  // Insert extra points along outline edges longer than `maxLen` so the
  // Delaunay hull follows the outline closely (see triangulate's own note)
  // and so a deformed boundary bends smoothly instead of staying a long
  // straight chord between two far-apart control points.
  function densify(poly, maxLen) {
    var out = [];
    for (var i = 0; i < poly.length; i++) {
      var p = poly[i], q = poly[(i + 1) % poly.length];
      out.push([p[0], p[1]]);
      var dx = q[0] - p[0], dy = q[1] - p[1];
      var len = Math.sqrt(dx * dx + dy * dy);
      var n = Math.floor(len / maxLen);
      for (var s = 1; s <= n; s++) {
        var t = s / (n + 1);
        out.push([p[0] + dx * t, p[1] + dy * t]);
      }
    }
    return out;
  }

  // Rebuilds `verts`/`tris` from `mesh.outline` + the requested grid
  // density, preserving nothing: this is the "topology changed" path
  // (outline edited, density changed), so any existing per-vertex offsets
  // are reset — vertex INDICES are not stable across a retopology and
  // silently carrying old offsets onto new vertices would scramble the pose.
  function rebuild(mesh) {
    var cols = Math.max(0, Math.min(64, mesh.cols | 0));
    var rows = Math.max(0, Math.min(64, mesh.rows | 0));
    // Outline first (index invariant, see the header comment).
    var outline = mesh.outline.map(function (p) { return [p[0], p[1]]; });
    var pts = outline.slice();
    // Interior grid: cell CENTRES of a (cols+1)x(rows+1) grid would sit on
    // the boundary for a full-rect outline and duplicate the outline points,
    // so interior samples are strictly inside (1..cols-1 over cols).
    for (var gy = 1; gy < rows; gy++) {
      for (var gx = 1; gx < cols; gx++) {
        var u = gx / cols, v = gy / rows;
        if (pointInPoly(outline, u, v)) pts.push([u, v]);
      }
    }
    mesh.verts = pts;
    mesh.tris = triangulate(pts, outline.length);
    mesh.offsets = pts.map(function () { return [0, 0]; });
    return mesh;
  }

  // A brand-new mesh over the whole image: rectangular outline (so the mask
  // starts as "the whole picture", i.e. creating a mesh changes nothing
  // visible until a vertex is actually moved) + a cols x rows grid.
  function createMesh(opts) {
    opts = opts || {};
    var cols = Math.max(1, Math.min(64, opts.cols || 4));
    var rows = Math.max(1, Math.min(64, opts.rows || 4));
    // The rect outline is densified to cols/rows steps per side so boundary
    // vertices line up with the interior grid — otherwise every boundary
    // triangle would be a long thin wedge reaching from a corner.
    var outline = [];
    var x, y;
    for (x = 0; x < cols; x++) outline.push([x / cols, 0]);
    for (y = 0; y < rows; y++) outline.push([1, y / rows]);
    for (x = cols; x > 0; x--) outline.push([x / cols, 1]);
    for (y = rows; y > 0; y--) outline.push([0, y / rows]);
    var mesh = { outline: outline, cols: cols, rows: rows, verts: [], tris: [], offsets: [] };
    return rebuild(mesh);
  }

  // Replaces the outline with an arbitrary closed polygon in normalized
  // space and retriangulates inside it. Winding is normalized so the
  // engine's non-zero fill of the outline never depends on draw direction.
  function setOutline(meshId, poly, opts) {
    var mesh = get(meshId);
    if (!mesh || !poly || poly.length < 3) return null;
    var p = poly.map(function (q) { return [q[0], q[1]]; });
    if (signedArea2(p) < 0) p.reverse();
    // Density FIRST: densify() below sizes its step off cols/rows, so
    // applying the new density afterwards would densify the outline at the
    // OLD grid step and leave boundary and interior samples at mismatched
    // spacings (caught while checking a vertex count that didn't add up).
    if (opts && opts.cols) mesh.cols = opts.cols;
    if (opts && opts.rows) mesh.rows = opts.rows;
    // Densify to roughly the grid step so a hand-drawn outline gets enough
    // boundary samples for the hull to hug it (see triangulate's note).
    var step = 1 / Math.max(mesh.cols || 4, mesh.rows || 4);
    mesh.outline = densify(p, step);
    return rebuild(mesh);
  }

  // ---- attach / detach on a raster ------------------------------------
  //
  // The tag is `data.meshId` on the live Paper.js Raster and `meshId` on its
  // serialized stroke dict — CLAUDE.md §1's "new data.* tag must be handled
  // or excluded in EVERY layer.children consumer" applies, and the audited
  // list is: serR/desR (app.js, round-trip), buildSceneJson + onionLayerItems
  // + buildGhostAllItems (engine-bridge.js, render), export.js (goes through
  // the same buildSceneJson), select-bridge/tools (a meshed raster is still
  // an ordinary selectable/movable raster — nothing to change, the mesh is
  // normalized to its rect so it follows for free), tweens.js (rasters never
  // enter tween matching at all).
  // Writes `meshId` onto the matching raster dict in EVERY frame of the
  // layer, not just the frame being looked at.
  //
  // Found live while measuring the JSON impact, and it is the whole reason
  // this helper exists: a still image's dict is written once per frame by
  // the importer, and saveActiveLayerFrame only ever re-serializes the
  // CURRENT frame — so tagging the live Paper.js Raster and saving tagged
  // exactly ONE frame. Scrubbing one frame away showed the image
  // undeformed and unmasked again, with no error anywhere. Exactly the
  // CLAUDE.md §1 shape: the tag was handled where it was introduced and
  // nowhere else.
  //
  // Matching rule: same INDEX in that frame's strokes array, and still an
  // isRaster entry. Exact for a footage layer (importStandalone/
  // importSequence/video all write exactly one raster per frame), and
  // best-effort on a hand-built layer that mixes rasters and paths — a
  // mismatch there leaves a frame untagged (image renders plain), never
  // tags the wrong item.
  function propagate(raster, meshId) {
    if (typeof userLayers === 'undefined' || !raster || !raster.parent) return 0;
    var li = userLayers.indexOf(raster.parent);
    if (li < 0 || !state.layers[li]) return 0;
    var si = raster.parent.children.indexOf(raster);
    if (si < 0) return 0;
    var frames = state.layers[li].frames || [], n = 0;
    for (var f = 0; f < frames.length; f++) {
      var strokes = frames[f] && frames[f].strokes;
      var st = strokes && strokes[si];
      if (!st || !st.isRaster) continue;
      if (meshId) st.meshId = meshId; else delete st.meshId;
      n++;
    }
    return n;
  }

  function attach(raster, opts) {
    if (!raster) return null;
    var id = newId();
    store()[id] = createMesh(opts);
    raster.data = raster.data || {};
    raster.data.meshId = id;
    propagate(raster, id);
    return id;
  }
  // Every layer whose frames could reference a mesh: the scene's own, plus
  // each Component's (state.symbols[id].layers is the same shape — see
  // app.js's enterSymbol, which swaps one for the other).
  function allLayerSets() {
    var sets = [state.layers || []];
    var syms = state.symbols || {};
    for (var k in syms) {
      if (Object.prototype.hasOwnProperty.call(syms, k) && syms[k] && syms[k].layers) sets.push(syms[k].layers);
    }
    return sets;
  }
  // Is this mesh still referenced by any stored stroke anywhere?
  function isReferenced(meshId) {
    var sets = allLayerSets();
    for (var s = 0; s < sets.length; s++) {
      var layers = sets[s];
      for (var l = 0; l < layers.length; l++) {
        var frames = layers[l] && layers[l].frames;
        if (!frames) continue;
        for (var f = 0; f < frames.length; f++) {
          var strokes = frames[f] && frames[f].strokes;
          if (!strokes) continue;
          for (var i = 0; i < strokes.length; i++) {
            if (strokes[i] && strokes[i].meshId === meshId) return true;
          }
        }
      }
    }
    return false;
  }
  // Drops a mesh from the store once nothing points at it any more.
  //
  // Without this, turning Mesh off in the panel left the whole topology
  // behind: invisible, but persisted in every save from then on AND enough
  // to keep exportHasImageMesh (export.js) true, quietly forcing every
  // export down the engine path. Found by watching the store grow to
  // ['im_1','im_2'] across one on/off/on cycle.
  //
  // Reference-counted rather than an unconditional delete: the store is
  // keyed by id, so if a future feature ever lets two images share one mesh
  // (a duplicate that keeps its source's), deleting on the first detach
  // would silently blank the other one.
  function releaseIfUnused(meshId) {
    if (!meshId) return false;
    if (isReferenced(meshId)) return false;
    delete store()[meshId];
    return true;
  }
  function detach(raster) {
    if (!raster || !raster.data) return;
    var id = raster.data.meshId;
    propagate(raster, null);
    delete raster.data.meshId;
    releaseIfUnused(id);
  }

  // ---- scene payload ---------------------------------------------------

  // Maps normalized (u,v) through the FINAL resolved image rect — the exact
  // same placement math engine.rs applies to a plain image item
  // (translate(x,y) * scale, then rotate about the rect centre), so a mesh
  // whose vertices are all at rest lands pixel-for-pixel where the
  // undeformed image would.
  function worldOf(rect, u, v, cos, sin, cx, cy) {
    var lx = (u - 0.5) * rect.width, ly = (v - 0.5) * rect.height;
    return [cx + lx * cos - ly * sin, cy + lx * sin + ly * cos];
  }

  // Inverse of worldOf — a world point back into the rect's normalized
  // space. Used by the editor to turn a pointer position into a vertex
  // offset. Kept next to its forward twin on purpose: these two are the
  // only place the rect mapping is written, and a drift between them would
  // show up as handles that don't follow the cursor (CLAUDE.md §3).
  function normalizedOf(rect, wx, wy) {
    var rad = (rect.rotation || 0) * Math.PI / 180;
    var cos = Math.cos(rad), sin = Math.sin(rad);
    var cx = rect.x + rect.width / 2, cy = rect.y + rect.height / 2;
    var dx = wx - cx, dy = wy - cy;
    var lx = dx * cos + dy * sin;
    var ly = -dx * sin + dy * cos;
    return [lx / (rect.width || 1) + 0.5, ly / (rect.height || 1) + 0.5];
  }

  // Deformed vertex positions in WORLD space for a raster's own rect — the
  // editor's overlay and hit-testing both read this, so the handles sit
  // exactly on the geometry the engine draws. `rect` comes from the caller
  // (engine-bridge's rasterImageRect) rather than being recomputed here, so
  // there is one definition of "an image's display rect" in the app.
  function worldVerts(mesh, rect, poseAt) {
    if (!mesh) return null;
    var rad = (rect.rotation || 0) * Math.PI / 180;
    var cos = Math.cos(rad), sin = Math.sin(rad);
    var cx = rect.x + rect.width / 2, cy = rect.y + rect.height / 2;
    var out = new Array(mesh.verts.length);
    for (var i = 0; i < mesh.verts.length; i++) {
      var rest = mesh.verts[i];
      var off = (mesh.offsets && mesh.offsets[i]) || [0, 0];
      var du = off[0], dv = off[1];
      if (poseAt) { var a = poseAt(i); if (a) { du += a[0]; dv += a[1]; } }
      out[i] = worldOf(rect, rest[0] + du, rest[1] + dv, cos, sin, cx, cy);
    }
    return out;
  }

  var R2 = function (n) { return Math.round(n * 100) / 100; };   // scene JSON rounds to 2dp (CLAUDE.md §5.4)
  var R5 = function (n) { return Math.round(n * 100000) / 100000; };

  // Builds the `mesh` payload for an image scene item, or null when this
  // raster has no mesh (the overwhelmingly common case — one property read
  // and out, so an ordinary imported image pays nothing).
  //
  // `rect` MUST be the final post-Motion/post-3D rect the caller is about to
  // put in the item, not the raw raster bounds: the mesh has to ride the
  // same transform chain as the image it deforms.
  //
  // `poseAt` (optional) is a function(vertexIndex) -> [du,dv] returning the
  // ANIMATED offset for this frame, layered on top of the mesh's own static
  // offsets. PR3 (Motion vertex keyframes) supplies it; PR1 passes nothing.
  function scenePayload(raster, rect, poseAt) {
    var meshId = raster && raster.data && raster.data.meshId;
    if (!meshId) return null;
    var mesh = get(meshId);
    if (!mesh || !mesh.tris || !mesh.tris.length) return null;
    var rad = (rect.rotation || 0) * Math.PI / 180;
    var cos = Math.cos(rad), sin = Math.sin(rad);
    var cx = rect.x + rect.width / 2, cy = rect.y + rect.height / 2;
    var verts = new Array(mesh.verts.length);
    var uvs = new Array(mesh.verts.length);
    for (var i = 0; i < mesh.verts.length; i++) {
      var rest = mesh.verts[i];
      var off = (mesh.offsets && mesh.offsets[i]) || [0, 0];
      var du = off[0], dv = off[1];
      if (poseAt) { var a = poseAt(i); if (a) { du += a[0]; dv += a[1]; } }
      var w = worldOf(rect, rest[0] + du, rest[1] + dv, cos, sin, cx, cy);
      verts[i] = [R2(w[0]), R2(w[1])];
      // Source UV is the REST position: that is where these pixels come
      // from in the image, which is the whole point — the offset moves
      // where they LAND, never where they are sampled.
      uvs[i] = [R5(rest[0]), R5(rest[1])];
    }
    var outline = [];
    for (var k = 0; k < mesh.outline.length; k++) outline.push(k); // index invariant: outline is verts[0..n-1]
    return { verts: verts, uvs: uvs, tris: mesh.tris.slice(), outline: outline };
  }

  // ---- persistence ------------------------------------------------------
  //
  // Serialized wholesale (like state.symbols / ld.duplicator) — a mesh is
  // plain numbers by construction: no Paper.js item, no live reference, so
  // none of CLAUDE.md §1's "a live object reference never survives a
  // save/reload" hazard applies here. `tris` is written as a plain array
  // because Delaunator hands back a typed array, which JSON.stringify would
  // otherwise turn into an OBJECT ({"0":1,...}) — silently unusable on
  // reload, and exactly the kind of round-trip break that only shows up
  // after a save.
  function serialize() {
    var s = store(), out = {};
    for (var id in s) {
      if (!Object.prototype.hasOwnProperty.call(s, id)) continue;
      var m = s[id];
      out[id] = {
        outline: m.outline.map(function (p) { return [p[0], p[1]]; }),
        verts: m.verts.map(function (p) { return [p[0], p[1]]; }),
        tris: Array.prototype.slice.call(m.tris),
        offsets: (m.offsets || []).map(function (p) { return [p[0], p[1]]; }),
        cols: m.cols, rows: m.rows,
      };
    }
    return out;
  }
  function load(data) {
    var s = {};
    if (data && typeof data === 'object') {
      for (var id in data) {
        if (!Object.prototype.hasOwnProperty.call(data, id)) continue;
        var m = data[id] || {};
        s[id] = {
          outline: m.outline || [],
          verts: m.verts || [],
          tris: m.tris || [],
          offsets: m.offsets || (m.verts || []).map(function () { return [0, 0]; }),
          cols: m.cols || 4, rows: m.rows || 4,
        };
        // Keep newId() from colliding with an imported project's ids.
        var n = parseInt(String(id).replace(/^im_/, ''), 10);
        if (n > _seq) _seq = n;
      }
    }
    state.imageMeshes = s;
  }

  NS.get = get;
  NS.attach = attach;
  NS.detach = detach;
  NS.propagate = propagate;
  NS.isReferenced = isReferenced;
  NS.releaseIfUnused = releaseIfUnused;
  NS.createMesh = createMesh;
  NS.setOutline = setOutline;
  NS.rebuild = function (meshId) { var m = get(meshId); return m ? rebuild(m) : null; };
  NS.scenePayload = scenePayload;
  NS.worldVerts = worldVerts;
  NS.normalizedOf = normalizedOf;
  NS.serialize = serialize;
  NS.load = load;
  NS.pointInPoly = pointInPoly;
  // True when a vertex is part of the mask outline (verts[0..outline-1] —
  // see the index invariant in this file's header). Dragging one of these
  // reshapes the MASK as well as the deformation, which the editor says out
  // loud by drawing them differently.
  NS.isOutlineVertex = function (mesh, i) { return !!mesh && i < mesh.outline.length; };
  // Static pose editing (PR2's on-canvas drag writes through this).
  NS.setOffset = function (meshId, i, du, dv) {
    var m = get(meshId);
    if (!m || !m.offsets || i < 0 || i >= m.offsets.length) return false;
    m.offsets[i] = [du, dv];
    return true;
  };
  NS.resetOffsets = function (meshId) {
    var m = get(meshId);
    if (!m) return false;
    m.offsets = m.verts.map(function () { return [0, 0]; });
    return true;
  };

  window.SMImageMesh = NS;
})();
