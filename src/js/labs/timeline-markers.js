// ---- LABS PROTOTYPE — Timeline markers (Callipeg marker system) ----
// Named, colored markers on timeline frames ("clé pose", "cut", "accent
// audio"...) rendered as a small flag on the frame-header cell, tooltip =
// label. Display-only organization aid — playback/undo/save never read
// them.
//
//   SMLabs.addMarker(frame, 'label', '#66ccff')
//   SMLabs.removeMarker(frame)
//   SMLabs.listMarkers()
//
// Persistence is deliberately Labs-local (localStorage keyed by the
// project key), NOT the project file — feedback-bridge set the precedent
// (CLAUDE.md §6: debug/meta data stays out of exportJSON) and it keeps
// this prototype at zero core-file touches. If picked for real, markers
// would move into the project format as a proper decision.
//
// Re-decoration after every timeline rebuild is a MutationObserver on
// #frame-hdr (renderTimeline clears and rebuilds its children), not a
// monkey-patch of renderTimeline — observing DOM output can't break the
// function it watches.
(function () {
  function projectKey() {
    try { if (window.SMProject && SMProject.getProjectKey) return SMProject.getProjectKey(); } catch (e) {}
    return 'default';
  }
  function storeKey() { return 'nemo-labs-markers-' + projectKey(); }
  function load() {
    try { return JSON.parse(localStorage.getItem(storeKey()) || '{}'); } catch (e) { return {}; }
  }
  function save(m) { localStorage.setItem(storeKey(), JSON.stringify(m)); }

  function decorate() {
    if (!window.SMLabs.isOn('timeline-markers')) return;
    var hdr = document.getElementById('frame-hdr');
    if (!hdr) return;
    var markers = load();
    // Clear previous flags first (cells are reused between decorations
    // only when renderTimeline DIDN'T rebuild — e.g. we re-ran after a
    // marker edit).
    hdr.querySelectorAll('.labs-marker-flag').forEach(function (el) { el.remove(); });
    Object.keys(markers).forEach(function (fs) {
      var fi = parseInt(fs, 10);
      var cell = hdr.children[fi];
      if (!cell) return;
      var mk = markers[fs];
      var flag = document.createElement('div');
      flag.className = 'labs-marker-flag';
      flag.title = mk.label || '';
      flag.style.cssText = 'position:absolute;top:0;left:0;right:0;height:3px;background:' + (mk.color || '#66ccff') + ';pointer-events:auto;cursor:help;';
      if (getComputedStyle(cell).position === 'static') cell.style.position = 'relative';
      cell.appendChild(flag);
    });
  }

  var mo = null;
  function observe() {
    var hdr = document.getElementById('frame-hdr');
    if (!hdr || mo) return;
    mo = new MutationObserver(function (muts) {
      // Only react to renderTimeline rebuilds (child churn), not to our own
      // flag insertions — a flag addition is a subtree of a cell, rebuilds
      // replace the cells themselves.
      var rebuilt = muts.some(function (m) {
        return Array.prototype.some.call(m.addedNodes, function (n) { return n.classList && !n.classList.contains('labs-marker-flag'); });
      });
      if (rebuilt) decorate();
    });
    mo.observe(hdr, { childList: true });
  }

  window.SMLabs.addMarker = function (frame, label, color) {
    var m = load();
    m[frame] = { label: label || '', color: color || '#66ccff' };
    save(m); observe(); decorate();
    return m[frame];
  };
  window.SMLabs.removeMarker = function (frame) {
    var m = load(); delete m[frame]; save(m); decorate();
  };
  window.SMLabs.listMarkers = function () { return load(); };

  window.SMLabs.register('timeline-markers', {
    flag: 'nemo-labs-markers',
    describe: 'Marqueurs nommés/colorés sur les frames (SMLabs.addMarker(frame,label,couleur)) — repères de prod façon Callipeg',
    onEnable: function () { observe(); decorate(); },
    onDisable: function () { document.querySelectorAll('.labs-marker-flag').forEach(function (el) { el.remove(); }); },
  });
  if (window.SMLabs.isOn('timeline-markers')) { observe(); decorate(); }
})();
