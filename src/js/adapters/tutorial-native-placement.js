/* Tutorial native UI placement and step reads; document/render authority stays in Rust. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoTutorialNativePlacement = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  function positionTooltip(tw, th, rect, pinCorner, vw, vh) {
    var pad = 16;
    if (pinCorner === 'top-right') return { top: pad, left: vw - tw - pad };
    if (!rect) return { top: vh / 2 - th / 2, left: vw / 2 - tw / 2 };
    function fits(top, left) { return left >= pad && top >= pad && left + tw <= vw - pad && top + th <= vh - pad; }
    function overlaps(top, left) { return left < rect.right && left + tw > rect.left && top < rect.bottom && top + th > rect.top; }
    var candidates = [
      { top: rect.bottom + pad, left: Math.min(vw - tw - pad, Math.max(pad, rect.left)) },
      { top: rect.top - th - pad, left: Math.min(vw - tw - pad, Math.max(pad, rect.left)) },
      { top: Math.min(vh - th - pad, Math.max(pad, rect.top)), left: rect.right + pad },
      { top: Math.min(vh - th - pad, Math.max(pad, rect.top)), left: rect.left - tw - pad }
    ];
    for (var i = 0; i < candidates.length; i++) if (fits(candidates[i].top, candidates[i].left) && !overlaps(candidates[i].top, candidates[i].left)) return candidates[i];
    var cx = rect.left + rect.width / 2, cy = rect.top + rect.height / 2;
    return { top: cy < vh / 2 ? vh - th - pad : pad, left: cx < vw / 2 ? vw - tw - pad : pad };
  }

  function plan(available, height) {
    if (!Number.isFinite(available) || available < 396 || !Number.isFinite(height) || height < 200) return null;
    return { width: Math.min(340, available - 176) };
  }

  function matches(pin, view, frame, receipt, latest, latestView, latestFrame) {
    return !!(pin && view && receipt && latest && latestView &&
      typeof pin.instanceId === 'string' && pin.instanceId.trim() && typeof pin.documentId === 'string' && pin.documentId.trim() &&
      Number.isSafeInteger(pin.contentRevision) && pin.contentRevision >= 0 && Number.isSafeInteger(frame) && frame >= 0 &&
      receipt.owner === 'native' && receipt.status === 'presented' &&
      receipt.instanceId === pin.instanceId && receipt.documentId === pin.documentId &&
      receipt.contentRevision === pin.contentRevision && receipt.frame === frame && latestFrame === frame &&
      Number.isSafeInteger(receipt.lifecycleGeneration) && receipt.lifecycleGeneration > 0 &&
      receipt.lifecycleGeneration === view.generation && latestView.generation === view.generation &&
      view.documentId === pin.documentId && latestView.documentId === pin.documentId &&
      latest.instanceId === pin.instanceId && latest.documentId === pin.documentId && latest.contentRevision === pin.contentRevision &&
      Number.isSafeInteger(receipt.viewGeneration) && receipt.viewGeneration > 0 &&
      typeof receipt.workId === 'string' && receipt.workId.trim());
  }

  function boundary(root) {
    return new Promise(function (resolve) {
      var finished = false, timer = root.setTimeout(done, 200);
      function done() { if (!finished) { finished = true; root.clearTimeout(timer); resolve(); } }
      root.requestAnimationFrame(function () { root.requestAnimationFrame(done); });
    });
  }

  // Native measurement for the original Perspective toggle step; no lesson definition or document writes.
  function perspectiveStep(root, step, stillCurrent) {
    var baseline = null;
    var pinKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
    function samePin(a, b) { return !!a && !!b && pinKeys.every(function (key) { return a[key] === b[key]; }); }
    function sample() {
      try {
        var project = root.NemoNativeObjectProject, controller = root.NemoNativePerspectiveController;
        var session = project && project.desktop(root), pin = session && session.info();
        if (!session || !session.isActive() || !pin || !controller || typeof root.readPerspectiveSettings !== 'function') return null;
        var projection = controller.read(), settings = root.readPerspectiveSettings();
        if (!projection || !settings || !projection.perspective || projection.perspective.settings !== settings ||
            !samePin(projection.info, pin) || controller.read() !== projection || project.desktop(root) !== session) return null;
        var shown = session.readPresentedObjectSession(), ticket = session.readSubscriptionGeneration();
        if (!samePin(shown, pin) || !samePin(shown.presentation, pin) || shown.presentation.owner !== 'native' ||
            shown.presentation.status !== 'presented' || !samePin(session.info(), pin) ||
            !Number.isSafeInteger(ticket) || !Number.isSafeInteger(pin.contentRevision) || pin.contentRevision < 0 ||
            typeof pin.documentSnapshotId !== 'string' ||
            settings.perspectiveEnabled !== undefined && typeof settings.perspectiveEnabled !== 'boolean') return null;
        return { session: session, ticket: ticket, instanceId: pin.instanceId, documentId: pin.documentId,
          lifecycleGeneration: pin.lifecycleGeneration, revision: pin.contentRevision,
          snapshot: pin.documentSnapshotId, value: !!settings.perspectiveEnabled };
      } catch (_) { return null; }
    }
    function sameOwner(a, b) {
      return a && b && a.session === b.session && a.ticket === b.ticket && a.instanceId === b.instanceId &&
        a.documentId === b.documentId && a.lifecycleGeneration === b.lifecycleGeneration;
    }
    return Object.assign({}, step, {
      before: function (win) {
        baseline = win === root && stillCurrent() ? sample() : null;
      },
      check: function (win) {
        if (win !== root || !stillCurrent()) return false;
        var next = sample();
        if (!next) return false; // Unavailable does not become a false baseline.
        if (!sameOwner(baseline, next)) { baseline = next; return false; }
        if (next.revision === baseline.revision && next.snapshot === baseline.snapshot) return false;
        if (next.revision <= baseline.revision || next.snapshot === baseline.snapshot) { baseline = next; return false; }
        var changed = next.value !== baseline.value;
        baseline = next;
        return changed;
      }
    });
  }

  function create(root, onFailure) {
    var doc = root.document, top = doc.getElementById('top-area'), canvas = doc.getElementById('canvas-area');
    var drawing = doc.getElementById('drawing-canvas'), props = doc.getElementById('props-panel');
    var token = 0, ready = true, pending = false, space = null, card = null, target = null, corner = null, launcher = false;
    var saved = null, observer = null, geometry = '', buttons = [];
    function nativeMode() {
      var native = root.NemoNativeOpacityCutover;
      if (!native && !root.NemoNativeOpacityProject) return false;
      if (!native) return true;
      try {
        if (native.isActive()) return true;
        // The lifecycle's released/initial state exposes null identities and
        // blocksLegacy=false. Inactive, disconnected or indeterminate owners
        // cannot silently put a centered launcher over an attached native view.
        return !(native.identity() === null && native.getNativeIdentity() === null &&
          typeof native.blocksLegacy === 'function' && native.blocksLegacy() === false);
      } catch (_) { return true; }
    }
    function notice(error) { if (root.showToast) root.showToast(error.message); }
    function enable(value) {
      buttons.forEach(function (item) { item.button.disabled = value ? item.disabled : true; });
      if (card) card.classList.toggle('tutorial-native-pending', !value);
    }
    function stamp() {
      if (!space) return '';
      var a = space.getBoundingClientRect(), b = drawing.getBoundingClientRect();
      return [a.left, a.top, a.width, a.height, b.left, b.top, b.width, b.height].join('/');
    }
    function notifyResize() { root.dispatchEvent(new root.Event('resize')); }
    async function present() {
      notifyResize(); await boundary(root);
      var native = root.NemoNativeOpacityCutover;
      if (!native || !native.isActive()) throw new Error('The native document is unavailable.');
      var pin = native.identity(), view = native.getNativeIdentity(), frame = root.state.currentFrame;
      var receipt = await native.presentPreview(frame);
      if (!matches(pin, view, frame, receipt, native.identity(), native.getNativeIdentity(), root.state.currentFrame)) {
        throw new Error('Could not present the current document after placing the tutorial.');
      }
    }
    function releaseLayout() {
      if (!space) return false;
      space.remove(); space = null; top.classList.remove('tutorial-native-open');
      card.classList.remove('tutorial-native-card', 'tutorial-native-pending');
      saved.forEach(function (item) {
        if (item.value) card.style.setProperty(item.key, item.value, item.priority);
        else card.style.removeProperty(item.key);
      });
      saved = null; geometry = ''; enable(true);
      root.removeEventListener('resize', resized);
      if (observer) { observer.disconnect(); observer = null; }
      notifyResize(); return true;
    }
    function reserve() {
      if (!top || !canvas || !drawing || !props || doc.getElementById('history-column-space')) throw new Error('Close version History before opening the tutorial.');
      var area = canvas.getBoundingClientRect();
      var layout = plan(area.width + (space ? space.getBoundingClientRect().width : 0), top.getBoundingClientRect().height);
      if (!layout) throw new Error('Widen the window to view the tutorial.');
      if (!space) {
        saved = ['left', 'top', 'width', 'max-height'].map(function (key) {
          return { key: key, value: card.style.getPropertyValue(key), priority: card.style.getPropertyPriority(key) };
        });
        space = doc.createElement('div'); space.id = 'tutorial-native-space';
        space.setAttribute('aria-hidden', 'true'); top.appendChild(space);
        top.classList.add('tutorial-native-open'); card.classList.add('tutorial-native-card');
        root.addEventListener('resize', resized);
        if (root.ResizeObserver) {
          observer = new root.ResizeObserver(resized);
          [top, canvas, drawing, props].forEach(function (element) { observer.observe(element); });
        }
      }
      space.style.width = layout.width + 'px';
      var rect = space.getBoundingClientRect(), native = drawing.getBoundingClientRect();
      if (rect.width < 220 || rect.height < 200 || rect.left < 0 || rect.right > root.innerWidth ||
          native.width < 160 || native.height <= 0 || rect.left < native.right && rect.right > native.left && rect.top < native.bottom && rect.bottom > native.top) {
        throw new Error('The tutorial needs an uncovered column and a positive native viewport.');
      }
      card.style.left = (rect.left + 8) + 'px'; card.style.top = (rect.top + 8) + 'px';
      card.style.width = (rect.width - 16) + 'px'; card.style.maxHeight = (rect.height - 16) + 'px';
      geometry = stamp();
    }
    function controlsFit() {
      var area = space.getBoundingClientRect();
      // Clipped scrolling rows and the optional hidden Back button are not
      // fixed launcher controls. They remain disabled with the whole card
      // while pending, then are reached through the ordinary scrolling list.
      var controls = launcher ? ['#tut-launcher-close', '#tut-launcher-search', '#tut-ask-input', '#tut-ask-btn', '.modal-bdy']
        .map(function (selector) { return card.querySelector(selector); }) : buttons.map(function (item) { return item.button; });
      return controls.every(function (control) {
        if (!control) return false;
        var r = control.getBoundingClientRect();
        return r.width > 0 && r.height > 0 && r.left >= area.left && r.right <= area.right &&
          r.top >= area.top && r.bottom <= area.bottom && r.bottom <= root.innerHeight &&
          (!launcher || control !== card.querySelector('.modal-bdy') || r.height >= 24);
      });
    }
    async function place(nextCard, nextTarget, pinCorner, kind) {
      if (space && card !== nextCard) {
        await close(); notice(new Error('Close the current tutorial surface before opening another.'));
        if (onFailure) onFailure(); return 'failed';
      }
      var current = ++token; ready = false; pending = true;
      card = nextCard; target = nextTarget; corner = pinCorner; launcher = kind === 'launcher';
      buttons = Array.from(card.querySelectorAll('button,input')).map(function (button) { return { button: button, disabled: button.disabled }; });
      try {
        if (nativeMode()) {
          enable(false); reserve(); await present();
          if (current !== token) return 'stale';
          if (stamp() !== geometry) throw new Error('The tutorial layout changed while presenting the native viewport.');
          if (!controlsFit()) throw new Error('Resize the window to reach every tutorial control.');
          target(); enable(true);
        } else {
          var pos = positionTooltip(card.offsetWidth, card.offsetHeight, target(), corner, root.innerWidth, root.innerHeight);
          card.style.top = pos.top + 'px'; card.style.left = pos.left + 'px';
        }
        ready = true; return 'ready';
      } catch (error) {
        if (current !== token) return 'stale';
        if (!releaseLayout()) enable(true);
        card.classList.remove('on'); notice(error);
        if (onFailure) onFailure();
        return 'failed';
      } finally { if (current === token) pending = false; }
    }
    function resized() {
      if (!space || pending || stamp() === geometry) return;
      place(card, target, corner, launcher ? 'launcher' : undefined);
    }
    function close() {
      ++token; ready = false; pending = false;
      if (releaseLayout() && nativeMode()) return present().then(function () { return true; }).catch(function (error) { notice(error); return false; });
      return Promise.resolve(true);
    }
    return Object.freeze({ place: place, close: close, ready: function () { return ready; } });
  }
  return Object.freeze({ create: create, plan: plan, matches: matches, positionTooltip: positionTooltip, perspectiveStep: perspectiveStep });
}));
