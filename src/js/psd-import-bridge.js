// ---- PSD import (2026-07 — audit gap "pas d'import/export PSD") ----
// Read-only import: one Nemo layer per top-level PSD layer, each a plain
// Raster (the layer's already-composited pixels, exactly as Photoshop
// rendered them — ag-psd does NOT re-render vector/text/smart-object
// layers, it hands back the bitmap Photoshop itself baked into the file,
// which is all any PSD reader can do without reimplementing Photoshop's
// own rendering engine). No PSD *write*/export — reading is the tractable
// half of this gap; writing a spec-correct .psd back out is a much larger
// undertaking (layer effects, text re-flow, color modes) deliberately left
// for later.
//
// Vendored via esm.sh's bundle mode (ag-psd.vendor.mjs, 293KB, ONE
// remaining external import patched to the local node-buffer-shim.vendor.mjs,
// 28KB Buffer polyfill) rather than fetched live from a CDN at runtime —
// same "self-contained vendored dependency" precedent as opentype.js
// (vector-text-bridge.js) earlier this session, since this is a desktop
// app that should work offline.
//
// Scope of THIS pass, deliberately: flat top-level layers only. A PSD
// layer GROUP is skipped as a group and its children are NOT recursively
// flattened — ag-psd represents a group as a `children` node with no
// `canvas` of its own, so it's simply invisible to the "does this child
// have a canvas" check below. Recursing into groups (and deciding how to
// name/order the flattened result) is a reasonable follow-up, not
// attempted here to keep this slice small and correct rather than large
// and half-tested.
(function () {
  var modPromise = null;
  function loadModule() {
    if (!modPromise) modPromise = import('./ag-psd.vendor.mjs');
    return modPromise;
  }
  // Pre-warm on script load, same "don't make the first real use pay for
  // the network/parse cost" precedent as vector-text-bridge.js's font
  // pre-warm.
  loadModule().catch(function (e) { console.warn('[psd-import] failed to load ag-psd module', e); });

  // Walks a PSD's top-level children, importing each one that actually has
  // painted pixels (a group node has none — see header comment) as a new
  // Nemo layer + Raster, positioned/sized/opacity-matched to that PSD
  // layer. Returns the count actually imported.
  function importChildren(psd, children) {
    pushUndo();
    var imported = 0;
    children.forEach(function (layer) {
      if (!layer.canvas || layer.hidden) return;
      var idx = createUserLayer(layer.name || ('Calque PSD ' + (imported + 1)));
      state.layers[idx].visible = true;
      var url = layer.canvas.toDataURL('image/png');
      var target = userLayers[idx];
      var prevActive = project.activeLayer; target.activate();
      var r = new Raster(url);
      var left = layer.left || 0, top = layer.top || 0;
      var w = layer.canvas.width, h = layer.canvas.height;
      r.onLoad = function () {
        r.size = new Size(w, h);
        r.position = new Point(left + w / 2, top + h / 2);
        r.opacity = layer.opacity !== undefined ? layer.opacity : 1;
        r.data.src = url;
        saveActiveLayerFrame();
        updateUI();
        if (window.SMEngineBridge) window.SMEngineBridge.renderNow();
        // Media library registration (2026-07-31) — PSD imports were a
        // real import surface entirely invisible to the catalog until now,
        // same shape as images.js's own addEntry calls.
        if (window.SMMediaLibrary) SMMediaLibrary.addEntry(state.layers[idx].name, 'image', url, state.layers[idx].name, { layerUid: state.layers[idx].layerUid });
      };
      prevActive.activate();
      imported++;
    });
    return imported;
  }

  // Public entry point — reads a File (from an <input type="file"> change
  // event or drag-drop), parses it, imports its layers. Async (module load
  // + file read + parse are all async); returns a Promise<number> (layers
  // imported) so callers can toast a result.
  function importFile(file) {
    return loadModule().then(function (mod) {
      return file.arrayBuffer().then(function (buffer) {
        var psd = mod.readPsd(buffer);
        // Bring a wildly different canvas size in line with the PSD's own
        // — matches Import Image(s)' own behavior of trusting the source
        // material's dimensions rather than silently clipping/scaling.
        if (psd.width && psd.height && (psd.width !== state.canvasW || psd.height !== state.canvasH)) {
          window.SM.setCanvasSize(psd.width, psd.height);
        }
        var count = importChildren(psd, psd.children || []);
        if (!count) throw new Error('Aucun calque avec des pixels trouvé (fichier vide, ou uniquement des groupes/calques masqués)');
        return count;
      });
    });
  }

  window.SMPsdImport = { importFile: importFile };

  function init() {
    var btn = document.getElementById('btn-import-psd');
    var input = document.getElementById('psd-input');
    if (!btn || !input) return;
    btn.addEventListener('click', function () { input.click(); });
    input.addEventListener('change', function (e) {
      var file = e.target.files && e.target.files[0];
      input.value = '';
      if (!file) return;
      if (window.showToast) showToast('Import PSD…');
      importFile(file).then(function (count) {
        if (window.showToast) showToast(count + ' calque' + (count > 1 ? 's' : '') + ' importé' + (count > 1 ? 's' : '') + ' depuis le PSD');
      }).catch(function (err) {
        console.error('[psd-import] failed', err);
        if (window.showToast) showToast('Échec de l’import PSD : ' + (err && err.message ? err.message : 'erreur inconnue'));
      });
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
