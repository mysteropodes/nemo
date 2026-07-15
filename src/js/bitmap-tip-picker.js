// ---- Bitmap Brush tip picker popover (2026-07, panel unification pass)
// ----
// Mirrors brush-preset-picker.js's swatch-button-opens-a-preview-grid
// pattern exactly, so choosing a Bitmap Brush tip finally shows what it
// looks like before picking it — same as the vector presets already do
// ("avec aperçu comme brush vecto"). The preview IS the real tip: it draws
// window.SMBitmapBrush's own buildTipCanvas() output (tinted with the
// current stroke color, same white-mask + destination-in tint the real
// bake uses) rather than a separate lookalike approximation — "what you
// see in the picker" can't drift from "what you actually get".
(function () {
  var popover = null, closeHandlers = null;

  function closePopover() {
    if (!popover) return;
    popover.remove();
    popover = null;
    if (closeHandlers) { closeHandlers(); closeHandlers = null; }
  }

  function drawPreview(canvas, tipKey) {
    var ctx = canvas.getContext('2d');
    var w = canvas.width, h = canvas.height;
    ctx.clearRect(0, 0, w, h);
    if (!window.SMBitmapBrush) return;
    var mask = window.SMBitmapBrush.buildTipCanvas(tipKey, 1); // fixed seed — the picker shows the TIP shape, not a particular stroke's jitter
    var color = (window.state && state.strokeColor) || '#000000';
    // Tint pass — identical convention to bitmap-brush.js's own
    // beginLivePreview tinting: fill with color, clip to the mask's alpha.
    var scale = Math.min(w, h) * 0.85 / mask.width;
    var dw = mask.width * scale, dh = mask.height * scale;
    var dx = (w - dw) / 2, dy = (h - dh) / 2;
    ctx.save();
    ctx.fillStyle = color;
    ctx.fillRect(0, 0, w, h);
    ctx.globalCompositeOperation = 'destination-in';
    ctx.drawImage(mask, dx, dy, dw, dh);
    ctx.restore();
  }

  function labelFor(key) {
    if (!window.SMBitmapBrush) return key;
    var customName = window.SMBitmapBrush.customTipName(key);
    if (customName) return '📥 ' + customName;
    var found = null;
    window.SMBitmapBrush.tipGroups().forEach(function (g) { g.keys.forEach(function (k) { if (k === key) found = k; }); });
    if (!found) return key;
    return key.split('-').map(function (w) { return w[0].toUpperCase() + w.slice(1); }).join(' ');
  }

  function paintButton(tipKey) {
    var canvas = document.getElementById('p-bitmaptip-preview');
    var label = document.getElementById('p-bitmaptip-label');
    if (canvas) drawPreview(canvas, tipKey);
    if (label) label.textContent = labelFor(tipKey || 'soft');
  }

  function selectTip(key) {
    state.bitmapTip = key;
    paintButton(key);
  }

  function open(anchorEl, currentKey, onSelect) {
    closePopover();
    if (!window.SMBitmapBrush) return;
    var el = document.createElement('div');
    el.className = 'ctx-menu bp-picker-pop';
    var html = '';
    window.SMBitmapBrush.tipGroups().forEach(function (g) {
      html += '<div class="bp-group-label">' + g.label + '</div><div class="bp-grid">';
      g.keys.forEach(function (k) {
        html += '<button class="bp-item' + (k === currentKey ? ' active' : '') + '" data-key="' + k + '" title="' + labelFor(k) + '">' +
          '<canvas width="150" height="20"></canvas><span>' + labelFor(k) + '</span></button>';
      });
      html += '</div>';
    });
    var custom = window.SMBitmapBrush.customTipKeys();
    if (custom.length) {
      html += '<div class="bp-group-label">Importés (.abr)</div><div class="bp-grid">';
      custom.forEach(function (k) {
        html += '<button class="bp-item' + (k === currentKey ? ' active' : '') + '" data-key="' + k + '" title="' + labelFor(k) + '">' +
          '<canvas width="150" height="20"></canvas><span>' + labelFor(k) + '</span></button>';
      });
      html += '</div>';
    }
    el.innerHTML = html;
    document.body.appendChild(el);
    popover = el;

    el.querySelectorAll('.bp-item').forEach(function (btn) {
      drawPreview(btn.querySelector('canvas'), btn.dataset.key);
      btn.addEventListener('click', function () {
        onSelect(btn.dataset.key);
        closePopover();
      });
    });

    var ar = anchorEl.getBoundingClientRect();
    el.style.visibility = 'hidden'; el.style.display = 'block';
    var ew = el.offsetWidth, eh = el.offsetHeight;
    var left = Math.min(ar.left, window.innerWidth - ew - 8);
    var top = Math.min(ar.bottom + 6, window.innerHeight - eh - 8);
    el.style.left = Math.max(4, left) + 'px'; el.style.top = Math.max(4, top) + 'px';
    el.style.visibility = '';

    function onOutside(e) { if (!el.contains(e.target) && e.target !== anchorEl) closePopover(); }
    function onKey(e) { if (e.key === 'Escape') closePopover(); }
    document.addEventListener('mousedown', onOutside, true);
    document.addEventListener('keydown', onKey);
    closeHandlers = function () {
      document.removeEventListener('mousedown', onOutside, true);
      document.removeEventListener('keydown', onKey);
    };
  }

  window.BitmapTipPicker = { open: open, paintButton: paintButton, drawPreview: drawPreview, labelFor: labelFor, selectTip: selectTip };

  function init() {
    var btn = document.getElementById('p-bitmaptip-btn');
    if (!btn) return;
    btn.addEventListener('click', function (e) {
      e.preventDefault(); e.stopPropagation();
      open(btn, state.bitmapTip || 'soft', selectTip);
    });
    paintButton(state.bitmapTip || 'soft');
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init);
  else init();
})();
