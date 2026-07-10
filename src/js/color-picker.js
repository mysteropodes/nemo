// Custom color picker popover (SV square + hue slider + Hex/RGB fields +
// quick swatches + eyedropper), replacing the plain OS-native <input
// type="color"> click target with a Graphite/Figma-style anchored popover.
//
// Integration is deliberately non-invasive: the native <input type="color">
// elements (color-stroke/color-fill/pm-stroke-c/pm-fill-c) stay in the DOM
// exactly as before and remain the single source of truth every other piece
// of code (setStrokeColor/setFillColor, the eyedropper, project load/save)
// already reads/writes via `.value` + an 'input' event. This popover only
// becomes an alternate way to set that same value — it writes into the
// native input and dispatches a real 'input' event, so it doesn't need any
// changes anywhere else. The native input's own click-to-open-OS-picker
// behavior is suppressed via pointer-events:none (see wireColorSwatches()
// below) so our popover opens instead.
//
// Alpha channel: the color itself can carry transparency (#rrggbbaa) on top
// of the pre-existing, separate per-object opacity control (#p-opacity) —
// the two are independent (one is "how see-through is this paint", the
// other "how see-through is the whole object"), same as every other vector
// tool's fill-alpha vs layer-opacity split. Colors round-trip through
// serP/desP and engine-bridge's cssColorToRgba/draw-bridge's hexToRgba,
// all of which now read the 4th hex byte pair when present.
(function () {
  function hexToRgb(hex) {
    hex = (hex || '#000000').replace('#', '');
    if (hex.length === 3) hex = hex.split('').map(function (c) { return c + c; }).join('');
    return { r: parseInt(hex.substr(0, 2), 16) || 0, g: parseInt(hex.substr(2, 2), 16) || 0, b: parseInt(hex.substr(4, 2), 16) || 0 };
  }
  function hexToAlpha(hex) {
    hex = (hex || '').replace('#', '');
    if (hex.length !== 8) return 1;
    return (parseInt(hex.substr(6, 2), 16) || 0) / 255;
  }
  function rgbToHex(r, g, b, a) {
    function h(n) { return Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, '0'); }
    var hex = '#' + h(r) + h(g) + h(b);
    if (a !== undefined && a < 1) hex += h(a * 255);
    return hex;
  }
  function rgbToHsv(r, g, b) {
    r /= 255; g /= 255; b /= 255;
    var max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min;
    var h = 0, s = max === 0 ? 0 : d / max, v = max;
    if (d !== 0) {
      if (max === r) h = ((g - b) / d) % 6;
      else if (max === g) h = (b - r) / d + 2;
      else h = (r - g) / d + 4;
      h *= 60; if (h < 0) h += 360;
    }
    return { h: h, s: s, v: v };
  }
  function hsvToRgb(h, s, v) {
    var c = v * s, x = c * (1 - Math.abs((h / 60) % 2 - 1)), m = v - c, r, g, b;
    if (h < 60) { r = c; g = x; b = 0; } else if (h < 120) { r = x; g = c; b = 0; }
    else if (h < 180) { r = 0; g = c; b = x; } else if (h < 240) { r = 0; g = x; b = c; }
    else if (h < 300) { r = x; g = 0; b = c; } else { r = c; g = 0; b = x; }
    return { r: (r + m) * 255, g: (g + m) * 255, b: (b + m) * 255 };
  }

  var popover = null, closeHandlers = null;

  function closePopover() {
    if (!popover) return;
    popover.remove();
    popover = null;
    if (closeHandlers) { closeHandlers(); closeHandlers = null; }
    if (window.setPaletteHighlight) window.setPaletteHighlight(null);
  }

  // Opens an anchored popover next to `anchorEl`, seeded with `initialHex`,
  // calling `onChange(hex)` live as the user edits (mirrors the native
  // <input type="color"> firing 'input' continuously while open).
  function open(anchorEl, initialHex, onChange) {
    closePopover();
    var hsv = rgbToHsv.apply(null, (function (c) { return [c.r, c.g, c.b]; })(hexToRgb(initialHex)));
    var alpha = hexToAlpha(initialHex);

    var el = document.createElement('div');
    el.className = 'ctx-menu color-picker-pop';
    el.innerHTML =
      '<div class="cp-sv"><canvas class="cp-sv-canvas" width="180" height="140"></canvas><div class="cp-sv-thumb"></div></div>' +
      '<div class="cp-sliders">' +
      '<div class="cp-hue"><canvas class="cp-hue-canvas" width="180" height="14"></canvas><div class="cp-hue-thumb"></div></div>' +
      '<div class="cp-alpha"><canvas class="cp-alpha-canvas" width="180" height="14"></canvas><div class="cp-alpha-thumb"></div></div>' +
      '</div>' +
      '<div class="cp-preview-row"><div class="cp-preview"></div>' +
      '<div class="cp-fields">' +
      '<label>Hex<input class="cp-hex" type="text" maxlength="9"></label>' +
      '<div class="cp-rgb-row">' +
      '<input class="cp-r" type="number" min="0" max="255" title="R"><input class="cp-g" type="number" min="0" max="255" title="G"><input class="cp-b" type="number" min="0" max="255" title="B"><input class="cp-a" type="number" min="0" max="100" title="Alpha %">' +
      '</div></div></div>' +
      '<div class="cp-swatch-row">' +
      '<button class="cp-swatch cp-swatch-none" title="None"></button>' +
      '<button class="cp-swatch" data-hex="#000000" style="background:#000" title="Black"></button>' +
      '<button class="cp-swatch" data-hex="#ffffff" style="background:#fff" title="White"></button>' +
      '<button class="cp-eyedrop" title="Pick from canvas"><span class="material-symbols-rounded" style="font-size:14px">&#xe3b8;</span></button>' +
      '</div>';
    document.body.appendChild(el);
    popover = el;

    var svCanvas = el.querySelector('.cp-sv-canvas'), svCtx = svCanvas.getContext('2d');
    var svThumb = el.querySelector('.cp-sv-thumb');
    var hueCanvas = el.querySelector('.cp-hue-canvas'), hueCtx = hueCanvas.getContext('2d');
    var hueThumb = el.querySelector('.cp-hue-thumb');
    var alphaCanvas = el.querySelector('.cp-alpha-canvas'), alphaCtx = alphaCanvas.getContext('2d');
    var alphaThumb = el.querySelector('.cp-alpha-thumb');
    var preview = el.querySelector('.cp-preview');
    var hexInput = el.querySelector('.cp-hex');
    var rInput = el.querySelector('.cp-r'), gInput = el.querySelector('.cp-g'), bInput = el.querySelector('.cp-b'), aInput = el.querySelector('.cp-a');

    function drawHue() {
      var grad = hueCtx.createLinearGradient(0, 0, 180, 0);
      for (var i = 0; i <= 6; i++) grad.addColorStop(i / 6, 'hsl(' + (i * 60) + ',100%,50%)');
      hueCtx.fillStyle = grad; hueCtx.fillRect(0, 0, 180, 14);
    }
    function drawSv() {
      var rgb = hsvToRgb(hsv.h, 1, 1);
      svCtx.fillStyle = 'rgb(' + Math.round(rgb.r) + ',' + Math.round(rgb.g) + ',' + Math.round(rgb.b) + ')';
      svCtx.fillRect(0, 0, 180, 140);
      var whiteGrad = svCtx.createLinearGradient(0, 0, 180, 0);
      whiteGrad.addColorStop(0, 'rgba(255,255,255,1)'); whiteGrad.addColorStop(1, 'rgba(255,255,255,0)');
      svCtx.fillStyle = whiteGrad; svCtx.fillRect(0, 0, 180, 140);
      var blackGrad = svCtx.createLinearGradient(0, 0, 0, 140);
      blackGrad.addColorStop(0, 'rgba(0,0,0,0)'); blackGrad.addColorStop(1, 'rgba(0,0,0,1)');
      svCtx.fillStyle = blackGrad; svCtx.fillRect(0, 0, 180, 140);
    }
    // Checkerboard base (so 0% reads as "transparent", not "black") plus a
    // gradient from fully-transparent to the current opaque RGB — hue-
    // dependent just like the SV square, so this also needs a redraw
    // whenever the color (not just alpha) changes.
    function drawAlpha() {
      alphaCtx.clearRect(0, 0, 180, 14);
      for (var x = 0; x < 180; x += 7) {
        for (var y = 0; y < 14; y += 7) {
          alphaCtx.fillStyle = ((x / 7 + y / 7) % 2 === 0) ? '#3a3a48' : '#232330';
          alphaCtx.fillRect(x, y, 7, 7);
        }
      }
      var rgb = hsvToRgb(hsv.h, hsv.s, hsv.v);
      var grad = alphaCtx.createLinearGradient(0, 0, 180, 0);
      grad.addColorStop(0, 'rgba(' + Math.round(rgb.r) + ',' + Math.round(rgb.g) + ',' + Math.round(rgb.b) + ',0)');
      grad.addColorStop(1, 'rgba(' + Math.round(rgb.r) + ',' + Math.round(rgb.g) + ',' + Math.round(rgb.b) + ',1)');
      alphaCtx.fillStyle = grad; alphaCtx.fillRect(0, 0, 180, 14);
    }
    function positionThumbs() {
      svThumb.style.left = (hsv.s * 180) + 'px'; svThumb.style.top = ((1 - hsv.v) * 140) + 'px';
      hueThumb.style.left = (hsv.h / 360 * 180) + 'px';
      alphaThumb.style.left = (alpha * 180) + 'px';
    }
    function currentHex() {
      var rgb = hsvToRgb(hsv.h, hsv.s, hsv.v);
      return rgbToHex(rgb.r, rgb.g, rgb.b, alpha);
    }
    function pushChange() {
      var hex = currentHex(), rgb = hexToRgb(hex);
      preview.style.backgroundColor = hex; // backgroundColor, not the `background` shorthand — the latter clears the checkerboard background-image already set in CSS for .cp-preview
      hexInput.value = hex;
      rInput.value = rgb.r; gInput.value = rgb.g; bInput.value = rgb.b; aInput.value = Math.round(alpha * 100);
      drawSv(); // the SV square's gradient is hue-dependent — must redraw whenever hue changes (hue slider drag, hex/rgb entry), not just at popover open
      drawAlpha();
      positionThumbs();
      onChange(hex);
      // Rings whichever swatch(es) match the live color in the Nuancier
      // panel (feedback #20) — refreshed on every change, not just at open,
      // so dragging around the picker keeps it in sync.
      if (window.setPaletteHighlight) window.setPaletteHighlight(hex);
    }
    function setFromHsv() { pushChange(); }
    function setFromHex(hex) {
      if (!/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(hex)) return;
      var rgb = hexToRgb(hex);
      hsv = rgbToHsv(rgb.r, rgb.g, rgb.b);
      alpha = hexToAlpha(hex);
      pushChange();
    }
    function setFromRgb() {
      var r = Math.max(0, Math.min(255, parseInt(rInput.value) || 0));
      var g = Math.max(0, Math.min(255, parseInt(gInput.value) || 0));
      var b = Math.max(0, Math.min(255, parseInt(bInput.value) || 0));
      var aPct = parseInt(aInput.value); if (isNaN(aPct)) aPct = 100;
      alpha = Math.max(0, Math.min(100, aPct)) / 100;
      hsv = rgbToHsv(r, g, b);
      pushChange();
    }

    function dragOn(canvasEl, moveFn) {
      var active = false;
      function toLocal(e) {
        var r = canvasEl.getBoundingClientRect();
        return { x: Math.max(0, Math.min(r.width, e.clientX - r.left)), y: Math.max(0, Math.min(r.height, e.clientY - r.top)) };
      }
      canvasEl.addEventListener('pointerdown', function (e) { active = true; canvasEl.setPointerCapture(e.pointerId); moveFn(toLocal(e)); e.preventDefault(); });
      canvasEl.addEventListener('pointermove', function (e) { if (active) moveFn(toLocal(e)); });
      canvasEl.addEventListener('pointerup', function () { active = false; });
    }
    dragOn(svCanvas, function (p) { hsv.s = p.x / 180; hsv.v = 1 - p.y / 140; setFromHsv(); });
    dragOn(hueCanvas, function (p) { hsv.h = p.x / 180 * 360; setFromHsv(); });
    dragOn(alphaCanvas, function (p) { alpha = p.x / 180; setFromHsv(); });

    hexInput.addEventListener('input', function () { var v = this.value.trim(); if (v[0] !== '#') v = '#' + v; setFromHex(v); });
    [rInput, gInput, bInput, aInput].forEach(function (inp) { inp.addEventListener('input', setFromRgb); });
    el.querySelectorAll('.cp-swatch[data-hex]').forEach(function (b) {
      b.addEventListener('click', function () { setFromHex(this.dataset.hex); });
    });
    el.querySelector('.cp-swatch-none').addEventListener('click', function () {
      // "None" only makes sense for Fill — closing here and letting the
      // caller's dblclick-to-toggle affordance handle actually disabling
      // it would duplicate that logic, so this just closes the popover;
      // the caller wires .cp-eyedrop/.cp-swatch-none via a data attribute
      // it inspects after close if it cares (see wireColorSwatches).
      el.dataset.noneClicked = '1';
      closePopover();
    });
    el.querySelector('.cp-eyedrop').addEventListener('click', function () {
      el.dataset.eyedropClicked = '1';
      closePopover();
    });

    drawHue(); drawSv(); drawAlpha(); positionThumbs();
    hexInput.value = currentHex();
    var rgb0 = hexToRgb(currentHex());
    rInput.value = rgb0.r; gInput.value = rgb0.g; bInput.value = rgb0.b; aInput.value = Math.round(alpha * 100);
    preview.style.backgroundColor = currentHex();
    if (window.setPaletteHighlight) window.setPaletteHighlight(currentHex());

    // Anchor + clamp to viewport, same idiom as showContextMenu (ui.js).
    var ar = anchorEl.getBoundingClientRect();
    el.style.visibility = 'hidden'; el.style.display = 'block';
    var ew = el.offsetWidth, eh = el.offsetHeight;
    var left = Math.min(ar.left, window.innerWidth - ew - 8);
    var top = Math.min(ar.bottom + 6, window.innerHeight - eh - 8);
    el.style.left = Math.max(4, left) + 'px'; el.style.top = Math.max(4, top) + 'px';
    el.style.visibility = '';

    var lastNoneClicked = false, lastEyedropClicked = false;
    function onOutside(e) { if (!el.contains(e.target) && e.target !== anchorEl) closePopover(); }
    function onKey(e) { if (e.key === 'Escape') closePopover(); }
    document.addEventListener('mousedown', onOutside, true);
    document.addEventListener('keydown', onKey);
    closeHandlers = function () {
      document.removeEventListener('mousedown', onOutside, true);
      document.removeEventListener('keydown', onKey);
      if (el.dataset.noneClicked) open._lastNone && open._lastNone();
      if (el.dataset.eyedropClicked) open._lastEyedrop && open._lastEyedrop();
    };
  }

  // Wires a swatch wrapper (.cw-mini / .color-well) + its paired native
  // <input type="color"> to open this popover instead of the OS picker,
  // writing back into the same input so every existing listener keeps
  // working unchanged.
  //
  // Alpha caveat: a native <input type="color"> silently truncates any
  // value assigned to it down to 6 hex digits (browser-enforced — confirmed
  // by direct test, it just drops bytes 7-8) — so an 8-digit hex can't
  // round-trip through `.value` alone. The full hex (with alpha) is also
  // stashed on `input.dataset.hex8`; every listener that reads the color
  // back out of these inputs must prefer `.dataset.hex8` over `.value`.
  function wireColorSwatches(pairs) {
    pairs.forEach(function (pair) {
      var wrap = document.getElementById(pair.wrap), input = document.getElementById(pair.input);
      if (!wrap || !input) return;
      input.style.pointerEvents = 'none'; // suppress the native OS picker; popover takes over
      wrap.addEventListener('click', function (e) {
        e.preventDefault(); e.stopPropagation();
        open(wrap, input.dataset.hex8 || input.value, function (hex) {
          input.value = hex;
          input.dataset.hex8 = hex;
          input.dispatchEvent(new Event('input', { bubbles: true }));
        });
        open._lastNone = pair.onNone || null;
        open._lastEyedrop = pair.onEyedrop || null;
      });
    });
  }

  window.ColorPicker = { open: open, wireColorSwatches: wireColorSwatches, close: closePopover };
})();
