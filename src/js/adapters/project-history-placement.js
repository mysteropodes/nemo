/* Temporary History UI column; document/history authority stays in Rust. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoProjectHistoryPlacement = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function plan(bounds) {
    if (!bounds || !Number.isFinite(bounds.available) || bounds.available < 396 ||
        !Number.isFinite(bounds.height) || bounds.height < 200) return null;
    return { width: Math.min(340, bounds.available - 176) };
  }

  function create(ports) {
    var generation = 0, active = false, ready = false, busy = false, context = null;
    function current(token) { return active && token === generation; }
    function release() {
      generation++; active = ready = false;
      ports.enable(false); ports.hide(); ports.release();
    }
    async function refresh() {
      if (!active || busy) return false;
      var token = ++generation;
      ready = false; ports.enable(false);
      try {
        var layout = plan(ports.measure());
        if (!layout) throw new Error('Widen the window to view version History.');
        if (ports.reserve(layout.width) !== true) throw new Error('History needs a visible column and a positive viewport.');
        context = ports.context(); ports.show(); ports.loading();
        await ports.present();
        if (!current(token)) return false;
        if (ports.context() !== context) throw new Error('The document changed while opening History.');
        var rows = await ports.list();
        if (!current(token)) return false;
        if (ports.context() !== context) throw new Error('The document changed while listing History.');
        ports.render(rows); ready = true; ports.enable(true); return true;
      } catch (error) {
        if (current(token)) { release(); ports.notice(error.message); }
        return false;
      }
    }
    async function close() {
      if (busy) return false;
      if (!active) return true;
      release();
      try { await ports.present(); } catch (error) { ports.notice(error.message); }
      return true;
    }
    async function restore(path) {
      if (!active || !ready || busy) return false;
      if (ports.context() !== context) { await close(); ports.notice('The document changed; reopen History.'); return false; }
      busy = true; ready = false; ports.enable(false);
      var restored = false;
      try { restored = await ports.restore(path) === true; }
      catch (error) { ports.notice(error.message); }
      finally { busy = false; }
      if (restored) await close();
      else if (active) await refresh();
      return restored;
    }
    return Object.freeze({ open: function () {
      if (busy) return Promise.resolve(false);
      active = true; return refresh();
    }, close: close, refresh: refresh, restore: restore });
  }

  function boundary(root) {
    return new Promise(function (resolve) {
      var finished = false, timer = root.setTimeout(done, 200);
      function done() { if (!finished) { finished = true; root.clearTimeout(timer); resolve(); } }
      root.requestAnimationFrame(function () { root.requestAnimationFrame(done); });
    });
  }

  function bind(root, ui) {
    var document = root.document, modal = document.getElementById('history-modal');
    var top = document.getElementById('top-area'), canvas = document.getElementById('canvas-area');
    var props = document.getElementById('props-panel'), list = document.getElementById('history-list');
    var box = modal.querySelector('.modal-box'), closeButton = document.getElementById('history-close');
    var spacer = null, savedStyle = null, controls = [], observed = false, pending = false, geometry = '';
    function dimensions() {
      var area = canvas.getBoundingClientRect(), right = (spacer || props).getBoundingClientRect();
      return { available: area.width + right.width, height: top.getBoundingClientRect().height };
    }
    function notifyResize() { root.dispatchEvent(new root.Event('resize')); }
    function position() {
      var rect = spacer.getBoundingClientRect();
      box.style.left = (rect.left + 8) + 'px'; box.style.top = (rect.top + 8) + 'px';
      box.style.width = (rect.width - 16) + 'px'; box.style.maxHeight = (rect.height - 16) + 'px';
      geometry = [rect.left, rect.top, rect.width, rect.height].join('/');
      return canvas.getBoundingClientRect().width >= 160 && rect.width >= 220 &&
        rect.left >= 0 && rect.right <= root.innerWidth && rect.height >= 200;
    }
    async function present() {
      notifyResize(); await boundary(root);
      var native = root.NemoNativeOpacityCutover;
      if (!root.NemoNativeOpacityProject) return;
      if (!native || !native.isActive()) throw new Error('The native document is unavailable.');
      var pin = native.identity(), view = native.getNativeIdentity(), frame = root.state.currentFrame;
      var receipt = await native.presentPreview(frame), latest = native.identity(), latestView = native.getNativeIdentity();
      if (!pin || !latest || !view || !latestView || !receipt || receipt.owner !== 'native' ||
          receipt.status !== 'presented' || receipt.frame !== frame ||
          !Number.isSafeInteger(receipt.lifecycleGeneration) || receipt.lifecycleGeneration < 1 ||
          receipt.lifecycleGeneration !== view.generation || latestView.generation !== view.generation ||
          view.documentId !== pin.documentId || latestView.documentId !== pin.documentId ||
          !Number.isSafeInteger(receipt.viewGeneration) || receipt.viewGeneration < 1 ||
          typeof receipt.workId !== 'string' || !receipt.workId.trim() ||
          receipt.instanceId !== pin.instanceId || receipt.documentId !== pin.documentId ||
          receipt.contentRevision !== pin.contentRevision || latest.instanceId !== pin.instanceId ||
          latest.documentId !== pin.documentId || latest.contentRevision !== pin.contentRevision ||
          root.state.currentFrame !== frame) throw new Error('Could not present the current document after resizing History.');
    }
    var controller = create({
      measure: dimensions,
      reserve: function (width) {
        if (!spacer) {
          savedStyle = ['left', 'top', 'width', 'max-height'].map(function (key) {
            return [key, box.style.getPropertyValue(key), box.style.getPropertyPriority(key)];
          });
          spacer = document.createElement('div'); spacer.id = 'history-column-space';
          spacer.setAttribute('aria-hidden', 'true'); top.appendChild(spacer);
          top.classList.add('history-column-open'); modal.classList.add('history-column');
        }
        spacer.style.width = width + 'px'; notifyResize();
        var valid = position();
        if (!observed && root.ResizeObserver) {
          new root.ResizeObserver(resized).observe(top); observed = true;
        }
        return valid;
      },
      release: function () {
        if (!spacer) return;
        spacer.remove(); spacer = null; top.classList.remove('history-column-open');
        modal.classList.remove('history-column');
        savedStyle.forEach(function (entry) {
          if (entry[1]) box.style.setProperty(entry[0], entry[1], entry[2]);
          else box.style.removeProperty(entry[0]);
        });
        savedStyle = null; geometry = ''; notifyResize();
      },
      show: function () { modal.style.display = 'flex'; }, hide: function () { modal.style.display = 'none'; },
      loading: function () { list.textContent = 'Loading versions…'; controls = []; },
      enable: function (value) {
        controls.forEach(function (button) { button.disabled = !value; });
        // Close is still allowed while loading, but the controller refuses it
        // during Restore so a failed native publication cannot reopen a closed lease.
      },
      context: function () {
        var native = root.NemoNativeOpacityCutover;
        return JSON.stringify([ui.context(), native && native.identity(), native && native.getNativeIdentity()]);
      },
      present: present, list: ui.list, restore: ui.restore, notice: ui.notice,
      render: function (rows) {
        list.textContent = ''; controls = [];
        if (!rows.length) { list.textContent = root.__TAURI__ ? 'Aucun instantané pour l’instant — revenez dans 30s.' : 'Historique disque disponible uniquement dans l’app desktop.'; return; }
        rows.forEach(function (version) {
          var row = document.createElement('div'), label = document.createElement('span'), button = document.createElement('button');
          row.className = 'history-version-row';
          label.textContent = ui.relTime(version.ts) + ' (' + new Date(version.ts).toLocaleTimeString(undefined,
            { hour: '2-digit', minute: '2-digit', second: '2-digit' }) + ')';
          button.className = 'pbtn'; button.textContent = 'Restaurer'; button.disabled = true;
          button.addEventListener('click', function () {
            if (root.confirm('Restaurer cette version ? Le document actuel non sauvegardé sera remplacé.')) controller.restore(version.path);
          });
          controls.push(button); row.appendChild(label); row.appendChild(button); list.appendChild(row);
        });
      }
    });
    function resized() {
      if (!spacer || pending) return;
      var rect = spacer.getBoundingClientRect();
      if ([rect.left, rect.top, rect.width, rect.height].join('/') === geometry) return;
      pending = true;
      root.setTimeout(function () { pending = false; controller.refresh(); }, 0);
    }
    root.addEventListener('resize', resized);
    closeButton.addEventListener('click', controller.close);
    modal.addEventListener('click', function (event) { if (event.target === modal) controller.close(); });
    return controller;
  }

  return Object.freeze({ plan: plan, create: create, bind: bind });
}));
