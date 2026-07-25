// ---- SCRIPTUI ----
//
// Most tools on aescripts are not "a script" — they are a PALETTE: a
// ScriptUI window with buttons that call into the AE DOM. aescript-host.js
// answers the DOM half; without this half those tools open nothing, so the
// bridge would only ever serve the minority of scripts that have no interface.
//
// This maps ScriptUI onto real DOM. The script still runs unmodified: it calls
// `new Window("palette", "…")`, `.add("button", undefined, "Go")`,
// `btn.onClick = …`, `w.show()` — and gets a real floating panel whose buttons
// really fire.
//
// THE ONE STRUCTURAL DIFFERENCE, stated rather than hidden: ScriptUI lays out
// with `bounds`/`preferredSize` in absolute pixels and a "lay out once, then
// freeze" model. Reproducing that faithfully would mean reimplementing its
// layout manager. Instead the bounds arguments are read for SIZE where they
// carry one and otherwise ignored, and containers become flexbox — which is
// what "group" (row) and "panel" (column, titled) already mean in every
// palette. Scripts that hand-place controls by absolute bounds will look
// different here; scripts that use groups and panels, which is the overwhelming
// majority, look right.
(function () {
  'use strict';

  var _open = [];

  function el(tag, cls, css) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (css) e.style.cssText = css;
    return e;
  }
  // ScriptUI passes bounds/size as [x,y,w,h] or [w,h], often `undefined`.
  // Only the size half is ever meaningful here (see the header).
  function sizeFrom(b) {
    if (!b || !b.length) return null;
    if (b.length >= 4) return { w: b[2] - b[0], h: b[3] - b[1] };
    if (b.length === 2) return { w: b[0], h: b[1] };
    return null;
  }

  // Every control shares this: ScriptUI exposes `.text`, `.enabled`,
  // `.visible`, `.preferredSize`, and containers expose `.add`. Properties are
  // real accessors so `btn.text = "…"` updates the DOM, which palettes do
  // constantly to reflect state.
  function decorate(obj, node) {
    obj._node = node;
    Object.defineProperty(obj, 'enabled', {
      get: function () { return !node.disabled; },
      set: function (v) { node.disabled = !v; node.style.opacity = v ? '' : '.45'; }
    });
    Object.defineProperty(obj, 'visible', {
      get: function () { return node.style.display !== 'none'; },
      set: function (v) { node.style.display = v ? '' : 'none'; }
    });
    Object.defineProperty(obj, 'preferredSize', {
      get: function () { return [node.offsetWidth, node.offsetHeight]; },
      set: function (v) { var s = sizeFrom(v); if (s) { if (s.w) node.style.width = s.w + 'px'; if (s.h) node.style.height = s.h + 'px'; } }
    });
    // Called by nearly every palette; ScriptUI needs it, we don't.
    obj.layout = { layout: function () {}, resize: function () {} };
    obj.onDraw = null;
    return obj;
  }

  function makeControl(type, parentNode, bounds, text, props) {
    type = String(type || '').toLowerCase();
    props = props || {};
    var o = {}, node;

    if (type === 'button') {
      node = el('button', 'sui-btn');
      node.textContent = text || '';
      node.addEventListener('click', function () { if (typeof o.onClick === 'function') o.onClick(); });
      Object.defineProperty(o, 'text', { get: function () { return node.textContent; }, set: function (v) { node.textContent = v; } });

    } else if (type === 'statictext') {
      node = el('div', 'sui-text');
      node.textContent = text || '';
      Object.defineProperty(o, 'text', { get: function () { return node.textContent; }, set: function (v) { node.textContent = v; } });

    } else if (type === 'edittext') {
      node = el('input', 'sui-input');
      node.type = 'text'; node.value = text || '';
      node.addEventListener('change', function () { if (typeof o.onChange === 'function') o.onChange(); });
      node.addEventListener('input', function () { if (typeof o.onChanging === 'function') o.onChanging(); });
      Object.defineProperty(o, 'text', { get: function () { return node.value; }, set: function (v) { node.value = v; } });

    } else if (type === 'checkbox') {
      node = el('label', 'sui-check');
      var cb = el('input'); cb.type = 'checkbox';
      node.appendChild(cb); node.appendChild(document.createTextNode(' ' + (text || '')));
      cb.addEventListener('change', function () { if (typeof o.onClick === 'function') o.onClick(); });
      Object.defineProperty(o, 'value', { get: function () { return cb.checked; }, set: function (v) { cb.checked = !!v; } });
      Object.defineProperty(o, 'text', { get: function () { return text; }, set: function (v) { text = v; } });

    } else if (type === 'radiobutton') {
      node = el('label', 'sui-check');
      var rb = el('input'); rb.type = 'radio'; rb.name = 'sui-radio-' + (parentNode._radioGroup || (parentNode._radioGroup = 'g' + Math.floor(Math.random() * 1e6)));
      node.appendChild(rb); node.appendChild(document.createTextNode(' ' + (text || '')));
      rb.addEventListener('change', function () { if (typeof o.onClick === 'function') o.onClick(); });
      Object.defineProperty(o, 'value', { get: function () { return rb.checked; }, set: function (v) { rb.checked = !!v; } });

    } else if (type === 'dropdownlist' || type === 'listbox') {
      node = el('select', 'sui-select');
      (props.items || []).forEach(function (it) { var op = el('option'); op.textContent = it; node.appendChild(op); });
      node.addEventListener('change', function () { if (typeof o.onChange === 'function') o.onChange(); });
      // ScriptUI's `selection` is an ITEM object with .index/.text, and
      // assigning an integer selects that index — both forms are used in the
      // wild, so both are supported.
      Object.defineProperty(o, 'selection', {
        get: function () {
          var i = node.selectedIndex;
          return i < 0 ? null : { index: i, text: node.options[i].textContent };
        },
        set: function (v) { node.selectedIndex = (v && typeof v === 'object') ? v.index : (v | 0); }
      });
      o.add = function (kind, txt) { var op = el('option'); op.textContent = txt; node.appendChild(op); return { index: node.options.length - 1, text: txt }; };
      Object.defineProperty(o, 'items', { get: function () { return Array.prototype.map.call(node.options, function (op, i) { return { index: i, text: op.textContent }; }); } });

    } else if (type === 'slider') {
      node = el('input', 'sui-slider');
      node.type = 'range';
      node.min = (props.minvalue != null ? props.minvalue : 0);
      node.max = (props.maxvalue != null ? props.maxvalue : 100);
      node.value = (text != null ? text : node.min);
      node.addEventListener('input', function () { if (typeof o.onChanging === 'function') o.onChanging(); });
      node.addEventListener('change', function () { if (typeof o.onChange === 'function') o.onChange(); });
      Object.defineProperty(o, 'value', { get: function () { return parseFloat(node.value); }, set: function (v) { node.value = v; } });

    } else if (type === 'progressbar') {
      node = el('progress', 'sui-progress');
      node.max = 100; node.value = 0;
      Object.defineProperty(o, 'value', { get: function () { return node.value; }, set: function (v) { node.value = v; } });

    } else if (type === 'group' || type === 'panel') {
      node = el('div', type === 'panel' ? 'sui-panel' : 'sui-group');
      if (type === 'panel' && text) { var t = el('div', 'sui-panel-title'); t.textContent = text; node.appendChild(t); }
      makeContainer(o, node);

    } else {
      // Same rule as the DOM half: name what was asked for rather than
      // silently producing nothing.
      throw new Error('Non supporté par ScriptUI : contrôle "' + type + '"');
    }

    var s = sizeFrom(bounds);
    if (s) { if (s.w) node.style.width = s.w + 'px'; if (s.h) node.style.height = s.h + 'px'; }
    parentNode.appendChild(node);
    return decorate(o, node);
  }

  function makeContainer(obj, node) {
    obj.children = [];
    obj.add = function (type, bounds, text, props) {
      var c = makeControl(type, node, bounds, text, props);
      obj.children.push(c);
      return c;
    };
    obj.remove = function (c) {
      if (c && c._node && c._node.parentNode) c._node.parentNode.removeChild(c._node);
    };
    // `orientation` decides row vs column — palettes set it constantly and a
    // palette laid out the wrong way is unusable even if every control works.
    Object.defineProperty(obj, 'orientation', {
      get: function () { return node.style.flexDirection === 'row' ? 'row' : 'column'; },
      set: function (v) { node.style.flexDirection = (v === 'row') ? 'row' : 'column'; }
    });
    Object.defineProperty(obj, 'alignChildren', {
      get: function () { return node.style.alignItems; },
      set: function (v) { node.style.alignItems = (String(v).indexOf('fill') >= 0) ? 'stretch' : 'flex-start'; }
    });
    Object.defineProperty(obj, 'spacing', {
      get: function () { return parseInt(node.style.gap) || 6; },
      set: function (v) { node.style.gap = (v | 0) + 'px'; }
    });
    Object.defineProperty(obj, 'margins', {
      get: function () { return parseInt(node.style.padding) || 8; },
      set: function (v) { node.style.padding = ((typeof v === 'number' ? v : 8) | 0) + 'px'; }
    });
    return obj;
  }

  // ---- Window ----
  function SUIWindow(type, title, bounds, props) {
    if (!(this instanceof SUIWindow)) return new SUIWindow(type, title, bounds, props);
    var self = this;
    var win = el('div', 'sui-window');
    var bar = el('div', 'sui-titlebar');
    var ttl = el('span'); ttl.textContent = title || 'Script';
    var close = el('button', 'sui-close'); close.textContent = '×';
    close.addEventListener('click', function () { self.close(); });
    bar.appendChild(ttl); bar.appendChild(close);
    var body = el('div', 'sui-body');
    win.appendChild(bar); win.appendChild(body);

    // Draggable by its title bar. A palette that can't be moved covers the
    // canvas it is meant to act on.
    var drag = null;
    bar.addEventListener('mousedown', function (e) {
      if (e.target === close) return;
      drag = { x: e.clientX - win.offsetLeft, y: e.clientY - win.offsetTop };
      e.preventDefault();
    });
    document.addEventListener('mousemove', function (e) {
      if (!drag) return;
      win.style.left = Math.max(0, e.clientX - drag.x) + 'px';
      win.style.top = Math.max(0, e.clientY - drag.y) + 'px';
    });
    document.addEventListener('mouseup', function () { drag = null; });

    this._win = win;
    this._mounted = false;
    makeContainer(this, body);
    decorate(this, win);
    Object.defineProperty(this, 'text', { get: function () { return ttl.textContent; }, set: function (v) { ttl.textContent = v; } });

    var s = sizeFrom(bounds);
    if (s && s.w) win.style.width = s.w + 'px';

    this.show = function () {
      if (!self._mounted) { document.body.appendChild(win); self._mounted = true; _open.push(self); }
      win.style.display = '';
      if (!win.style.left) { win.style.left = '120px'; win.style.top = '90px'; }
      return self;
    };
    this.hide = function () { win.style.display = 'none'; };
    this.close = function () {
      if (typeof self.onClose === 'function') { try { self.onClose(); } catch (e) {} }
      if (win.parentNode) win.parentNode.removeChild(win);
      self._mounted = false;
      var i = _open.indexOf(self); if (i >= 0) _open.splice(i, 1);
    };
    this.center = function () {};
    this.update = function () {};
  }

  function closeAll() { _open.slice().forEach(function (w) { w.close(); }); }

  window.SMScriptUI = {
    Window: SUIWindow,
    // A Panel constructed standalone is rare but legal; palettes mostly use
    // win.add("panel", …), which goes through makeControl above.
    Panel: function (parent, bounds, text) { throw new Error('Non supporté par ScriptUI : Panel autonome (utiliser win.add("panel"))'); },
    closeAll: closeAll,
    openCount: function () { return _open.length; }
  };
})();
