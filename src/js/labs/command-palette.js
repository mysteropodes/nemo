// ---- LABS PROTOTYPE — Command palette (Umoupen "command search") ----
// Cmd/Ctrl+K opens a fuzzy-filtered list of commands: tools, transport,
// frame ops, and every Labs prototype toggle. Type to filter, Enter or
// click to run, Escape closes. Pure DOM overlay.
//
// The command list is a curated static map of existing window.SM.*
// entries (the same functions every toolbar button already calls) —
// nothing new is executed, this is just a keyboard front-door to what
// the UI already exposes.
(function () {
  function commands() {
    var list = [
      { label: SM.t('labsCmdToolBrush'), run: function () { SM.setTool('draw'); } },
      { label: SM.t('labsCmdToolPen'), run: function () { SM.setTool('pen'); } },
      { label: SM.t('labsCmdToolEraser'), run: function () { SM.setTool('eraser'); } },
      { label: SM.t('labsCmdToolBucket'), run: function () { SM.setTool('fill'); } },
      { label: SM.t('labsCmdToolFillBrush'), run: function () { SM.setTool('fillbrush'); } },
      { label: SM.t('labsCmdToolSelect'), run: function () { SM.setTool('select'); } },
      { label: SM.t('labsCmdToolSubselect'), run: function () { SM.setTool('subselect'); } },
      { label: SM.t('labsCmdToolText'), run: function () { SM.setTool('text'); } },
      { label: SM.t('labsCmdToolHand'), run: function () { SM.setTool('hand'); } },
      { label: SM.t('labsCmdToolZoom'), run: function () { SM.setTool('zoom'); } },
      { label: SM.t('labsCmdToolCamera'), run: function () { SM.setTool('camera'); } },
      { label: SM.t('labsCmdToolComment'), run: function () { SM.setTool('comment'); } },
      { label: SM.t('labsCmdPlayStop'), run: function () { togglePlay(); } },
      { label: SM.t('labsCmdLoopToggle'), run: function () { SM.toggleLoopPlayback(); } },
      { label: SM.t('labsCmdPingpongToggle'), run: function () { if (SM.togglePingPongPlayback) SM.togglePingPongPlayback(); } },
      { label: SM.t('labsCmdOnionToggle'), run: function () { SM.toggleOnion(); } },
      { label: SM.t('labsCmdInsertKeyframe'), run: function () { insertKeyframe(); } },
      { label: SM.t('labsCmdBlankKeyframe'), run: function () { insertBlankKeyframe(); } },
      { label: SM.t('labsCmdInsertFrame'), run: function () { insertFrame(); } },
      { label: SM.t('labsCmdRemoveFrame'), run: function () { removeFrame(); } },
      { label: SM.t('labsCmdNewLayer'), run: function () { SM.addLayer(); } },
      { label: SM.t('labsCmdDuplicateLayer'), run: function () { SM.duplicateLayer(); } },
      { label: SM.t('labsCmdUndo'), run: function () { undo(); } },
      { label: SM.t('labsCmdRedo'), run: function () { redo(); } },
    ];
    // Every Labs prototype becomes a toggle entry, current state shown.
    window.SMLabs.list().forEach(function (p) {
      list.push({ label: 'Labs : ' + p.name + (p.on ? SM.t('labsCmdSuffixOn') : SM.t('labsCmdSuffixOff')), run: function () { window.SMLabs.toggle(p.name); } });
    });
    return list;
  }

  var el = null;
  function close() { if (el) { el.remove(); el = null; } }
  function open() {
    close();
    var cmds = commands();
    el = document.createElement('div');
    el.style.cssText = 'position:fixed;inset:0;z-index:99999;background:rgba(0,0,0,.35);display:flex;align-items:flex-start;justify-content:center;padding-top:12vh;';
    el.addEventListener('pointerdown', function (e) { if (e.target === el) close(); });
    var box = document.createElement('div');
    box.style.cssText = 'width:420px;max-height:60vh;background:#201f25;border:1px solid rgba(255,255,255,.14);border-radius:12px;box-shadow:0 12px 40px rgba(0,0,0,.6);display:flex;flex-direction:column;overflow:hidden;';
    var input = document.createElement('input');
    input.placeholder = SM.t('labsCmdPlaceholder');
    input.style.cssText = 'padding:12px 14px;background:transparent;border:none;outline:none;color:#eceae7;font:14px system-ui;border-bottom:1px solid rgba(255,255,255,.08);';
    var listEl = document.createElement('div');
    listEl.style.cssText = 'overflow:auto;padding:4px 0;';
    box.appendChild(input); box.appendChild(listEl); el.appendChild(box);
    document.body.appendChild(el);

    var filtered = cmds, sel = 0;
    function render() {
      listEl.innerHTML = '';
      filtered.slice(0, 12).forEach(function (c, i) {
        var row = document.createElement('div');
        row.textContent = c.label;
        row.style.cssText = 'padding:7px 14px;font:12.5px system-ui;color:#eceae7;cursor:pointer;' + (i === sel ? 'background:#4E6FF2;' : '');
        row.addEventListener('pointerenter', function () { sel = i; render(); });
        row.addEventListener('pointerdown', function (e) { e.stopPropagation(); close(); c.run(); });
        listEl.appendChild(row);
      });
      if (!filtered.length) {
        var none = document.createElement('div');
        none.textContent = SM.t('labsCmdNoResults');
        none.style.cssText = 'padding:10px 14px;color:#666;font:12px system-ui;';
        listEl.appendChild(none);
      }
    }
    function filter() {
      var q = input.value.trim().toLowerCase();
      filtered = !q ? cmds : cmds.filter(function (c) {
        // simple subsequence fuzzy match
        var l = c.label.toLowerCase(), qi = 0;
        for (var i = 0; i < l.length && qi < q.length; i++) if (l[i] === q[qi]) qi++;
        return qi === q.length;
      });
      sel = 0; render();
    }
    input.addEventListener('input', filter);
    input.addEventListener('keydown', function (e) {
      if (e.key === 'Escape') { close(); }
      else if (e.key === 'ArrowDown') { sel = Math.min(sel + 1, Math.min(filtered.length, 12) - 1); render(); e.preventDefault(); }
      else if (e.key === 'ArrowUp') { sel = Math.max(sel - 1, 0); render(); e.preventDefault(); }
      else if (e.key === 'Enter') { var c = filtered[sel]; close(); if (c) c.run(); }
      e.stopPropagation(); // never leak keys to the app's own shortcuts while typing
    });
    render();
    input.focus();
  }

  document.addEventListener('keydown', function (e) {
    if (!window.SMLabs.isOn('command-palette')) return;
    if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) {
      e.preventDefault(); e.stopPropagation();
      if (el) close(); else open();
    }
  }, true);

  window.SMLabs.register('command-palette', {
    flag: 'nemo-labs-palette',
    describe: 'labsDescribeCommandPalette',
    onDisable: close,
  });
})();
