/* L10-J desktop composition of the native importers: the ports of native-import-core.js from the presented native Project session, the
 * Paper / REST / ag-psd / dialog ports, the drop, paste, menu, PSD and Figma panel bindings of the baseline `init()` functions, and a
 * `SMSvgImport`-shaped facade so the two legacy callers (timeline.js: menu entry and the Cmd+V paste fallback) reach the native adapter
 * without any legacy importer script. UI intent only: every outcome is a typed status; there is no legacy fallback writer. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else { root.NemoNativeImportDesktop = api; api.install(root); }
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const installed = new WeakMap();
  const TOKEN_KEY = 'sm-figma-token';

  // The user-visible line for a typed status that is not a success (success wording is the baseline's own toast, raised by the adapters).
  function noticeFor(result) {
    if (!result) return null;
    if (result.reason === 'native_mixed_import_batch_unavailable') return 'Import PNG images separately from other file groups. Nothing was imported.';
    if (result.reason === 'native_sequence_unavailable') return 'Image sequences are unavailable. Nothing was imported.';
    if (result.reason === 'native_png_only') return 'Only PNG images can be imported here. Nothing was imported.';
    if (result.blocked) {
      // The native PNG-only reference group already raised the baseline toast; every other blocked group needs a provider.
      const groups = result.blocked.filter((b) => b.reason !== 'use-import-reference').map((b) => b.group);
      return groups.length ? `Nothing was imported: ${groups.join(', ')} cannot be imported natively yet.` : null;
    }
    switch (result.status) {
      case 'unavailable': return result.unavailable && result.unavailable.length
        ? `This import needs ${result.unavailable.join(', ')}, which the native engine does not provide yet. Nothing was imported.`
        : 'Native import is unavailable.';
      case 'unconfirmed': return 'The import was not confirmed. Do not retry until the document is synchronized.';
      case 'too_large': return `The import is larger than the native request bound (${result.limit} bytes). Nothing was imported.`;
      // reason 'layer': the baseline toast (toastInvalidLockedLayer) was already raised by the adapter.
      case 'refused': return result.reason === 'layer' ? null : `The native engine refused the import (${result.code || result.reason}). Nothing was imported.`;
      case 'invalid': return `The import was refused before sending (${result.reason}). Nothing was imported.`;
      case 'stale': return 'The document changed during the import. Nothing was imported.';
      case 'busy': return 'Another import is still running.';
      case 'nothing': return null;
      default: return null;
    }
  }

  // `overrides` replaces named ports (readPsd, pickFile, resizeCanvas, readText, imageRoute, videoRoute) and exists for the tests of the composition only.
  function install(root, overrides) {
    if (installed.has(root)) return installed.get(root);
    overrides = overrides || {};
    // The importer modules are classic scripts loaded before this one; this file reads them from the window, it requires nothing.
    const m = { core: root.NemoNativeImportCore, svg: root.NemoNativeImportSvg, drop: root.NemoNativeImportDrop, figma: root.NemoNativeImportFigma,
      psd: root.NemoNativeImportPsd, montage: root.NemoNativeImportMontage };
    if (!Object.values(m).every(Boolean)) throw new Error('Native importer modules are unavailable');
    const current = () => root.NemoNativeObjectProject && root.NemoNativeObjectProject.desktop(root);
    function selection() {
      const state = root.state, indices = root._layerSel;
      if (!state || !Array.isArray(state.layers) || !Array.isArray(indices)) return null;
      const uids = indices.map((i) => Number.isSafeInteger(i) && i >= 0 && state.layers[i] && state.layers[i].layerUid);
      return uids.every((u) => typeof u === 'string' && u) && new Set(uids).size === uids.length ? uids : null;
    }
    const core = m.core.create({
      currentSession: current, frame: () => root.state && root.state.currentFrame, selection,
      available: (session, info) => !!(session && session === current() && session.isActive() && root.NemoNativeObjectProjectFile &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info) === true),
      select: (uid, info, session) => {
        if (current() !== session) return false;
        const index = info.layers.findIndex((row) => row.layerUid === uid);
        return index >= 0 && root.NemoNativeObjectProject.selectLayerUI(root, index, false, uid, info) === true;
      },
    });
    const t = (key) => (root.SM && typeof root.SM.t === 'function' ? root.SM.t(key) : key);
    const notify = (message) => { try { if (typeof root.showToast === 'function') root.showToast(message); } catch (_) { /* a toast never fails an import */ } };
    const surface = (result) => {
      const retained = result && result.staging && result.staging.status === 'retained' ? result : null;
      if (retained) notify(retained.status === 'imported'
        ? 'The import is confirmed, but temporary resources still need cleanup.'
        : 'Temporary import resources remain retained until the outcome is confirmed.');
      if (result && result.staging && result.status === 'imported' && result.readback !== 'confirmed') notify('The import committed, but its view is not confirmed. Wait for synchronization before importing again.');
      const line = noticeFor(result); if (line) notify(line); return result;
    };
    let stage = null;
    try {
      const module = root.NemoNativeImportPngStage, tauri = root.__TAURI__;
      if (module && typeof module.create === 'function' && tauri && root.crypto && typeof root.crypto.randomUUID === 'function') {
        stage = module.create({ core, fs: tauri.fs, path: tauri.path, uuid: () => root.crypto.randomUUID() });
      }
    } catch (_) { /* Missing staging ports are explicit unavailability, never a legacy writer. */ }
    const paper = () => { const P = root.paper; return { get project() { return P.project; }, Matrix: P.Matrix, Path: P.Path, CompoundPath: P.CompoundPath, Segment: P.Segment, Point: P.Point }; };
    const readText = overrides.readText || ((file) => (typeof file.text === 'function' ? file.text() : new Promise((resolve, reject) => {
      const r = new root.FileReader();
      r.onload = () => resolve(String(r.result || '')); r.onerror = () => reject(r.error || new Error('FileReader failed'));
      r.readAsText(file);
    })));
    const pickFile = overrides.pickFile || (async () => {
      if (root.__TAURI__) {
        const path = await root.__TAURI__.dialog.open({ title: 'Importer un SVG', multiple: false, filters: [{ name: 'SVG', extensions: ['svg'] }] });
        return path ? { name: String(path), text: await root.__TAURI__.fs.readTextFile(path) } : null;
      }
      return new Promise((resolve) => {
        const input = root.document.createElement('input');
        input.type = 'file'; input.accept = '.svg,image/svg+xml'; input.style.display = 'none';
        input.addEventListener('change', async () => {
          const file = input.files && input.files[0];
          input.remove();
          resolve(file ? { name: file.name, text: await readText(file) } : null);
        });
        root.document.body.appendChild(input);
        input.click();
      });
    });
    const activeLayerUid = () => { const s = root.state; return s && Array.isArray(s.layers) && s.layers[s.activeLayerIdx] ? s.layers[s.activeLayerIdx].layerUid : null; };
    const resizeCanvas = overrides.resizeCanvas || (async (width, height) => {
      const c = root.NemoNativeProjectSettingsController;
      if (!c || typeof c.edit !== 'function') return 'unavailable';
      return (await c.edit({ kind: 'setCanvasSize', width, height })).status;
    });
    const svgRaw = m.svg.create({ paper: paper(), core, activeLayerUid, t, notify, readText, pickFile });
    const svg = Object.freeze({
      importText: (text, opts) => svgRaw.importText(text, opts).then(surface), importFiles: (files, opts) => svgRaw.importFiles(files, opts).then(surface),
      importTexts: (items, opts) => svgRaw.importTexts(items, opts).then(surface), openFile: (opts) => svgRaw.openFile(opts).then(surface),
    });
    // The ordinary image menu keeps its native picker; DOM PNGs use owned staging for one batch.
    // PNG placement uses the declared native route; tests may explicitly disable it.
    const imageDeclared = overrides.imageRoute === undefined || overrides.imageRoute === true;
    let imagePending = false, imageRetired = false, imageUncertain = null;
    const pinKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
    const samePins = (a, b) => !!a && !!b && pinKeys.every((key) => a[key] === b[key]);
    const sameSelection = (a, b) => Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((uid, i) => uid === b[i]);
    const sameImageOwner = (a, b) => !!a && !!b && pinKeys.slice(0, 3).every((key) => a[key] === b[key]);
    function uncertainImageStatus() {
      if (!imageUncertain) return null;
      try {
        const session = current(), now = session && session.info();
        // Absence, temporary unavailability or an ordinary revision/frame change never proves a reopen.
        if (session && session.isActive() && now && typeof now.instanceId === 'string' && now.instanceId.length > 0 && typeof now.documentId === 'string' && now.documentId.length > 0 &&
          Number.isSafeInteger(now.lifecycleGeneration) && now.lifecycleGeneration > 0 && !sameImageOwner(now, imageUncertain.ctx.before)) imageUncertain = null;
      } catch (_) { /* preserve the unresolved native disposition */ }
      if (!imageUncertain) return null;
      const { ctx, error, receipt, lastFailure } = imageUncertain;
      return { status: 'unconfirmed', code: error && error.code || 'image_presentation_unconfirmed',
        requestId: error && error.requestId || receipt && receipt.requestId || null,
        owner: Object.fromEntries(pinKeys.slice(0, 3).map((key) => [key, ctx.before[key]])),
        receipt: receipt || null, details: lastFailure && lastFailure.details || error && error.details || null,
        recovery: error && typeof error.replay === 'function' ? 'exact-request' : receipt ? 'presentation' : 'reopen-owner' };
    }
    function uncertainImageError(error) {
      const details = error && error.details, code = error && error.code;
      const disposition = details && (Object.prototype.hasOwnProperty.call(details, 'disposition') || Object.prototype.hasOwnProperty.call(details, 'committed'));
      return !!(error && typeof error.replay === 'function') || code === 'asset_command_indeterminate' || code === 'uncorrelated' ||
        error && error.reason === 'reconcile_failed' || !code || !!(disposition &&
          !(details.disposition === 'notDispatched' && details.committed === false));
    }
    function finishImage(ctx, state, receipt, recovering) {
      const after = ctx.session.info();
      const shown = !imageRetired && current() === ctx.session && ctx.session.isActive() && sameImageOwner(after, ctx.before) &&
        after.contentRevision === receipt.contentRevision && receipt.contentRevision === ctx.before.contentRevision + 1 &&
        root.state === state && state.currentFrame === ctx.frame &&
        root.NemoNativeObjectProjectFile.projectionReadyForUI(root, after) === true && m.core.presented(ctx.session, after, ctx.frame);
      if (!shown || !after.layers.some((row) => row.layerUid === receipt.target.layerUid)) {
        imageUncertain = { ctx, state, receipt, error: null };
        return uncertainImageStatus() || { status: 'unconfirmed', code: 'image_owner_changed', receipt };
      }
      imageUncertain = null;
      const out = { status: 'imported', readback: 'confirmed', receipt, selected: false };
      // A late recovery resolves the receipt without taking the user's current selection.
      if (!recovering && sameSelection(selection(), ctx.selection)) {
        const index = after.layers.findIndex((row) => row.layerUid === receipt.target.layerUid);
        const consumed = root.NemoNativeObjectProject.selectLayerUI(root, index, false, receipt.target.layerUid, after) === true;
        out.selected = consumed && current() === ctx.session && samePins(ctx.session.info(), after) && state.currentFrame === ctx.frame &&
          sameSelection(selection(), [receipt.target.layerUid]) && m.core.presented(ctx.session, after, ctx.frame);
      }
      return out;
    }
    const importPngFiles = root.NemoNativeImportPngProvider ? root.NemoNativeImportPngProvider.create({
      core, stage, retired: () => imageRetired, declared: () => imageDeclared, pending: () => imagePending,
      setPending: value => { imagePending = value; }, uncertain: uncertainImageStatus,
      hold: (ctx, state) => { imageUncertain = {ctx,state,receipt:null,error:{code:'image_batch_unconfirmed'}}; },
      state: () => root.state, current, activeLayerUid, selection, samePins, sameSelection, notify, t,
      ready: info => root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info), presented: m.core.presented,
    }) : async () => ({status:'unavailable',reason:'native_drop_handle_absent'});
    const image = Object.freeze({
      available: () => !!stage && !!root.NemoNativeImportPngProvider && imageDeclared && !imageRetired,
      importFiles: stage ? files => importPngFiles(files).then(surface) : importPngFiles,
      status: uncertainImageStatus,
      async recover() {
        if (imageRetired) return { status: 'unavailable', reason: 'retired' };
        if (imagePending) return { status: 'busy' };
        const unresolved = uncertainImageStatus();
        if (!unresolved) return { status: 'nothing' };
        const held = imageUncertain;
        if (!held.receipt && !(held.error && typeof held.error.replay === 'function')) return unresolved;
        imagePending = true;
        try {
          const receipt = held.receipt || await held.error.replay();
          return finishImage(held.ctx, held.state, receipt, true);
        } catch (error) {
          // Even a later refusal cannot erase the earlier unknown commit or its exact-request recovery.
          held.lastFailure = error;
          return uncertainImageStatus() || { status: 'unavailable', reason: 'owner_reopened' };
        } finally { imagePending = false; }
      },
      async openFile() {
        if (imageRetired || !imageDeclared) return { status: 'unavailable', reason: 'native_image_route_absent' };
        if (imagePending) return { status: 'busy' };
        const unresolved = uncertainImageStatus();
        if (unresolved) return unresolved;
        const started = core.begin();
        if (!started.ok) return { status: started.status, reason: started.reason };
        const ctx = started.ctx, state = root.state, symbol = state.activeSymbolId, activeLayer = state.activeLayerIdx;
        if (typeof ctx.session.readSubscriptionGeneration !== 'function') return { status: 'unavailable', reason: 'native_subscription_identity_absent' };
        const subscriptionGeneration = ctx.session.readSubscriptionGeneration();
        const currentIntent = (queueScope) => !imageRetired && current() === ctx.session && ctx.session.isActive() && root.state === state &&
          state.currentFrame === ctx.frame && state.activeSymbolId === symbol && state.activeLayerIdx === activeLayer &&
          ctx.session.readSubscriptionGeneration() === subscriptionGeneration && samePins(ctx.session.info(), ctx.before) && sameSelection(selection(), ctx.selection) &&
          root.NemoNativeObjectProjectFile.projectionReadyForUI(root, ctx.before) === true &&
          (!queueScope || typeof queueScope.readPresentedObjectSession === 'function') && m.core.presented(queueScope || ctx.session, ctx.before, ctx.frame);
        const api = root.NemoNativeObjectRasterAsset;
        const client = api && api.desktop(root, { imageRoute: () => !imageRetired && imageDeclared });
        if (!client) return { status: 'unavailable', reason: 'native_image_client_absent' };
        imagePending = true;
        try {
          const receipt = await client.importStillPicked({ kind: 'newLayer' }, { origin: ctx.before, current: currentIntent });
          return receipt ? finishImage(ctx, state, receipt, false) : { status: 'cancelled' };
        } catch (error) {
          const code = error && error.code;
          if (uncertainImageError(error)) {
            imageUncertain = { ctx, state, error, receipt: null };
            return uncertainImageStatus() || { status: 'unconfirmed', code: code || 'native_image_failed', requestId: error && error.requestId };
          }
          if (code === 'stale_revision' || code === 'stale_owner') return { status: 'stale', code };
          if (code === 'unavailable') return { status: 'unavailable', code };
          if (code === 'invalid_request') return { status: 'invalid', code };
          return { status: 'refused', code };
        } finally { imagePending = false; }
      },
    });
    // T12 point 8: the video menu and the footage panel's Replace reach the native linked-video command; tests may disable it.
    const video = root.NemoNativeImportVideo ? root.NemoNativeImportVideo.create({ root, core, presented: m.core.presented, current, selection,
      declared: overrides.videoRoute === undefined || overrides.videoRoute === true }) : null;
    const drop = m.drop.create({
      svg, svgAvailable: () => core.begin().ok === true, providers: { image, video }, notify, surface,
      admitDrop: (_files, plan) => !plan.groups.some(row => row.group === 'image') || plan.groups.length === 1,
      cancelPasteFallback: () => { if (root._svgPasteFallback) { root.clearTimeout(root._svgPasteFallback); root._svgPasteFallback = null; } },
    });
    const figma = m.figma.create({ paper: paper(), core, stage, readCatalog: () => root.NemoFontAssets && root.NemoFontAssets.readCatalog(), t, notify, resizeCanvas, fetch: (...a) => root.fetch(...a) });
    // ag-psd stays an unprofiled vendor module: its one literal dynamic import lives in the retained legacy loader psd-import-bridge.js
    // (patch 13), which hands the cached module promise over as `window.SMAgPsdLoader.load`. This profiled adapter carries no dynamic import.
    const loadAgPsd = () => (root.SMAgPsdLoader && typeof root.SMAgPsdLoader.load === 'function'
      ? root.SMAgPsdLoader.load() : Promise.reject(new Error('The ag-psd loader is unavailable')));
    const psd = m.psd.create({ core, stage, t, notify, readPsd: overrides.readPsd || m.psd.readerFor(loadAgPsd) });
    const montage = m.montage.create({ declared: () => false });

    // The panel wiring of the baseline psd-import-bridge.js and figma-import.js `init()` (same ids, same strings).
    function bindPanels(doc) {
      const detach = [];
      const on = (el, type, fn) => { el.addEventListener(type, fn); detach.push(() => el.removeEventListener(type, fn)); };
      const btn = doc.getElementById('btn-import-psd'), input = doc.getElementById('psd-input');
      if (btn && input) {
        on(btn, 'click', () => input.click());
        on(input, 'change', (e) => {
          const file = e.target.files && e.target.files[0];
          input.value = '';
          if (!file) return;
          notify('Import PSD…');
          psd.importFile(file).then(surface);
        });
      }
      const tokenInput = doc.getElementById('figma-token'), tokenSave = doc.getElementById('figma-token-save'), urlInput = doc.getElementById('figma-url'), importBtn = doc.getElementById('figma-import-btn');
      if (tokenInput && importBtn) {
        try { tokenInput.value = root.localStorage.getItem(TOKEN_KEY) || ''; } catch (_) { /* storage may be blocked */ }
        if (tokenSave) on(tokenSave, 'click', () => { try { root.localStorage.setItem(TOKEN_KEY, tokenInput.value || ''); } catch (_) { /* ignore */ } notify('Token Figma enregistré'); });
        on(importBtn, 'click', () => {
          const token = (() => { try { return root.localStorage.getItem(TOKEN_KEY) || tokenInput.value; } catch (_) { return tokenInput.value; } })();
          const url = urlInput ? urlInput.value : '';
          if (!token) { notify('Colle ton token Figma personnel d\'abord'); return; }
          if (!url) { notify(t('hsFigmaPaste')); return; }
          figma.run(url, token).then(surface);
        });
      }
      return () => detach.forEach((fn) => fn());
    }
    function attach(doc) {
      const stopDrop = drop.attach(doc), stopPanels = bindPanels(doc);
      return () => { stopDrop(); stopPanels(); };
    }
    // What timeline.js reads from `window.SMSvgImport`: the menu entry and the existence check that arms the paste fallback.
    const facade = Object.freeze({
      native: true, openFile: () => svg.openFile(), importString: (text, opts) => svg.importText(text, opts),
      importFiles: (files, opts) => svg.importFiles(files, opts), isSvgFile: m.drop.isSvgFile,
    });
    const controller = Object.freeze({
      svg, drop, image, video, figma, psd, montage, core, attach, noticeFor, surface,
      openSvg: facade.openFile, importPsd: (file) => psd.importFile(file).then(surface),
      importFigma: (url, token, opts) => figma.run(url, token, opts).then(surface),
      dispose() { imageRetired = true; if (video) video.dispose(); core.dispose(); },
    });
    root.SMSvgImport = facade;
    root.NemoNativeImportController = controller;
    installed.set(root, controller);
    if (root.document) {
      if (root.document.readyState === 'loading') root.document.addEventListener('DOMContentLoaded', () => attach(root.document));
      else attach(root.document);
    }
    return controller;
  }
  return Object.freeze({ install, noticeFor });
}));
