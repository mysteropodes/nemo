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
      { label: 'Outil : Pinceau', run: function () { SM.setTool('draw'); } },
      { label: 'Outil : Plume', run: function () { SM.setTool('pen'); } },
      { label: 'Outil : Gomme', run: function () { SM.setTool('eraser'); } },
      { label: 'Outil : Pot de peinture', run: function () { SM.setTool('fill'); } },
      { label: 'Outil : Pinceau de remplissage', run: function () { SM.setTool('fillbrush'); } },
      { label: 'Outil : Sélection', run: function () { SM.setTool('select'); } },
      { label: 'Outil : Sous-sélection (noeuds)', run: function () { SM.setTool('subselect'); } },
      { label: 'Outil : Texte', run: function () { SM.setTool('text'); } },
      { label: 'Outil : Main', run: function () { SM.setTool('hand'); } },
      { label: 'Outil : Zoom', run: function () { SM.setTool('zoom'); } },
      { label: 'Outil : Caméra', run: function () { SM.setTool('camera'); } },
      { label: 'Outil : Commentaire', run: function () { SM.setTool('comment'); } },
      { label: 'Lecture / Stop', run: function () { togglePlay(); } },
      { label: 'Loop on/off', run: function () { SM.toggleLoopPlayback(); } },
      { label: 'Lecture ping-pong on/off', run: function () { if (SM.togglePingPongPlayback) SM.togglePingPongPlayback(); } },
      { label: 'Onion skin on/off', run: function () { SM.toggleOnion(); } },
      { label: 'Insérer une keyframe (F6)', run: function () { insertKeyframe(); } },
      { label: 'Keyframe vide (F7)', run: function () { insertBlankKeyframe(); } },
      { label: 'Insérer une frame (F5)', run: function () { insertFrame(); } },
      { label: 'Supprimer la frame', run: function () { removeFrame(); } },
      { label: 'Nouveau calque', run: function () { SM.addLayer(); } },
      { label: 'Dupliquer le calque', run: function () { SM.duplicateLayer(); } },
      { label: 'Annuler (Cmd+Z)', run: function () { undo(); } },
      { label: 'Refaire', run: function () { redo(); } },
    ];
    // Every Labs prototype becomes a toggle entry, current state shown.
    window.SMLabs.list().forEach(function (p) {
      list.push({ label: 'Labs : ' + p.name + (p.on ? ' ✓ (désactiver)' : ' (activer)'), run: function () { window.SMLabs.toggle(p.name); } });
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
    input.placeholder = 'Commande…';
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
        none.textContent = 'Aucune commande';
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
    describe: 'Palette de commandes Cmd/Ctrl+K (Umoupen command search) : outils, transport, frames, toggles Labs — filtre fuzzy, Entrée exécute',
    onDisable: close,
  });
})();
