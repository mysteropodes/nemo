'use strict';
// Controlled native invoke/CSS/DOM boundaries; Project, transport, queue and callers are production code.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const clone = value => JSON.parse(JSON.stringify(value));

function fixture({ composition = false, shadow = false, palette = false } = {}) {
  const events = new WeakMap(), trace = [], calls = [], notices = [], diagnostics = [], labels = [], timers = new Set(), frames = [];
  const nativeListeners = new Map(), observers = new Set(), previewReleases = new Set();
  let source = 'fixture', context, root, disposed = false, installed = false, generation = 0, serial = 0, previewHold = null;
  let compositionReadHold = null, compositionUnavailable = false;
  let shadowReadHold = null, shadowUnavailable = false;
  let canonicalPorts = null;
  let metadataControl = null;
  let rect = { left: 66, top: 102, width: 1038, height: 504 };
  const nativeDocument = { canvasW: 320, canvasH: 180, fps: 24, totalFrames: 40 };
  const originalDocument = JSON.stringify(nativeDocument);
  function node(tag = 'div') {
    const classes = new Set(), styles = new Map();
    const element = { tagName: tag.toUpperCase(), id: '', children: [], parentNode: null, style: {},
      classList: { add: key => classes.add(key), remove: key => classes.delete(key), contains: key => classes.has(key),
        toggle(key, on) { if (on === undefined) on = !classes.has(key); if (on) classes.add(key); else classes.delete(key); } },
      appendChild(child) { if (child.parentNode) child.remove(); this.children.push(child); child.parentNode = this; return child; },
      remove() { if (this.parentNode) this.parentNode.children = this.parentNode.children.filter(child => child !== this); this.parentNode = null; },
      addEventListener(kind, fn, options) {
        const records = events.get(this) || []; events.set(this, records);
        records.push({ kind, fn, capture: options === true || !!(options && options.capture), once: !!(options && options.once), source });
      },
      removeEventListener(kind, fn, options) {
        const capture = options === true || !!(options && options.capture);
        events.set(this, (events.get(this) || []).filter(record => record.kind !== kind || record.fn !== fn || record.capture !== capture));
      },
      getBoundingClientRect: () => ({ ...rect }),
      setAttribute(key, value) { this[key] = String(value); },
      getContext: () => ({ setTransform() {}, clearRect() {}, fillRect() {}, beginPath() {}, moveTo() {}, lineTo() {}, stroke() {},
        fillText(...args) { labels.push(args); }, save() {}, translate() {}, rotate() {}, restore() {} })
    };
    Object.assign(element.style, { getPropertyValue: key => styles.get(key) || '', getPropertyPriority: () => '',
      setProperty: (key, value) => styles.set(key, value), removeProperty: key => styles.delete(key) });
    Object.defineProperties(element, { clientWidth: { configurable: true, get: () => rect.width },
      clientHeight: { configurable: true, get: () => rect.height } });
    return element;
  }
  const document = node('document'), html = node('html'), body = node('body'), area = node(), canvas = node('canvas');
  const h = node('canvas'), v = node('canvas'), pill = node('input');
  area.id = 'canvas-area'; canvas.id = 'drawing-canvas'; h.id = 'ruler-h'; v.id = 'ruler-v'; pill.id = 'zoom-scrub';
  document.appendChild(html); html.appendChild(body); body.appendChild(area);
  [canvas, h, v, pill].forEach(element => area.appendChild(element));
  function descendants(element) { return [element, ...element.children.flatMap(descendants)]; }
  Object.assign(document, { documentElement: html, body, readyState: 'loading', hidden: false, activeElement: null,
    getElementById: id => descendants(document).find(element => element.id === id) || null, createElement: node });
  Object.defineProperties(html, { clientWidth: { get: () => 1400 }, clientHeight: { get: () => 900 } });
  function dispatch(target, kind, extra = {}) {
    let stopped = false, immediate = false;
    const event = { type: kind, target, currentTarget: null, eventPhase: 0, bubbles: true, defaultPrevented: false,
      pointerId: 1, button: 0, clientX: 466, clientY: 111, results: [], ...extra,
      preventDefault() { this.defaultPrevented = true; }, stopPropagation() { stopped = true; },
      stopImmediatePropagation() { immediate = true; stopped = true; } };
    const path = []; for (let element = target; element; element = element.parentNode) path.push(element);
    function invoke(element, capture, phase) {
      event.currentTarget = element; event.eventPhase = phase;
      for (const record of [...events.get(element) || []]) {
        if (record.kind !== kind || record.capture !== capture) continue;
        if (!(events.get(element) || []).includes(record)) continue;
        trace.push({ kind, target: target.id, current: element.id || element.tagName, phase, source: record.source });
        const prior = source; source = record.source;
        try { event.results.push(record.fn.call(element, event)); } finally { source = prior; }
        if (record.once) element.removeEventListener(kind, record.fn, record.capture);
        if (immediate) break;
      }
    }
    for (const element of path.slice(1).reverse()) { invoke(element, true, 1); if (stopped) return event; }
    invoke(target, true, 2); if (!immediate) invoke(target, false, 2);
    if (!stopped && event.bubbles) for (const element of path.slice(1)) { invoke(element, false, 3); if (stopped) break; }
    return event;
  }
  function info() {
    return { apiVersion: 2, instanceId: 'guide-instance', documentId: 'guide-document-' + generation,
      contentRevision: 0, documentSnapshotId: 'guide-snapshot-' + generation, lifecycleGeneration: generation,
      documentDigest: 'a'.repeat(64), byteCount: originalDocument.length, undoDepth: 0, redoDepth: 0, ...nativeDocument,
      layers: [{ layerUid: 'drawing', visible: true }], reference: null };
  }
  function realm(value) { return vm.runInContext('JSON.parse(' + JSON.stringify(JSON.stringify(value)) + ')', context); }
  function binding() {
    // native_revision_sync.rs RevisionBinding is closed and contains only these five scalar fields.
    const pin = info();
    return { instanceId: pin.instanceId, documentId: pin.documentId, lifecycleGeneration: pin.lifecycleGeneration,
      contentRevision: pin.contentRevision, subscriptionId: 'guide-subscription-' + generation };
  }
  function nativeView(request) {
    const z = request.navigation ? request.navigation.zoom : 1;
    const pan = request.navigation ? request.navigation.pan : [0, 0];
    // The controlled host boundary supplies the documented canvas-mode projection, not a JS viewport owner.
    const m = [z, 0, 0, z, 160 - z * 160 - z * pan[0], 90 - z * 90 - z * pan[1]];
    const [a, b, c, d, e, f] = m, determinant = a * d - b * c;
    const view = { mode: 'canvas', cameraAvailable: false, navigation: { zoom: z, pan: clone(pan) }, documentToComposition: m,
      compositionToDocument: [d / determinant, -b / determinant, -c / determinant, a / determinant,
        (c * f - d * e) / determinant, (b * e - a * f) / determinant] };
    return view;
  }
  async function invoke(command, args) {
    const request = args && (args.request || JSON.parse(args.requestJson));
    calls.push({ command, request: request && clone(request) });
    if (command === 'nemo_native_object_client_status') {
      // native_object_client.rs client_status deliberately omits project metadata.
      const operations = ['query.document.object', 'command.document.object.fill.set', 'command.document.object.undo',
        'command.document.object.redo', 'query.document.object.serialize', 'command.document.object.layer.visibility.set',
        'command.document.object.project.settings.edit', 'command.document.object.layer.policy.edit',
        'command.document.object.project.placement.edit', 'command.document.object.audio.track.edit', 'command.document.object.layer.cut'];
      return realm({ apiVersion: 2, instanceId: 'guide-instance', available: installed, route: 'nemo_native_object_dispatch',
        operations: installed ? operations : [], documentId: installed ? info().documentId : null,
        contentRevision: installed ? 0 : null, lifecycleGeneration: installed ? generation : null,
        publicCapabilityAvailable: installed, mcpAvailable: installed,
        saveAvailable: false, viewportAvailable: false, exportAvailable: false });
    }
    if (command === 'nemo_native_object_bootstrap') {
      installed = true; generation++;
      return realm({ apiVersion: 2, requestId: request.requestId, instanceId: 'guide-instance', documentId: info().documentId,
        contentRevision: 0, resourceCount: 0, viewportAvailable: false });
    }
    if (command === 'nemo_native_object_reference') {
      assert.equal(request.action.kind, 'inspect'); return realm({ ...info(), requestId: request.requestId });
    }
    if (command === 'nemo_native_object_preview') {
      if (previewHold) { const held = previewHold; previewHold = null; held.entered(); await held.wait; }
      return realm({ status: 'presented', workId: 'guide-work-' + (++serial), viewGeneration: serial, view: nativeView(request) });
    }
    if (command === 'nemo_native_status') {
      const status = { apiVersion: 2, requestId: 'guide-native-status-' + (++serial), instanceId: 'guide-instance', available: installed };
      if (installed) Object.assign(status, { documentId: info().documentId, contentRevision: 0 });
      else status.reason = 'native application is unavailable';
      return realm(status);
    }
    if (command === 'nemo_native_revision_sync') {
      if (request.action === 'binding') return realm(binding());
      if (request.action === 'subscribe') return realm(request.binding);
      assert(['disconnect', 'acknowledge'].includes(request.action)); return null;
    }
    if (command === 'nemo_native_dispatch') {
      const guideRead = request.operation === 'query.document.object.project.guide.read';
      const compositionRead = request.operation === 'query.document.object.project.composition.read';
      const shadowRead = request.operation === 'query.document.object.project.shadow.read';
      const paletteRead = request.operation === 'query.document.object.project.palette.read';
      assert(guideRead || composition && compositionRead || shadow && shadowRead || palette && paletteRead,
        'Cancellation must never dispatch an edit');
      const pin = info(), payload = request.payload, selector = payload.selector;
      assert.equal(payload.atRevision, 0); assert.equal(payload.documentSnapshotId, pin.documentSnapshotId);
      assert.equal(payload.contextId, 'scene-root');
      if (metadataControl) {
        const control = metadataControl, key = request.operation + ':' + JSON.stringify(selector);
        const count = (control.counts.get(key) || 0) + 1; control.counts.set(key, count);
        if (count > 1) {
          control.repeated({ operation: request.operation, selector: clone(selector) });
          // Hold an overrun until disposal so a failing regression cannot spin the real completion queue.
          await control.overrun;
          return realm({ apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId,
            documentId: pin.documentId, contentRevision: 0, ok: false,
            error: { code: 'unavailable', message: 'Controlled metadata budget refusal' } });
        }
        if (paletteRead && selector.kind === 'settings') {
          control.entered(); await control.wait;
          if (control.refuse) return realm({ apiVersion: 2, requestId: request.requestId,
            instanceId: pin.instanceId, documentId: pin.documentId, contentRevision: 0, ok: false,
            error: { code: 'unavailable', message: 'Controlled native Palette read refusal' } });
        }
      }
      if (compositionRead && selector.kind === 'settings') {
        if (compositionReadHold) {
          const held = compositionReadHold; compositionReadHold = null; held.entered(); await held.wait;
        }
        if (compositionUnavailable) return realm({ apiVersion: 2, requestId: request.requestId,
          instanceId: pin.instanceId, documentId: pin.documentId, contentRevision: 0, ok: false,
          error: { code: 'unavailable', message: 'Controlled native Composition read refusal' } });
      }
      if (shadowRead && selector.kind === 'settings') {
        if (shadowReadHold) {
          const held = shadowReadHold; shadowReadHold = null; held.entered(); await held.wait;
        }
        if (shadowUnavailable) return realm({ apiVersion: 2, requestId: request.requestId,
          instanceId: pin.instanceId, documentId: pin.documentId, contentRevision: 0, ok: false,
          error: { code: 'unavailable', message: 'Controlled native Shadow read refusal' } });
      }
      let result;
      if (paletteRead) {
        if (selector.kind === 'settings') result = { palettesPresent: false, paletteCount: 0, effectiveActivePaletteIdx: 0 };
        else {
          assert.equal(selector.kind, 'palettePage'); assert.equal(selector.cursor, 0); assert.equal(selector.limit, 16);
          result = { kind: 'palettePage', palettesPresent: false, paletteCount: 0, cursor: 0, items: [], nextCursor: null };
        }
      } else if (shadowRead) {
        if (selector.kind === 'settings') result = { shadowPalettePresent: false, shadowColorCount: 0 };
        else {
          assert.equal(selector.kind, 'palettePage'); assert.equal(selector.cursor, 0); assert.equal(selector.limit, 16);
          result = { kind: 'shadowPalettePage', shadowPalettePresent: false, shadowColorCount: 0,
            cursor: 0, items: [], nextCursor: null };
        }
      } else if (compositionRead) {
        if (selector.kind === 'settings') result = { markersPresent: false, markerCount: 0, bpm: 120, bpmOffset: 0, bpmShow: false,
          effectiveBpm: 120, effectiveBpmOffset: 0, effectiveBpmShow: false };
        else {
          assert.equal(selector.kind, 'markerPage'); assert.equal(selector.cursor, 0); assert.equal(selector.limit, 16);
          result = { kind: 'markerPage', markersPresent: false, markerCount: 0, cursor: 0, items: [], nextCursor: null };
        }
      } else if (selector.kind === 'settings') result = { guidesPresent: false, hPresent: false, vPresent: false, hCount: 0, vCount: 0 };
      else {
        assert.equal(selector.kind, 'axisPage'); assert(['h', 'v'].includes(selector.axis));
        assert.equal(selector.cursor, 0); assert.equal(selector.limit, 16);
        result = { kind: 'guideAxisPage', axis: selector.axis, guidesPresent: false, axisPresent: false,
          guideCount: 0, cursor: 0, items: [], nextCursor: null };
      }
      return realm({ apiVersion: 2, requestId: request.requestId, instanceId: pin.instanceId, documentId: pin.documentId,
        contentRevision: 0, ok: true, result: { instanceId: pin.instanceId, documentId: pin.documentId,
          atRevision: 0, documentSnapshotId: pin.documentSnapshotId, contextId: 'scene-root', result } });
    }
    throw Error('Unexpected controlled native boundary: ' + command);
  }
  function timeout(fn, delay, ...args) {
    const timer = setTimeout(() => { timers.delete(timer); if (!disposed) fn(...args); }, delay);
    timers.add(timer); return timer;
  }
  root = node('window');
  Object.assign(root, { document, state: { currentFrame: 0, tool: 'draw', cameraView: false, rulersOn: false,
    guidesLocked: false, guidesSnap: true, ...(shadow ? { shadowMode: false } : {}) }, devicePixelRatio: 2, stageLayer: { visible: true },
    paper: { view: { update() {} } }, performance, setTimeout: timeout,
    clearTimeout(timer) { timers.delete(timer); clearTimeout(timer); },
    requestAnimationFrame: fn => { frames.push(fn); return frames.length; },
    getComputedStyle: () => ({ getPropertyValue: () => '' }),
    localStorage: { getItem: () => '0', setItem() {} }, showToast: text => notices.push(text),
    __NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__: (event, code) => diagnostics.push([event, code]),
    Point: function Point(x, y) { this.x = x; this.y = y; },
    updateUI() {}, SMReference: { refreshNative() {} }, SMProject: { refreshActiveTabDirtyDot() {} },
    // Unused legacy service ports are explicit; no legacy document operation is admitted by this fixture.
    SM: { importJSON() { throw Error('Legacy import is outside the native Guide route'); } }, SMEngineBridge: {},
    NemoApplication: { handle() { throw Error('Legacy application dispatch is outside this fixture'); } },
    NemoOpacityApplication: { meta() { return null; }, legacy() { throw Error('Legacy opacity dispatch is outside this fixture'); } },
    ResizeObserver: class {
      constructor(callback) { this.callback = callback; observers.add(this); }
      observe(target) { this.target = target; }
      disconnect() { observers.delete(this); }
    },
    NemoNativeObjectUI: { refreshPolicy() {}, refreshTiming() {} },
    console: { error(...args) { notices.push(args.join(' ')); }, log() {} },
    __TAURI__: { core: { invoke }, event: { async listen(name, listener) {
      nativeListeners.set(name, listener); return () => nativeListeners.delete(name);
    } } } });
  // Paper transforms are poison: all native ruler/guide conversion must use the presented affine.
  Object.defineProperty(root, 'view', { get() { throw Error('Native production guide used Paper view'); } });
  root.window = root; root.globalThis = root; context = vm.createContext(root);
  const selected = [
    'js/application/native-opacity-contract.js', 'js/application/native-opacity-replacement.js',
    'js/application/native-opacity-export-workflow.js', 'js/application/native-opacity-preview-workflow.js',
    'js/application/native-opacity-v1.js', 'js/application/native-opacity-lifecycle.js', 'js/application/native-opacity-viewport.js',
    'js/adapters/native-frame-edit-contract.js', 'js/adapters/native-viewport-underlay.js',
    'js/adapters/native-object-project-metadata.js',
    'js/adapters/native-object-query-contract.js', 'js/adapters/native-object-project-reads.js',
    'js/adapters/native-object-project-properties.js',
    'js/adapters/native-object-project-desktop.js', 'js/adapters/native-object-open-exposure.js',
    'js/adapters/native-object-project-entry-contract.js',
    'js/adapters/native-object-project.js', 'js/adapters/native-object-layer-splice.js', 'js/adapters/native-object-project-file.js',
    'js/application/native-opacity-operations.js', 'js/adapters/native-opacity-legacy-surface.js',
    'js/draw-bridge.js', 'js/adapters/native-object-draw-paint.js', 'js/adapters/native-object-draw-symmetry.js', 'js/adapters/native-object-shape-tool.js', 'js/rulers-bridge.js',
    ...(composition ? ['js/adapters/native-composition-contract.js'] : []),
    ...(palette ? ['js/adapters/native-palette-contract.js'] : []),
    'js/adapters/native-guide-contract.js',
    ...(shadow ? ['js/adapters/native-shadow-contract.js'] : []),
    'js/adapters/native-application.js',
    ...(composition ? ['js/adapters/native-composition-desktop.js', 'js/adapters/native-composition-ui.js'] : []),
    ...(palette ? ['js/adapters/native-palette-desktop.js'] : []),
    'js/adapters/native-guide-desktop.js',
    ...(shadow ? ['js/adapters/native-shadow-desktop.js'] : []),
    ...(palette ? ['js/adapters/native-palette-ui.js'] : []), 'js/adapters/native-guide-ui.js',
    ...(shadow ? ['js/adapters/native-shadow-ui.js'] : []),
    'js/application/native-edit-guard.js', 'js/adapters/application-mcp.js'
  ];
  const htmlSource = fs.readFileSync(require.resolve('../../src/index.html'), 'utf8');
  const actual = [...htmlSource.matchAll(/<script\b[^>]*\bsrc="([^"]+)"[^>]*>/g)]
    .filter(match => selected.includes(match[1]));
  assert.deepEqual(actual.map(match => match[1]), selected, 'Use actual classic HTML order, including Draw before Rulers');
  for (const match of actual) {
    assert.equal(match[0], '<script src="' + match[1] + '">'); source = match[1];
    // Legacy MCP service is outside this native-v2 fixture. Loading its actual module must not fake a v1 service.
    if (source === 'js/adapters/application-mcp.js') {
      const tauri = root.__TAURI__; delete root.__TAURI__;
      try { vm.runInContext(fs.readFileSync(require.resolve('../../src/' + source), 'utf8'), context, { filename: source }); }
      finally { root.__TAURI__ = tauri; }
    } else vm.runInContext(fs.readFileSync(require.resolve('../../src/' + source), 'utf8'), context, { filename: source });
    if (composition && source === 'js/adapters/native-object-project-desktop.js') {
      const actual = root.NemoNativeObjectProjectDesktop;
      // Observation only: Project receives the exact production ports object, with no callback or state changed.
      root.NemoNativeObjectProjectDesktop = Object.freeze({ ...actual, createPorts(value) {
        const ports = actual.createPorts(value); assert.equal(canonicalPorts, null);
        canonicalPorts = ports; return ports;
      } });
    }
  }
  const tools = fs.readFileSync(require.resolve('../../src/js/tools.js'), 'utf8');
  const start = tools.indexOf('function nativeViewportNavigation('), end = tools.indexOf('\nvar _nativeToolNavigation=', start);
  assert(start >= 0 && end > start); source = 'js/tools.js';
  vm.runInContext(tools.slice(start, end), context, { filename: source });
  const browser = vm.runInContext('globalThis', context), session = browser.NemoNativeObjectProject.desktop(browser);
  // bootstrap/native-opacity-application -> Core.createNative -> these actual lifecycle/operations installers.
  // Invoke the same production installation seam without the unrelated Motion/service UI composition.
  const transport = browser.NemoApplicationMcpTransport.createNativeTauriTransport(browser.__TAURI__.core.invoke);
  const ports = browser.NemoNativeOpacityLegacySurface.desktopPorts(browser, transport);
  let operations;
  const lifecycle = browser.NemoNativeOpacityLifecycle.create(Object.assign({}, ports, {
    legacyImport() { return operations.reenterLegacy.apply(null, arguments); },
    onDisposed(value) { return operations.disposeSession(value); }, afterChange() { return operations.afterChange(); }
  }), browser.NemoNativeOpacityContract, browser.NemoNativeOpacityReplacement,
  browser.NemoNativeOpacityExportWorkflow, browser.NemoNativeOpacityPreviewWorkflow);
  operations = browser.NemoNativeOpacityOperations.create(lifecycle, ports, browser.NemoNativeOpacityContract,
    browser.NemoNativeOpacityViewport, browser.NemoNativeOpacityV1);
  const publication = operations.install();
  assert((events.get(browser) || []).some(record => record.kind === 'resize'));
  assert.equal(observers.size, 1, 'Actual LegacySurface publication must observe the drawing canvas');
  source = 'fixture'; dispatch(document, 'DOMContentLoaded'); document.readyState = 'complete';
  const drawRegistrations = (events.get(area) || []).filter(record => record.source === 'js/draw-bridge.js');
  assert(drawRegistrations.some(record => record.kind === 'pointerdown' && record.capture));
  assert(drawRegistrations.some(record => record.kind === 'pointermove' && record.capture));
  assert(drawRegistrations.some(record => record.kind === 'pointerup' && record.capture));
  function tick() { const frame = frames.shift(); if (frame && !disposed) frame(); }
  async function settle() {
    for (let attempt = 0; attempt < 80; attempt++) {
      await new Promise(resolve => setTimeout(resolve, 0)); tick();
      if (session.readPresentedObjectSession() && !browser.NemoNativeGuideController.isPending() &&
          (!composition || !browser.NemoNativeCompositionController.isPending()) &&
          (!shadow || !browser.NemoNativeShadowController.isPending()) && timers.size === 0) return;
    }
    assert.fail('Production Guide did not settle: ' + JSON.stringify({ calls, notices }));
  }
  async function open() {
    await session.openPath('/controlled/Guide.json', { async reveal(info) {
      browser.NemoNativeObjectProjectFile.projectView(browser, info); browser.updateUI();
    } });
    await settle();
  }
  async function prepare() {
    await open();
    rect = { left: 66, top: 102, width: 800, height: 450 };
    dispatch(browser, 'resize');
    for (const observer of observers) observer.callback([{ target: canvas }]);
    await settle();
    browser.SMRulers.toggleOn(true);
    assert.equal(browser.nativeViewportNavigation('reset').handled, true);
    await settle(); tick();
    assert(labels.length > 0, 'Real native Rulers must paint tick labels before the ordinary gesture');
  }
  async function dispose() {
    for (const release of previewReleases) release();
    dispatch(browser, 'pagehide'); dispatch(browser, 'beforeunload');
    publication.dispose();
    disposed = true; for (const timer of timers) clearTimeout(timer); timers.clear(); frames.length = 0;
    await new Promise(setImmediate);
  }
  return { root: browser, document, area, h, v, session, calls, trace, notices, diagnostics, labels, prepare, settle, open, tick, dispose,
    installImageMenu() {
      const button = node('button'); button.id = 'btn-import-img'; body.appendChild(button);
      for (const id of ['image-input', 'video-input', 'btn-import-video']) {
        const element = node(); element.id = id; body.appendChild(element);
      }
      for (const [name, file] of Object.entries({ Core: 'core', Svg: 'svg', Drop: 'drop', Figma: 'figma', Psd: 'psd', Montage: 'montage' })) {
        browser['NemoNativeImport' + name] = require('../../src/js/adapters/native-import-' + file + '.js');
      }
      browser.NemoNativeObjectRasterAsset = require('../../src/js/adapters/native-object-raster-asset.js');
      require('../../src/js/adapters/native-import-desktop.js').install(browser);
      source = 'js/images.js';
      vm.runInContext(fs.readFileSync(require.resolve('../../src/js/images.js'), 'utf8'), context, { filename: source });
      source = 'fixture';
      return async function click() {
        const event = dispatch(button, 'click');
        assert.equal(event.results.length, 1, 'The ordinary image button has exactly one real listener');
        return event.results[0];
      };
    },
    holdPaletteRead({ refuse = false } = {}) {
      assert(palette && !metadataControl);
      let entered, release, repeated, stopOverrun;
      const started = new Promise(resolve => { entered = resolve; });
      const wait = new Promise(resolve => { release = resolve; });
      const overrun = new Promise(resolve => { stopOverrun = resolve; });
      const repeatedQuery = new Promise(resolve => { repeated = resolve; });
      const cleanup = () => { release(); stopOverrun(); previewReleases.delete(cleanup); };
      previewReleases.add(cleanup);
      metadataControl = { entered, wait, overrun, repeated, refuse, counts: new Map() };
      return { started, repeated: repeatedQuery, release,
        counts: () => Array.from(metadataControl.counts.values()) };
    },
    dispatch, saved: () => JSON.stringify(nativeDocument), originalDocument,
    ghosts: () => descendants(document).filter(element => element.style.opacity === '.6'),
    edits: () => calls.filter(call => call.request && String(call.request.operation).startsWith('command.')),
    holdPreview() {
      let entered, release;
      const started = new Promise(resolve => { entered = resolve; });
      const wait = new Promise(resolve => { release = () => { previewReleases.delete(release); resolve(); }; });
      previewReleases.add(release); previewHold = { entered, wait };
      return { started, release };
    },
    refuseNextCompositionRead() {
      assert(composition && !compositionReadHold);
      let entered, release;
      const started = new Promise(resolve => { entered = resolve; });
      const wait = new Promise(resolve => { release = () => { previewReleases.delete(release); resolve(); }; });
      // This is a typed native response boundary, not a controller, cache or queue replacement.
      compositionUnavailable = true; previewReleases.add(release); compositionReadHold = { entered, wait };
      return { started, release };
    },
    refuseNextShadowRead() {
      assert(shadow && !shadowReadHold);
      let entered, release;
      const started = new Promise(resolve => { entered = resolve; });
      const wait = new Promise(resolve => { release = () => { previewReleases.delete(release); resolve(); }; });
      // Every later settings request also refuses; the fixture never suppresses a retry.
      shadowUnavailable = true; previewReleases.add(release); shadowReadHold = { entered, wait };
      return { started, release };
    },
    async notifyWhilePending() {
      assert(composition && canonicalPorts);
      assert.equal(session.readPresentedObjectSession(), null);
      // Explicit controlled notification boundary, not a claim that installed input delivered this event.
      canonicalPorts.presented(session.info());
      for (let attempt = 0; attempt < 80; attempt++) {
        await new Promise(resolve => setTimeout(resolve, 0)); tick();
        if (!browser.NemoNativeGuideController.isPending() && !browser.NemoNativeCompositionController.isPending() && timers.size === 0) return;
      }
      assert.fail('Controlled production fanout did not settle while resource remained held');
    } };
}

module.exports = { fixture };
