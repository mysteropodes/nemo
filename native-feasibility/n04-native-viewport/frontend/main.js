(() => {
  const invoke = window.__TAURI__.core.invoke;
  const host = document.querySelector('#native-host');
  const status = document.querySelector('#status');
  const evidence = document.querySelector('#evidence');
  let revision = 7;
  let generation = 0;
  let pointer = null;
  let scheduled = false;
  let lastOrdinaryKey = '';
  let finalizing = false;

  function ordinaryKey(request) {
    // AppKit/WebKit can report sub-device-pixel compositing jitter while the
    // child NSView is presented. Native allocation rounds to physical pixels,
    // so coalescing at half a CSS pixel preserves a real resize/pointer change
    // without turning that jitter into a render loop.
    return [
      request.cssX, request.cssY, request.cssWidth, request.cssHeight,
      request.cssViewportWidth, request.cssViewportHeight,
      request.devicePixelRatio, request.revision, request.pointerX, request.pointerY,
    ].map(value => Number.isFinite(value) ? Math.round(value * 2) / 2 : 'none').join('|');
  }

  function measuredRequest(extra = {}) {
    const rect = host.getBoundingClientRect();
    return {
      cssX: rect.left, cssY: rect.top, cssWidth: rect.width, cssHeight: rect.height,
      cssViewportWidth: window.innerWidth, cssViewportHeight: window.innerHeight,
      devicePixelRatio: window.devicePixelRatio, revision, viewGeneration: 0,
      pointerX: pointer && pointer.x, pointerY: pointer && pointer.y, ...extra,
    };
  }
  async function configure(extra = {}) {
    try {
      const request = measuredRequest(extra);
      const ordinary = Object.keys(extra).length === 0;
      const key = ordinaryKey(request);
      if (ordinary && key === lastOrdinaryKey) return null;
      if (ordinary) lastOrdinaryKey = key;
      request.viewGeneration = ++generation;
      const result = await invoke('n04_configure', { request });
      status.textContent = `${result.status}: r${result.revision}, ${result.physicalWidth}×${result.physicalHeight}, ${result.presents} presents`;
      evidence.textContent = JSON.stringify(result, null, 2);
      if (result.hitTargetSelected && !finalizing) finishReceipt();
      return result;
    } catch (error) {
      status.textContent = `blocked: ${error}`;
      evidence.textContent = String(error);
      throw error;
    }
  }
  function schedule() {
    if (scheduled || finalizing) return;
    scheduled = true;
    requestAnimationFrame(() => { scheduled = false; configure(); });
  }
  // A native child view does not resize the WebView. Use the window resize event
  // rather than observing the host: WebKit reports its compositing changes to a
  // ResizeObserver, which would otherwise create an artificial configure loop.
  window.addEventListener('resize', schedule);
  host.addEventListener('pointermove', event => {
    // Presentation/compositing can replay a stationary hover. Only a real motion
    // submits compact input to the native fixture.
    if (event.movementX === 0 && event.movementY === 0) return;
    pointer = { x: event.clientX, y: event.clientY };
    schedule();
  });
  // WebKit may suppress synthesized movement, whereas a real click is still a
  // compact, pass-through input event. Accept it without movementX/Y gating.
  host.addEventListener('pointerdown', event => {
    pointer = { x: event.clientX, y: event.clientY };
    schedule();
  });
  host.addEventListener('pointerleave', () => { pointer = null; schedule(); });
  document.querySelector('#revision').onclick = () => { revision += 1; configure(); };
  document.querySelector('#stale').onclick = () => { revision += 1; configure({ staleRevision: revision - 1 }); };
  document.querySelector('#loss').onclick = () => configure({ forceSurfaceLoss: true });
  document.querySelector('#shutdown').onclick = async () => {
    const result = await invoke('n04_shutdown');
    status.textContent = 'disposed'; evidence.textContent = JSON.stringify(result, null, 2);
  };
  async function finishReceipt() {
    finalizing = true;
    status.textContent = 'input recorded; running 120-present native burst…';
    const burst = await invoke('n04_burst', { count: 120 });
    const terminal = await invoke('n04_shutdown');
    // This must reject: the Rust terminal receipt increments the rejection count
    // without recreating a native surface or submitting another present.
    try { await invoke('n04_configure', { request: measuredRequest() }); } catch (_) {}
    const result = await invoke('n04_evidence');
    status.textContent = `disposed after ${burst.burstCompleted} burst presents`;
    evidence.textContent = JSON.stringify({ burst, terminal, postDisposal: result }, null, 2);
  }

  // A real desktop launch renders a fixture, advances a revision, proves stale-token
  // rejection and the reconfigure path, then executes an actual CSS host resize.
  configure().then(async () => {
    revision = 8;
    await configure({ staleRevision: 7, forceSurfaceLoss: true });
    const before = host.getBoundingClientRect();
    host.style.height = `${Math.max(360, Math.floor(before.height - 64))}px`;
    await new Promise(requestAnimationFrame);
    await configure();
    status.textContent = 'move the real pointer inside the mint fixture to finalize receipt';
  }).catch(() => {});
})();
