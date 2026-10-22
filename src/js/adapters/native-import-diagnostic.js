/* Optional post-result native import diagnostics. Only fixed vocabulary and boolean
 * observations reach disk; diagnostic failure never changes an import result. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeImportDiagnostic = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const pinKeys = ['instanceId', 'documentId', 'lifecycleGeneration', 'contentRevision', 'documentSnapshotId'];
  const samePins = (a, b) => !!a && !!b && pinKeys.every((key) => a[key] === b[key]);
  const reason = (value) => [
    'retired', 'native_image_route_absent', 'native_media_route_absent', 'native_media_picker_absent',
    'no_presented_native_session', 'native_declaration_absent', 'native_subscription_identity_absent',
    'native_image_client_absent', 'image_host_route_unavailable', 'no_native_project', 'picker_unavailable',
    'image_codec_unavailable', 'owner_reopened',
  ].includes(value) ? value : 'other';
  const readOrNull = (read) => { try { return read(); } catch (_) { return null; } };
  const observed = (read) => { try { return read() === true; } catch (_) { return 'error'; } };

  function create({ root, current, selection, state }) {
    const imageCatchReasons = new WeakMap();
    let importGatePending = null, importGateWriting = false;
    function unavailable(error) {
      const result = { status: 'unavailable', code: 'unavailable' };
      imageCatchReasons.set(result, reason(readOrNull(() => error && error.reason)));
      return result;
    }

    // One writer drains the latest pending receipt without delaying the importer.
    function write(receipt) {
      importGatePending = receipt;
      if (importGateWriting) return;
      importGateWriting = true;
      (async () => {
        try {
          const api = root.__TAURI__, base = await api.path.appDataDir(), dir = base.replace(/[\\/]+$/, '') + '/diagnostics';
          await api.fs.mkdir(dir, { recursive: true });
          while (importGatePending !== null) {
            const bytes = importGatePending;
            importGatePending = null;
            await api.fs.writeTextFile(dir + '/native-import-gate.pending.json', bytes);
            await api.fs.rename(dir + '/native-import-gate.pending.json', dir + '/native-import-gate.json');
          }
        } catch (_) { importGatePending = null; /* diagnostics are optional */ }
        finally { importGateWriting = false; }
      })();
    }

    // Post-result observation only: no admission, importer calls, identity or content output.
    function observe(result) {
      try {
        if (!result || result.status !== 'unavailable') return;
        const gate = readOrNull(state);
        const session = readOrNull(current);
        const info = readOrNull(() => session && session.info());
        const frame = readOrNull(() => root.state && root.state.currentFrame);
        const selected = readOrNull(selection);
        const presented = readOrNull(() => session && session.readPresentedObjectSession());
        const view = readOrNull(() => presented && presented.presentation);
        const subscriptionTicket = readOrNull(() => session && typeof session.readSubscriptionGeneration === 'function'
          ? session.readSubscriptionGeneration() : null);
        const snapshot = {
          phase: 'post-result',
          routeDeclared: observed(() => gate.routeDeclared), routeRetired: observed(() => gate.routeRetired),
          imagePending: observed(() => gate.imagePending),
          sessionPresent: !!session, active: observed(() => !!session && session.isActive()),
          infoValid: observed(() => !!info && typeof info.instanceId === 'string' && !!info.instanceId &&
            typeof info.documentId === 'string' && !!info.documentId && Number.isSafeInteger(info.lifecycleGeneration) &&
            Array.isArray(info.layers)),
          frameValid: observed(() => !!info && Number.isSafeInteger(frame) && frame >= 0 && frame < info.totalFrames),
          selectionValid: Array.isArray(selected),
          projectionReady: observed(() => !!info && !!root.NemoNativeObjectProjectFile &&
            root.NemoNativeObjectProjectFile.projectionReadyForUI(root, info)),
          presentedAvailable: observed(() => !!presented && presented.available === true),
          presentedPinsMatch: observed(() => !!info && !!presented && samePins(info, presented) && samePins(info, view)),
          presentedOwnerNative: observed(() => !!view && view.owner === 'native'),
          presentedStatusReady: observed(() => !!view && view.status === 'presented'),
          presentedFrameMatch: observed(() => !!view && view.frame === frame),
          importBatchCallable: observed(() => !!session && typeof session.importBatch === 'function'),
          subscriptionIdentityCallable: observed(() => !!session && typeof session.readSubscriptionGeneration === 'function'),
          subscriptionTicketPositive: Number.isSafeInteger(subscriptionTicket) && subscriptionTicket > 0,
          desktopEventListenPresent: observed(() => !!root.__TAURI__ && !!root.__TAURI__.event && typeof root.__TAURI__.event.listen === 'function'),
          transportModulePresent: observed(() => !!root.NemoApplicationMcpTransport &&
            typeof root.NemoApplicationMcpTransport.createNativeTauriTransport === 'function'),
          rasterModulePresent: observed(() => !!root.NemoNativeObjectRasterAsset && typeof root.NemoNativeObjectRasterAsset.desktop === 'function'),
        };
        write(JSON.stringify({ schemaVersion: 1, kind: 'native-import-unavailable', status: 'unavailable',
          reason: reason(result.reason), code: result.code === 'unavailable' ? 'unavailable' : 'other',
          imageCatchReason: reason(imageCatchReasons.get(result)), snapshot }));
      } catch (_) { /* Root/session getters and serialization never affect import. */ }
    }
    return Object.freeze({ observe, unavailable });
  }
  return Object.freeze({ create });
}));
