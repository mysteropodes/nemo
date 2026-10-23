/* Ordinary palette management consumes native readback; it owns only DOM and gestures. */
(function (root, factory) {
  var common = typeof module === 'object' && module.exports;
  var api = factory(common ? require('./native-palette-desktop.js') : root.NemoNativePaletteDesktop);
  if (common) module.exports = api;
  else { root.NemoNativePaletteUI = api; api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function (Desktop) {
  'use strict';
  var installed = new WeakMap();
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    var retired = false, loading = null, pending = false, drag = null, highlight = null;
    var projectionEpoch = 0, refreshNeeded = false, busyStamp = null, failedRead = null;
    var identityKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
    var document = root.document;
    function native() { return !!(root.__TAURI__ && root.__TAURI__.core); }
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return native() && !retired && session === current() && !!root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = Desktop.create({ currentSession: current, available: available, publish: function () { return true; } });
    function notice(message) {
      if (typeof root.showToast === 'function') root.showToast(message || 'Palette changes are not confirmed. Wait for synchronization before trying again.');
    }
    function busy() { return retired || pending || !!loading; }
    function identity() {
      try {
        var session = current(), info = session && session.info(), pin = {};
        if (!session || !info || !session.isActive()) return null;
        identityKeys.forEach(function (key) { pin[key] = info[key]; });
        return { session: session, info: info, pin: pin, epoch: projectionEpoch, ticket: session.readSubscriptionGeneration() };
      } catch (_) { return null; }
    }
    function changed(stamp, next) {
      return !stamp || stamp.session !== next.session || stamp.epoch !== next.epoch || stamp.ticket !== next.ticket ||
        identityKeys.some(function (key) { return stamp.pin[key] !== next.pin[key]; });
    }
    function sameReadOwner(stamp) {
      try {
        var next = identity();
        return !retired && !pending && !!next && !changed(stamp, next) && available(next.session, next.info);
      } catch (_) { return false; }
    }
    function readStamp() {
      var stamp = identity(), shown;
      if (!stamp || !available(stamp.session, stamp.info)) return null;
      shown = stamp.session.readPresentedObjectSession();
      var p = shown && shown.presentation;
      if (!p || p.owner !== 'native' || p.status !== 'presented' ||
          !identityKeys.every(function (key) { return shown[key] === stamp.pin[key] && p[key] === stamp.pin[key]; })) return null;
      stamp.presentation = p; return stamp;
    }
    function settled() {
      busyStamp = null;
      var refresh = refreshNeeded; refreshNeeded = false; render();
      if (refresh && !retired && !model.read()) presented();
    }
    function shown(origin) { return !busy() && !!origin && model.read() === origin; }
    function element(id) { return document.getElementById(id); }
    function button(parent, className, label) {
      var node = document.createElement('button');
      node.type = 'button'; node.className = className; node.title = label; parent.appendChild(node);
      return node;
    }
    function menu(event, items) {
      event.preventDefault();
      if (typeof root.showContextMenu === 'function') root.showContextMenu(event.clientX, event.clientY, items);
    }
    async function edit(action, origin) {
      if (!shown(origin)) { notice(); return { status: 'unavailable' }; }
      busyStamp = identity(); pending = true; drag = null; render();
      try {
        var result = await model.edit(action, origin);
        if (!['applied-presented', 'unchanged'].includes(result.status)) notice();
        return result;
      } catch (error) {
        notice(); return { status: 'unconfirmed', code: error && error.code || 'unavailable' };
      } finally { pending = false; settled(); }
    }
    function id() { return 'p' + Date.now().toString(36) + Math.floor(Math.random() * 1e4); }
    function create() {
      var origin = model.read();
      if (!shown(origin)) { notice(); return; }
      var name = root.prompt('Nom de la nouvelle palette :', 'Palette ' + (origin.palette.palettes.length + 1));
      if (name === null) return;
      return edit({ kind: 'create', id: id(), name: name.trim() || 'Palette' }, origin);
    }
    function rename(summary, origin) {
      if (!shown(origin)) { notice(); return; }
      var name = root.prompt('Renommer la palette :', summary.name);
      if (name === null || !name.trim()) return;
      return edit({ kind: 'rename', ordinal: summary.ordinal, name: name.trim() }, origin);
    }
    function duplicate(summary, origin) {
      return edit({ kind: 'duplicate', ordinal: summary.ordinal, id: id(), name: summary.name + ' copie' }, origin);
    }
    function remove(summary, origin) {
      if (!shown(origin)) { notice(); return; }
      if (origin.palette.palettes.length <= 1) { notice('Keep at least one palette.'); return; }
      return edit({ kind: 'remove', ordinal: summary.ordinal }, origin);
    }
    function append(color) {
      var origin = model.read(), active = origin && origin.palette.active;
      if (!shown(origin) || !active) { notice(); return; }
      if (active.colors.some(function (item) { return item.color === color; })) {
        notice(root.SM && root.SM.t ? root.SM.t('toastAlreadyInPalette') : 'Color is already in this palette.'); return;
      }
      return edit({ kind: 'appendColor', ordinal: active.summary.ordinal, color: color }, origin);
    }
    function tabs(parent, origin) {
      var active = origin.palette.active;
      origin.palette.palettes.forEach(function (summary) {
        var selected = !!active && active.summary.ordinal === summary.ordinal;
        var node = button(parent, 'sym-tab' + (selected ? ' act' : ''), summary.name);
        node.textContent = summary.name; node.setAttribute('aria-pressed', String(selected));
        node.addEventListener('click', function () { if (!selected) edit({ kind: 'select', ordinal: summary.ordinal }, origin); });
        node.addEventListener('dblclick', function () { rename(summary, origin); });
        node.addEventListener('contextmenu', function (event) {
          menu(event, [
            { label: 'Renommer…', action: function () { rename(summary, origin); } },
            { label: 'Dupliquer', action: function () { duplicate(summary, origin); } },
            { sep: true },
            { label: 'Supprimer', disabled: origin.palette.palettes.length <= 1, action: function () { remove(summary, origin); } }
          ]);
        });
      });
    }
    function colors(parent, origin) {
      var active = origin.palette.active;
      if (!active) { parent.textContent = 'Select a palette, or create one.'; return; }
      active.colors.forEach(function (item) {
        var selected = typeof highlight === 'string' && highlight.toLowerCase() === item.color.toLowerCase();
        var node = button(parent, 'palette-swatch' + (selected ? ' sel-match' : ''), item.color);
        node.style.setProperty('--sw-color', item.color); node.setAttribute('aria-label', item.color);
        // Applying/replacing object colors requires its own admitted native consumer.
        node.addEventListener('click', function () { notice('Applying palette colors is unavailable.'); });
        node.addEventListener('contextmenu', function (event) {
          menu(event, [{ label: 'Retirer de la palette', action: function () {
            edit({ kind: 'removeColor', ordinal: active.summary.ordinal, index: item.index }, origin);
          } }]);
        });
        node.draggable = true;
        node.addEventListener('dragstart', function (event) {
          if (!shown(origin)) { event.preventDefault(); return; }
          drag = { origin: origin, ordinal: active.summary.ordinal, index: item.index };
          event.dataTransfer.effectAllowed = 'move';
        });
        node.addEventListener('dragover', function (event) { event.preventDefault(); event.dataTransfer.dropEffect = 'move'; });
        node.addEventListener('drop', function (event) {
          event.preventDefault(); var gesture = drag; drag = null;
          if (!gesture || gesture.origin !== origin || !shown(origin)) { notice(); return; }
          if (gesture.index !== item.index) edit({ kind: 'moveColor', ordinal: gesture.ordinal, from: gesture.index, to: item.index }, origin);
        });
        node.addEventListener('dragend', function () { drag = null; });
      });
    }
    function render(hex) {
      if (!native()) return;
      if (arguments.length) highlight = hex;
      var grid = element('palette-grid'), wrap = element('palette-tabs'), origin = model.read();
      var ready = shown(origin), active = ready && origin.palette.active;
      ['btn-palette-new', 'btn-palette-add-fill', 'btn-palette-add-stroke'].forEach(function (key) {
        var node = element(key); if (node) node.disabled = !ready || key !== 'btn-palette-new' && !active;
      });
      ['btn-palette-replace', 'btn-palette-swap', 'palette-replace-scope'].forEach(function (key) {
        var node = element(key); if (node) node.disabled = true;
      });
      if (wrap) { wrap.textContent = ''; if (ready) tabs(wrap, origin); }
      if (grid) {
        grid.textContent = ''; grid.setAttribute('role', ready ? 'group' : 'status');
        if (ready) colors(grid, origin);
        else grid.textContent = pending || loading ? 'Synchronizing palettes…' : 'Palette editing is unavailable.';
      }
    }
    function presented() {
      if (!native() || retired) return Promise.resolve(null);
      if (pending || loading) {
        var next = identity();
        if (next && changed(busyStamp, next) && available(next.session, next.info)) refreshNeeded = true;
        return loading || Promise.resolve(null);
      }
      var stamp;
      try { stamp = readStamp(); } catch (_) {}
      // A different queued resource's public fence is not a fresh read attempt.
      if (!stamp) return Promise.resolve(null);
      var value = model.read(); if (value) return Promise.resolve(value);
      if (failedRead && sameReadOwner(failedRead) && failedRead.presentation === stamp.presentation) return Promise.resolve(null);
      failedRead = null; busyStamp = stamp;
      loading = model.refresh().then(function (value) {
        if (sameReadOwner(stamp)) failedRead = null;
        return value;
      }).catch(function () {
        try { if (sameReadOwner(stamp)) failedRead = stamp; } catch (_) {}
        return null;
      }).finally(function () { loading = null; settled(); });
      render(); return loading;
    }
    var controller = Object.freeze({ read: model.read, edit: edit, create: create, append: append, render: render,
      presented: presented, isPending: function () { return pending || !!loading; },
      willProject: function () { projectionEpoch++; failedRead = null; drag = null; if (!pending) model.invalidate(); render(); },
      dispose: function () { retired = true; failedRead = null; refreshNeeded = false; drag = null; model.dispose(); render(); }
    });
    installed.set(root, controller); root.NemoNativePaletteController = controller;
    if (typeof root.addEventListener === 'function') root.addEventListener('pagehide', controller.dispose, { once: true });
    render(); return controller;
  }
  return Object.freeze({ install: install });
}));
