'use strict';
// A disposable stand-in for the packaged app behind nemo-mcp: just enough of the project, vector, group, asset, raster, image
// mesh and bitmap-brush contract for the kit, the runner and the lane rows to be exercised. Nothing here launches an app; every
// behaviour that matters can be switched to misbehave with a flag, so a test can inject the fault a weak assertion would miss.
//
// Reply shapes follow the published contract (nemo-mcp request/result validators and the capability declarations): for example
// image.mesh.edit answers ONLY {applied, historyEntriesAdded}; the mesh id is read through the imageMesh query view.
const fs = require('node:fs');
const path = require('node:path');
const png = require('../../fixtures/lib/png.cjs');

const INSTANCE = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', FOREIGN = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';
const BUILD = '1.2.3:' + 'a'.repeat(40);
const MAX_DIMENSION = 8192, ROOT = 'scene-root';

const FLAGS = {
  // the original misbehaviours
  noHistory: false, foreignReply: false, malformed: false, unavailable: false, brokenUndo: false, wrongRequestId: false, foreignBuild: false, noDocumentId: false,
  // refusals: answer them as the transport's front door would, or with another code
  frontDoorRefusals: false, refusalCode: null,
  // save and reopen: project.open keeps the in-memory document instead of loading the file
  keepsInMemory: false,
  // groups: flatten only bumps the revision and the history
  flattenNoop: false,
  // image mesh: the attach is acknowledged but nothing is bound; a sculpt is acknowledged but changes nothing
  meshNeverBound: false, meshOffsetsIgnored: false,
  // bitmap brush: a publication is wrongly treated as an edit
  publishAddsHistory: false,
  // assets: removal ignores that a placement still uses the asset
  removeIgnoresUse: false,
};

const pointInPolygon = (poly, x, y) => {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
};
const bounds = segments => {
  const xs = segments.map(s => s.point.x), ys = segments.map(s => s.point.y);
  return { x0: Math.min(...xs), y0: Math.min(...ys), x1: Math.max(...xs), y1: Math.max(...ys) };
};

function createStubApp(options = {}) {
  const state = { doc: null, dirty: false, docSeq: 0, generation: 0, brushes: {}, calls: [], ...options.initial };
  const flags = { ...FLAGS, ...options };
  const structured = value => ({ result: { structuredContent: value } });
  const newDoc = (w, h, body = {}) => ({ id: 'doc-' + (++state.docSeq), rev: 0, w, h, strokes: [], rasters: [], assets: [], meshes: {}, layers: [{ layerUid: 'L1', name: 'Layer 1' }], undo: [], redo: [], seq: 0, ...body });
  const snapshotId = () => 'native-object:' + state.doc.id + ':' + state.doc.rev;
  const envelope = (version, request, extra) => {
    const base = { apiVersion: version, requestId: flags.wrongRequestId ? 'other' : request.requestId, instanceId: flags.foreignReply ? FOREIGN : INSTANCE,
      documentId: state.doc ? state.doc.id : '', ...(version === 2 ? { contentRevision: state.doc ? state.doc.rev : 0 } : { revision: state.doc ? state.doc.rev : 0 }) };
    return { ...base, ...extra };
  };
  const ok = (version, request, result) => envelope(version, request, { ok: true, result });
  const refuse = (version, request, code, message, details) => envelope(version, request, { ok: false, error: { code, message, ...(details ? { details } : {}) } });
  // A rule refusal of the native owner (or, with a flag, of the transport front door, or under another code).
  const ownerRefusal = (request, code, message) => {
    if (flags.frontDoorRefusals) return { ok: false, error: { code: 'invalid_request', message } };
    return refuse(2, request, flags.refusalCode || code, message);
  };
  const receipt = (extra = {}) => ({ applied: true, historyEntriesAdded: flags.noHistory ? 0 : 1, ...extra });
  const noop = () => ({ applied: false, historyEntriesAdded: 0 });
  const snap = d => JSON.stringify({ strokes: d.strokes, rasters: d.rasters, assets: d.assets, meshes: d.meshes, layers: d.layers });
  const restore = (d, text) => Object.assign(d, JSON.parse(text));
  function commit(doc, label, mutate) {
    const before = snap(doc);
    mutate();
    doc.rev += 1; state.dirty = true;
    if (!flags.noHistory) { doc.undo.push({ label, state: before }); doc.redo = []; }
  }

  // ---- rendering: fills as axis-aligned boxes, rasters as their four quadrant colours masked by the (sculpted) mesh outline ----
  function meshPolygon(doc, raster) {
    const mesh = raster.meshId && doc.meshes[raster.meshId];
    // The outline is the stable vertex prefix: outline point i is vertex i, and a sculpt moves it.
    return mesh ? mesh.outline.map(([x, y], i) => [x + mesh.offsets[i][0], y + mesh.offsets[i][1]]) : null;
  }
  function render(doc) {
    // The real export is opaque: the canvas background (#ffffff) is painted under everything and fills composite over it.
    const image = png.canvas(doc.w, doc.h, [255, 255, 255, 255]);
    for (const s of doc.strokes) {
      const { x0, y0, x1, y1 } = bounds(s.record.geometry.segments), fill = s.record.fill;
      if (!fill) continue;
      const c = [fill.r, fill.g, fill.b].map(v => Math.round(v * 255)), a = fill.a;
      for (let y = Math.max(0, Math.round(y0)); y < Math.min(doc.h, Math.round(y1)); y++) for (let x = Math.max(0, Math.round(x0)); x < Math.min(doc.w, Math.round(x1)); x++) {
        png.fillRect(image, x, y, 1, 1, c.map(v => Math.round(v * a + 255 * (1 - a))));
      }
    }
    for (const r of doc.rasters) {
      const asset = doc.assets.find(a => a.id === r.assetId), polygon = flags.meshNeverBound ? null : meshPolygon(doc, r);
      for (let y = 0; y < r.height; y++) for (let x = 0; x < r.width; x++) {
        if (polygon && !pointInPolygon(polygon, (x + 0.5) / r.width, (y + 0.5) / r.height)) continue;
        const color = asset.colors[(y >= r.height / 2 ? 2 : 0) + (x >= r.width / 2 ? 1 : 0)];
        png.fillRect(image, r.x + x, r.y + y, 1, 1, color);
      }
    }
    return png.encodeRGBA(doc.w, doc.h, image.rgba);
  }
  const fileOf = doc => ({ format: 'stub', w: doc.w, h: doc.h, layers: doc.layers,
    drawing: { layers: doc.layers.map(l => ({ layerUid: l.layerUid, name: l.name })), objects: doc.strokes.map(s => ({ target: { strokeId: s.id }, layerUid: s.layerUid, ...(s.groupId ? { groupId: s.groupId } : {}), record: s.record })),
      items: { raster: doc.rasters.map(r => ({ target: { strokeId: r.id }, item: { layerUid: r.layerUid, x: r.x, y: r.y, width: r.width, height: r.height, assetId: r.assetId, ...(r.meshId ? { meshId: r.meshId } : {}) } })) } },
    assets: doc.assets, meshes: doc.meshes });

  function decodePng(bytes) {
    try {
      const header = bytes.length >= 24 && bytes.subarray(1, 4).toString('latin1') === 'PNG' ? { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) } : null;
      if (!header) return { error: 'corrupt' };
      if (header.width > MAX_DIMENSION || header.height > MAX_DIMENSION) return { error: 'too large', ...header };
      const image = png.decode(bytes);
      const at = (u, v) => png.pixelAt(image, Math.floor(image.width * u), Math.floor(image.height * v));
      return { ...header, colors: [at(0.25, 0.25), at(0.75, 0.25), at(0.25, 0.75), at(0.75, 0.75)] };
    } catch { return { error: 'corrupt' }; }
  }

  function v1(request) {
    const p = request.payload;
    if (request.operation === 'project.new') {
      if (state.dirty) return refuse(1, request, 'unsaved_changes', 'The open project has unsaved changes; save it first.');
      state.doc = newDoc(p.canvasW, p.canvasH); state.dirty = false; state.generation++; state.brushes = {};
      return ok(1, request, { documentId: state.doc.id, contentRevision: 0, layers: state.doc.layers.map(l => ({ ...l })) });
    }
    if (!state.doc || request.documentId !== state.doc.id || request.expectedRevision !== state.doc.rev) return refuse(1, request, 'stale_revision', 'Read a current snapshot.');
    if (request.operation === 'project.saveAs') {
      // Observed on the installed app: the first save after a change is refused as unavailable, the next one is accepted.
      if (flags.saveRace && state.raceRev !== state.doc.rev) { state.raceRev = state.doc.rev; return refuse(1, request, 'unavailable', 'The project was not saved (refused: pin-changed-before-picker). App notice: Save cancelled or unavailable. Choose a new file path.'); }
      fs.writeFileSync(p.path, JSON.stringify(fileOf(state.doc)));
      state.dirty = false; return ok(1, request, { path: p.path });
    }
    if (request.operation === 'project.open') {
      const file = JSON.parse(fs.readFileSync(p.path, 'utf8'));
      if (!flags.keepsInMemory) {
        const strokes = file.drawing.objects.map(o => ({ id: o.target.strokeId, layerUid: o.layerUid, ...(o.groupId ? { groupId: o.groupId } : {}), record: o.record }));
        const rasters = file.drawing.items.raster.map(r => ({ id: r.target.strokeId, ...r.item }));
        state.doc = newDoc(file.w, file.h, { strokes, rasters, assets: file.assets, meshes: file.meshes, layers: file.layers });
        state.generation++; state.brushes = {};
      }
      state.dirty = false;
      return ok(1, request, { documentId: state.doc.id, contentRevision: 0, layers: state.doc.layers.map(l => ({ layerUid: l.layerUid })) });
    }
    if (request.operation === 'project.export') {
      fs.mkdirSync(path.join(p.destination, 'run'), { recursive: true });
      fs.writeFileSync(path.join(p.destination, 'run', 'frame-0000.png'), render(state.doc));
      return ok(1, request, { format: 'png', output: p.destination });
    }
    return refuse(1, request, 'invalid_request', 'unsupported');
  }

  const target = (layerUid, strokeId) => ({ contextId: ROOT, frameScope: { kind: 'authored', frame: 0 }, layerUid, strokeId });

  function v2(request) {
    const doc = state.doc, p = request.payload, op = request.operation;
    if (!doc || request.documentId !== doc.id) return refuse(2, request, 'wrong_document', 'wrong document');
    if (op.startsWith('command.') && request.expectedRevision !== doc.rev) return refuse(2, request, 'stale_revision', 'stale');
    switch (op) {
      // Real reply of the installed object owner (observed on the packaged candidate): the opacity-document queries are refused.
      case 'query.document.snapshot.acquire': case 'query.document.serialize': case 'query.document.evaluate':
        return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: doc.id, contentRevision: doc.rev, ok: false, error: { code: 'invalid_request', message: 'Unsupported object read operation.' } };
      case 'query.document.object.history':
        if (p.documentSnapshotId !== snapshotId()) return ownerRefusal(request, 'invalid_request', 'pin-changed');
        return ok(2, request, { atRevision: doc.rev, documentSnapshotId: snapshotId(), undoDepth: doc.undo.length, redoDepth: doc.redo.length, offset: 0, entries: [] });
      case 'query.document.object': return objectQuery(request, doc, p);
      case 'command.document.object.vector.edit': return vectorEdit(request, doc, p);
      case 'command.document.object.undo': case 'command.document.object.redo': {
        const undo = op.endsWith('undo'), from = undo ? doc.undo : doc.redo, to = undo ? doc.redo : doc.undo;
        if (!from.length) return refuse(2, request, 'unavailable', 'nothing to ' + (undo ? 'undo' : 'redo'));
        const entry = from.pop();
        to.push({ label: entry.label, state: snap(doc) });
        if (!(undo && flags.brokenUndo)) restore(doc, entry.state);
        doc.rev += 1; state.dirty = true;
        return ok(2, request, { applied: true, historyEntriesAdded: 0 });
      }
      case 'command.document.object.group.create': {
        const missing = p.strokeIds.filter(id => !doc.strokes.some(s => s.id === id));
        if (missing.length) return ownerRefusal(request, 'not_found', 'unknown stroke');
        commit(doc, 'group', () => { for (const s of doc.strokes) if (p.strokeIds.includes(s.id)) s.groupId = p.groupId; });
        return ok(2, request, receipt());
      }
      case 'command.document.object.boolean.flatten': {
        const members = doc.strokes.filter(s => s.groupId === p.groupId);
        if (!members.length) return ownerRefusal(request, 'not_found', 'unknown group');
        commit(doc, 'flatten', () => {
          if (flags.flattenNoop) return;
          const box = members.map(s => bounds(s.record.geometry.segments)), x0 = Math.min(...box.map(b => b.x0)), y0 = Math.min(...box.map(b => b.y0)), x1 = Math.max(...box.map(b => b.x1)), y1 = Math.max(...box.map(b => b.y1));
          doc.strokes = doc.strokes.filter(s => s.groupId !== p.groupId);
          const id = 'native-stroke:' + (doc.rev + 1) + ':' + doc.seq++;
          doc.strokes.push({ id, layerUid: p.layerUid, record: { ...members[0].record, target: target(p.layerUid, id), geometry: { ...members[0].record.geometry, segments: [{ point: { x: x0, y: y0 } }, { point: { x: x1, y: y1 } }] } } });
        });
        return ok(2, request, receipt());
      }
      case 'command.document.object.layer.delete': {
        const uid = p.selection.active;
        if (!doc.layers.some(l => l.layerUid === uid)) return ownerRefusal(request, 'not_found', 'unknown layer');
        commit(doc, 'layer delete', () => { doc.layers = doc.layers.filter(l => l.layerUid !== uid); doc.rasters = doc.rasters.filter(r => r.layerUid !== uid); doc.strokes = doc.strokes.filter(s => s.layerUid !== uid); });
        return ok(2, request, receipt());
      }
      case 'command.document.object.asset.attach': return assetAttach(request, doc, p);
      case 'command.document.object.asset.remove': {
        const asset = doc.assets.find(a => a.id === p.assetId);
        if (!asset) return ok(2, request, noop());
        if (!flags.removeIgnoresUse && doc.rasters.some(r => r.assetId === asset.id)) return ownerRefusal(request, 'invalid_request', 'asset in use');
        commit(doc, 'asset remove', () => { doc.assets = doc.assets.filter(a => a.id !== asset.id); });
        return ok(2, request, receipt());
      }
      case 'command.document.object.image.mesh.edit': return meshEdit(request, doc, p);
      case 'command.document.object.brush.bitmap.publish': return brushPublish(request, doc, p);
      case 'query.document.object.brush.bitmap': {
        const row = state.brushes[p.brushId];
        return ok(2, request, { action: 'inspect', atRevision: doc.rev, documentSnapshotId: snapshotId(), brush: row ? { brushId: p.brushId, resident: true, version: row.version, width: row.width, height: row.height, bytes: row.bytes } : { brushId: p.brushId, resident: false } });
      }
      default: return refuse(2, request, 'invalid_request', 'unknown operation ' + op);
    }
  }

  function objectQuery(request, doc, p) {
    if (p.view && p.view.kind === 'presentation') {
      return ok(2, request, { kind: 'presentation', atRevision: p.atRevision, documentSnapshotId: snapshotId(), contextId: ROOT, frame: 0, workId: 'native-work-1', viewGeneration: 1 });
    }
    if (p.view && p.view.kind === 'imageMesh') {
      const v = p.view, raster = doc.rasters.find(r => r.layerUid === v.layerUid);
      if (!raster) return ownerRefusal(request, 'not_found', 'no raster on the layer');
      const mesh = raster.meshId && doc.meshes[raster.meshId] && !flags.meshNeverBound ? doc.meshes[raster.meshId] : null;
      const counts = mesh && { meshId: raster.meshId, outlineCount: mesh.outline.length, vertexCount: mesh.verts.length, triangleIndexCount: mesh.tris.length, offsetsPresent: !!mesh.offsets, cols: mesh.cols, rows: mesh.rows };
      let page = { kind: 'selection' };
      if (v.page.kind !== 'selection') {
        const items = v.page.kind === 'offsets' ? mesh.offsets : v.page.kind === 'verts' ? mesh.verts : v.page.kind === 'tris' ? mesh.tris : mesh.outline;
        const slice = items.slice(v.page.cursor, v.page.cursor + v.page.limit), end = v.page.cursor + slice.length;
        page = { kind: v.page.kind, cursor: v.page.cursor, total: items.length, nextCursor: end === items.length ? null : end, items: slice };
      }
      return ok(2, request, { kind: 'imageMesh', atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, contextId: v.contextId, layerUid: v.layerUid, frame: v.frame, workId: v.workId, viewGeneration: v.viewGeneration,
        target: target(raster.layerUid, raster.id), rect: { x: raster.x, y: raster.y, width: raster.width, height: raster.height, rotation: 0 }, mesh: counts, page });
    }
    const found = doc.strokes.find(s => s.id === p.stableTarget.strokeId);
    return found ? ok(2, request, { atRevision: doc.rev, documentSnapshotId: snapshotId(), object: found.record }) : ownerRefusal(request, 'not_found', 'no such stroke');
  }

  function vectorEdit(request, doc, p) {
    if (flags.unavailable) return { ok: false, error: { code: 'unsupported_capability', message: 'native vector edit is not registered' } };
    if (p.command === 'object.vector.create') {
      const id = 'native-stroke:' + (doc.rev + 1) + ':' + (doc.seq++);
      commit(doc, 'create', () => doc.strokes.push({ id, layerUid: p.record.target.layerUid, record: { ...p.record, target: { ...p.record.target, strokeId: id } } }));
      return ok(2, request, receipt({ operation: 'create', strokeId: id }));
    }
    const id = p.command === 'object.vector.replace' ? p.record.target.strokeId : p.stableTarget.strokeId, found = doc.strokes.find(s => s.id === id);
    if (!found) return ownerRefusal(request, 'not_found', 'no such stroke');
    if (p.command === 'object.vector.delete') { commit(doc, 'delete', () => { doc.strokes = doc.strokes.filter(s => s.id !== id); }); return ok(2, request, receipt({ operation: 'delete' })); }
    if (JSON.stringify(found.record) === JSON.stringify(p.record)) return ok(2, request, noop());
    commit(doc, 'replace', () => { found.record = p.record; });
    return ok(2, request, receipt({ operation: 'replace' }));
  }

  function assetAttach(request, doc, p) {
    if (p.lifecycleGeneration !== state.generation) return refuse(2, request, 'unavailable', 'stale lifecycle generation');
    if (p.documentSnapshotId !== snapshotId()) return ownerRefusal(request, 'invalid_request', 'stale pin');
    let bytes;
    try { bytes = fs.readFileSync(p.source.path); } catch {
      return refuse(2, request, 'unavailable', 'native image source unavailable', { disposition: 'notDispatched', committed: false, retryExecution: true });
    }
    const decoded = decodePng(bytes);
    if (decoded.error) return ownerRefusal(request, 'invalid_request', 'image refused: ' + decoded.error);
    const sha = require('node:crypto').createHash('sha256').update(bytes).digest('hex');
    let asset = doc.assets.find(a => a.sha === sha);
    let result;
    commit(doc, 'attach', () => {
      if (!asset) { asset = { id: 'asset-' + sha.slice(0, 12), sha, name: p.name, width: decoded.width, height: decoded.height, colors: decoded.colors }; doc.assets.push(asset); }
      const layerUid = 'L' + (doc.layers.length + 1 + doc.seq++), id = 'native-stroke:' + (doc.rev + 1) + ':' + doc.seq++;
      doc.layers.push({ layerUid, name: p.name, rasters: [id] });
      doc.rasters.push({ id, layerUid, assetId: asset.id, x: Math.round((doc.w - decoded.width) / 2), y: Math.round((doc.h - decoded.height) / 2), width: decoded.width, height: decoded.height });
      result = receipt({ assetId: asset.id, naturalWidth: decoded.width, naturalHeight: decoded.height, displayWidth: decoded.width, displayHeight: decoded.height, target: target(layerUid, id), newLayer: true });
    });
    return ok(2, request, result);
  }

  function meshEdit(request, doc, p) {
    const action = p.action;
    if (action.kind === 'attach') {
      const raster = doc.rasters.find(r => r.id === action.target.strokeId);
      if (!raster) return ownerRefusal(request, 'not_found', 'no such raster');
      commit(doc, 'mesh attach', () => {
        if (flags.meshNeverBound) return;
        const id = 'im_' + (Object.keys(doc.meshes).length + 1);
        doc.meshes[id] = JSON.parse(JSON.stringify(action.mesh)); raster.meshId = id;
      });
      return ok(2, request, receipt());
    }
    const mesh = doc.meshes[p.meshId];
    if (!mesh) return ownerRefusal(request, 'not_found', 'no such mesh');
    if (!(action.index >= 0 && action.index < mesh.verts.length)) return ownerRefusal(request, 'invalid_request', 'vertex out of range');
    commit(doc, 'mesh sculpt', () => { if (!flags.meshOffsetsIgnored) mesh.offsets[action.index] = [action.du, action.dv]; });
    return ok(2, request, receipt());
  }

  function brushPublish(request, doc, p) {
    if (p.documentSnapshotId !== snapshotId()) return ownerRefusal(request, 'invalid_request', 'stale pin');
    const asset = doc.assets.find(a => a.id === p.assetId);
    if (!asset) return ownerRefusal(request, 'not_found', 'no such asset');
    if (asset.width !== p.width || asset.height !== p.height) return ownerRefusal(request, 'invalid_request', 'wrong-size asset');
    const held = state.brushes[p.brushId], same = held && held.assetId === asset.id;
    if (!same) state.brushes[p.brushId] = { assetId: asset.id, version: (held ? held.version : 0) + 1, width: p.width, height: p.height, bytes: p.width * p.height * 4 + 128 };
    const row = state.brushes[p.brushId];
    if (flags.publishAddsHistory) commit(doc, 'publish', () => {});
    return ok(2, request, { applied: !same, brushId: p.brushId, width: row.width, height: row.height, version: row.version, bytes: row.bytes });
  }

  return {
    state, flags,
    async callTool(name, args) {
      state.calls.push(name === 'nemo_command' ? args.operation : name);
      if (flags.malformed === 'text') return { result: { content: [{ type: 'text', text: 'not json at all' }] } };
      if (name === 'nemo_discover') {
        return structured({ instances: [{ instanceId: INSTANCE, buildId: flags.foreignBuild ? '1.2.3:' + 'b'.repeat(40) : BUILD,
          nativeHostStatus: { instanceId: INSTANCE, available: !!state.doc, ...(state.doc ? { documentId: flags.noDocumentId ? '' : state.doc.id, contentRevision: state.doc.rev, lifecycleGeneration: state.generation } : {}) } }] });
      }
      if (name !== 'nemo_command') throw new Error('unexpected tool ' + name);
      if (flags.malformed === 'shape') return structured({ apiVersion: args.apiVersion, requestId: args.requestId, instanceId: INSTANCE, documentId: state.doc ? state.doc.id : '', contentRevision: 0 });
      return structured(args.apiVersion === 1 ? v1(args) : v2(args));
    },
    async close() { state.closed = true; },
  };
}

module.exports = { createStubApp, INSTANCE, FOREIGN, BUILD };
