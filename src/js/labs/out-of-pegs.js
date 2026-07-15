// ---- LABS PROTOTYPE — Out of Pegs (TVPaint light table) ----
// TVPaint's signature inbetweening aid: slide the PREVIOUS/NEXT onion
// ghosts off their pegs so both poses line up under the drawing you're
// about to make, trace the inbetween, put them back on pegs.
//
//   SMLabs.setPegOffset('prev', dx, dy)   — world-px offset for the
//   SMLabs.setPegOffset('next', dx, dy)     previous / next ghosts
//   SMLabs.resetPegs()                    — back on pegs (0,0)
//
// Implementation: a runtime WRAPPER around window.renderOS (installed on
// enable, removed on disable — never a file edit). renderOS rebuilds
// onionPrevLayer/onionNextLayer from scratch on every call (tweens.js),
// so translating the freshly-built children right after it runs can never
// accumulate; the engine picks the moved Paper items up on its next tick
// via onionLayerItems() like any other onion content. Wrapping is more
// intrusive than the MutationObserver pattern used elsewhere in Labs, but
// there is no DOM to observe here (Paper scene graph) and the wrapper is
// call-through + post-translate only.
(function () {
  var offs = { prev: [0, 0], next: [0, 0] };
  var origRenderOS = null;

  function applyOffsets() {
    if (typeof onionPrevLayer === 'undefined') return;
    if (offs.prev[0] || offs.prev[1]) onionPrevLayer.translate(new Point(offs.prev[0], offs.prev[1]));
    if (offs.next[0] || offs.next[1]) onionNextLayer.translate(new Point(offs.next[0], offs.next[1]));
    // renderOS already bumped _sceneVersion for its own rebuild; the
    // translate happens before the engine's next tick reads the layers, so
    // no extra bump is needed — but a direct setPegOffset call outside a
    // rebuild does need one (renderOS is re-run there instead, see below).
  }
  function wrapped() {
    origRenderOS.apply(this, arguments);
    applyOffsets();
  }

  window.SMLabs.setPegOffset = function (which, dx, dy) {
    if (which !== 'prev' && which !== 'next') { console.warn('[labs] setPegOffset: \'prev\' ou \'next\''); return; }
    offs[which] = [+dx || 0, +dy || 0];
    if (window.SMLabs.isOn('out-of-pegs') && typeof renderOS === 'function') renderOS();
    return offs[which].slice();
  };
  window.SMLabs.resetPegs = function () {
    offs.prev = [0, 0]; offs.next = [0, 0];
    if (typeof renderOS === 'function') renderOS();
    if (typeof showToast === 'function') showToast('Fantômes remis sur pegs');
  };

  window.SMLabs.register('out-of-pegs', {
    flag: 'nemo-labs-pegs',
    describe: 'Out of Pegs (TVPaint) : décale les fantômes onion prev/next pour caler l\'intervalle (SMLabs.setPegOffset(\'prev\',dx,dy), resetPegs())',
    onEnable: function () {
      if (!origRenderOS && typeof window.renderOS === 'function') {
        origRenderOS = window.renderOS;
        window.renderOS = wrapped;
      }
      if (typeof renderOS === 'function') renderOS();
    },
    onDisable: function () {
      if (origRenderOS) { window.renderOS = origRenderOS; origRenderOS = null; }
      offs.prev = [0, 0]; offs.next = [0, 0];
      if (typeof renderOS === 'function') renderOS();
    },
  });
  // Reload persistence: this file loads BEFORE tweens.js in index.html, so
  // window.renderOS doesn't exist yet at parse time — install the wrapper
  // once every script is up.
  window.addEventListener('load', function () {
    if (window.SMLabs.isOn('out-of-pegs') && !origRenderOS && typeof window.renderOS === 'function') {
      origRenderOS = window.renderOS;
      window.renderOS = wrapped;
    }
  });
})();
