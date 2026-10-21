/* Existing workspace presentation and native layout-settle use case. No document mirror. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else {
    root.NemoNativeWorkspaceUI = api;
    root.document.addEventListener('DOMContentLoaded', function ready() { api.install(root); }, { once: true });
  }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  var presenters = new WeakMap(), installed = new WeakMap();
  var keys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  var storageKeys = ['nemo-layer-panel-width', 'nemo-tools-panel-width', 'nemo-props-panel-width',
    'nemo-props-panel-collapsed', 'nemo-timeline-height', 'nemo-panel-order', 'nemo-motion-columns'];
  function native(root) { return !!(root.__TAURI__ && root.__TAURI__.core); }
  function same(a, b) { return !!a && !!b && keys.every(function (key) { return a[key] === b[key]; }); }
  function presentation(root, ports) {
    var window = root, document = root.document, localStorage = root.localStorage, Event = root.Event;
    var _secOrderNow = ports.order, _applySecOrder = ports.applyOrder, motion = null, applying = 0, selectors = new Set();
    function _w(id) { var el = document.getElementById(id); return el && el.style.width ? el.style.width : null; }
    function syncPropsPanelCollapseTitle() { if (root.syncPropsPanelCollapseTitle) root.syncPropsPanelCollapseTitle(); }
    function renderPropsPanelRail(show) { if (root.renderPropsPanelRail) root.renderPropsPanelRail(show); }
  var original={
    capture:function(){
      var ta=document.getElementById('timeline-area');
      var pp=document.getElementById('props-panel');
      var ws={
        layerPanelWidth:_w('layer-panel'),
        toolsPanelWidth:_w('tools-panel'),
        // A collapsed props panel has no inline width of its own — the
        // expanded one is stashed aside by togglePropsPanelCollapse, and
        // that is the width worth restoring.
        propsPanelWidth:(pp&&pp.classList.contains('collapsed'))?(window._propsExpandedWidth||null):_w('props-panel'),
        propsPanelCollapsed:!!(pp&&pp.classList.contains('collapsed')),
        timelineHeight:(ta&&ta.style.height)?ta.style.height:null,
        panelOrder:_secOrderNow(),
      };
      try{ws.motionColumns=localStorage.getItem('nemo-motion-columns')||null;}catch(e){}
      return ws;
    },
    apply:function(ws){
      if(!ws||typeof ws!=='object')return false;
      function set(id,v,key){
        var el=document.getElementById(id);
        if(!el||!v||typeof v!=='string')return;
        el.style.width=v;
        try{localStorage.setItem(key,v);}catch(e){}
      }
      set('layer-panel',ws.layerPanelWidth,'nemo-layer-panel-width');
      set('tools-panel',ws.toolsPanelWidth,'nemo-tools-panel-width');
      var pp=document.getElementById('props-panel');
      if(pp&&ws.propsPanelWidth&&typeof ws.propsPanelWidth==='string'){
        window._propsExpandedWidth=ws.propsPanelWidth;
        if(!pp.classList.contains('collapsed'))pp.style.width=ws.propsPanelWidth;
        try{localStorage.setItem('nemo-props-panel-width',ws.propsPanelWidth);}catch(e){}
      }
      if(typeof ws.propsPanelCollapsed==='boolean'&&pp&&window.togglePropsPanelCollapse){
        var isCollapsed=pp.classList.contains('collapsed');
        if(isCollapsed!==ws.propsPanelCollapsed)window.togglePropsPanelCollapse(ws.propsPanelCollapsed);
      }
      var ta=document.getElementById('timeline-area');
      if(ta&&ws.timelineHeight&&typeof ws.timelineHeight==='string'){
        ta.style.height=ws.timelineHeight;
        try{localStorage.setItem('nemo-timeline-height',ws.timelineHeight);}catch(e){}
      }
      if(Array.isArray(ws.panelOrder)&&ws.panelOrder.length){
        _applySecOrder(ws.panelOrder);
        try{localStorage.setItem('nemo-panel-order',JSON.stringify(ws.panelOrder));}catch(e){}
      }
      if(ws.motionColumns&&typeof ws.motionColumns==='string'){
        try{localStorage.setItem('nemo-motion-columns',ws.motionColumns);}catch(e){}
        var lp=document.getElementById('layer-panel');
        if(lp)lp.dataset.motionColumns=ws.motionColumns;
      }
      // The canvas sizes itself from a window resize (see the panel drag
      // handlers above, which fire one for the same reason).
      window.dispatchEvent(new Event('resize'));
      return true;
    },
  };
    function applyRecord(ws) {
      applying++;
      try {
        var result = original.apply(ws);
        if (native(root) && motion && ws && typeof ws.motionColumns === 'string' && ws.motionColumns) { motion.apply(ws.motionColumns); selectors.forEach(function (row) { row.select.value = ws.motionColumns; }); }
        return result;
      } finally { applying--; }
    }
    function snapshot() {
      var nodes = ['layer-panel', 'tools-panel', 'props-panel', 'timeline-area', 'props-panel-resize'].map(function (id) {
        var el = document.getElementById(id);
        return el && { el: el, width: el.style.width, height: el.style.height, display: el.style.display,
          collapsed: el.classList.contains('collapsed'), motion: el.dataset.motionColumns };
      }).filter(Boolean);
      var sections = Array.prototype.slice.call(document.querySelectorAll('.psec')).map(function (el) {
        return { el: el, parent: el.parentNode, next: el.nextSibling, floating: el.classList.contains('floating'),
          left: el.style.left, top: el.style.top, pointerEvents: el.style.pointerEvents };
      });
      var storage = storageKeys.map(function (key) {
        try { return { key: key, value: localStorage.getItem(key) }; } catch (_) { return null; }
      }).filter(Boolean);
      return { nodes: nodes, sections: sections, storage: storage, expanded: root._propsExpandedWidth,
        motion: motion && motion.read() };
    }
    function restore(saved) {
      applying++;
      try {
        saved.sections.slice().reverse().forEach(function (row) {
          if (row.parent) row.parent.insertBefore(row.el, row.next && row.next.parentNode === row.parent ? row.next : null);
          row.el.classList.toggle('floating', row.floating); row.el.style.left = row.left; row.el.style.top = row.top;
          row.el.style.pointerEvents = row.pointerEvents;
        });
        saved.nodes.forEach(function (row) {
          row.el.style.width = row.width; row.el.style.height = row.height; row.el.style.display = row.display;
          row.el.classList.toggle('collapsed', row.collapsed);
          if (row.motion === undefined) delete row.el.dataset.motionColumns; else row.el.dataset.motionColumns = row.motion;
        });
        root._propsExpandedWidth = saved.expanded;
        if (motion && saved.motion !== null) { motion.apply(saved.motion); selectors.forEach(function (row) { row.select.value = saved.motion; }); }
        saved.storage.forEach(function (row) { try {
          if (row.value === null) localStorage.removeItem(row.key); else localStorage.setItem(row.key, row.value);
        } catch (_) {} });
        syncPropsPanelCollapseTitle(); root.dispatchEvent(new Event('resize'));
      } finally { applying--; }
    }
    function begin(source) {
      if (applying || !native(root)) return;
      var controller = installed.get(root); if (controller) return controller.begin(source);
    }
    function settle(source) {
      if (applying || !native(root)) return;
      var controller = installed.get(root); if (controller) return controller.settle(source);
    }
function toggleProps(force){
  var panel=document.getElementById('props-panel');
  if(!panel)return;
  if (begin('collapse') === false) return;
  var collapsed=force!==undefined?force:!panel.classList.contains('collapsed');
  // Panel-width persistence (ui.js) sets a plain inline style.width, which
  // always wins over the .collapsed class's own `width:36px` rule (inline
  // beats any selector, regardless of specificity) — without clearing it
  // here first, a resized-then-collapsed panel would stay visually wide
  // with just the rail's few narrow items floating in the leftover space.
  // Stashed on window (not a local var) so ui.js's own startup restore can
  // prime it too, for the "starts collapsed" case.
  if(collapsed){
    if(panel.style.width)window._propsExpandedWidth=panel.style.width;
    panel.style.width='';
  }else if(window._propsExpandedWidth){
    panel.style.width=window._propsExpandedWidth;
  }
  panel.classList.toggle('collapsed',collapsed);
  // Dragging a 36px rail wider makes no sense — hide the resize handle
  // rather than leave a control that would just fight the collapsed width.
  var resizeHandle=document.getElementById('props-panel-resize');
  if(resizeHandle)resizeHandle.style.display=collapsed?'none':'';
  if(collapsed&&window._lastPropsShow)renderPropsPanelRail(window._lastPropsShow);
  syncPropsPanelCollapseTitle();
  try{localStorage.setItem('nemo-props-panel-collapsed',collapsed?'1':'0');}catch(e){}
  settle('collapse');
}
    var api = Object.freeze({ capture: original.capture, apply: function (ws) {
      if (!native(root)) return applyRecord(ws);
      var controller = installed.get(root); if (controller) controller.presented(); return false;
    }, begin: begin, settle: settle, toggleProps: toggleProps,
      release: function (source) { var c = installed.get(root); if (c) c.release(source); },
      bindMotion: function (port) { motion = port; }, cancel: function () { var c = installed.get(root); if (c) c.cancel(); } });
    presenters.set(root, { api: api, snapshot: snapshot, restore: restore, apply: applyRecord,
      cancelGesture: ports.cancelGesture, selectors: selectors, isApplying: function () { return applying > 0; } });
    return api;
  }
  function bindMotion(root, port) {
    var presenter = presenters.get(root); if (!presenter) return false;
    presenter.api.bindMotion(port); return true;
  }
  // Default column preset (2026-08-29, Cyril: "3D/mograph par default") — the
  // 3D Layer/Duplicator toggle icons only show under this preset (see
  // style.css's `[data-motion-columns]` rules); 'animation' (the prior
  // default) hid them, which read as "these icons disappeared" until you
  // knew to open the column picker and switch presets.
  function motionColumns(root, panel, port) {
    var document = root.document, label = document.createElement('label'); label.textContent = 'Colonnes';
    var select = document.createElement('select'); select.className = 'motion-filter-select';
    [['compact', 'Compact'], ['animation', 'Animation'], ['compositing', 'Compositing'], ['3d', '3D / Mograph']].forEach(function (item) {
      var option = document.createElement('option'); option.value = item[0]; option.textContent = item[1]; select.appendChild(option);
    });
    select.value = port.read();
    function change() {
      if (root.SMWorkspace.begin('motionColumns') === false) { select.value = port.read(); return; }
      port.apply(select.value); panel.dataset.motionColumns = select.value;
      try { root.localStorage.setItem('nemo-motion-columns', select.value); } catch (_) {}
      root.SMWorkspace.settle('motionColumns');
    }
    var presenter = presenters.get(root), row = { select: select, dispose: function () {
      select.removeEventListener('change', change); if (presenter) presenter.selectors.delete(row);
    } };
    if (presenter) presenter.selectors.add(row); select.addEventListener('change', change);
    return { label: label, select: select, dispose: row.dispose };
  }
  function install(root) {
    if (installed.has(root)) return installed.get(root);
    if (!native(root)) return null;
    var presenter = presenters.get(root), Desktop = root.NemoNativeWorkspaceDesktop;
    if (!presenter || !Desktop) return null;
    var retired = false, loading = null, pending = false, gesture = null, refreshNeeded = false, lastApplied = null, busyStamp = null;
    function current() { return root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root); }
    function available(session, info) {
      return !retired && session === current() && root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true;
    }
    var model = Desktop.create({ currentSession: current, available: available, publish: function () { return true; } });
    function owner() {
      try { var s = current(); return s && s.info() ? { session: s, info: Object.assign({}, s.info()), ticket: s.readSubscriptionGeneration() } : null; }
      catch (_) { return null; }
    }
    function changed(a, b) { return !!b && (!a || a.session !== b.session || !same(a.info, b.info) || a.ticket !== b.ticket); }
    function owns(token) {
      var next = owner(); return !!token && !!next && token.session === next.session && same(token.info, next.info) && token.ticket === next.ticket;
    }
    function notice() { if (!retired && root.showToast) root.showToast('Workspace change is unavailable or unconfirmed. Wait for synchronization before trying again.'); }
    function rollback(token) {
      // Retire only this still-owned DOM draft, even if its native pin was just replaced.
      // This does not publish the old metadata proof or authorize a command.
      if (owns(token) || token.origin && lastApplied === token.origin) presenter.restore(token.saved);
    }
    function cancel() {
      var held = gesture; gesture = null; if (presenter.cancelGesture) presenter.cancelGesture();
      if (held) rollback(held);
    }
    function apply(value) {
      if (retired || !value || model.read() !== value) return false;
      if (lastApplied === value) return true;
      if (value.workspace.workspacePresent && presenter.apply(value.workspace.workspace) !== true) return false;
      if (model.read() !== value) return false;
      lastApplied = value;
      // Truly absent workspace preserves the original current-layout fallback. No defaults are stored.
      return true;
    }
    async function settle(source) {
      var token = gesture; if (!token || token.source !== source) return { status: 'not-started' };
      gesture = null;
      var draft, equal;
      try { draft = presenter.api.capture(); equal = root.NemoNativeWorkspaceContract.equal(draft, token.before); }
      catch (_) { rollback(token); notice(); return { status: 'unavailable' }; }
      rollback(token);
      if (equal) return { status: 'unchanged', applied: false };
      if (retired || pending || loading || !owns(token) || !token.origin || model.read() !== token.origin) {
        notice(); return { status: 'unavailable' };
      }
      pending = true; busyStamp = owner();
      try {
        var result = await model.edit({ kind: 'capture', workspace: draft }, token.origin);
        if (['applied-presented', 'unchanged'].includes(result.status)) {
          if (!apply(result.projection)) { notice(); return { status: 'unconfirmed' }; }
        } else notice();
        return result;
      } catch (_) { notice(); return { status: 'unconfirmed' }; }
      finally { pending = false; busyStamp = null; if (refreshNeeded) { refreshNeeded = false; presented(); } }
    }
    function presented() {
      if (retired) return Promise.resolve(null);
      if (loading || pending) { if (changed(busyStamp, owner())) refreshNeeded = true; return loading || Promise.resolve(null); }
      var value = model.read(); if (value) { apply(value); return Promise.resolve(value); }
      busyStamp = owner();
      loading = model.refresh().then(function (projection) { apply(projection); return projection; }).catch(function () { return null; })
        .finally(function () { loading = null; busyStamp = null; var again = refreshNeeded; refreshNeeded = false; if (again && !model.read()) presented(); });
      return loading;
    }
    function key(event) { if (event.key === 'Escape') cancel(); }
    root.addEventListener('blur', cancel); root.addEventListener('pointercancel', cancel); root.addEventListener('mouseleave', cancel); root.addEventListener('keydown', key);
    var controller = Object.freeze({ read: model.read, presented: presented, settle: settle, cancel: cancel,
      release: function (source) { if (gesture && gesture.source === source) gesture = null; },
      begin: function (source) {
        if (retired) return false; if (presenter.isApplying()) return;
        cancel(); var token = owner(), origin = !pending && !loading ? model.read() : null;
        if (!token || !origin || origin !== lastApplied) { notice(); return false; }
        token.saved = presenter.snapshot(); token.before = presenter.api.capture(); token.source = source; token.origin = origin;
        gesture = token; return true;
      },
      willProject: function () { cancel(); if (!pending) model.invalidate(); lastApplied = null; },
      dispose: function () {
        if (retired) return; cancel(); retired = true; refreshNeeded = false; model.dispose();
        root.removeEventListener('blur', cancel); root.removeEventListener('pointercancel', cancel); root.removeEventListener('mouseleave', cancel); root.removeEventListener('keydown', key);
        presenter.selectors.forEach(function (row) { row.dispose(); });
        root.removeEventListener('pagehide', controller.dispose);
      }
    });
    installed.set(root, controller); root.NemoNativeWorkspaceController = controller;
    root.addEventListener('pagehide', controller.dispose, { once: true }); presented(); return controller;
  }
  return Object.freeze({ presentation: presentation, motionColumns: motionColumns, bindMotion: bindMotion, install: install });
}));
