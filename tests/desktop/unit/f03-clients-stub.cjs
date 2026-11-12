'use strict';
// A disposable stand-in for the packaged app behind nemo-mcp, just enough of the opacity contract for the F03 harness: open and
// save project files, read the static and evaluated opacity, one write command, undo and redo, stale-revision and
// wrong-document refusals. It can be made to misbehave. `scriptedClient` plays the part of an agent client by sending raw
// requests. Nothing here starts a process.
const fs = require('node:fs');

const INSTANCE = 'cccccccc-cccc-4ccc-8ccc-cccccccccccc', BUILD = '1.2.3:' + 'a'.repeat(40), UID = 'r08_curve_layer';

function createApp(flags = {}) {
  const state = { doc: null, seq: 0, gen: 0, closed: false, calls: [] };
  const env = (v, req, extra) => ({ apiVersion: v, requestId: req.requestId, instanceId: INSTANCE, documentId: state.doc ? state.doc.id : '',
    ...(v === 2 ? { contentRevision: state.doc ? state.doc.rev : 0 } : { revision: state.doc ? state.doc.rev : 0 }), ...extra });
  const ok = (v, req, result) => env(v, req, { ok: true, result });
  const no = (v, req, code) => env(v, req, { ok: false, error: { code, message: code } });
  const valueAt = (doc, frame) => {
    const k = doc.keys;
    if (!k || k.length < 2) return doc.value;
    const [a, b] = k; return a.v[0] + (b.v[0] - a.v[0]) * Math.min(1, Math.max(0, (frame - a.frame) / (b.frame - a.frame)));
  };
  function v1(req) {
    const p = req.payload;
    if (req.operation === 'project.open') {
      const layer = JSON.parse(fs.readFileSync(p.path, 'utf8')).layers[0];
      state.gen += 1;
      state.doc = { id: 'doc-' + (++state.seq), rev: 0, value: layer.motionStatic.opacity[0], keys: layer.motion && layer.motion.opacity && layer.motion.opacity.keys, undo: [], redo: [] };
      return ok(1, req, { documentId: state.doc.id, contentRevision: 0, layers: [{ layerUid: UID }] });
    }
    if (!state.doc || req.documentId !== state.doc.id) return no(1, req, 'wrong_document');
    if (req.expectedRevision !== state.doc.rev) return no(1, req, 'stale_revision');
    if (req.operation === 'project.saveAs') {
      fs.writeFileSync(p.path, JSON.stringify({ layers: [{ layerUid: UID, motionStatic: { opacity: [flags.saveWrong ? 1 : state.doc.value] } }] }));
      return ok(1, req, { path: p.path });
    }
    return no(1, req, 'invalid_request');
  }
  function v2(req) {
    const doc = state.doc, p = req.payload;
    if (!doc || (req.documentId !== doc.id && !flags.acceptWrongDocument)) return no(2, req, 'wrong_document');
    switch (req.operation) {
      case 'query.document.opacity': return ok(2, req, { layerUid: UID, value: doc.value });
      case 'query.document.evaluate': return ok(2, req, { contentRevision: doc.rev, frame: p.frame, layers: [{ layerUid: UID, value: valueAt(doc, p.frame) }] });
      case 'command.document.apply':
        if (req.expectedRevision !== doc.rev && !(flags.acceptStale)) return no(2, req, 'stale_revision');
        doc.undo.push(doc.value); doc.redo = []; doc.value = p.value; doc.rev += 1;
        return ok(2, req, { applied: true, historyEntriesAdded: 1 });
      case 'history.undo': { if (!doc.undo.length) return no(2, req, 'unavailable'); doc.redo.push(doc.value); doc.value = doc.undo.pop(); doc.rev += 1; return ok(2, req, { applied: true, historyEntriesAdded: 0 }); }
      default: return no(2, req, 'invalid_request');
    }
  }
  return {
    state, flags,
    async callTool(name, args) {
      state.calls.push(name === 'nemo_command' ? args.operation : name);
      const wrap = value => ({ result: { structuredContent: value } });
      if (name === 'nemo_discover') {
        return wrap({ instances: [{ instanceId: INSTANCE, buildId: BUILD, nativeHostStatus: { instanceId: INSTANCE, available: !!state.doc,
          ...(state.doc ? { documentId: state.doc.id, contentRevision: state.doc.rev, lifecycleGeneration: state.gen } : {}) } }] });
      }
      return wrap(args.apiVersion === 1 ? v1(args) : v2(args));
    },
    async close() { state.closed = true; },
  };
}

// An agent, as raw requests: the workflow of the prompt. `skip` plays a client that does nothing; `stop` ends it after a step.
async function scriptedClient(app, files, { skip = false, stop = 99 } = {}) {
  if (skip) return;
  let n = 0;
  const send = async (v, operation, payload, extra = {}) => {
    const found = (await app.callTool('nemo_discover', {})).result.structuredContent.instances[0], s = found.nativeHostStatus;
    return (await app.callTool('nemo_command', { apiVersion: v, requestId: 'c' + (++n), instanceId: INSTANCE, documentId: s.documentId, expectedRevision: s.contentRevision, operation, payload, ...extra })).result.structuredContent;
  };
  const set = (value, extra) => send(2, 'command.document.apply', { command: 'layer.opacity.set', stableTarget: { layerUid: UID }, value }, extra);
  const steps = [
    () => set(40), () => set(60), () => send(1, 'project.saveAs', { path: files.saved60 }), () => send(2, 'history.undo', {}),
    () => set(99, { expectedRevision: 0 }), () => send(1, 'project.saveAs', { path: files.saved }), () => send(1, 'project.open', { path: files.saved }),
  ];
  for (let i = 0; i < steps.length && i < stop; i++) await steps[i]();
  if (stop < 99) return;
  const before = (await app.callTool('nemo_discover', {})).result.structuredContent.instances[0].nativeHostStatus.documentId;
  await send(1, 'project.open', { path: files.keyed });
  await set(99, { documentId: before });
}

module.exports = { createApp, scriptedClient, INSTANCE, BUILD, UID };
