// Split code editor for expressions (2026-08-30, "un code editor window qui
// split la zone du canvas avec la possibilité d'éditer le code d'expression
// pouvoir le fermer où l'ouvrir depuis un bouton dans la zone d'expression
// existante").
//
// This panel is a SECOND VIEW of an expression the inline ƒx row already
// edits — never a second copy of it. It addresses its target by the same
// {uid, elem} holder ref copySelectedKeys uses, reads through
// SMMotion.exprSnapshotFor and writes through SMMotion.applyExprCode, which
// IS the inline editor's commit path. One writer, two surfaces: the
// two-writers-disagreeing bug (the color picker, same session) is the exact
// failure mode this avoids.
//
// Layout: #canvas-area is a flex:1 child of the flex-column #canvas-col, so
// splitting it means wrapping it in a flex ROW and putting the panel first.
// The wrapper is created on open and dismantled on close, with #canvas-area
// put back exactly where it was — the canvas element itself is only ever
// MOVED, never recreated, so Paper.js keeps its view and the engine keeps
// its GPU surface. Both are re-measured by dispatching a window resize,
// which app.js (syncCanvasSize) and engine-bridge.js (handleResize) already
// listen for; there is no third resize path to invent.
(function () {
  'use strict';

  var WRAP_ID = 'expr-split-row';
  var PANEL_ID = 'expr-code-panel';
  var WIDTH_KEY = 'nemo-expr-panel-width';
  var MIN_W = 220, MIN_CANVAS = 200;

  var _ref = null, _prop = null, _label = '';
  var _panel = null, _ta = null, _gutter = null, _errEl = null, _titleEl = null, _cb = null;

  function canvasArea() { return document.getElementById('canvas-area'); }

  function savedWidth() {
    var v = parseInt(localStorage.getItem(WIDTH_KEY) || '', 10);
    return (isFinite(v) && v >= MIN_W) ? v : 380;
  }

  // Paper.js and the Rust engine both size themselves off #canvas-area's box.
  // Nothing here measures the canvas itself, so the one honest way to tell
  // them the box changed is the event they already listen to.
  function reflow() {
    window.dispatchEvent(new Event('resize'));
    if (window.SMEngineBridge && SMEngineBridge.renderNow) SMEngineBridge.renderNow();
  }

  function sameRef(a, b) {
    if (!a || !b) return false;
    return a.uid === b.uid && (a.elem || null) === (b.elem || null);
  }

  function isOpen() { return !!document.getElementById(PANEL_ID); }
  function isShowing(ref, prop) { return isOpen() && _prop === prop && sameRef(_ref, ref); }

  function paintGutter() {
    if (!_gutter || !_ta) return;
    var n = (_ta.value.split('\n').length) || 1;
    var s = '';
    for (var i = 1; i <= n; i++) s += i + '\n';
    _gutter.textContent = s;
    _gutter.scrollTop = _ta.scrollTop;
  }

  function commit() {
    if (!_ref || !_ta) return;
    // applyExprCode no-ops when the text is unchanged, so this is safe to
    // call from blur, Cmd+Enter and close alike without guarding each one.
    SMMotion.applyExprCode(_ref, _prop, _ta.value);
    refresh();
  }

  // Re-reads the expression from the model. Called after our own commit (to
  // pick up an error the evaluator recorded) and available to callers for
  // when the inline editor changed the same expression underneath us.
  function refresh() {
    if (!isOpen() || !_ref) return;
    var snap = SMMotion.exprSnapshotFor(_ref, _prop);
    if (!snap) { close(); return; }
    if (document.activeElement !== _ta && _ta.value !== snap.code) {
      _ta.value = snap.code;
      paintGutter();
    }
    if (_cb) _cb.checked = snap.enabled;
    if (_errEl) {
      _errEl.textContent = snap.lastError || '';
      _errEl.style.display = snap.lastError ? '' : 'none';
    }
  }

  function build() {
    var panel = document.createElement('div');
    panel.id = PANEL_ID;

    var head = document.createElement('div');
    head.className = 'ecp-head';
    _titleEl = document.createElement('div');
    _titleEl.className = 'ecp-title';
    head.appendChild(_titleEl);
    var closeBtn = document.createElement('button');
    closeBtn.className = 'ecp-close';
    closeBtn.textContent = '×';
    closeBtn.title = SM.t('titleCloseCodeEditor');
    closeBtn.addEventListener('click', function () { close(); if (window.renderLayerList) renderLayerList(); });
    head.appendChild(closeBtn);
    panel.appendChild(head);

    // The enable checkbox is mirrored here rather than left behind in the
    // inline row: with the panel open beside the canvas, the inline row is
    // usually scrolled out of sight, and writing an expression you cannot
    // switch on from the same place is a dead end.
    var sub = document.createElement('label');
    sub.className = 'ecp-enable';
    _cb = document.createElement('input');
    _cb.type = 'checkbox';
    _cb.addEventListener('change', function () {
      SMMotion.setExprEnabled(_ref, _prop, _cb.checked);
      refresh();
    });
    sub.appendChild(_cb);
    sub.appendChild(document.createTextNode(' ' + SM.t('exprEnableLabel')));
    panel.appendChild(sub);

    var pane = document.createElement('div');
    pane.className = 'ecp-pane';
    _gutter = document.createElement('div');
    _gutter.className = 'ecp-gutter';
    _ta = document.createElement('textarea');
    _ta.className = 'ecp-code';
    _ta.spellcheck = false;
    pane.appendChild(_gutter);
    pane.appendChild(_ta);
    panel.appendChild(pane);

    _errEl = document.createElement('div');
    _errEl.className = 'ecp-err';
    _errEl.style.display = 'none';
    panel.appendChild(_errEl);

    _ta.addEventListener('scroll', function () { _gutter.scrollTop = _ta.scrollTop; });
    _ta.addEventListener('input', paintGutter);
    _ta.addEventListener('blur', commit);
    _ta.addEventListener('keydown', function (e) {
      // Same three bindings the inline box has — a code field that behaves
      // differently depending on which of two views you are in would be its
      // own bug.
      if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') { e.preventDefault(); commit(); }
      if (e.key === 'Escape') { refresh(); _ta.blur(); }
      if (e.key === 'Tab') {
        e.preventDefault();
        var s = _ta.selectionStart, en = _ta.selectionEnd;
        _ta.value = _ta.value.slice(0, s) + '  ' + _ta.value.slice(en);
        _ta.selectionStart = _ta.selectionEnd = s + 2;
        paintGutter();
      }
      // Typing code must never reach the app's global shortcuts (space =
      // play, B = brush, and so on).
      e.stopPropagation();
    });

    var grip = document.createElement('div');
    grip.className = 'ecp-grip';
    grip.title = SM.t('titleCodeEditorResize');
    grip.addEventListener('mousedown', function (e) {
      e.preventDefault();
      var startX = e.clientX, startW = panel.getBoundingClientRect().width;
      var wrap = document.getElementById(WRAP_ID);
      function mv(ev) {
        var maxW = wrap ? wrap.getBoundingClientRect().width - MIN_CANVAS : 9999;
        var w = Math.max(MIN_W, Math.min(maxW, startW + (ev.clientX - startX)));
        panel.style.width = w + 'px';
        reflow();
      }
      function up() {
        document.removeEventListener('mousemove', mv);
        document.removeEventListener('mouseup', up);
        localStorage.setItem(WIDTH_KEY, String(Math.round(panel.getBoundingClientRect().width)));
        reflow();
      }
      document.addEventListener('mousemove', mv);
      document.addEventListener('mouseup', up);
    });
    panel.appendChild(grip);

    return panel;
  }

  function open(ref, prop, label) {
    if (!window.SMMotion || !SMMotion.exprSnapshotFor) return;
    var snap = SMMotion.exprSnapshotFor(ref, prop);
    if (!snap) return;
    _ref = ref; _prop = prop; _label = label || '';

    var ca = canvasArea();
    if (!ca) return;

    if (!isOpen()) {
      _panel = build();
      // Wrap in place: remember where #canvas-area sat so close() can put it
      // back at exactly the same index among its siblings.
      var parent = ca.parentElement;
      var wrap = document.createElement('div');
      wrap.id = WRAP_ID;
      parent.insertBefore(wrap, ca);
      wrap.appendChild(_panel);
      wrap.appendChild(ca);
      // Clamp the remembered width against the stage actually available, or
      // opening on a narrow window crushes the canvas to a sliver. Measured
      // on a 437px stage: an unclamped 380 left 57px of canvas. The grip
      // enforces the same MIN_CANVAS while dragging; this is the same rule
      // applied to the value restored from a previous, wider session.
      var avail = wrap.getBoundingClientRect().width;
      var maxW = Math.max(MIN_W, avail - MIN_CANVAS);
      _panel.style.width = Math.min(savedWidth(), maxW) + 'px';
    } else {
      _panel = document.getElementById(PANEL_ID);
    }

    _titleEl.textContent = _label;
    _ta.value = snap.code;
    _cb.checked = snap.enabled;
    paintGutter();
    refresh();
    reflow();
    _ta.focus();
  }

  function close() {
    var wrap = document.getElementById(WRAP_ID);
    if (!wrap) return;
    // Commit before tearing down — closing the panel is not a way to discard
    // what you typed (Escape is).
    if (_ta && _ref) SMMotion.applyExprCode(_ref, _prop, _ta.value);
    var ca = canvasArea();
    if (ca && wrap.parentElement) wrap.parentElement.insertBefore(ca, wrap);
    wrap.remove();
    _panel = _ta = _gutter = _errEl = _titleEl = _cb = null;
    _ref = null; _prop = null; _label = '';
    reflow();
  }

  window.SMExprPanel = {
    open: open,
    close: close,
    isOpen: isOpen,
    isShowing: isShowing,
    refresh: refresh
  };
})();
