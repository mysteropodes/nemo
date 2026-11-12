'use strict';
// A disposable stand-in for the packaged app behind nemo-mcp: just enough of the project and vector contract for the kit and the
// runner to be exercised. Nothing here launches an app; every behaviour that matters can be switched to misbehave.
const fs = require('node:fs');
const path = require('node:path');
const png = require('../../fixtures/lib/png.cjs');

const INSTANCE = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', FOREIGN = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const BUILD = '1.2.3:' + 'a'.repeat(40);

function createStubApp(options = {}) {
  const state = { doc: null, dirty: false, docSeq: 0, calls: [], ...options.initial };
  const flags = { noHistory: false, foreignReply: false, malformed: false, unavailable: false, brokenUndo: false, wrongRequestId: false, foreignBuild: false, noDocumentId: false, ...options };
  const structured = value => ({ result: { structuredContent: value } });
  const newDoc = (w, h, strokes = []) => ({ id: 'doc-' + (++state.docSeq), rev: 0, w, h, strokes, undo: [], redo: [], seq: 0 });
  const snapshotId = () => 'snap-' + state.doc.id + '-' + state.doc.rev;
  const envelope = (version, request, extra) => {
    const base = { apiVersion: version, requestId: flags.wrongRequestId ? 'other' : request.requestId, instanceId: flags.foreignReply ? FOREIGN : INSTANCE,
      documentId: state.doc ? state.doc.id : '', ...(version === 2 ? { contentRevision: state.doc ? state.doc.rev : 0 } : { revision: state.doc ? state.doc.rev : 0 }) };
    return { ...base, ...extra };
  };
  const ok = (version, request, result) => envelope(version, request, { ok: true, result });
  const refuse = (version, request, code, message) => envelope(version, request, { ok: false, error: { code, message } });
  function render(doc) {
    const image = png.canvas(doc.w, doc.h, [0, 0, 0, 0]);
    for (const s of doc.strokes) png.fillRect(image, s.x, s.y, s.w, s.h, s.rgba);
    return png.encodeRGBA(doc.w, doc.h, image.rgba);
  }
  function bump(doc, label) { doc.rev += 1; state.dirty = true; if (!flags.noHistory) { doc.undo.push(label); doc.redo = []; } }

  function v1(request) {
    const p = request.payload;
    if (request.operation === 'project.new') {
      if (state.dirty) return refuse(1, request, 'unsaved_changes', 'The open project has unsaved changes; save it first.');
      state.doc = newDoc(p.canvasW, p.canvasH); state.dirty = false;
      return ok(1, request, { documentId: state.doc.id, contentRevision: 0, layers: [{ layerUid: 'L1', name: 'Layer 1' }] });
    }
    if (!state.doc || request.documentId !== state.doc.id || request.expectedRevision !== state.doc.rev) return refuse(1, request, 'stale_revision', 'Read a current snapshot.');
    if (request.operation === 'project.saveAs') {
      fs.writeFileSync(p.path, JSON.stringify({ format: 'stub', w: state.doc.w, h: state.doc.h, drawing: { objects: state.doc.strokes.map(s => ({ target: { strokeId: s.id } , ...s })) }, assets: [] }));
      state.dirty = false; return ok(1, request, { path: p.path });
    }
    if (request.operation === 'project.open') {
      const file = JSON.parse(fs.readFileSync(p.path, 'utf8'));
      state.doc = newDoc(file.w, file.h, file.drawing.objects.map(({ target, ...s }) => s)); state.dirty = false;
      return ok(1, request, { documentId: state.doc.id, contentRevision: 0, layers: [{ layerUid: 'L1' }] });
    }
    if (request.operation === 'project.export') {
      fs.mkdirSync(path.join(p.destination, 'run'), { recursive: true });
      fs.writeFileSync(path.join(p.destination, 'run', 'frame-0000.png'), render(state.doc));
      return ok(1, request, { format: 'png', output: p.destination });
    }
    return refuse(1, request, 'invalid_request', 'unsupported');
  }

  function v2(request) {
    const doc = state.doc, p = request.payload;
    if (!doc || request.documentId !== doc.id) return refuse(2, request, 'wrong_document', 'wrong document');
    switch (request.operation) {
      case 'query.document.snapshot.acquire': return ok(2, request, { atRevision: doc.rev, documentSnapshotId: snapshotId() });
      case 'query.document.object.history':
        if (p.documentSnapshotId !== snapshotId()) return refuse(2, request, 'invalid_request', 'pin-changed');
        return ok(2, request, { atRevision: doc.rev, documentSnapshotId: snapshotId(), undoDepth: doc.undo.length, redoDepth: doc.redo.length, offset: 0, entries: [] });
      case 'query.document.object': {
        const found = doc.strokes.find(s => s.id === p.stableTarget.strokeId);
        return found ? ok(2, request, { atRevision: doc.rev, documentSnapshotId: snapshotId(), object: { target: p.stableTarget, fill: { r: 0.9 }, x: found.x } })
          : refuse(2, request, 'not_found', 'no such stroke');
      }
      case 'command.document.object.vector.edit': {
        if (flags.unavailable) return { ok: false, error: { code: 'unsupported_capability', message: 'native vector edit is not registered' } };
        if (request.expectedRevision !== doc.rev) return refuse(2, request, 'stale_revision', 'stale');
        const id = 'native-stroke:' + (doc.rev + 1) + ':' + (doc.seq++);
        doc.strokes.push({ id, x: 20, y: 20, w: 140, h: 120, rgba: [230, 51, 51, 255] }); bump(doc, 'create');
        return ok(2, request, { applied: true, historyEntriesAdded: flags.noHistory ? 0 : 1, operation: 'create', strokeId: id });
      }
      case 'command.document.object.undo': case 'command.document.object.redo': {
        const undo = request.operation.endsWith('undo');
        const from = undo ? doc.undo : doc.redo, to = undo ? doc.redo : doc.undo;
        if (!from.length) return refuse(2, request, 'unavailable', 'nothing to ' + (undo ? 'undo' : 'redo'));
        to.push(from.pop()); doc.rev += 1; state.dirty = true;
        if (undo && !flags.brokenUndo) doc.strokes.pop(); if (!undo) doc.strokes.push({ id: 'native-stroke:redo:' + doc.seq++, x: 20, y: 20, w: 140, h: 120, rgba: [230, 51, 51, 255] });
        return ok(2, request, { applied: true, historyEntriesAdded: 0 });
      }
      default: return refuse(2, request, 'invalid_request', 'unknown operation ' + request.operation);
    }
  }

  return {
    state, flags,
    async callTool(name, args) {
      state.calls.push(name === 'nemo_command' ? args.operation : name);
      if (flags.malformed === 'text') return { result: { content: [{ type: 'text', text: 'not json at all' }] } };
      if (name === 'nemo_discover') {
        return structured({ instances: [{ instanceId: INSTANCE, buildId: flags.foreignBuild ? '1.2.3:' + 'b'.repeat(40) : BUILD,
          nativeHostStatus: { instanceId: INSTANCE, available: !!state.doc, ...(state.doc ? { documentId: flags.noDocumentId ? '' : state.doc.id, contentRevision: state.doc.rev, lifecycleGeneration: 1 } : {}) } }] });
      }
      if (name !== 'nemo_command') throw new Error('unexpected tool ' + name);
      if (flags.malformed === 'shape') return structured({ apiVersion: args.apiVersion, requestId: args.requestId, instanceId: INSTANCE, documentId: state.doc ? state.doc.id : '', contentRevision: 0 });
      return structured(args.apiVersion === 1 ? v1(args) : v2(args));
    },
    async close() { state.closed = true; },
  };
}

module.exports = { createStubApp, INSTANCE, FOREIGN, BUILD };
