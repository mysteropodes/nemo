/* L10-J drop / paste / menu entry points of the native importers. A pure router (the decisions of the baseline `handleDrop` of
 * drop-import.js and `onPaste` of svg-import.js, mirrored by the accepted L10-C kernel) plus the DOM glue that feeds it. Routing is
 * ALL OR NOTHING: every non-empty group needs its provider before anything runs, so a drop that mixes an SVG with a file no native
 * provider can take applies nothing and says which group is blocked (typed data; the only wording is the baseline's own toast for the
 * native PNG-only reference). The SVG group goes to native-import-svg.js as ONE batch; no legacy importer is ever called. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportDrop = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  const SVG_MIME = 'image/svg+xml';
  const TOAST_PREFIX = 'Import : ';
  // reference-bridge.js `importFiles` toast under the accepted native PNG-only reference owner (L10-C NATIVE_REFERENCE_DROP_TOAST).
  const NATIVE_REFERENCE_DROP_TOAST = 'Use Import Reference to choose a PNG file for this native project.';
  // The order handleDrop calls the importers in, and the order its toast lists the groups in (they differ on purpose).
  const EXECUTION_ORDER = Object.freeze(['audio', 'svg', 'image', 'video', 'other']);
  const TOAST_ORDER = Object.freeze(['audio', 'image', 'svg', 'video', 'other']);
  // What would take a group natively; the task ids are the lane-10 inherited provider dependencies (L10-C Provider::task_ids).
  const PROVIDERS = Object.freeze({
    audio: Object.freeze({ name: 'audio-tracks', tasks: Object.freeze(['G30']) }),
    svg: Object.freeze({ name: 'vector-import', tasks: Object.freeze(['G35']) }),
    image: Object.freeze({ name: 'raster-assets', tasks: Object.freeze(['G02', 'G27']) }),
    video: Object.freeze({ name: 'video-media', tasks: Object.freeze(['G28', 'G42']) }),
    other: Object.freeze({ name: 'native-reference', tasks: Object.freeze([]) }),
  });
  const PART = { audio: (n) => `${n} piste(s) audio`, image: (n) => `${n} image(s)`, svg: (n) => `${n} SVG (vecteurs)`, video: (n) => `${n} vidéo(s)`, other: () => 'référence' };

  // `isSvgFile`: type === 'image/svg+xml' || /\.svg$/i.test(name) (ASCII case folding only).
  function isSvgFile(f) { return !!f && (f.type === SVG_MIME || /\.svg$/i.test(f.name || '')); }
  function looksLikeSvg(t) { return typeof t === 'string' && /<svg[\s>]/i.test(t); }
  // `isDraggingFiles`: only the exact, case-sensitive 'Files' type opens the gate.
  function isDraggingFiles(e) { return !!(e && e.dataTransfer) && Array.prototype.indexOf.call(e.dataTransfer.types || [], 'Files') >= 0; }

  // The five `files.filter(...)` predicates of handleDrop, SVG peeled off before the image pile and out of `other`. They overlap for a
  // `.svg`-named file typed audio/* or video/* (the baseline imports it twice); nothing is ever dropped.
  function memberships(file) {
    const type = file.type, svg = isSvgFile(file);
    const audio = type.indexOf('audio/') === 0, image = type.indexOf('image/') === 0, video = type.indexOf('video/') === 0;
    return { audio, svg, image: image && !svg, video, other: !audio && !image && !video && !svg };
  }
  function planGroups(files) {
    if (!files.length) return null;
    const all = files.map(memberships);
    const groups = [];
    EXECUTION_ORDER.forEach((group) => {
      const indices = files.map((_, i) => i).filter((i) => all[i][group]);
      if (indices.length) groups.push({ group, indices });
    });
    return groups.length ? { groups, fileCount: files.length } : null;
  }
  function toastText(plan) {
    const parts = [];
    TOAST_ORDER.forEach((group) => {
      const entry = plan.groups.find((g) => g.group === group);
      if (entry) parts.push(PART[group](entry.indices.length));
    });
    return TOAST_PREFIX + parts.join(' + ');
  }
  // providers: { audio, svg, image, video: boolean (available), reference: 'native-png-only' | 'absent' }.
  function route(files, providers) {
    const plan = planGroups(files);
    if (!plan) return { kind: 'empty' };
    const blocked = [];
    plan.groups.forEach((g) => {
      let reason = null;
      if (g.group === 'other') reason = providers.reference === 'absent' ? 'reference-owner-absent' : 'use-import-reference';
      else if (providers[g.group] !== true) reason = 'provider-unavailable';
      if (reason) blocked.push({ group: g.group, provider: PROVIDERS[g.group].name, tasks: PROVIDERS[g.group].tasks.slice(), reason, indices: g.indices.slice() });
    });
    return blocked.length ? { kind: 'unavailable', fileCount: plan.fileCount, blocked } : { kind: 'ready', plan };
  }

  // `onPaste` decisions. `skip` names why nothing is handled; a handled paste carries either the pasted file or the extracted markup.
  function routePaste(e) {
    const t = e.target;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return { handled: false, skip: 'editable-target' };
    const cd = e.clipboardData;
    if (!cd) return { handled: false, skip: 'no-clipboard-data' };
    const items = Array.prototype.slice.call(cd.items || []);
    const svgItem = items.filter((i) => i.type === SVG_MIME && i.kind === 'file')[0];
    const file = svgItem ? svgItem.getAsFile() : null;
    let txt = file ? null : (cd.getData('text/plain') || '');
    if (!file && !looksLikeSvg(txt)) txt = cd.getData('text/html') || '';
    if (!file && !looksLikeSvg(txt)) return { handled: false, skip: 'no-svg' };
    if (file) return { handled: true, kind: 'file', file };
    const m = txt.match(/<svg[\s\S]*?<\/svg\s*>/i);
    return { handled: true, kind: 'markup', markup: m ? m[0] : txt };
  }

  // ports: svg (native-import-svg.js instance), svgAvailable(), notify(msg), providers {audio|image|video: {available(), importFiles(files)}},
  // reference ('native-png-only' by default), cancelPasteFallback(). None of them is a legacy importer.
  function create(ports) {
    const notify = ports.notify || function () {};
    const reference = ports.reference || 'native-png-only';
    const slot = (name) => (ports.providers && ports.providers[name]) || null;
    function availability() {
      const on = (name) => { const p = slot(name); return !!p && p.available() === true; };
      return { audio: on('audio'), svg: !!ports.svg && typeof ports.svgAvailable === 'function' && ports.svgAvailable() === true, image: on('image'), video: on('video'), reference };
    }
    async function run(files, plan) {
      const done = [];
      for (const { group, indices } of plan.groups) {
        const subset = indices.map((i) => files[i]);
        if (group === 'svg') done.push({ group, result: await ports.svg.importFiles(subset) });
        else if (group === 'audio' || group === 'video') for (const f of subset) done.push({ group, result: await slot(group).importFiles([f]) });
        else done.push({ group, result: await slot(group).importFiles(subset) });
      }
      return done;
    }
    async function drop(fileList) {
      const files = Array.prototype.slice.call(fileList || []);
      const routed = route(files, availability());
      if (routed.kind === 'empty') return { status: 'empty' };
      if (routed.kind === 'unavailable') {
        if (routed.blocked.some((b) => b.reason === 'use-import-reference')) notify(NATIVE_REFERENCE_DROP_TOAST);
        return { status: 'unavailable', blocked: routed.blocked, applied: [] };
      }
      notify(toastText(routed.plan));
      return { status: 'routed', plan: routed.plan, applied: await run(files, routed.plan) };
    }
    // Media-library/image-menu semantics: an SVG selected as an image stays an image.
    // Providers own native queue admission and correlated ACK/readback; this router never retries.
    async function media(fileList, options) {
      options = options || {};
      const files = Array.prototype.slice.call(fileList || []);
      const groups = ['image', 'video'].map((group) => ({ group, indices: files.map((_, i) => i).filter((i) =>
        options.kind ? options.kind === group : (files[i].type || '').indexOf(group + '/') === 0) })).filter((g) => g.indices.length);
      if (!groups.length) return { status: 'empty' };
      const available = availability();
      const blocked = groups.filter((g) => !available[g.group]).map((g) => ({
        group: g.group, provider: PROVIDERS[g.group].name, tasks: PROVIDERS[g.group].tasks.slice(),
        reason: 'provider-unavailable', indices: g.indices.slice(),
      }));
      if (blocked.length) return { status: 'unavailable', blocked, applied: [] };
      const applied = [];
      for (const entry of groups) {
        const batches = entry.group === 'video' ? entry.indices.map((i) => [files[i]]) : [entry.indices.map((i) => files[i])];
        for (const batch of batches) {
          if (options.current && !options.current(applied.length === 0)) return { status: 'stale', reason: 'context_changed', applied };
          let result;
          try { result = await slot(entry.group).importFiles(batch); }
          catch (_) { return { status: 'unconfirmed', reason: 'native_media_failed', applied }; }
          if (!result || result.status !== 'imported' || result.readback !== 'confirmed') {
            const refused = result && ['busy','stale','refused','unavailable','invalid','too_large','unconfirmed','cancelled'].includes(result.status);
            return { status: refused ? result.status : 'unconfirmed', group: entry.group, result, applied };
          }
          applied.push({ group: entry.group, result });
          if (options.current && !options.current(false)) return { status: 'stale', reason: 'context_changed', applied };
        }
      }
      return { status: 'routed', applied };
    }
    function onDragOver(e) {
      if (!isDraggingFiles(e)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
      e.currentTarget.classList.add('drop-hover');
    }
    function onDragLeave(e) { e.currentTarget.classList.remove('drop-hover'); }
    // `ports.surface(result)` lets the host turn a blocked drop into a user-visible line; it never changes the result.
    const surfaced = (result) => { if (ports.surface) ports.surface(result); return result; };
    function onDrop(e) {
      if (!isDraggingFiles(e)) return Promise.resolve({ status: 'ignored' });
      e.preventDefault();
      e.currentTarget.classList.remove('drop-hover');
      return drop(e.dataTransfer.files).then(surfaced);
    }
    async function onPaste(e) {
      const decision = routePaste(e);
      if (!decision.handled) return { status: 'ignored', skip: decision.skip };
      e.preventDefault();
      // Cmd+V armed a "paste frames" fallback before letting the default through; real SVG arrived, so cancel it.
      if (ports.cancelPasteFallback) ports.cancelPasteFallback();
      if (!ports.svg) return { status: 'unavailable', reason: 'vector_import_absent' };
      const result = decision.kind === 'file' ? await ports.svg.importFiles([decision.file]) : await ports.svg.importText(decision.markup);
      return { status: 'handled', kind: decision.kind, result };
    }
    // Wires the baseline drop targets; the ids are the ones drop-import.js wired. Returns a function that removes every listener.
    function attach(doc, ids) {
      const detach = [];
      (ids || ['canvas-area', 'timeline-area']).forEach((id) => {
        const el = doc.getElementById(id);
        if (!el) return;
        [['dragover', onDragOver], ['dragleave', onDragLeave], ['drop', onDrop]].forEach(([type, fn]) => {
          el.addEventListener(type, fn);
          detach.push(() => el.removeEventListener(type, fn));
        });
      });
      doc.addEventListener('paste', onPaste);
      detach.push(() => doc.removeEventListener('paste', onPaste));
      return () => detach.forEach((fn) => fn());
    }
    return Object.freeze({ drop, media, onDragOver, onDragLeave, onDrop, onPaste, attach, availability, openSvg: (opts) => (ports.svg ? ports.svg.openFile(opts) : Promise.resolve({ status: 'unavailable', reason: 'vector_import_absent' })) });
  }
  // null means this is not a native-owned action. Every native-owned outcome is a Promise,
  // including unavailable/refused states, so callers cannot fall through into legacy writers.
  function desktopMedia(host, files, kind) {
    const project = host.NemoNativeObjectProject;
    let session;
    try { session = project && project.desktop(host); if (!session || !session.blocksLegacy()) return null; }
    catch (_) { return Promise.resolve({ status: 'unavailable', reason: 'native_session_unavailable' }); }
    const controller = host.NemoNativeImportController;
    const surface = (result) => {
      try {
        if (controller && typeof controller.surface === 'function') controller.surface(result);
        else if (result.status !== 'routed' && result.status !== 'empty' && host.showToast) host.showToast('Native media import is unavailable.');
      } catch (_) {}
      return result;
    };
    return (async () => {
      if (!controller || !controller.drop || typeof controller.drop.media !== 'function') return { status: 'unavailable', reason: 'native_media_route_absent' };
      if (files === null) return { status: 'unavailable', reason: 'native_media_picker_absent', blocked: [{ group: kind, reason: 'provider-unavailable' }], applied: [] };
      const keys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
      const info = session.info();
      const origin = info && Object.fromEntries(keys.map((key) => [key, info[key]]));
      const state = host.state, frame = state && state.currentFrame, layer = state && state.activeLayerIdx;
      const current = (first) => {
        try {
        const now = session.info();
        return project.desktop(host) === session && session.isActive() && origin && now &&
          keys.slice(0, first ? keys.length : 3).every((key) => now[key] === origin[key]) &&
          (!state || (host.state === state && state.currentFrame === frame && (!first || state.activeLayerIdx === layer)));
        } catch (_) { return false; }
      };
      return controller.drop.media(files, { kind, current });
    })().catch(() => ({ status: 'unconfirmed', reason: 'native_media_failed', applied: [] })).then(surface);
  }
  // File-input reset and dispatch are the same for the ordinary image/video controls.
  function fileInput(event, kind, dispatch) {
    const files = Array.prototype.slice.call(event.target.files);
    event.target.value = '';
    if (kind === 'video' && !files.length) return;
    return dispatch(files);
  }
    // One at a time, not Promise.all (2026-09, feedback #806): each import
    // creates a layer, decodes and writes frames through the same shared
    // state — running them concurrently would interleave those writes. The
    // sequential loop keeps the picked order in the layer stack too.
  async function videoFiles(files, pickInfos, picked, file, notify) {
    var okN=0,failed=[];
    for(var i=0;i<files.length;i++){
      try{
        if(pickInfos&&pickInfos[i])await picked(pickInfos[i]);
        else await file(files[i]);
        okN++;
      }catch(e){failed.push((files[i]&&files[i].name)||('#'+(i+1)));}
    }
    if(files.length>1)notify(failed.length?(okN+'/'+files.length+' vidéos importées — échec : '+failed.join(', ')):(okN+' vidéos importées'),failed.length?'warn':'success');
    else if(failed.length)notify('Import vidéo échoué : '+failed[0],'warn');
  }
  // The complete OS-file media-library dispatch; native ownership is tested before any legacy callback.
  async function libraryMedia(host, files, legacyImage, legacyVideo) {
    const native = desktopMedia(host, files);
    if (native) {
      const result = await native;
      if (result.status === 'empty' && host.showToast) host.showToast(host.SM.t('toastFormatNotRecognizedDropHint'));
      return result;
    }
    const images = files.filter((file) => file.type.indexOf('image/') === 0);
    const videos = files.filter((file) => file.type.indexOf('video/') === 0);
    if (images.length && typeof legacyImage === 'function') legacyImage(images);
    videos.forEach((file) => { if (typeof legacyVideo === 'function') legacyVideo(file); });
    if (!images.length && !videos.length && host.showToast) host.showToast(host.SM.t('toastFormatNotRecognizedDropHint'));
  }
  return Object.freeze({
    desktopMedia, fileInput, videoFiles, libraryMedia, EXECUTION_ORDER, TOAST_ORDER, PROVIDERS, NATIVE_REFERENCE_DROP_TOAST, TOAST_PREFIX,
    isSvgFile, looksLikeSvg, isDraggingFiles, memberships, planGroups, toastText, route, routePaste, create,
  });
}));
