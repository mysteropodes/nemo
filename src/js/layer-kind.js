// ---- LAYER KIND — one answer to "what sort of layer is this?" ----
// A layer's type was spread across six independent flags (symbolId,
// nativeVideo, montageId, isNullLayer, isEffectLayer, isTextLayer), each
// tested inline wherever someone happened to need it — so nothing could
// SHOW the type, and every new consumer re-derived it slightly differently.
// Animation 2D differentiated exactly one case (a component's name in
// italics) and Motion none at all; a video, an imported sequence and a
// hand-drawn layer were three identical rows (2026-07-27: "essaye de
// différencier les types de calques en fonction des éléments footage
// (image, vidéo, séquence images), calque caméra, calque d'effet, texte…").
//
// This is the single place that decides, so a row badge, a context menu and
// a future property panel can't disagree about what they're looking at.
// Order is most-specific-first and matters: a component built FROM a video
// is a component, because that is what you now manipulate.
//
// Footage detection has two tiers. `ld.footage` is the explicit tag the
// importer writes (images.js) — authoritative. Older projects predate it, so
// a layer whose every frame holds nothing but rasters is read as footage
// too: that is exactly the shape importImages/importSequence produce, and a
// hand-drawn layer never matches it (one vector stroke anywhere is enough to
// disqualify). Still vs sequence is decided by whether the frames actually
// carry different images, not by how many frames exist — a still placed on
// a 120-frame layer is still a still.
(function () {
  var SVG = function (d, extra) {
    return '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" ' +
      'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' + d + '</svg>' + (extra || '');
  };
  var ICONS = {
    draw: SVG('<path d="M4 18c4 0 4-12 8-12s4 12 8 12"/>'),
    image: SVG('<rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="8.5" cy="10" r="1.5"/><path d="M21 16l-5-5-6 6"/>'),
    sequence: SVG('<rect x="2" y="6" width="12" height="12" rx="1.5"/><path d="M17 8v8M21 10v4"/>'),
    video: SVG('<rect x="2" y="6" width="14" height="12" rx="2"/><path d="m17 10 5-3v10l-5-3Z"/>'),
    camera: SVG('<path d="M3 8.5 15 5v14L3 15.5Z"/><path d="M15 10.5 21 8v8l-6-2.5Z"/>'),
    effect: SVG('<path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1"/><circle cx="12" cy="12" r="3"/>'),
    text: SVG('<path d="M5 6h14M12 6v13M9 19h6"/>'),
    'null': SVG('<path d="M12 4v16M4 12h16"/><circle cx="12" cy="12" r="8" stroke-dasharray="3 3"/>'),
    // Folder (2026-08) — a Null-shaped layer (isNullLayer:true too, see
    // addFolderLayer/timeline.js) that also groups+scopes-effects for its
    // children, so it gets its own badge rather than showing as 'null'.
    folder: SVG('<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/>'),
    // Open-folder variant (2026-08-29, feedback #148: "quand on clic dessus
    // pour ouvrir faudrait avoir la même icon en icon folder ouvert") — same
    // tab-on-top silhouette as the closed icon above, split into a back wall
    // (drawn only down to the fold line) and a front flap tilted open below
    // it, the standard two-path "open folder" convention. Selected by
    // keyOf/of below based on ld.folderCollapsed, not a separate `key` (the
    // CSS class / i18n label stay 'folder' either way — this is purely a
    // glyph swap, not a different layer kind).
    folderOpen: SVG('<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1H3Z"/><path d="M3 8l1.3 8.3A2 2 0 0 0 6.3 18h11.4a2 2 0 0 0 2-1.7L21 8Z"/>'),
    // Two overlapping squares ("an instance stamped from a shared
    // definition"), not the isometric-cube glyph this used to be
    // (2026-07-29 fix, "l'icon component du calque est le même que celui
    // pour le 3D layer") — that cube path was visually the same hexagon-
    // wireframe silhouette as ICO_3D (timeline.js) at this same 12px render
    // size, so a Component layer and a 3D-toggled layer were indistinguishable
    // at a glance in the row.
    component: SVG('<rect x="4" y="8" width="11" height="11" rx="2"/><rect x="9" y="3" width="11" height="11" rx="2"/>'),
    montage: SVG('<rect x="3" y="5" width="7" height="14" rx="1.5"/><rect x="14" y="5" width="7" height="14" rx="1.5"/>'),
  };
  // Fallback labels, used when i18n hasn't loaded (or has no entry). The real
  // strings live in i18n.js under layerKind<Key>.
  var FALLBACK = {
    draw: 'Dessin', image: 'Image', sequence: 'Séquence', video: 'Vidéo', camera: 'Caméra',
    effect: 'Effet', text: 'Texte', 'null': 'Null', component: 'Composant', montage: 'Montage', folder: 'Dossier',
  };

  function everyStrokeIsRaster(ld) {
    var frames = ld && ld.frames;
    if (!frames || !frames.length) return false;
    var seen = 0;
    for (var i = 0; i < frames.length; i++) {
      var st = frames[i] && frames[i].strokes;
      if (!st || !st.length) continue;
      for (var j = 0; j < st.length; j++) {
        if (!st[j] || !st[j].isRaster) return false;
        seen++;
      }
    }
    return seen > 0;
  }
  // Distinct image sources across the layer's frames — 2+ means the frames
  // animate, i.e. a sequence rather than one still sitting on a long layer.
  function distinctSources(ld) {
    var srcs = {}, n = 0;
    (ld.frames || []).forEach(function (f) {
      (f && f.strokes || []).forEach(function (s) {
        if (!s || !s.isRaster || !s.src) return;
        if (!srcs[s.src]) { srcs[s.src] = 1; n++; }
      });
    });
    return n;
  }

  function keyOf(ld) {
    if (!ld) return 'draw';
    if (ld.isCameraLayer) return 'camera';
    if (ld.montageId) return 'montage';
    if (ld.symbolId) return 'component';
    if (ld.nativeVideo) return 'video';
    if (ld.isEffectLayer) return 'effect';
    if (ld.isFolderLayer) return 'folder';
    if (ld.isNullLayer) return 'null';
    if (ld.isGuideLayer) return 'guide';
    if (ld.isTextLayer) return 'text';
    if (ld.footage && ld.footage.kind) return ld.footage.kind === 'sequence' ? 'sequence' : 'image';
    if (everyStrokeIsRaster(ld)) return distinctSources(ld) > 1 ? 'sequence' : 'image';
    return 'draw';
  }

  function labelOf(key) {
    var t = (window.SM && window.SM.t) ? window.SM.t('layerKind' + key.charAt(0).toUpperCase() + key.slice(1)) : null;
    // SM.t echoes the key back when it has no entry — treat that as "no
    // translation yet" rather than printing "layerKindImage" in the UI.
    if (t && t.indexOf('layerKind') !== 0) return t;
    return FALLBACK[key] || key;
  }

  window.SMLayerKind = {
    // {key, label, icon} — key is stable and safe as a CSS class suffix.
    of: function (ld) {
      var k = keyOf(ld);
      // Folder: same key/label/CSS class whether collapsed or open (it's
      // still the same layer kind) — only the glyph swaps, mirroring the
      // disclosure arrow right next to it (feedback #148).
      var iconKey = (k === 'folder' && ld && !ld.folderCollapsed) ? 'folderOpen' : k;
      return { key: k, label: labelOf(k), icon: ICONS[iconKey] || ICONS.draw };
    },
    keyOf: keyOf,
    labelOf: labelOf,
    iconOf: function (k) { return ICONS[k] || ICONS.draw; },
  };
})();
