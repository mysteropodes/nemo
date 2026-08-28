// ---- LABS PROTOTYPE — Value check / view filters (Clip Studio Paint) ----
// CSP-style display checks: preview the canvas in grayscale (value
// structure), boosted contrast, or any CSS filter — without touching the
// artwork. Same display-only construction as mirror-check: a CSS filter
// on the canvas area element, engine/Paper/tools untouched.
//
//   SMLabs.setViewFilter('values')      — grayscale (niveaux de valeurs)
//   SMLabs.setViewFilter('contrast')    — contraste renforcé
//   SMLabs.setViewFilter('none')        — retour normal
//   SMLabs.setViewFilter('blur(2px)')   — n'importe quel filtre CSS
//
// Drawing WHILE filtered is fine (unlike the mirror: coordinates are
// unchanged, only colors are re-rendered) — that's the point: paint
// values in grayscale, flip back to color.
(function () {
  var PRESETS = {
    none: '',
    values: 'grayscale(1)',
    contrast: 'contrast(1.6) saturate(1.1)',
    dim: 'brightness(.55)',
  };
  function area() {
    return document.getElementById('canvas-area') ||
           (document.getElementById('drawing-canvas') && document.getElementById('drawing-canvas').parentElement);
  }

  window.SMLabs.setViewFilter = function (nameOrCss) {
    if (!window.SMLabs.isOn('view-filter')) { console.warn('[labs] enable(\'view-filter\') d\'abord'); return; }
    var t = area(); if (!t) return;
    var css = PRESETS.hasOwnProperty(nameOrCss) ? PRESETS[nameOrCss] : String(nameOrCss || '');
    t.style.filter = css;
    if (typeof showToast === 'function') showToast(css ? SM.t('labsToastViewFilterPrefix') + (PRESETS.hasOwnProperty(nameOrCss) ? nameOrCss : css) : SM.t('labsToastViewNormal'));
    return css;
  };

  window.SMLabs.register('view-filter', {
    flag: 'nemo-labs-viewfilter',
    describe: 'labsDescribeViewFilter',
    // Turning the prototype on used to set the flag and NOTHING else: the
    // filter only ever applied via the console API, so the floating panel's
    // "Contrôle des valeurs" button was a dead button for anyone who hadn't
    // read this file (measured: flag 1, computed filter still 'none').
    // Enabling now applies the preset the button's own label promises —
    // grayscale value check — and setViewFilter stays the way to pick any
    // other preset. Same "a button that silently does nothing is worse than
    // no button" fix selectGhostAll already got (timeline.js:1930).
    onEnable: function () {
      var t = area(); if (!t) return;
      if (!t.style.filter) {
        t.style.filter = PRESETS.values;
        if (typeof showToast === 'function') showToast(SM.t('labsToastViewFilterGrayscaleOn'));
      }
    },
    onDisable: function () { var t = area(); if (t) t.style.filter = ''; },
  });
})();
