// ---- NEMO PANEL UI ----
//
// Lets a script put a real interface on screen: a floating, draggable panel of
// controls, styled as one of the app's own panels.
//
// The API is a BUILDER — `p.button('Go', fn)` returns the panel so calls chain
// — rather than the add(type, bounds, text, props) shape older scripting
// toolkits use. That older shape exists because those toolkits had to describe
// an absolute-pixel layout; we don't, so the vocabulary can just say what the
// control is. Every control also returns a handle through `p.last()` for the
// cases where a script needs to read or update it later.
//
// Nothing here is specific to any other application's toolkit — it is a small
// generic widget set over DOM, which is why it can ship.
(function () {
  'use strict';

  var _panels = [];

  function el(tag, cls) { var e = document.createElement(tag); if (cls) e.className = cls; return e; }

  function Panel(opts) {
    var self = this;
    this._handles = [];
    var win = el('div', 'npanel');
    var bar = el('div', 'npanel-bar');
    var ttl = el('span'); ttl.textContent = opts.title || 'Panneau';
    var close = el('button', 'npanel-close'); close.textContent = '×';
    bar.appendChild(ttl); bar.appendChild(close);
    var body = el('div', 'npanel-body');
    win.appendChild(bar); win.appendChild(body);
    if (opts.width) win.style.width = (opts.width | 0) + 'px';
    document.body.appendChild(win);
    win.style.left = (opts.x != null ? opts.x : 140) + 'px';
    win.style.top = (opts.y != null ? opts.y : 100) + 'px';

    // Draggable by its bar — a panel that can't move covers the canvas it acts on.
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
    close.addEventListener('click', function () { self.close(); });

    this._win = win; this._body = body; this._title = ttl;
    this._target = body;
    _panels.push(this);
  }

  function row(p) { var r = el('div', 'npanel-row'); p._target.appendChild(r); return r; }

  Panel.prototype.text = function (s) {
    var d = el('div', 'npanel-text'); d.textContent = s;
    this._target.appendChild(d);
    this._handles.push({ node: d, get: function () { return d.textContent; }, set: function (v) { d.textContent = v; } });
    return this;
  };
  Panel.prototype.button = function (label, fn) {
    var b = el('button', 'npanel-btn'); b.textContent = label;
    b.addEventListener('click', function () { if (fn) fn(); });
    this._target.appendChild(b);
    this._handles.push({ node: b, get: function () { return b.textContent; }, set: function (v) { b.textContent = v; } });
    return this;
  };
  Panel.prototype.field = function (label, value, fn) {
    var r = row(this);
    if (label) { var l = el('span', 'npanel-label'); l.textContent = label; r.appendChild(l); }
    var i = el('input', 'npanel-input'); i.type = 'text'; i.value = value == null ? '' : value;
    i.addEventListener('change', function () { if (fn) fn(i.value); });
    r.appendChild(i);
    this._handles.push({ node: i, get: function () { return i.value; }, set: function (v) { i.value = v; } });
    return this;
  };
  Panel.prototype.number = function (label, value, fn) {
    var r = row(this);
    if (label) { var l = el('span', 'npanel-label'); l.textContent = label; r.appendChild(l); }
    // `scrub` is this app's own convention for drag-to-change numeric fields
    // (CLAUDE.md §10) — a scripted field behaves like every built-in one.
    var i = el('input', 'npanel-input scrub'); i.type = 'number'; i.value = value == null ? 0 : value;
    i.addEventListener('change', function () { if (fn) fn(parseFloat(i.value)); });
    r.appendChild(i);
    this._handles.push({ node: i, get: function () { return parseFloat(i.value); }, set: function (v) { i.value = v; } });
    return this;
  };
  Panel.prototype.check = function (label, value, fn) {
    var lab = el('label', 'npanel-check');
    var c = el('input'); c.type = 'checkbox'; c.checked = !!value;
    c.addEventListener('change', function () { if (fn) fn(c.checked); });
    lab.appendChild(c); lab.appendChild(document.createTextNode(' ' + label));
    this._target.appendChild(lab);
    this._handles.push({ node: c, get: function () { return c.checked; }, set: function (v) { c.checked = !!v; } });
    return this;
  };
  Panel.prototype.select = function (label, items, fn) {
    var r = row(this);
    if (label) { var l = el('span', 'npanel-label'); l.textContent = label; r.appendChild(l); }
    var s = el('select', 'npanel-select');
    (items || []).forEach(function (it) { var o = el('option'); o.textContent = it; s.appendChild(o); });
    s.addEventListener('change', function () { if (fn) fn(s.value, s.selectedIndex); });
    r.appendChild(s);
    this._handles.push({ node: s, get: function () { return s.value; }, set: function (v) { s.value = v; } });
    return this;
  };
  Panel.prototype.slider = function (label, value, min, max, fn) {
    var r = row(this);
    if (label) { var l = el('span', 'npanel-label'); l.textContent = label; r.appendChild(l); }
    var s = el('input', 'npanel-slider'); s.type = 'range';
    s.min = min == null ? 0 : min; s.max = max == null ? 100 : max; s.value = value == null ? s.min : value;
    s.addEventListener('input', function () { if (fn) fn(parseFloat(s.value)); });
    r.appendChild(s);
    this._handles.push({ node: s, get: function () { return parseFloat(s.value); }, set: function (v) { s.value = v; } });
    return this;
  };
  // A titled box that following controls go into, until group(null) ends it.
  Panel.prototype.group = function (title) {
    if (title == null) { this._target = this._body; return this; }
    var g = el('div', 'npanel-group');
    var t = el('div', 'npanel-group-title'); t.textContent = title;
    g.appendChild(t);
    this._body.appendChild(g);
    this._target = g;
    return this;
  };
  Panel.prototype.last = function () { return this._handles[this._handles.length - 1]; };
  Panel.prototype.handle = function (i) { return this._handles[i]; };
  Object.defineProperty(Panel.prototype, 'title', {
    get: function () { return this._title.textContent; },
    set: function (v) { this._title.textContent = v; }
  });
  Panel.prototype.close = function () {
    if (this._win.parentNode) this._win.parentNode.removeChild(this._win);
    var i = _panels.indexOf(this); if (i >= 0) _panels.splice(i, 1);
  };

  window.SMPanelUI = {
    create: function (opts) { return new Panel(opts || {}); },
    closeAll: function () { _panels.slice().forEach(function (p) { p.close(); }); },
    openCount: function () { return _panels.length; }
  };
})();
