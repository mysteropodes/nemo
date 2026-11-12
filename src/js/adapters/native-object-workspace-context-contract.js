/* #2256: JS side of query.document.object.workspace.context, the pure enter/exit component-context transition. Pure validation; it
 * owns no document, keeps no workspace and issues no request. The caller sends the workspace it holds (the entered context or null,
 * at most 8 unique open tabs, the playhead and work area of the presented context and, while a component is entered, the saved scene
 * view) with the pinned snapshot and one action: enter, exit, close or refresh. The native owner applies the baseline rules
 * (enterSymbol / exitToScene / closeSymbolTab) against the contexts the snapshot declares and answers the new workspace plus the
 * context to present. A pinned read carries no expectedRevision. Mirrors nemo-mcp/src/native_workspace_context_contract.rs. */
(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoNativeObjectWorkspaceContextContract = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  const OP = 'query.document.object.workspace.context', SCENE = 'scene-root', MAX_ID = 128, MAX_TABS = 8;
  const OUTCOMES = ['entered', 'already_active', 'refused_nested', 'exited', 'not_in_context', 'closed', 'close_noop', 'refreshed', 'context_gone'];
  const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/;
  const has = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
  const fail = (message) => { throw new TypeError(message); };
  const revision = (value) => Number.isSafeInteger(value) && value >= 0;
  const uint32 = (value) => Number.isSafeInteger(value) && value >= 0 && value <= 4294967295;
  const idText = (value) => typeof value === 'string' && value.length > 0 && value.length <= MAX_ID && !value.includes('\0');
  function exact(value, required, label) {
    if (!value || typeof value !== 'object' || Array.isArray(value) || ![Object.prototype, null].includes(Object.getPrototypeOf(value))) fail(label + ' must be a plain object');
    if (!required.every((key) => has(value, key)) || Object.keys(value).some((key) => !required.includes(key))) fail(label + ' has missing or unknown fields');
    return value;
  }
  function view(value, label) {
    exact(value, ['frame', 'workIn', 'workOut'], label);
    if (!uint32(value.frame) || !uint32(value.workIn) || !uint32(value.workOut) || value.workIn > value.workOut) fail('invalid ' + label);
  }
  // The held workspace: tabs are unique and bounded, an entered context is an open tab and saves the scene view (and only then).
  function workspace(value) {
    exact(value, ['activeContextId', 'openTabs', 'view', 'sceneView'], 'workspace');
    const active = value.activeContextId !== null;
    if (active && !idText(value.activeContextId)) fail('invalid active context id');
    if (!Array.isArray(value.openTabs) || value.openTabs.length > MAX_TABS || !value.openTabs.every(idText) || new Set(value.openTabs).size !== value.openTabs.length) fail('invalid open tabs');
    if (active && !value.openTabs.includes(value.activeContextId)) fail('the active context is not an open tab');
    view(value.view, 'workspace view');
    if (active !== (value.sceneView !== null)) fail('an entered context saves the scene view and the scene does not');
    if (active) view(value.sceneView, 'saved scene view');
  }
  function action(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) fail('invalid workspace action');
    if (value.kind === 'enter') {
      // `fromLayerUid` (optional) names the scene instance layer entered: the owner starts at the component frame it shows at the held playhead.
      const from = has(value, 'fromLayerUid');
      exact(value, from ? ['kind', 'contextId', 'entryFrame', 'fromLayerUid'] : ['kind', 'contextId', 'entryFrame'], 'enter action');
      if (!idText(value.contextId) || !(value.entryFrame === null || uint32(value.entryFrame))) fail('invalid enter action');
      if (from && !(value.fromLayerUid === null || idText(value.fromLayerUid))) fail('invalid enter action');
      if (from && value.fromLayerUid !== null && value.entryFrame !== null) fail('an enter action takes an entry frame or an instance layer, not both');
    } else if (value.kind === 'close') {
      exact(value, ['kind', 'contextId'], 'close action');
      if (!idText(value.contextId)) fail('invalid close action');
    } else if (value.kind === 'exit' || value.kind === 'refresh') exact(value, ['kind'], value.kind + ' action');
    else fail('invalid workspace action');
  }
  function payload(operation, value) {
    if (operation !== OP) fail('workspace context operation is not declared');
    exact(value, ['atRevision', 'documentSnapshotId', 'workspace', 'action'], 'workspace context payload');
    if (!revision(value.atRevision) || typeof value.documentSnapshotId !== 'string' || !IDENTIFIER.test(value.documentSnapshotId)) fail('invalid workspace context pin');
    workspace(value.workspace);
    action(value.action);
  }
  function result(operation, value, request) {
    if (operation !== OP) fail('workspace context operation is not declared');
    exact(value, ['atRevision', 'documentSnapshotId', 'outcome', 'contextId', 'totalFrames', 'fps', 'workspace'], 'workspace context result');
    if (!revision(value.atRevision) || typeof value.documentSnapshotId !== 'string' || !IDENTIFIER.test(value.documentSnapshotId)) fail('invalid workspace context result pin');
    if (request && (value.atRevision !== request.atRevision || value.documentSnapshotId !== request.documentSnapshotId)) fail('workspace context result does not answer its pin');
    if (!OUTCOMES.includes(value.outcome) || !uint32(value.totalFrames) || value.totalFrames < 1 || typeof value.fps !== 'number' || !Number.isFinite(value.fps) || !(value.fps > 0)) fail('invalid workspace context result');
    workspace(value.workspace);
    // The presented context is the entered one, else the scene.
    if (value.contextId !== (value.workspace.activeContextId === null ? SCENE : value.workspace.activeContextId)) fail('workspace context result presents another context');
  }
  function correlate(request, response) {
    if (response.requestId !== request.requestId || response.instanceId !== request.instanceId) fail('workspace context response owner mismatch');
    const replaced = !response.ok && response.error && response.error.code === 'wrong_document';
    if (response.documentId !== request.documentId && !replaced) fail('workspace context response document mismatch');
    if (!response.ok) return;
    result(OP, response.result, request.payload);
    if (response.contentRevision !== request.payload.atRevision) fail('workspace context pin mismatch');
  }
  const admitted = (operation) => operation === OP;
  const own = Object.freeze({ admitted, requiresRevision: () => false, payload, result, correlate, paths: () => null });
  function compose(base) {
    if (!base) return own;
    return Object.freeze(Object.assign({}, base, {
      admitted: (operation) => admitted(operation) || base.admitted(operation),
      requiresRevision: (operation) => !admitted(operation) && base.requiresRevision(operation),
      payload: (operation, value, envelope) => (admitted(operation) ? payload(operation, value) : base.payload(operation, value, envelope)),
      result: (operation, value) => (admitted(operation) ? result(operation, value) : base.result(operation, value)),
      correlate: (request, response) => (admitted(request.operation) ? correlate(request, response) : base.correlate(request, response)),
      paths: (operation) => (admitted(operation) ? null : base.paths(operation)),
    }));
  }
  return Object.freeze({ OP, SCENE, MAX_TABS, OUTCOMES, admitted, requiresRevision: own.requiresRevision, payload, result, correlate, paths: own.paths, compose });
}));
