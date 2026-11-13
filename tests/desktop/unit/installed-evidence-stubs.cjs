'use strict';
// A disposable stand-in for the packaged app behind nemo-mcp: just enough of the project, vector, group, asset, raster, image
// mesh and bitmap-brush contract for the kit, the runner and the lane rows to be exercised. Nothing here launches an app; every
// behaviour that matters can be switched to misbehave with a flag, so a test can inject the fault a weak assertion would miss.
//
// Every reply a lane row READS follows the shape the packaged candidate answered, captured from a run (hit-test listing, anchor pages, element
// property read, canvas background edit, brush cancellation and quota refusals): installed-evidence-reads.test.cjs holds literal copies of those
// replies and compares the stub's shapes with them.
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
  // groups: the export ignores group.combine (a defect the first real runs found); flatten folds a nested combining group as ONE operand (the paint rule)
  // instead of folding every leaf once; a flatten whose fold is empty deletes the members instead of being refused (the baseline JS quirk)
  groupCombineIgnored: false, flattenNestedAsOperand: false, flattenEmptyDeletes: false,
  // groups: flatten of a box result keeps the sharp box instead of the baseline's Paper.js simplify(2.5) refit; the combined paint takes the OUTERMOST member's style instead of the topmost; the result gets a new id instead of the style member's
  flattenSharpBox: false, combineStyleFromFirst: false, flattenNewId: false,
  // image mesh: the attach is acknowledged but nothing is bound; a sculpt is acknowledged but changes nothing
  meshNeverBound: false, meshOffsetsIgnored: false,
  // bitmap brush: a publication is wrongly treated as an edit
  publishAddsHistory: false,
  // assets: removal ignores that a placement still uses the asset
  removeIgnoresUse: false,
  // the hit-test listing is refused this many times as "Subselect presentation changed" (observed right after a command), then answers
  listingRace: 0,
  // the shared image quota in bytes for bitmap-brush publications (the installed app: 384 MiB); null = unlimited
  quotaBytes: null,
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

// Paper.js simplify(2.5) of a closed box, in the corner order the engine's fold gives (left-bottom, left-top, right-top, right-bottom): what the baseline's flatten leaves behind.
// Uses the shipped src/paper-full.min.js, so the stub's refit is Paper's own and never a number typed here.
let paperCache = null;
function refitPath(x0, y0, x1, y1) {
  paperCache = paperCache || require('../../helpers/native-import-paper.cjs').load();
  const path = new paperCache.Path({ insert: false });
  [[x0, y1], [x0, y0], [x1, y0], [x1, y1]].forEach(([x, y]) => path.add(new paperCache.Point(x, y)));
  path.closed = true;
  path.simplify(2.5);
  return path;
}
function refitBox(x0, y0, x1, y1) {
  const path = refitPath(x0, y0, x1, y1);
  const hull = path.segments.reduce((a, g) => {
    const xs = [g.point.x, g.point.x + g.handleIn.x, g.point.x + g.handleOut.x], ys = [g.point.y, g.point.y + g.handleIn.y, g.point.y + g.handleOut.y];
    return [Math.min(a[0], ...xs), Math.min(a[1], ...ys), Math.max(a[2], ...xs), Math.max(a[3], ...ys)];
  }, [Infinity, Infinity, -Infinity, -Infinity]);
  const b = path.bounds;
  return { hull, box: [b.x, b.y, b.right, b.bottom], count: path.segments.length, refit: [x0, y0, x1, y1] };
}
// A stub document is cloned as JSON for undo, so a result shape is data: a refit box (Paper's path rebuilt on demand) or the exact rows an unrefit fold covered.
const refitPaths = new Map();
function shapeCovers(shape, x, y) {
  if (shape.rows) return (shape.rows[y] || []).some(([a, b]) => x >= a && x < b);
  const key = shape.refit.join(',');
  if (!refitPaths.has(key)) refitPaths.set(key, refitPath(...shape.refit));
  return refitPaths.get(key).contains(new paperCache.Point(x + 0.5, y + 0.5));
}
function coveredRows(pred, y0, y1, width) {
  const rows = {};
  for (let y = y0; y < y1; y++) { const spans = []; for (let x = 0; x < width; x++) if (pred(x, y)) { if (spans.length && spans[spans.length - 1][1] === x) spans[spans.length - 1][1] = x + 1; else spans.push([x, x + 1]); } rows[y] = spans; }
  return rows;
}
function createStubApp(options = {}) {
  const state = { doc: null, dirty: false, docSeq: 0, generation: 0, brushes: {}, calls: [], ...options.initial };
  const flags = { ...FLAGS, ...options };
  const structured = value => ({ result: { structuredContent: value } });
  const newDoc = (w, h, body = {}) => ({ id: 'doc-' + (++state.docSeq), rev: 0, w, h, strokes: [], rasters: [], assets: [], meshes: {}, layers: [{ layerUid: 'L1', name: 'Layer 1' }], groups: {}, canvasBg: '#ffffff', motion: {}, undo: [], redo: [], seq: 0, ...body });
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
  const snap = d => JSON.stringify({ strokes: d.strokes, groups: d.groups, rasters: d.rasters, assets: d.assets, meshes: d.meshes, layers: d.layers, canvasBg: d.canvasBg, motion: d.motion });
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
  // #rrggbb or #rrggbbaa -> [r, g, b, a] (0..255)
  const parseColor = text => { const h = String(text).replace('#', ''); return [0, 2, 4].map(i => parseInt(h.slice(i, i + 2), 16)).concat([h.length === 8 ? parseInt(h.slice(6, 8), 16) : 255]); };
  // Straight-alpha source-over of one pixel, the way a transparent export carries paint: out alpha = a + ba (1 - a).
  function over(image, x, y, rgb, a) {
    const o = (y * image.width + x) * 4, ba = image.rgba[o + 3] / 255, oa = a + ba * (1 - a);
    for (let i = 0; i < 3; i++) image.rgba[o + i] = oa === 0 ? 0 : Math.round((rgb[i] * a + image.rgba[o + i] * ba * (1 - a)) / oa);
    image.rgba[o + 3] = Math.round(oa * 255);
  }
  // ---- groups: a stroke carries its group path (outermost first); a group has a name and a combine mode. Only axis-aligned boxes are modelled. ----
  const coversPoint = (stroke, x, y) => { if (stroke.shape) return shapeCovers(stroke.shape, x, y); const b = bounds(stroke.record.geometry.segments); return x >= Math.round(b.x0) && x < Math.round(b.x1) && y >= Math.round(b.y0) && y < Math.round(b.y1); };
  const FOLDS = { unite: (a, b) => a || b, intersect: (a, b) => a && b, subtract: (a, b) => a && !b, exclude: (a, b) => a !== b };
  const fold = (mode, preds) => (x, y) => preds.slice(1).reduce((acc, pred) => FOLDS[mode](acc, pred(x, y)), preds[0](x, y));
  const groupsOf = (doc, stroke) => stroke.groupPath || [];
  // The operands a group contributes to its parent, the way the paint path reads nested groups (baseline emit()): a combining child is ONE operand,
  // a child with mode none contributes its own operands as if the group were absent; a combining group with fewer than two operands passes them on.
  function emitOperands(doc, gid) {
    const operands = [], seen = new Set();
    for (const s of doc.strokes) {
      const path = groupsOf(doc, s), at = path.indexOf(gid);
      if (at < 0) continue;
      if (at === path.length - 1) operands.push({ pred: (x, y) => coversPoint(s, x, y), fill: s.record.fill, ids: [s.id], folded: false });
      else if (!seen.has(path[at + 1])) { seen.add(path[at + 1]); operands.push(...emitOperands(doc, path[at + 1])); }
    }
    const mode = flags.groupCombineIgnored ? 'none' : doc.groups[gid].mode;
    if (mode === 'none' || operands.length < 2) return operands;
    return [{ pred: fold(mode, operands.map(o => o.pred)), fill: operands[flags.combineStyleFromFirst ? 0 : operands.length - 1].fill, ids: operands.flatMap(o => o.ids), folded: true }];
  }
  function render(doc) {
    // The export fills the project canvas background (new projects: #ffffff, opaque; #00000000 is a transparent export) and the paint composites over it.
    const image = png.canvas(doc.w, doc.h, parseColor(doc.canvasBg));
    const folded = [], tops = [...new Set(doc.strokes.filter(s => groupsOf(doc, s).length).map(s => groupsOf(doc, s)[0]))];
    for (const top of tops) folded.push(...emitOperands(doc, top).filter(o => o.folded));
    const suppressed = new Set(folded.flatMap(o => o.ids));
    for (const s of doc.strokes) {
      const { x0, y0, x1, y1 } = s.shape ? { x0: s.shape.box[0], y0: s.shape.box[1], x1: s.shape.box[2], y1: s.shape.box[3] } : bounds(s.record.geometry.segments), fill = s.record.fill;
      if (!fill || suppressed.has(s.id)) continue;
      const c = [fill.r, fill.g, fill.b].map(v => Math.round(v * 255));
      for (let y = Math.max(0, Math.floor(y0)); y < Math.min(doc.h, Math.ceil(y1)); y++) for (let x = Math.max(0, Math.floor(x0)); x < Math.min(doc.w, Math.ceil(x1)); x++) if (!s.shape || shapeCovers(s.shape, x, y)) over(image, x, y, c, fill.a);
    }
    for (const o of folded) {
      const c = [o.fill.r, o.fill.g, o.fill.b].map(v => Math.round(v * 255));
      for (let y = 0; y < doc.h; y++) for (let x = 0; x < doc.w; x++) if (o.pred(x, y)) over(image, x, y, c, o.fill.a);
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
  const fileOf = doc => ({ format: 'stub', w: doc.w, h: doc.h, layers: doc.layers, project: { canvasW: doc.w, canvasH: doc.h, canvasBg: doc.canvasBg }, motion: doc.motion,
    drawing: { layers: doc.layers.map(l => ({ layerUid: l.layerUid, name: l.name })), objects: doc.strokes.map(s => ({ target: { strokeId: s.id }, layerUid: s.layerUid, ...(groupsOf(doc, s).length ? { groupId: groupsOf(doc, s)[groupsOf(doc, s).length - 1], groupPath: groupsOf(doc, s) } : {}), record: s.record })),
      items: { raster: doc.rasters.map(r => ({ target: { strokeId: r.id }, item: { layerUid: r.layerUid, x: r.x, y: r.y, width: r.width, height: r.height, assetId: r.assetId, ...(r.meshId ? { meshId: r.meshId } : {}) } })) } },
    groups: doc.groups, assets: doc.assets, meshes: doc.meshes });

  function decodePng(bytes) {
    // Corrupt bytes are a decode failure whatever their header says; only a PNG that decodes can be too large.
    try {
      const image = png.decode(bytes);
      const header = { width: image.width, height: image.height };
      if (header.width > MAX_DIMENSION || header.height > MAX_DIMENSION) return { error: 'too large', ...header };
      const at = (u, v) => png.pixelAt(image, Math.floor(image.width * u), Math.floor(image.height * v));
      return { ...header, colors: [at(0.25, 0.25), at(0.75, 0.25), at(0.25, 0.75), at(0.75, 0.75)] };
    } catch { return { error: 'corrupt' }; }
  }

  function v1(request) {
    const p = request.payload;
    if (request.operation === 'project.new' && flags.unconfirmed) return refuse(1, request, 'unavailable', 'The new project was not created. App notice: Native project state is unconfirmed. Reopen Nemo before continuing.');
    if (request.operation === 'project.new') {
      if (state.dirty) return refuse(1, request, 'unsaved_changes', 'The open project has unsaved changes; save it first.');
      state.doc = newDoc(p.canvasW, p.canvasH); state.dirty = false; state.generation++; state.brushes = {};
      return ok(1, request, { documentId: state.doc.id, contentRevision: 0, layers: state.doc.layers.map(l => ({ ...l })) });
    }
    // Real reply seen on the installed app once a failed command had left the project released.
    if (flags.released) return { apiVersion: 1, requestId: request.requestId, instanceId: request.instanceId, documentId: '', ok: false, revision: 0, error: { code: 'unavailable', message: 'No native project is open.' } };
    if (!state.doc || request.documentId !== state.doc.id || request.expectedRevision !== state.doc.rev) return refuse(1, request, 'stale_revision', 'Read a current snapshot.');
    if (request.operation === 'project.saveAs') {
      // Observed on the installed app: the first save after a change is refused as unavailable, the next one is accepted.
      if (flags.saveRace && (flags.saveRace === 'always' || state.raceRev !== state.doc.rev)) { state.raceRev = state.doc.rev; return refuse(1, request, 'unavailable', 'The project was not saved (refused: pin-changed-before-picker). App notice: Save cancelled or unavailable. Choose a new file path.'); }
      fs.writeFileSync(p.path, JSON.stringify(fileOf(state.doc)));
      state.dirty = false; return ok(1, request, { path: p.path });
    }
    if (request.operation === 'project.open') {
      const file = JSON.parse(fs.readFileSync(p.path, 'utf8'));
      if (!flags.keepsInMemory) {
        const strokes = file.drawing.objects.map(o => ({ id: o.target.strokeId, layerUid: o.layerUid, ...(o.groupPath ? { groupPath: o.groupPath } : o.groupId ? { groupPath: [o.groupId] } : {}), record: o.record }));
        const rasters = file.drawing.items.raster.map(r => ({ id: r.target.strokeId, ...r.item }));
        state.doc = newDoc(file.w, file.h, { strokes, rasters, assets: file.assets, meshes: file.meshes, groups: file.groups || {}, layers: file.layers, canvasBg: file.project.canvasBg, motion: file.motion || {} });
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
        return { apiVersion: 2, requestId: request.requestId, instanceId: request.instanceId, documentId: doc.id, contentRevision: doc.rev, ok: false, error: { code: 'invalid_request', message: 'Unsupported object read operation. ' + op + ' applies to opacity documents only; this is an object document. Use ' + ({ 'query.document.snapshot.acquire': 'query.document.object', 'query.document.serialize': 'project.saveAs', 'query.document.evaluate': 'query.document.object.property.read' })[op] + ' instead.' } };
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
        commit(doc, 'group', () => { doc.groups[p.groupId] = { name: p.name, mode: 'none' }; for (const s of doc.strokes) if (p.strokeIds.includes(s.id)) s.groupPath = [p.groupId, ...groupsOf(doc, s)]; });
        return ok(2, request, receipt());
      }
      case 'command.document.object.group.rename': {
        if (!doc.groups[p.groupId]) return ownerRefusal(request, 'not_found', 'unknown group');
        commit(doc, 'group rename', () => { doc.groups[p.groupId].name = p.name; });
        return ok(2, request, receipt());
      }
      case 'command.document.object.group.ungroup': {
        const members = doc.strokes.filter(s => p.strokeIds.includes(s.id) && groupsOf(doc, s).length);
        if (!members.length) return ownerRefusal(request, 'not_found', 'no grouped stroke');
        commit(doc, 'ungroup', () => { for (const s of members) { const gid = groupsOf(doc, s)[0]; for (const t of doc.strokes) if (groupsOf(doc, t).includes(gid)) t.groupPath = groupsOf(doc, t).slice(groupsOf(doc, t).indexOf(gid) + 1); delete doc.groups[gid]; } });
        return ok(2, request, receipt());
      }
      // Setting a mode on a group that exists (combine also creates the group from its members when it does not).
      case 'command.document.object.group.combine': case 'command.document.object.group.mode.set': {
        if (!doc.groups[p.groupId]) return ownerRefusal(request, 'not_found', 'unknown group');
        if (doc.groups[p.groupId].mode === p.mode) return ok(2, request, noop());
        commit(doc, 'group mode', () => { doc.groups[p.groupId].mode = p.mode; });
        return ok(2, request, receipt());
      }
      // Flatten follows the baseline rule (git show 1065e9d8:src/js/group-bridge.js, flattenGroup lines 399-452 over resolveGroupMembers lines 303-315): ONE fold of ALL
      // leaves of the group (nested groups are not collapsed into operands), the group's own mode (none = unite). An empty fold is refused by the native owner as
      // invalid_request, with the reply the installed app gave (evidence-1112 dev50 wire log, seq 1059); the baseline JS deleted the members instead.
      case 'command.document.object.boolean.flatten': {
        const members = doc.strokes.filter(s => groupsOf(doc, s).includes(p.groupId));
        if (!doc.groups[p.groupId] || !members.length) return ownerRefusal(request, 'not_found', 'unknown group');
        const mode = doc.groups[p.groupId].mode === 'none' ? 'unite' : doc.groups[p.groupId].mode;
        const operands = flags.flattenNestedAsOperand ? emitOperands(doc, p.groupId) : members.map(s => ({ pred: (x, y) => coversPoint(s, x, y), fill: s.record.fill }));
        const pred = operands.length > 1 ? fold(mode, operands.map(o => o.pred)) : operands[0].pred, fill = operands[operands.length - 1].fill;
        let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity, count = 0;
        if (!flags.flattenNoop) for (let y = 0; y < doc.h; y++) for (let x = 0; x < doc.w; x++) if (pred(x, y)) { count++; x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x + 1); y1 = Math.max(y1, y + 1); }
        if (!flags.flattenNoop && count === 0 && !flags.flattenEmptyDeletes) return ownerRefusal(request, 'invalid_request', 'Boolean payload is invalid or target is absent.');
        commit(doc, 'flatten', () => {
          if (flags.flattenNoop) return;
          const consumed = new Set(members.flatMap(s => groupsOf(doc, s).slice(groupsOf(doc, s).indexOf(p.groupId))));
          doc.strokes = doc.strokes.filter(s => !members.includes(s));
          for (const gid of consumed) delete doc.groups[gid];
          if (count === 0) return;
          // The baseline keeps the STYLE member's (the last one's) stroke id on the first result island (tools.js BOOL_KEEP_DATA_FIRST). A plain box fold (no hole) is refit with
          // Paper.js simplify(2.5) like the baseline (a 2-3 anchor curve, not the sharp box); a fold with a hole keeps the exact outline (the slit blocks the refit).
          const style = members[members.length - 1], id = flags.flattenNewId ? 'native-stroke:' + (doc.rev + 1) + ':' + doc.seq++ : style.id;
          const isBox = count === (x1 - x0) * (y1 - y0), shape = isBox && !flags.flattenSharpBox ? refitBox(x0, y0, x1, y1) : { hull: [x0, y0, x1, y1], box: [x0, y0, x1, y1], count: 4, rows: coveredRows(pred, y0, y1, doc.w) };
          doc.strokes.push({ id, layerUid: p.layerUid, shape, record: { ...style.record, target: target(p.layerUid, id), fill, geometry: { ...style.record.geometry, segments: [{ point: { x: x0, y: y0 } }, { point: { x: x1, y: y1 } }] } } });
        });
        return ok(2, request, receipt());
      }
      case 'command.document.object.layer.delete': {
        const uid = p.selection.active;
        if (!doc.layers.some(l => l.layerUid === uid)) return ownerRefusal(request, 'not_found', 'unknown layer');
        commit(doc, 'layer delete', () => { doc.layers = doc.layers.filter(l => l.layerUid !== uid); doc.rasters = doc.rasters.filter(r => r.layerUid !== uid); doc.strokes = doc.strokes.filter(s => s.layerUid !== uid); });
        return ok(2, request, receipt());
      }
      case 'command.document.object.item.edit': {
        if (p.command !== 'object.raster.delete') return refuse(2, request, 'invalid_request', 'unsupported item edit');
        const id = p.stableTarget.strokeId;
        if (!doc.rasters.some(r => r.id === id)) return ownerRefusal(request, 'not_found', 'unknown raster');
        commit(doc, 'raster delete', () => { doc.rasters = doc.rasters.filter(r => r.id !== id); });
        return ok(2, request, receipt({ operation: 'raster.delete', strokeId: id }));
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
      // Real replies of the installed owner: the receipt only; the colour is stored as sent (#rrggbbaa admitted).
      case 'command.document.object.project.settings.edit': {
        if (p.action.kind !== 'setCanvasBackground') return refuse(2, request, 'invalid_request', 'unsupported setting');
        if (doc.canvasBg === p.action.color) return ok(2, request, noop());
        commit(doc, 'canvas background', () => { doc.canvasBg = p.action.color; });
        return ok(2, request, receipt());
      }
      // An element (stroke-level) Motion value at frame 0: the raw sample of the property read is the value that was set, [0, 0] otherwise.
      case 'command.document.object.property.edit': {
        const id = p.target.strokeId;
        if (!id || !doc.strokes.some(s => s.id === id)) return ownerRefusal(request, 'not_found', 'no such element');
        commit(doc, 'element motion', () => { doc.motion[id] = { [p.property]: p.action.values }; });
        return ok(2, request, receipt());
      }
      case 'query.document.object.property.read': {
        const t = p.view.target, held = t.strokeId && doc.motion[t.strokeId] && doc.motion[t.strokeId][p.view.property];
        return ok(2, request, { instanceId: request.instanceId, documentId: doc.id, atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, kind: 'rawSample', target: t, property: p.view.property, dimension: 2, frame: p.view.frame, value: held || [0, 0] });
      }
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
    if (p.view && p.view.kind === 'subselect' && p.view.hitTest) {
      if (state.listingRaceLeft === undefined) state.listingRaceLeft = flags.listingRace;
      if (state.listingRaceLeft > 0) { state.listingRaceLeft--; return refuse(2, request, 'unavailable', 'Subselect presentation changed'); }
      const v = p.view, hex = fill => '#' + [fill.r, fill.g, fill.b].map(c => Math.round(c * 255).toString(16).padStart(2, '0')).join('');
      const listed = doc.strokes.filter(s => s.layerUid === v.layerUid).map(s => { const b = s.shape ? { x0: s.shape.hull[0], y0: s.shape.hull[1], x1: s.shape.hull[2], y1: s.shape.hull[3] } : bounds(s.record.geometry.segments), f = s.record.fill;
        return { stableTarget: target(s.layerUid, s.id), closed: s.record.geometry.closed, count: s.shape ? s.shape.count : s.record.geometry.segments.length, matrix: [1, 0, 0, 1, 0, 0], bounds: [b.x0, b.y0, b.x1, b.y1], role: 'plain', filled: !!f, strokeWidth: 0,
          ...(groupsOf(doc, s).length ? { groupPath: groupsOf(doc, s).map(id => ({ id, mode: doc.groups[id].mode, name: doc.groups[id].name })) } : {}), style: { cap: 'round', join: 'round', opacity: 1, strokeStyle: 'solid', width: 4, ...(f ? { fill: hex(f) } : {}) } }; })
        // `initial.listingExtra`: listed items the stub does not model (a text block, a dynamic shape) served exactly as the candidate answered them.
        .concat((state.listingExtra || []).map(i => ({ ...i, stableTarget: { ...i.stableTarget, layerUid: v.layerUid } })));
      const items = listed.slice(v.cursor, v.cursor + v.limit), end = v.cursor + items.length;
      return ok(2, request, { atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, kind: 'subselect', contextId: ROOT, layerUid: v.layerUid, frame: v.frame, workId: v.workId, viewGeneration: v.viewGeneration,
        cursor: v.cursor, total: listed.length, nextCursor: end === listed.length ? null : end, preview: false, hitTest: true, layerMatrix: [1, 0, 0, 1, 0, 0], items });
    }
    if (p.subselect && p.stableTarget && !p.view) {
      const found = doc.strokes.find(s => s.id === p.stableTarget.strokeId);
      if (!found) return ownerRefusal(request, 'not_found', 'no such stroke');
      const segs = found.record.geometry.segments, c = p.subselect.cursor, page = segs.slice(c, c + p.subselect.limit), end = c + page.length;
      return ok(2, request, { atRevision: p.atRevision, documentSnapshotId: p.documentSnapshotId, subselect: { stableTarget: p.stableTarget, closed: found.record.geometry.closed, total: segs.length, cursor: c, nextCursor: end === segs.length ? null : end, preview: false,
        segments: page.map(g => ({ point: [g.point.x, g.point.y], handleIn: [0, 0], handleOut: [0, 0] })) } });
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
    // Observed on the installed app: a corrupt PNG is invalid_request, a decodable PNG past the size cap is unavailable (image_too_large).
    if (decoded.error) return ownerRefusal(request, decoded.error === 'too large' ? 'unavailable' : 'invalid_request', 'Native image import refused.');
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
    // Real replies seen on the installed app: a publication cancelled before dispatch, and one past the shared quota.
    if (request.cancelledBeforeDispatch) return refuse(2, request, 'cancelled_before_dispatch', 'Native brush resource request refused.', { reason: 'cancelled' });
    if (p.documentSnapshotId !== snapshotId()) return ownerRefusal(request, 'invalid_request', 'stale pin');
    const asset = doc.assets.find(a => a.id === p.assetId);
    if (!asset) return ownerRefusal(request, 'not_found', 'no such asset');
    if (asset.width !== p.width || asset.height !== p.height) return ownerRefusal(request, 'invalid_request', 'wrong-size asset');
    const held = state.brushes[p.brushId], same = held && held.assetId === asset.id;
    if (!same && flags.quotaBytes !== null) {
      const others = Object.entries(state.brushes).filter(([id]) => id !== p.brushId).reduce((n, [, b]) => n + b.bytes, 0);
      if (others + p.width * p.height * 4 + 128 > flags.quotaBytes) return refuse(2, request, 'unavailable', 'Native brush resource request refused.', { reason: 'over-quota' });
    }
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
