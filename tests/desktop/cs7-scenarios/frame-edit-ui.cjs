'use strict';
// #2248 installed frame-edit row, foreground. The UI32 helper cannot send F5/F6/F7, right-click or free
// drags, so a person performs each gesture. The person never reports a verdict and never attaches document
// state: after every reply the HARNESS makes the read-only Nemo MCP reads itself (history depth, per-layer cell
// runs; session.read, bound to the candidate) and this oracle computes the effect itself (revision, history
// counts, total frames, cell kinds). Pixels come from the helper's own owned-window capture, not from the operator.
const path = require('node:path');
const { isDeepStrictEqual: same } = require('node:util');
const kit = require('./installed-kit.cjs');
const { plain, int, text, malformed, rawOnly } = kit;

const ROW = 'frame-edit-ui';
const CLAUSES = ['scene-f5-f6-f7', 'scene-menu-and-shortcuts', 'scene-undo-redo', 'scene-save-reopen',
  'component-edits', 'component-undo-redo', 'component-save-reopen', 'layer-row-styling'];
const KINDS = ['key', 'interpolated', 'held', 'unexposed'];
const OP = { history: 'query.document.object.history', timing: 'query.document.object.layer.timing' };
const CAPTURE = 'capture-1.png';

// ---- raw read-back -> state ---------------------------------------------------------------------
function pair(p, operation) {
  return plain(p) && plain(p.request) && plain(p.response) && p.request.operation === operation && plain(p.request.payload) &&
    p.response.ok === true && plain(p.response.result) && text(p.response.documentId) && text(p.response.instanceId) ? p : null;
}
// Expand the per-layer cell runs of one context into per-frame kind/content arrays. Anything that does not
// tile frames 0..total exactly, or was not pinned to the revision the history read reports, is refused.
function summarize(reads, context, why = {}) {
  const no = reason => { why.reason = reason; return null; };
  if (!plain(reads)) return no('reads is not an object');
  const h = pair(reads.history, OP.history);
  if (!h) return no('history is not an ok query.document.object.history request/response pair');
  const r = h.response.result;
  if (![r.atRevision, r.undoDepth, r.redoDepth].every(int) || !text(r.documentSnapshotId)) return no('history result lacks atRevision, undoDepth, redoDepth or documentSnapshotId');
  if (!Array.isArray(reads.layers) || !reads.layers.length) return no('layers is not a non-empty array');
  const rows = {}; let total = null;
  for (const item of reads.layers) {
    const p = pair(item, OP.timing);
    if (!p) return no('a layer read is not an ok query.document.object.layer.timing pair');
    const q = p.request.payload, res = p.response.result;
    if (q.view !== 'cells' || q.contextId !== context || q.atRevision !== r.atRevision || q.documentSnapshotId !== r.documentSnapshotId) return no('a layer read is not view "cells" of the expected context pinned to the history revision');
    if (p.response.documentId !== h.response.documentId || p.response.instanceId !== h.response.instanceId) return no('a layer read answers for another document or instance');
    if (res.kind !== 'layerCells' || res.contextId !== context || !text(res.layerUid) || res.layerUid !== q.layerUid ||
      res.atRevision !== r.atRevision || res.documentSnapshotId !== r.documentSnapshotId || Object.hasOwn(rows, res.layerUid) ||
      !plain(res.cells) || !Array.isArray(res.cells.runs) || res.cells.nextFrame !== null) return no('a layer cells result does not have the layerCells shape (kind, contextId, layerUid, revision, cells.runs, nextFrame null)');
    const k = [], c = [];
    for (const run of res.cells.runs) {
      if (!Array.isArray(run) || run.length !== 6) return no('a cell run is not a six-element array');
      const [start, count, kind, , has] = run;
      if (start !== k.length || !int(count) || count < 1 || count > 100000 || !KINDS.includes(kind) || typeof has !== 'boolean') return no('a cell run does not tile from the previous run or has an unknown kind');
      for (let i = 0; i < count; i++) { k.push(kind); c.push(has); }
    }
    if (!k.length || (total !== null && total !== k.length)) return no('layers disagree on the total frame count');
    total = k.length; rows[res.layerUid] = { k, c };
  }
  return { documentId: h.response.documentId, instanceId: h.response.instanceId, revision: r.atRevision, snapshot: r.documentSnapshotId,
    undo: r.undoDepth, redo: r.redoDepth, total, rows };
}

// ---- what each gesture must do (baseline timeline semantics), judged on the layer rows -----------
const eq = (p, i, n, j) => p.k[i] === n.k[j] && p.c[i] === n.c[j];
const PRE = reason => ({ pre: reason }), FAIL = reason => ({ fail: reason });
function grows(p, n, c) {          // F5, Insert a frame, Extend exposure: one held cell appears after frame c
  if (n.k.length !== p.k.length + 1) return FAIL('total-frames-not-plus-one');
  for (let i = 0; i <= c; i++) if (!eq(p, i, n, i)) return FAIL('frames-before-edit-changed');
  if (n.k[c + 1] !== 'held') return FAIL('inserted-cell-not-held');
  for (let i = c + 1; i < p.k.length; i++) if (!eq(p, i, n, i + 1)) return FAIL('later-frames-not-shifted');
  return null;
}
const JUDGES = {
  insertFrame: grows,
  extendExposure: grows,
  insertKey(p, n, c) {
    if (p.k[c] !== 'held') return PRE('selected cell is not a held cell');
    if (n.k.length !== p.k.length) return FAIL('total-frames-changed');
    if (n.k[c] !== 'key' || n.c[c] !== p.c[c]) return FAIL('cell-not-promoted-to-key-with-its-content');
    for (let i = 0; i < p.k.length; i++) if (i !== c && !eq(p, i, n, i)) return FAIL('other-frames-changed');
    return null;
  },
  insertBlankKey(p, n, c) {
    if (n.k.length !== p.k.length) return FAIL('total-frames-changed');
    if (n.k[c] !== 'key' || n.c[c] !== false) return FAIL('cell-not-a-blank-key');
    for (let i = 0; i < c; i++) if (!eq(p, i, n, i)) return FAIL('frames-before-edit-changed');
    for (let i = 0; i < p.k.length; i++) if (i !== c && p.k[i] !== n.k[i]) return FAIL('other-frame-kinds-changed');
    return null;
  },
  clearKey(p, n, c) {
    if (p.k[c] !== 'key' || c < 1) return PRE('selected cell is not a key after frame 1');
    if (n.k.length !== p.k.length) return FAIL('total-frames-changed');
    if (n.k[c] !== 'held' || n.c[c] !== n.c[c - 1]) return FAIL('key-not-demoted-to-the-previous-hold');
    for (let i = 0; i < c; i++) if (!eq(p, i, n, i)) return FAIL('frames-before-edit-changed');
    for (let i = 0; i < p.k.length; i++) if (i !== c && p.k[i] !== n.k[i]) return FAIL('other-frame-kinds-changed');
    return null;
  },
  duplicateKey(p, n, c) {
    if (p.c[c] !== true) return PRE('selected cell has no content to duplicate');
    if (n.k.length !== p.k.length + 1) return FAIL('total-frames-not-plus-one');
    for (let i = 0; i <= c; i++) if (!eq(p, i, n, i)) return FAIL('frames-before-edit-changed');
    if (n.k[c + 1] !== 'key' || n.c[c + 1] !== true) return FAIL('copy-not-a-key-with-content');
    for (let i = c + 1; i < p.k.length; i++) if (!eq(p, i, n, i + 1)) return FAIL('later-frames-not-shifted');
    return null;
  },
  removeFrames(p, n, c, span) {
    const count = span[1] - span[0] + 1;
    if (count < 1 || span[1] >= p.k.length || count >= p.k.length) return PRE('selected span is not removable');
    if (n.k.length !== p.k.length - count) return FAIL('total-frames-not-reduced-by-span');
    for (let i = 0; i < span[0]; i++) if (!eq(p, i, n, i)) return FAIL('frames-before-span-changed');
    for (let i = span[1] + 1; i < p.k.length; i++) if (!eq(p, i, n, i - count)) return FAIL('later-frames-not-shifted-back');
    return null;
  },
};
const sameDocument = (a, b) => a.documentId === b.documentId && a.instanceId === b.instanceId;
const layersOf = s => Object.keys(s.rows).sort();
const sameContent = (a, b) => a.total === b.total && same(a.rows, b.rows);
function oneEdit(p, n) {
  if (!sameDocument(p, n) || !same(layersOf(p), layersOf(n))) return 'document-or-layers-changed';
  if (n.revision !== p.revision + 1) return 'not-exactly-one-revision';
  if (n.undo !== p.undo + 1 || n.redo !== 0) return 'not-exactly-one-history-entry';
  return null;
}

// ---- the operator script -----------------------------------------------------------------------
const READS = 'Evidence reply (a JSON object): {observation: one plain sentence of what you saw, productFailure: true only if the app visibly ' +
  'misbehaved (crash, missing menu item, no reaction at all), selection: {layerUid, frame, span?}}. Do NOT attach any MCP reads or document ' +
  'state: the runner reads the document itself right after your reply, so after replying touch nothing in the app. Frames in `selection` are ' +
  '0-based (the UI shows frame N as N+1). Do not add pass/fail fields: the oracle refuses a reply that carries a verdict or any read.';
const SCENE_READS = READS + ' The runner reads context "scene-root".';
const COMP_READS = READS + ' The runner reads the entered component\'s context (the context id you report) and "scene-root".';
const gesture = (id, check, ctx, judge, how) => ({ id, check, ctx, type: 'gesture', judge, how });
const PLAN = [
  { id: 'window-placement', type: 'placement', check: null,
    how: kit.PLACEMENT + ' Reply with {observation: "placed"}.' },
  { id: 'scene-setup', type: 'setup', check: null,
    how: 'Make a clean test document through the ordinary UI: File > New with the defaults (Untitled, 1920x1080, 24 fps), close the tutorial. On ' +
      '"Layer 1" draw one stroke on frame 1 (the cell becomes a key with content). Add a second layer, draw one stroke on its frame 1, then ' +
      'right-click its row in the layer list (the Layer menu) and choose "Convert to component". Rename the FIRST layer to a name of about 70 ' +
      'characters, for example "A very long layer name that must stay on a single line with an ellipsis ok". Leave the layer list fully visible ' +
      'and no row hovered. Reply with {observation, productFailure, layers: {drawing: <layerUid of Layer 1>, component: <layerUid of the component ' +
      'layer>}}. ' + SCENE_READS },
  { id: 'layer-row-styling', type: 'styling', check: 'layer-row-styling',
    how: 'The runner has just saved an owned-window capture of the current screen. Open the PNG named in details.capturePath and measure, in ' +
      'pixels of that image, two rectangles: layerRows (the whole block of layer rows in the timeline list) and longName (the name area of the ' +
      'row with the long name). Reply with {observation, productFailure, rects: {layerRows: {x,y,w,h}, longName: {x,y,w,h}}}. Do not judge the ' +
      'styling yourself: report only the rectangles and anything visibly broken.' },
  gesture('scene-f5', 'scene-f5-f6-f7', 'scene', 'insertFrame',
    'Click the cell of Layer 1 at frame 1 so exactly that cell is selected, then press the REAL F5 key once. ' + SCENE_READS),
  gesture('scene-f6', 'scene-f5-f6-f7', 'scene', 'insertKey',
    'Click the (held) cell of Layer 1 at frame 2 only, then press the REAL F6 key once. ' + SCENE_READS),
  gesture('scene-f7', 'scene-f5-f6-f7', 'scene', 'insertBlankKey',
    'Click the (held) cell of Layer 1 at frame 3 only, then press the REAL F7 key once. ' + SCENE_READS),
  gesture('scene-menu-insert-frame', 'scene-menu-and-shortcuts', 'scene', 'insertFrame',
    'Right-click the cell of Layer 1 at frame 3 and choose "Insert a frame" in the context menu (click it with the mouse; do not use a key). ' + SCENE_READS),
  gesture('scene-menu-insert-keyframe', 'scene-menu-and-shortcuts', 'scene', 'insertKey',
    'Right-click the (held) cell of Layer 1 at frame 4 and choose "Insert a keyframe". ' + SCENE_READS),
  gesture('scene-menu-insert-blank', 'scene-menu-and-shortcuts', 'scene', 'insertBlankKey',
    'Right-click the (held) cell of Layer 1 at frame 6 and choose "Insert a blank keyframe". ' + SCENE_READS),
  gesture('scene-clear-key', 'scene-menu-and-shortcuts', 'scene', 'clearKey',
    'Right-click the blank key at frame 6 of Layer 1 and choose "Clear the keyframe". ' + SCENE_READS),
  gesture('scene-extend-exposure', 'scene-menu-and-shortcuts', 'scene', 'extendExposure',
    'Click the key cell of Layer 1 at frame 2 only and press the + key once (Extend exposure). ' + SCENE_READS),
  gesture('scene-duplicate-key', 'scene-menu-and-shortcuts', 'scene', 'duplicateKey',
    'Duplicate keyframe has no default key. In Settings > Shortcuts bind it to a free key (note which one in observation), close Settings, click ' +
    'the key cell of Layer 1 at frame 2 only (it has content), press that key once. ' + SCENE_READS),
  gesture('scene-delete-frames', 'scene-menu-and-shortcuts', 'scene', 'removeFrames',
    'Click the Layer 1 cell at frame 8, shift-click frame 9 so exactly frames 8 and 9 are selected, right-click and choose "Delete the frames". ' +
    'selection.frame is 7 and selection.span is [7, 8]. ' + SCENE_READS),
  { id: 'scene-undo', type: 'undo', check: 'scene-undo-redo', ctx: 'scene',
    how: 'Press Cmd+Z exactly once with the app focused (no menu). ' + SCENE_READS },
  { id: 'scene-redo', type: 'redo', check: 'scene-undo-redo', ctx: 'scene',
    how: 'Press Cmd+Shift+Z exactly once. ' + SCENE_READS },
  { id: 'scene-save', type: 'save', check: 'scene-save-reopen', ctx: 'scene',
    how: 'Save the document (File > Save As) into the private report directory as saved/frame-edit-scene.json. Reply additionally with savedFile: ' +
      '"saved/frame-edit-scene.json" (path relative to the report directory). ' + SCENE_READS },
  { id: 'scene-reopen', type: 'reopen', check: 'scene-save-reopen', ctx: 'scene',
    how: 'Open that same saved file through File > Open (it opens as the document; do not edit anything). Reply with the same savedFile. ' + SCENE_READS },
  { id: 'component-enter', type: 'enter', check: 'component-edits', ctx: 'component',
    how: 'Double-click the component layer row to enter the component (the tab bar shows the component beside Scene). Do not edit. Reply also with ' +
      'component: {context: <the entered component\'s context id>, layerUid: <one component layer that has a key with content at frame 1>}. ' + COMP_READS },
  gesture('component-f5', 'component-edits', 'component', 'insertFrame',
    'Inside the component click that layer\'s cell at frame 1 only and press the REAL F5 key once. ' + COMP_READS),
  gesture('component-f6', 'component-edits', 'component', 'insertKey',
    'Click the (held) cell at frame 2 only and press the REAL F6 key once. ' + COMP_READS),
  gesture('component-extend-exposure', 'component-edits', 'component', 'extendExposure',
    'Click the key cell at frame 2 only and press the + key once. ' + COMP_READS),
  gesture('component-f7', 'component-edits', 'component', 'insertBlankKey',
    'Click the (held) cell at frame 3 only and press the REAL F7 key once. ' + COMP_READS),
  gesture('component-duplicate-key', 'component-edits', 'component', 'duplicateKey',
    'Click the key cell at frame 2 only (it has content) and press the Duplicate keyframe key you bound earlier, once. ' + COMP_READS),
  gesture('component-clear-key', 'component-edits', 'component', 'clearKey',
    'Right-click the duplicate key at frame 3 (the new key with content) and choose "Clear the keyframe". ' + COMP_READS),
  gesture('component-delete-frames', 'component-edits', 'component', 'removeFrames',
    'Click the cell at frame 2 only, right-click and choose "Delete the frames". selection.frame is 1 and selection.span is [1, 1]. ' + COMP_READS),
  { id: 'component-undo', type: 'undo', check: 'component-undo-redo', ctx: 'component',
    how: 'Press Cmd+Z exactly once inside the component. ' + COMP_READS },
  { id: 'component-redo', type: 'redo', check: 'component-undo-redo', ctx: 'component',
    how: 'Press Cmd+Shift+Z exactly once. ' + COMP_READS },
  { id: 'component-save', type: 'save', check: 'component-save-reopen', ctx: 'component',
    how: 'Back in the Scene tab or not, save with File > Save As as saved/frame-edit-component.json (relative to the report directory); reply with ' +
      'savedFile. Stay inside the component for the reads. ' + COMP_READS },
  { id: 'component-reopen', type: 'reopen', check: 'component-save-reopen', ctx: 'component',
    how: 'Open the saved file again through File > Open, double-click the component layer to enter it again and do not edit. Then remove the ' +
      'Duplicate keyframe binding you set in Settings > Shortcuts and close Settings, so the component is in view again. Reply only after that. ' + COMP_READS },
];

// ---- the scenario --------------------------------------------------------------------------------
function createWorkflow({ reportDirectory }) {
  let used = false;
  return { async run({ session, row }) {
    if (used) throw new Error('frame-edit-ui cannot restart');
    used = true;
    if (row.issue !== 2248 || !same(row.checks.map(c => c.id), CLAUSES)) throw new Error('original frame-edit clauses changed');
    const outcome = Object.fromEntries(CLAUSES.map(id => [id, 'unrun']));
    const remaining = Object.fromEntries(CLAUSES.map(id => [id, PLAN.filter(s => s.check === id).length]));
    const dir = path.join(reportDirectory, ROW);
    const finish = (stop, reason) => {
      kit.privateEvidence(dir, 'outcome', { schema: 'nemo.frame-edit-ui-private/1', outcome, reason: reason || null });
      return { checks: CLAUSES.map(id => ({ id, status: outcome[id] })), safeToContinue: !stop };
    };
    const owner = await session.runtimeContext();
    // Latest verified state of the scene and of the entered component, the last edit's before/after pair per context
    // (for undo/redo), and the last saved file.
    let scene = null, comp = null, drawing = null, compLayer = null, componentContext = null, saved = null;
    const lastEdit = {};
    try {
    for (const step of PLAN) {
      if (!same(await session.identity(), owner.identity)) throw malformed(ROW, step.id);
      const details = { sourceSha: owner.identity.sourceSha, pid: owner.identity.pid, windowId: owner.window.windowId,
        reportDirectory, evidenceRule: 'Raw observations only. Source tests and posted input do not establish PASS.' };
      if (step.type === 'styling') {
        const captured = await session.input('capture', CAPTURE);
        if (!plain(captured) || captured.capture !== CAPTURE) throw malformed(ROW, step.id);
        details.capturePath = path.join(reportDirectory, CAPTURE);
      }
      const ev = await session.requestEvidence({ row: ROW, phase: step.id, instruction: step.how, details });
      if (!rawOnly(ev) || !text(ev.observation) || typeof ev.productFailure !== 'boolean') throw malformed(ROW, step.id);
      kit.privateEvidence(dir, step.id, ev);
      if (step.type === 'placement') continue;
      if (ev.productFailure) {
        // The operator may only report a failure, never a success; a reported failure is taken as stated.
        if (step.check) outcome[step.check] = 'fail';
        return finish(true, step.id + (step.check ? ' failure reported' : ' precondition failed'));
      }
      const inComponent = step.ctx === 'component', entering = step.type === 'enter';
      if (inComponent && !entering && !componentContext) throw malformed(ROW, step.id);
      if (entering && !(plain(ev.component) && text(ev.component.context) && ev.component.context !== 'scene-root' && text(ev.component.layerUid))) throw malformed(ROW, step.id);
      if (step.type === 'styling') {
        const image = kit.loadPng(kit.readInside(reportDirectory, CAPTURE)), observed = await session.input('observe');
        if (!image || !plain(ev.rects) || !Array.isArray(observed?.nodes)) throw malformed(ROW, step.id);
        // The capture is the owned window; its proportions confirm the requested 1620x1420 placement.
        if (Math.abs(image.width / image.height / (1620 / 1420) - 1) > 0.03) throw malformed(ROW, step.id);
        if (!kit.rectOk(image, ev.rects.layerRows, 120, 40) || !kit.rectOk(image, ev.rects.longName, 60, 8)) throw malformed(ROW, step.id);
        const scale = image.width / 1620, stats = kit.rectStats(image, ev.rects.layerRows);
        const bands = kit.textBands(image, ev.rects.longName, Math.max(2, Math.round(2 * scale)));
        const labels = observed.nodes.filter(n => plain(n) && n.role === 'AXButton' && Array.isArray(n.labels)).flatMap(n => n.labels.map(String));
        const named = [/^(Hide|Show) /, /^(Lock|Unlock) /, /^(Mark shy|Unmark shy) /].every(re => labels.some(l => re.test(l)));
        const dark = stats.meanLuma < 110 && stats.lightFraction < 0.08;
        const oneLine = bands.length === 1 && bands[0][1] - bands[0][0] + 1 <= 30 * scale;
        if (!(dark && oneLine && named)) {
          outcome[step.check] = 'fail';
          return finish(true, 'layer-row-styling ' + JSON.stringify({ dark, oneLine, named }));
        }
        outcome[step.check] = 'pass'; continue;
      }
      // The harness reads the document (never the operator). The first installed run may meet MCP replies shaped
      // differently from the contract this oracle reads. Such reads are never guessed at: the reason is kept
      // privately, then the batch stops.
      const readContext = inComponent ? (entering ? ev.component.context : componentContext) : 'scene-root';
      const seen = await kit.readTimeline(session, { contexts: inComponent ? [readContext, 'scene-root'] : ['scene-root'], record: { dir, name: step.id } });
      const reads = { history: seen.history, layers: seen.layers[readContext] }, sceneLayers = seen.layers['scene-root'];
      const why = {};
      const next = summarize(reads, inComponent ? componentContext : 'scene-root', why);
      const nextScene = inComponent ? summarize({ history: reads.history, layers: sceneLayers }, 'scene-root', why) : next;
      if ((!next || !nextScene) && step.type !== 'enter') kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.frame-edit-ui-private/1', phase: step.id, reason: why.reason });
      if (step.type === 'enter') {
        const c = ev.component;
        if (!plain(c) || !text(c.context) || c.context === 'scene-root' || !text(c.layerUid)) throw malformed(ROW, step.id);
        const entered = summarize(reads, c.context);
        const sceneNow = summarize({ history: reads.history, layers: sceneLayers }, 'scene-root');
        if (!entered || !sceneNow || !Object.hasOwn(entered.rows, c.layerUid) || entered.rows[c.layerUid].k[0] !== 'key' ||
          entered.rows[c.layerUid].c[0] !== true) throw malformed(ROW, step.id);
        // Entering is a view change: no edit, no history entry, the scene untouched.
        if (!sameDocument(scene, entered) || !sameContent(scene, sceneNow) || entered.undo !== scene.undo || entered.redo !== scene.redo) {
          outcome[step.check] = 'fail'; return finish(true, 'entering the component changed the document');
        }
        componentContext = c.context; compLayer = c.layerUid; comp = entered; scene = sceneNow;
        remaining[step.check]--; continue;
      }
      if (!next || !nextScene) throw malformed(ROW, step.id);
      const prev = inComponent ? comp : scene;
      const adopt = () => { if (inComponent) { comp = next; scene = nextScene; } else scene = next; };
      let failure = null;
      if (step.type === 'setup') {
        const l = ev.layers;
        if (!plain(l) || !text(l.drawing) || !text(l.component) || !Object.hasOwn(next.rows, l.drawing) || !Object.hasOwn(next.rows, l.component) ||
          next.rows[l.drawing].k[0] !== 'key' || next.rows[l.drawing].c[0] !== true || next.total < 12) throw malformed(ROW, step.id);
        drawing = l.drawing; scene = next; continue;
      }
      if (step.type === 'gesture') {
        const uid = inComponent ? compLayer : drawing, sel = ev.selection;
        if (!plain(sel) || sel.layerUid !== uid || !int(sel.frame) || sel.frame >= prev.total || !Object.hasOwn(next.rows, uid)) throw malformed(ROW, step.id);
        const span = step.judge === 'removeFrames' ? sel.span : null;
        if (span && !(Array.isArray(span) && span.length === 2 && int(span[0]) && int(span[1]) && span[0] === sel.frame && span[1] >= span[0])) throw malformed(ROW, step.id);
        const verdict = JUDGES[step.judge](prev.rows[uid], next.rows[uid], sel.frame, span);
        if (verdict?.pre) throw malformed(ROW, step.id);
        failure = verdict?.fail || oneEdit(prev, next);
        if (!failure && inComponent && !sameContent(scene, nextScene)) failure = 'scene-changed-by-component-edit';
        if (!failure) { lastEdit[step.ctx] = { before: prev, after: next }; adopt(); }
      } else if (step.type === 'undo' || step.type === 'redo') {
        const edit = lastEdit[step.ctx];
        if (!edit || prev !== edit.after && step.type === 'undo') throw malformed(ROW, step.id);
        const undoing = step.type === 'undo', want = undoing ? edit.before : edit.after;
        if (!sameDocument(prev, next) || !sameContent(want, next)) failure = undoing ? 'undo-did-not-restore-the-previous-timeline' : 'redo-did-not-restore-the-edit';
        else if (undoing ? next.undo !== prev.undo - 1 || next.redo !== prev.redo + 1 : next.undo !== prev.undo + 1 || next.redo !== prev.redo - 1) failure = 'not-exactly-one-history-step';
        else if (inComponent && !sameContent(scene, nextScene)) failure = 'scene-changed-by-component-undo-redo';
        if (!failure) { adopt(); if (undoing) lastEdit[step.ctx] = { before: edit.before, after: edit.after, undone: next }; }
      } else if (step.type === 'save' || step.type === 'reopen') {
        const bytes = kit.readInside(reportDirectory, ev.savedFile);
        let json = null;
        try { json = bytes ? JSON.parse(bytes.toString('utf8')) : null; } catch { json = null; }
        const doc = plain(json) ? (int(json.totalFrames) ? json : json.document) : null;
        if (!bytes || !plain(doc) || !int(doc.totalFrames) || !Array.isArray(doc.layers)) throw malformed(ROW, step.id);
        const hash = kit.sha256(bytes);
        if (step.type === 'save') {
          // Saving must not edit: the reads taken after Save are exactly the state just verified.
          if (!sameDocument(prev, next) || next.revision !== prev.revision || !sameContent(prev, next) || (inComponent && !sameContent(scene, nextScene))) throw malformed(ROW, step.id);
          failure = doc.totalFrames === nextScene.total ? null : 'saved-total-frames-differs';
          saved = { file: ev.savedFile, hash, timeline: next, scene: nextScene };
        } else {
          if (!saved || saved.file !== ev.savedFile) throw malformed(ROW, step.id);
          if (hash !== saved.hash) failure = 'saved-file-changed-by-opening';
          else if (next.undo !== 0 || next.redo !== 0) failure = 'reopened-history-not-fresh';
          else if (!sameContent(saved.timeline, next) || !sameContent(saved.scene, nextScene)) failure = 'reopened-timeline-differs-from-saved';
          if (!failure) adopt();
        }
      }
      if (failure) { outcome[step.check] = 'fail'; return finish(true, step.id + ': ' + failure); }
      if (--remaining[step.check] === 0) outcome[step.check] = 'pass';
    }
    return finish(false);
    } catch (error) {
      // Keep what was established before an unusable reply or an input refusal; the batch still stops.
      try { kit.privateEvidence(dir, 'partial', { schema: 'nemo.frame-edit-ui-private/1', outcome, stoppedBy: error?.code || 'error' }); } catch { /* receipt directory unavailable */ }
      throw error;
    }
  } };
}
function prepare({ config }) { return createWorkflow({ reportDirectory: config.reportDirectory }); }
module.exports = { prepare, createWorkflow, summarize, JUDGES, CLAUSES, PLAN };
