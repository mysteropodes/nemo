'use strict';
// A disposable stand-in for the packaged app behind nemo-mcp, just enough of what an OBJECT project answers for the F03 harness. The
// reply shapes are the ones observed from the installed candidate 0.7.0-alpha.4 (7d1cb6fc), see engineering/application/capabilities-v2:
//   apiVersion 1 through nemo_command: project.open, project.saveAs (a saved project is a nemo.native-project-document); reads and
//     property.* are refused (front door "Use nemo_query for reads", or `unavailable` "Native object projects answer only ...");
//     opening the project file that is already open is `unavailable` and says it is already the open document and how to continue.
//   apiVersion 2: query.document.object (view presentation, the snapshot pin), query.document.object.property.read (rawSample),
//     query.document.object.history, command.document.object.property.edit (setValue), command.document.object.undo; the opacity-document
//     operations (query.document.opacity, query.document.evaluate, query.document.snapshot.acquire, command.document.apply) are
//     `invalid_request` "Unsupported object read operation." followed by the operation to use instead.
// It can be made to misbehave. `scriptedClient` plays the part of an agent client by sending raw requests. Nothing here starts a process.
const fs = require('node:fs');

const INSTEAD = { 'property.get': 'query.document.object.property.read', 'property.set': 'command.document.object.property.edit', 'history.undo': 'command.document.object.undo',
  'query.document.opacity': 'query.document.object.property.read', 'query.document.evaluate': 'query.document.object.property.read',
  'query.document.snapshot.acquire': 'query.document.object', 'command.document.apply': 'command.document.object.property.edit' };
const INSTANCE = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc', BUILD = '1.2.3:' + 'a'.repeat(40), UID = 'r08_curve_layer', ROOT = 'scene-root';

function createApp(flags = {}) {
  const state = { doc: null, seq: 0, gen: 0, closed: false, calls: [] };
  const env = (v, req, extra) => ({ apiVersion: v, requestId: req.requestId, instanceId: INSTANCE, documentId: state.doc ? state.doc.id : '',
    ...(v === 2 ? { contentRevision: state.doc ? state.doc.rev : 0 } : { revision: state.doc ? state.doc.rev : 0 }), ...extra });
  const ok = (v, req, result) => env(v, req, { ok: true, result });
  const no = (v, req, code, message = code) => env(v, req, { ok: false, error: { code, message } });
  const frontDoor = (code, message) => ({ ok: false, error: { code, message } });
  const valueAt = (doc, frame) => {
    const k = doc.keys;
    if (!k || k.length < 2) return doc.value;
    const [a, b] = k; return a.v[0] + (b.v[0] - a.v[0]) * Math.min(1, Math.max(0, (frame - a.frame) / (b.frame - a.frame)));
  };
  // The opacity a project file holds: the fixture shells (layers[0].motionStatic / motion.opacity) or a saved native project.
  function load(file) {
    const json = JSON.parse(fs.readFileSync(file, 'utf8'));
    if (json.format === 'nemo.native-project-document') {
      const prop = json.drawing.propertyHolders[0].properties.find(p => p.property === 'opacity');
      return { value: prop.static ? prop.static[0] : 0, keys: prop.track && prop.track.keys };
    }
    const layer = json.layers[0];
    return { value: layer.motionStatic.opacity[0], keys: layer.motion && layer.motion.opacity && layer.motion.opacity.keys };
  }
  const summary = doc => ({ canvasH: 180, canvasW: 320, contentRevision: doc.rev, documentId: doc.id, fps: 24, layers: [{ layerUid: UID, locked: false, name: 'R08 rectangle', shy: false, visible: true }], totalFrames: 21 });
  function v1(req) {
    const p = req.payload;
    if (['property.get', 'snapshot', 'capabilities', 'diagnostics.trace'].includes(req.operation)) return frontDoor('invalid_request', 'Use nemo_query for reads');
    if (req.operation === 'project.open') {
      if (state.doc && state.doc.path === p.path) return no(1, req, 'unavailable', 'That project file is already the open document, so nothing was opened. Continue with it (read it with a snapshot) or open a different file.');
      const loaded = load(p.path);
      state.gen += 1;
      state.doc = { id: 'doc-' + (++state.seq), rev: 0, path: p.path, value: loaded.value, keys: loaded.keys, undo: [], redo: [] };
      return ok(1, req, summary(state.doc));
    }
    if (!state.doc || req.documentId !== state.doc.id) return no(1, req, 'wrong_document');
    if (req.expectedRevision !== state.doc.rev) return no(1, req, 'stale_revision');
    if (req.operation === 'project.saveAs') {
      fs.writeFileSync(p.path, JSON.stringify({ format: 'nemo.native-project-document', formatVersion: 1, drawing: { propertyHolders: [{ target: { contextId: ROOT, layerUid: UID },
        properties: [{ property: 'position', static: [0, 0] }, { property: 'opacity', static: [flags.saveWrong ? 1 : state.doc.value] }] }] } }));
      state.doc.path = p.path;
      return ok(1, req, { ...summary(state.doc), path: p.path });
    }
    return no(1, req, 'unavailable', 'Native object projects answer only capabilities, snapshot and native operations (apiVersion 2).' + (INSTEAD[req.operation] ? ' Use ' + INSTEAD[req.operation] + ' instead.' : ''));
  }
  const pin = doc => 'native-object:' + doc.id + ':' + doc.rev;
  function v2(req) {
    const doc = state.doc, p = req.payload;
    if (!doc || (req.documentId !== doc.id && !flags.acceptWrongDocument)) return no(2, req, 'wrong_document', 'Property edit request refused.');
    switch (req.operation) {
      case 'query.document.object':
        if (p.view && p.view.kind === 'presentation') return ok(2, req, { atRevision: doc.rev, contextId: ROOT, documentSnapshotId: pin(doc), frame: 0, kind: 'presentation', viewGeneration: 3, workId: 'native-work-3' });
        return frontDoor('invalid_request', 'native object selector does not match its declared contract');
      case 'query.document.object.property.read':
        if (p.atRevision !== doc.rev || p.documentSnapshotId !== pin(doc)) return no(2, req, 'stale_revision', 'Property read request refused.');
        return ok(2, req, { atRevision: doc.rev, dimension: 1, documentId: doc.id, documentSnapshotId: pin(doc), frame: p.view.frame, instanceId: INSTANCE, kind: 'rawSample', property: p.view.property, target: p.view.target, value: [valueAt(doc, p.view.frame)] });
      case 'query.document.object.history':
        return ok(2, req, { atRevision: doc.rev, documentSnapshotId: pin(doc), entries: doc.undo.map((_, index) => ({ index, label: 'Property: Opacity' })), offset: 0, redoDepth: doc.redo.length, undoDepth: doc.undo.length });
      case 'command.document.object.property.edit':
        if (req.expectedRevision !== doc.rev && !flags.acceptStale) return no(2, req, 'stale_revision', 'Property edit request refused.');
        doc.undo.push(doc.value); doc.redo = []; doc.value = p.action.values[0]; doc.keys = undefined; doc.rev += 1;
        return ok(2, req, { applied: true, historyEntriesAdded: 1 });
      case 'command.document.object.undo': { if (!doc.undo.length) return no(2, req, 'unavailable'); doc.redo.push(doc.value); doc.value = doc.undo.pop(); doc.rev += 1; return ok(2, req, { applied: true, historyEntriesAdded: 0 }); }
      case 'query.document.opacity': case 'query.document.evaluate': case 'query.document.snapshot.acquire': case 'command.document.apply':
        return no(2, req, 'invalid_request', 'Unsupported object read operation. ' + req.operation + ' applies to opacity documents only; this is an object document. Use ' + INSTEAD[req.operation] + ' instead.');
      default: return no(2, req, 'invalid_request');
    }
  }
  return {
    state, flags,
    async callTool(name, args) {
      state.calls.push(name === 'nemo_command' ? args.operation : name);
      const wrap = value => ({ result: { structuredContent: value } });
      if (name === 'nemo_discover') {
        return wrap({ detail: 'guide', instances: [{ instanceId: INSTANCE, buildId: BUILD, pid: 4242, documentId: state.doc ? state.doc.id : null,
          documentKind: state.doc ? 'object' : null, revision: state.doc ? state.doc.rev : null, nativeHostStatus: { instanceId: INSTANCE, available: !!state.doc,
          ...(state.doc ? { documentId: state.doc.id, contentRevision: state.doc.rev, lifecycleGeneration: state.gen, documentKind: 'object', pid: 4242 } : {}) } }] });
      }
      if (name === 'nemo_query') return wrap(args.operation === 'snapshot' && state.doc ? { apiVersion: 1, documentId: state.doc.id, instanceId: INSTANCE, ok: true, requestId: 'q', revision: state.doc.rev, result: { canvasH: 180, canvasW: 320, fps: 24, frame: 0, layers: [{ id: UID, locked: false, name: 'R08 rectangle', shy: false, visible: true }], totalFrames: 21 } } : frontDoor('invalid_request', 'unsupported'));
      return wrap(args.apiVersion === 1 ? v1(args) : v2(args));
    },
    async close() { state.closed = true; },
  };
}

// An agent, as raw requests: the workflow of the prompt, in the order of the prompt (the saved file is reopened after the keyed project
// replaced it). `skip` plays a client that does nothing; `stop` ends it after that many steps.
async function scriptedClient(app, files, { skip = false, stop = 99 } = {}) {
  if (skip) return;
  let n = 0;
  const status = async () => (await app.callTool('nemo_discover', {})).result.structuredContent.instances[0].nativeHostStatus;
  const send = async (v, operation, payload, extra = {}) => {
    const s = await status();
    return (await app.callTool('nemo_command', { apiVersion: v, requestId: 'c' + (++n), instanceId: INSTANCE, documentId: s.documentId, expectedRevision: s.contentRevision, operation, payload, ...extra })).result.structuredContent;
  };
  const set = (value, extra) => send(2, 'command.document.object.property.edit', { command: 'object.property.edit', target: { contextId: ROOT, layerUid: UID }, property: 'opacity', action: { kind: 'setValue', frame: 0, values: [value] } }, extra);
  let before;
  const steps = [
    () => set(40), () => set(60), () => send(1, 'project.saveAs', { path: files.saved60 }), () => send(2, 'command.document.object.undo', { command: 'object.undo' }),
    () => set(99, { expectedRevision: 0 }), () => send(1, 'project.saveAs', { path: files.saved }),
    async () => { before = (await status()).documentId; await send(1, 'project.open', { path: files.keyed }); },
    () => set(99, { documentId: before }), () => send(1, 'project.open', { path: files.saved }),
  ];
  for (let i = 0; i < steps.length && i < stop; i++) await steps[i]();
}

module.exports = { createApp, scriptedClient, INSTANCE, BUILD, UID };
