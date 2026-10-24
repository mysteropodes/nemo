// Display-only color checks. Native previews carry a closed filter through
// their existing owner to the final GPU surface; document/export pixels stay
// unchanged. The browser prototype retains its existing CSS-only behavior.
(function () {
  var KEY = 'nemo-labs-viewfilter-preset';
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
  function nativeDesktop() { return !!window.__TAURI__; }
  function preset() {
    var value; try { value = localStorage.getItem(KEY); } catch (_) {}
    return Object.prototype.hasOwnProperty.call(PRESETS, value) ? value : 'values';
  }
  window.SMLabs.getViewFilter = function () {
    return window.SMLabs.isOn('view-filter') ? preset() : 'none';
  };
  function apply(css) {
    var t = area();
    if (t) t.style.filter = nativeDesktop() ? '' : css;
    if (!nativeDesktop()) return;
    var project = window.NemoNativeObjectProject;
    if (!project || typeof project.desktop !== 'function') return;
    var session = project.desktop(window);
    if (session && session.isActive()) session.renderPreview(window.state.currentFrame);
  }

  window.SMLabs.setViewFilter = function (nameOrCss) {
    if (!window.SMLabs.isOn('view-filter')) { console.warn('[labs] enable(\'view-filter\') d\'abord'); return; }
    if (nameOrCss === 'grayscale') nameOrCss = 'values';
    var known = Object.prototype.hasOwnProperty.call(PRESETS, nameOrCss);
    if (nativeDesktop() && !known) throw new Error('Native display filter is unavailable');
    var css = known ? PRESETS[nameOrCss] : String(nameOrCss || '');
    if (known) localStorage.setItem(KEY, nameOrCss);
    apply(css);
    if (typeof showToast === 'function') showToast(css ? SM.t('labsToastViewFilterPrefix') + (PRESETS.hasOwnProperty(nameOrCss) ? nameOrCss : css) : SM.t('labsToastViewNormal'));
    return css;
  };

  window.SMLabs.register('view-filter', {
    flag: 'nemo-labs-viewfilter',
    describe: 'labsDescribeViewFilter',
    onEnable: function () {
      var t = area();
      if (!nativeDesktop() && (!t || t.style.filter)) return;
      // Each explicit value-check enable starts with its promised grayscale.
      localStorage.setItem(KEY, 'values');
      apply(PRESETS.values);
      if (typeof showToast === 'function') showToast(SM.t('labsToastViewFilterGrayscaleOn'));
    },
    onDisable: function () { apply(''); },
  });
  // Registration does not replay onEnable. Native startup reads the persisted
  // preference on its first preview; the browser applies it once DOM is ready.
  if (!nativeDesktop() && window.SMLabs.isOn('view-filter')) {
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', function () { apply(PRESETS[preset()]); }, { once: true });
    else apply(PRESETS[preset()]);
  }
})();
