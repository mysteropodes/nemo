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
  var _panel = null, _ta = null, _gutter = null, _errEl = null, _titleEl = null, _cb = null, _hl = null;
  var _menuEl = null, _menuKind = null, _menuExpandedCat = null;

  function canvasArea() { return document.getElementById('canvas-area'); }

  // ---- accordion dropdown, shared by Examples and Functions -------------
  // A two-level dropdown (category -> item), reusing showContextMenu's
  // visual language (.ctx-menu/.ctx-item) but built by hand: the shared
  // context-menu system is flat by design (see openExprControlsMenu's own
  // comment, motion.js — "showContextMenu has no real submenus"), and an
  // accordion (click a category to expand it in place) fits this panel's
  // existing vocabulary better than a second flyout mechanism anyway — the
  // Layer Properties panel right next to this one is itself one big
  // accordion of sections. ONE builder underneath both buttons: Examples
  // (window.SM_EXPR_EXAMPLES, expr-examples.js — complete recipes) and
  // Functions (window.SM_EXPR_FUNCTIONS, expr-functions.js — the atomic
  // building blocks those recipes are made of), same interaction, same look,
  // different data and a different insert behavior (a whole recipe gets
  // blank lines around it; a single function call is dropped inline).
  function closeMenu() {
    if (_menuEl) { _menuEl.remove(); _menuEl = null; }
    _menuKind = null;
  }
  function insertBlock(code) {
    if (!_ta) return;
    var s = _ta.selectionStart, en = _ta.selectionEnd;
    var before = _ta.value.slice(0, s), after = _ta.value.slice(en);
    // A blank editor gets the snippet with no extra ceremony; inserting into
    // existing code gets blank lines around it so it doesn't run into
    // whatever was already there.
    var sep = before.trim() ? (before.endsWith('\n\n') ? '' : (before.endsWith('\n') ? '\n' : '\n\n')) : '';
    var insert = sep + code + '\n';
    _ta.value = before + insert + after;
    var caret = (before + insert).length;
    _ta.selectionStart = _ta.selectionEnd = caret;
    afterProgrammaticEdit();
    if (window.showToast) showToast(SM.t('toastExprExampleInserted'));
  }
  function insertInline(text) {
    if (!_ta) return;
    var s = _ta.selectionStart, en = _ta.selectionEnd;
    _ta.value = _ta.value.slice(0, s) + text + _ta.value.slice(en);
    _ta.selectionStart = _ta.selectionEnd = s + text.length;
    afterProgrammaticEdit();
  }
  function afterProgrammaticEdit() {
    paintGutter();
    commit();
    _ta.focus();
  }
  function buildMenu(anchorBtn, kind, cats, getItems, getItemLabel, getItemTitle, onPick) {
    var menu = document.createElement('div');
    menu.className = 'ctx-menu ecp-examples-menu';
    cats.forEach(function (cat) {
      var hdr = document.createElement('div');
      hdr.className = 'ctx-item ecp-examples-cat-hdr';
      var expanded = _menuExpandedCat === cat.id;
      var arrow = document.createElement('span');
      arrow.className = 'lico larrow';
      arrow.textContent = expanded ? '▾' : '▸';
      var lbl = document.createElement('span');
      lbl.textContent = cat.label;
      lbl.style.flex = '1';
      hdr.appendChild(arrow); hdr.appendChild(lbl);
      hdr.addEventListener('click', function (e) {
        e.stopPropagation();
        _menuExpandedCat = expanded ? null : cat.id;
        openMenu(anchorBtn, kind, cats, getItems, getItemLabel, getItemTitle, onPick);
      });
      menu.appendChild(hdr);
      if (!expanded) return;
      getItems(cat).forEach(function (item) {
        var row = document.createElement('div');
        row.className = 'ctx-item ecp-examples-item';
        row.title = getItemTitle(item) || '';
        var rlbl = document.createElement('span');
        rlbl.textContent = getItemLabel(item);
        row.appendChild(rlbl);
        row.addEventListener('click', function (e) {
          e.stopPropagation();
          closeMenu();
          onPick(item);
        });
        menu.appendChild(row);
      });
    });
    if (!cats.length) {
      var empty = document.createElement('div');
      empty.className = 'ctx-item disabled';
      empty.textContent = '—';
      menu.appendChild(empty);
    }
    return menu;
  }
  function positionMenu(anchorBtn) {
    if (!_menuEl) return;
    document.body.appendChild(_menuEl); // measure at natural size first
    var r = anchorBtn.getBoundingClientRect();
    var mw = _menuEl.offsetWidth, mh = _menuEl.offsetHeight;
    _menuEl.style.position = 'fixed';
    _menuEl.style.left = Math.min(r.left, window.innerWidth - mw - 4) + 'px';
    _menuEl.style.top = Math.min(r.bottom + 4, window.innerHeight - mh - 4) + 'px';
  }
  function openMenu(anchorBtn, kind, cats, getItems, getItemLabel, getItemTitle, onPick) {
    if (_menuEl) _menuEl.remove();
    _menuKind = kind;
    _menuEl = buildMenu(anchorBtn, kind, cats, getItems, getItemLabel, getItemTitle, onPick);
    positionMenu(anchorBtn);
  }
  function toggleMenu(anchorBtn, kind, cats, getItems, getItemLabel, getItemTitle, onPick) {
    // Same button toggles closed; the OTHER button switches menus (closing
    // one and opening the other) rather than stacking two dropdowns.
    if (_menuEl && _menuKind === kind) { closeMenu(); return; }
    _menuExpandedCat = null;
    openMenu(anchorBtn, kind, cats, getItems, getItemLabel, getItemTitle, onPick);
    // Dismiss on an outside click, one tick later so THIS click (the one
    // that opened the menu) doesn't immediately close it again.
    setTimeout(function () {
      document.addEventListener('mousedown', function dismiss(e) {
        if (_menuEl && !_menuEl.contains(e.target) && e.target !== anchorBtn) {
          closeMenu();
          document.removeEventListener('mousedown', dismiss);
        } else if (!_menuEl) {
          document.removeEventListener('mousedown', dismiss);
        }
      });
    }, 0);
  }
  function toggleExamplesMenu(anchorBtn) {
    toggleMenu(anchorBtn, 'examples', window.SM_EXPR_EXAMPLES || [],
      function (cat) { return cat.examples; },
      function (ex) { return ex.label; },
      function (ex) { return ex.source; },
      function (ex) { insertBlock(ex.code); });
  }
  function toggleFunctionsMenu(anchorBtn) {
    toggleMenu(anchorBtn, 'functions', window.SM_EXPR_FUNCTIONS || [],
      function (cat) { return cat.fns; },
      function (fn) { return fn.name; },
      function (fn) { return fn.doc; },
      function (fn) { insertInline(fn.insert); });
  }

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

  // ---- syntax highlighting (2026-09-01) ----------------------------------
  // Cyril, comparing to aescripts' expressCode/Expressionist: "de manière
  // plus pro, couleur". A <textarea> can't color its own text, so this is
  // the standard trick those two also use under the hood for anything short
  // of a full Monaco/CodeMirror embed: an identically-sized, identically-
  // fonted <pre> sits BEHIND the textarea; the textarea's own text is made
  // transparent (color:transparent, caret-color kept real) so only its
  // native cursor/selection show, while the colored HTML underneath reads
  // through. One regex pass, ordered so longer/more specific matches (a
  // comment, a string) win over a bare identifier that happens to appear
  // inside them. Nemo's own public vocabulary (EXPR_PUBLIC_NAMES, motion.js)
  // gets its own color — never the undocumented AE aliases, same boundary
  // expr-examples.js/expr-functions.js already draw, so the highlighting
  // itself teaches which names are the "real" documented API.
  var HL_BUILTINS = ['time', 'frame', 'value', 'layer', 'self', 'comp', 'marker',
    'wiggle', 'noise', 'random', 'randomFixed', 'randomGauss', 'randomGaussFixed', 'seed',
    'clamp', 'remap', 'remapEase', 'remapEaseIn', 'remapEaseOut', 'degrees', 'radians',
    'add', 'sub', 'mul', 'div', 'length', 'normalize', 'dot', 'cross', 'angleTo',
    'stepTime', 'loopAfter', 'loopBefore', 'toFrames', 'toSeconds', 'contentBox',
    'control', 'layerControl'];
  var HL_KEYWORDS = ['var', 'let', 'const', 'if', 'else', 'return', 'function', 'true', 'false',
    'null', 'undefined', 'new', 'for', 'while', 'typeof', 'in', 'of', 'this', 'Math'];
  var HL_RE = new RegExp(
    '(\\/\\/[^\\n]*)' + // 1 line comment
    '|(\\/\\*[\\s\\S]*?\\*\\/)' + // 2 block comment
    "|('(?:[^'\\\\\\n]|\\\\.)*'|\"(?:[^\"\\\\\\n]|\\\\.)*\")" + // 3 string
    '|(\\b\\d+\\.?\\d*\\b)' + // 4 number
    '|(\\b(?:' + HL_KEYWORDS.join('|') + ')\\b)' + // 5 keyword
    '|(\\b(?:' + HL_BUILTINS.join('|') + ')\\b)', // 6 Nemo builtin
    'g');
  function escapeHtml(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }
  function highlightLine(line) {
    if (!line) return '';
    var out = '', last = 0;
    HL_RE.lastIndex = 0;
    var m;
    while ((m = HL_RE.exec(line))) {
      out += escapeHtml(line.slice(last, m.index));
      var cls = m[1] ? 'tok-com' : m[2] ? 'tok-com' : m[3] ? 'tok-str' : m[4] ? 'tok-num' : m[5] ? 'tok-kw' : 'tok-fn';
      out += '<span class="' + cls + '">' + escapeHtml(m[0]) + '</span>';
      last = m.index + m[0].length;
    }
    out += escapeHtml(line.slice(last));
    return out;
  }
  function paintGutter() {
    if (!_gutter || !_ta) return;
    var lines = _ta.value.split('\n');
    var errLine = 0;
    if (_ref && _prop && window.SMMotion && SMMotion.exprSnapshotFor) {
      var snap = SMMotion.exprSnapshotFor(_ref, _prop);
      if (snap && snap.lastError && snap.errorLine > 0) errLine = snap.errorLine;
    }
    var gutterHtml = '', codeHtml = '';
    for (var i = 0; i < lines.length; i++) {
      var isErr = (i + 1) === errLine;
      gutterHtml += '<div class="ecp-line' + (isErr ? ' err' : '') + '">' + (i + 1) + '</div>';
      codeHtml += '<div class="ecp-line' + (isErr ? ' err' : '') + '">' + (highlightLine(lines[i]) || '&nbsp;') + '</div>';
    }
    _gutter.innerHTML = gutterHtml;
    _gutter.scrollTop = _ta.scrollTop;
    if (_hl) {
      _hl.innerHTML = codeHtml;
      _hl.scrollTop = _ta.scrollTop;
      _hl.scrollLeft = _ta.scrollLeft;
    }
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
    if (document.activeElement !== _ta && _ta.value !== snap.code) _ta.value = snap.code;
    if (_cb) _cb.checked = snap.enabled;
    if (_errEl) {
      _errEl.textContent = snap.lastError || '';
      _errEl.style.display = snap.lastError ? '' : 'none';
    }
    // Repaint unconditionally, not just when the text changed: errorLine can
    // move (or clear) between calls — a frame change re-evaluating the same
    // unchanged code, or this very commit() finding/losing an error — and
    // the inline error-line highlight (paintGutter, below) must track that.
    paintGutter();
  }

  function build() {
    var panel = document.createElement('div');
    panel.id = PANEL_ID;

    var head = document.createElement('div');
    head.className = 'ecp-head';
    _titleEl = document.createElement('div');
    _titleEl.className = 'ecp-title';
    head.appendChild(_titleEl);
    var fnBtn = document.createElement('button');
    fnBtn.className = 'ecp-close ecp-examples-btn';
    fnBtn.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3 3 12l5 9M16 3l5 9-5 9M14 3l-4 18"/></svg>';
    fnBtn.title = SM.t('titleExprFunctions');
    fnBtn.addEventListener('click', function (e) { e.stopPropagation(); toggleFunctionsMenu(fnBtn); });
    head.appendChild(fnBtn);
    var examplesBtn = document.createElement('button');
    examplesBtn.className = 'ecp-close ecp-examples-btn';
    examplesBtn.innerHTML = '<svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4.5A2.5 2.5 0 0 1 6.5 2H20v18H6.5A2.5 2.5 0 0 0 4 22.5v-18Z"/><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/></svg>';
    examplesBtn.title = SM.t('titleExprExamples');
    examplesBtn.addEventListener('click', function (e) { e.stopPropagation(); toggleExamplesMenu(examplesBtn); });
    head.appendChild(examplesBtn);
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
    var codeWrap = document.createElement('div');
    codeWrap.className = 'ecp-code-wrap';
    _hl = document.createElement('pre');
    _hl.className = 'ecp-highlight';
    _ta = document.createElement('textarea');
    _ta.className = 'ecp-code';
    _ta.spellcheck = false;
    codeWrap.appendChild(_hl);
    codeWrap.appendChild(_ta);
    pane.appendChild(_gutter);
    pane.appendChild(codeWrap);
    panel.appendChild(pane);

    _errEl = document.createElement('div');
    _errEl.className = 'ecp-err';
    _errEl.style.display = 'none';
    panel.appendChild(_errEl);

    _ta.addEventListener('scroll', function () {
      _gutter.scrollTop = _ta.scrollTop;
      _hl.scrollTop = _ta.scrollTop;
      _hl.scrollLeft = _ta.scrollLeft;
    });
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
    closeMenu();
    var wrap = document.getElementById(WRAP_ID);
    if (!wrap) return;
    // Commit before tearing down — closing the panel is not a way to discard
    // what you typed (Escape is).
    if (_ta && _ref) SMMotion.applyExprCode(_ref, _prop, _ta.value);
    var ca = canvasArea();
    if (ca && wrap.parentElement) wrap.parentElement.insertBefore(ca, wrap);
    wrap.remove();
    _panel = _ta = _gutter = _errEl = _titleEl = _cb = _hl = null;
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
