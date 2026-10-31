/* The History panel on a native object project (#2376 G26): the baseline popover (1065e9d8 history-panel.js) rebuilt over the native
 * history read. Each row is one entry the engine labelled by its command family; the current marker sits at the cursor (the done entries
 * above it, the redoable ones dimmed below); a click lands on that row through ONE native undo/redo carrying a step count, so a jump is one
 * restore and one new revision, never a loop of UI-driven moves. This file reads and asks the project session to jump: it keeps no stack,
 * writes nothing itself and never reaches a legacy writer.
 * Clear is the baseline's "Effacer" control but the native history has no truncation, so it is shown disabled with its reason. The panel
 * follows the revision while it is open (a slow timer compares the project pin), and a read that lands after the revision moved is dropped. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectHistoryUI = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const PAGE = 32, WATCH_MS = 250;
  const NO_CLEAR = 'The native history cannot be cleared: it is kept with the project.';

  function session(win) {
    try {
      const module = win.NemoNativeObjectProject, s = module && typeof module.desktop === 'function' ? module.desktop(win) : null;
      return s && s.blocksLegacy() && s.isActive() && typeof s.readHistory === 'function' && typeof s.jumpHistory === 'function' ? s : null;
    } catch (_) { return null; }
  }
  function text(win, key, fallback) {
    try { const value = win.SM && typeof win.SM.t === 'function' ? win.SM.t(key) : null; return typeof value === 'string' && value && value !== key ? value : fallback; } catch (_) { return fallback; }
  }
  function pinKey(info) { return info ? [info.instanceId, info.documentId, info.lifecycleGeneration, info.contentRevision, info.documentSnapshotId].join('|') : ''; }

  function create(win) {
    let serial = 0, cache = null, offset, busy = false, timer = null, seen = null, openPop = null;
    const doc = win.document;
    function el(tag, cls, label) {
      const n = doc.createElement(tag);
      if (cls) n.className = cls;
      if (label !== undefined) n.textContent = label;
      return n;
    }
    function clear(pop) { while (pop.firstChild) pop.removeChild(pop.firstChild); }
    function row(label, cls) {
      const item = el('div', 'hist-item' + (cls ? ' ' + cls : ''));
      item.appendChild(el('div', 'hist-dot'));
      const body = el('div', 'hist-body');
      body.appendChild(el('div', 'hist-label', label));
      item.appendChild(body);
      return item;
    }
    function more(label, target) {
      const item = el('div', 'hist-empty', label);
      item.style.cursor = 'pointer';
      item.addEventListener('click', () => { offset = target; load(openPop); });
      return item;
    }
    function status(pop, message) { clear(pop); pop.appendChild(el('div', 'hist-empty', message)); const n = pop.firstChild; n.setAttribute('role', 'status'); }
    // Landing on a done row undoes down to just before it; on a redoable row redoes through it (the baseline's own counts).
    function jump(page, index) {
      const s = session(win);
      if (busy || !s) return;
      // The step count is relative to the cursor of the page the user clicked; if the project moved on since (a commit inside the watch
      // interval), a jump would land one entry off, so the panel is read again and nothing is sent.
      if (pinKey(page.info) !== pinKey(s.info())) { offset = undefined; if (openPop) load(openPop); return; }
      const undo = index < page.undoDepth;
      busy = true;
      Promise.resolve().then(() => s.jumpHistory(undo ? 'undo' : 'redo', undo ? page.undoDepth - index : index - page.undoDepth + 1))
        .catch(() => { /* the session already told the user why the jump was not confirmed */ })
        .then(() => { busy = false; offset = undefined; if (openPop) load(openPop); });
    }
    function paint(pop, page) {
      clear(pop);
      const total = page.undoDepth + page.redoDepth;
      const header = el('div', 'hist-header');
      header.appendChild(doc.createTextNode((total + 1) + ' états'));
      const wipe = el('span', 'hist-clear disabled', 'Effacer');
      wipe.title = NO_CLEAR; wipe.setAttribute('aria-disabled', 'true');
      wipe.addEventListener('click', (e) => { e.stopPropagation(); });
      header.appendChild(wipe);
      pop.appendChild(header);
      if (!total) { pop.appendChild(el('div', 'hist-empty', text(win, 'hsNoActionYet', 'Aucune action'))); return; }
      if (page.offset > 0) pop.appendChild(more('↑ ' + page.offset + ' …', Math.max(0, page.offset - PAGE)));
      const end = page.offset + page.entries.length;
      let current = null;
      const marker = () => { current = row('État actuel', 'current'); pop.appendChild(current); };
      page.entries.forEach((entry) => {
        if (entry.index === page.undoDepth) marker();
        const item = row(entry.label, entry.index >= page.undoDepth ? 'future' : '');
        item.addEventListener('click', () => jump(page, entry.index));
        pop.appendChild(item);
      });
      if (!current && page.undoDepth === end) marker();
      if (end < total) pop.appendChild(more('↓ ' + (total - end) + ' …', end));
      if (current && win.requestAnimationFrame) win.requestAnimationFrame(() => { try { current.scrollIntoView({ block: 'nearest' }); } catch (_) { /* scrolling is a nicety */ } });
    }
    // One read at a time per call; an answer for a pin the project has left, or from a superseded call, is dropped and re-asked.
    function load(pop) {
      const s = session(win);
      if (!pop) return;
      if (!s) { cache = null; status(pop, 'Native history panel is unavailable.'); return; }
      const token = ++serial, asked = pinKey(s.info());
      seen = asked;
      s.readHistory(offset).then((page) => {
        if (token !== serial) return;
        const live = session(win);
        if (!live || pinKey(live.info()) !== asked) { seen = null; return; }
        cache = { key: asked, page };
        paint(pop, page);
      }).catch((error) => {
        if (token !== serial) return;
        // A moved revision is not a failure: the watch asks again. Anything else is said once in the panel and asked again on the next pin.
        const moved = !!error && error.code === 'stale_revision';
        seen = moved ? null : asked;
        if (!moved) status(pop, 'The native history could not be read (' + ((error && (error.reason || error.code)) || 'unavailable') + ').');
      });
    }
    function render(pop) {
      const s = session(win);
      if (!s) return false;
      openPop = pop;
      const key = pinKey(s.info());
      if (cache && cache.key === key) paint(pop, cache.page); else status(pop, '…');
      load(pop);
      return true;
    }
    function tick() {
      timer = null;
      if (!openPop || !openPop.classList.contains('open') || !session(win)) { openPop = null; return; }
      const s = session(win);
      if (!busy && pinKey(s.info()) !== seen) { offset = undefined; load(openPop); }
      schedule();
    }
    function schedule() { if (timer === null && win.setTimeout) timer = win.setTimeout(tick, WATCH_MS); }
    // Follow the revision while the popover is open; nothing runs while it is closed.
    function watch(pop) {
      if (!session(win)) return false;
      openPop = pop; schedule();
      return true;
    }
    function dispose() { serial++; openPop = null; cache = null; if (timer !== null && win.clearTimeout) win.clearTimeout(timer); timer = null; }
    return Object.freeze({ render, watch, dispose, load });
  }
  const controllers = new WeakMap();
  function desktop(win) {
    if (!controllers.has(win)) controllers.set(win, create(win));
    return controllers.get(win);
  }
  // The page-level entry points history-panel.js calls; both answer false when no native object session owns the project.
  function render(win, pop) { return desktop(win).render(pop); }
  function watch(win, pop) { return desktop(win).watch(pop); }
  return Object.freeze({ create, desktop, render, watch, PAGE });
}));
