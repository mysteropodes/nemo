'use strict';
// #2256 installed row: drawing inside an entered component (PRs #2765, #2784, #2786, #2800, #2803), foreground.
// The UI32 helper has no pointer drawing and no double-click, so a person draws. The person never reports a
// verdict and never attach document state: after each reply the HARNESS makes the read-only Nemo MCP reads itself
// (history depth, per-layer cell runs; session.read, bound to the candidate) and this oracle computes the effect itself (revision step, history step, which cell gained
// content, whether the Scene stayed unchanged). Pixels come from the helper's own owned-window captures (instances; the
// region is the drawing canvas's own accessibility frame) and from PNGs the person exports into the private report directory
// (bitmap brush; their path, hash and mtime are measured by the runner and compared with a baseline export). The tool, the
// Fill Brush placement and the app mode are read from the helper's accessibility facts after each gesture. How a tool was
// picked (button or key) and how a menu item was reached cannot be observed: those surfaces are declared operator-attested.
// Expected behaviour is the baseline's (1065e9d8): drawing on a held frame in Animation 2D promotes that frame
// to its own key (app.js saveActiveLayerFrameOrPromote), enterSymbol swaps in the component's layers so every
// tool draws unchanged inside it, and every instance of the component shows the component's frames.
const path = require('node:path');
const { isDeepStrictEqual: same } = require('node:util');
const kit = require('./installed-kit.cjs');
const { summarize } = require('./frame-edit-ui.cjs');
const { plain, int, text, malformed, rawOnly } = kit;

const ROW = 'component-edit-draw';
const CLAUSES = ['component-draw-tools', 'component-draw-undo-redo', 'component-instances-pixels', 'component-bitmap-export',
  'component-break-apart', 'component-motion-draw', 'component-draw-save-reopen'];
const BEFORE = 'capture-21.png', AFTER = 'capture-22.png';
const BASELINE = 'exports/bitmap-baseline.png', EXPORT = 'exports/bitmap-brush.png', SAVED = 'saved/component-draw.json';
const DIFF = 24, INK_MIN = 200, TILE_PIXELS = 10, TILES_X = 8, TILES_Y = 6, TILES_MAX = 24, EXPORT_INK_MIN = 200;
const CANVAS = 'drawing-canvas', PLAYHEAD = 'info-frame';
// The page's own labels (all four languages of src/js/i18n.js, plus the static English of src/index.html) name each control.
const TOOLS = { select: /^(Select|Sélection|選択|Selección)/i, fillbrush: /^(Fill brush|Pinceau de remplissage|塗りブラシ|Pincel de relleno)/i, draw: /^(Brush|Pinceau(?! de remplissage)|ブラシ|Pincel(?! de relleno))/i,
  pen: /^(Pen|Plume|ペン|Pluma)/i, rect: /^(Rectangle|Rectángulo|長方形)/i, line: /^(Line|Ligne|直線|Línea)/i, ellipse: /^(Ellipse|Elipse|楕円)/i };
const MODES = { motion: /^(Motion|モーション)/i, anim2d: /(Animation 2D|2D Animation|Animación 2D|2Dアニメーション)/i };
const ABOVE = /^(Above|Au-dessus|上|Encima)/i;

// ---- raw pixel measurements (every rectangle comes from the helper's accessibility frames, every number from the image) ----
const far = (p, q) => Math.abs(p[0] - q[0]) > DIFF || Math.abs(p[1] - q[1]) > DIFF || Math.abs(p[2] - q[2]) > DIFF;
function changedPixels(a, b, rect) {
  let n = 0;
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) if (far(kit.pixel(a, i, j), kit.pixel(b, i, j))) n++;
  return n;
}
// Changed pixels of a rectangle, and how many of its TILES_X x TILES_Y tiles hold more than TILE_PIXELS of them: a drawing in a
// few tiles is a drawing; a change spread over the artboard (a pan, a zoom, a fill, a stale view) is not.
function changeMap(a, b, rect) {
  let total = 0, tiles = 0, tileCount = 0;
  for (let ty = 0; ty < TILES_Y; ty++) for (let tx = 0; tx < TILES_X; tx++) {
    const x0 = rect.x + Math.floor(rect.w * tx / TILES_X), x1 = rect.x + Math.floor(rect.w * (tx + 1) / TILES_X);
    const y0 = rect.y + Math.floor(rect.h * ty / TILES_Y), y1 = rect.y + Math.floor(rect.h * (ty + 1) / TILES_Y);
    if (x1 <= x0 || y1 <= y0) continue;
    const n = changedPixels(a, b, { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }); total += n; tileCount++; if (n > TILE_PIXELS) tiles++;
  }
  return { total, tiles, tileCount };
}
function lumaRange(image, rect) {
  let lo = 255, hi = 0;
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) {
    const p = kit.pixel(image, i, j), l = kit.luma(p[0], p[1], p[2]); if (l < lo) lo = l; if (l > hi) hi = l;
  }
  return hi - lo;
}
const RANGE_MIN = 30;

// ---- facts read from the helper's accessibility nodes ----------------------------------------------------------
const nodeText = n => [...kit.labelsOfNode(n), ...(typeof n?.value === 'string' ? [n.value] : [])];
const byIdAll = (nodes, id) => nodes.filter(n => kit.idOfNode(n) === id);
// The UI frame number the playhead shows (1-based, as the page prints it), or null when it is not readable.
function playheadOf(nodes) {
  const m = byIdAll(nodes, PLAYHEAD);
  if (m.length !== 1) return null;
  const v = nodeText(m[0]).map(t => /^\s*(\d+)\s*$/.exec(t)).find(Boolean);
  return v ? Number(v[1]) : null;
}
// The active tool: the one tool button carrying the page's `active` class. {tool} | {unread: reason}; `other` when it is a different tool.
function activeTool(nodes) {
  const lit = nodes.filter(n => kit.hasClass(n, 'tool-btn') && kit.hasClass(n, 'active'));
  if (!nodes.some(n => kit.hasClass(n, 'tool-btn'))) return { unread: 'no tool button with DOM classes in the helper read' };
  if (lit.length !== 1) return { unread: lit.length ? 'more than one tool button is active' : 'no tool button is active' };
  const label = nodeText(lit[0]).find(t => Object.values(TOOLS).some(re => re.test(t)));
  const tool = label && Object.keys(TOOLS).find(k => TOOLS[k].test(label));
  return tool ? { tool } : { unread: 'the active tool button has no known label' };
}
function activeMode(nodes) {
  const lit = nodes.filter(n => kit.hasClass(n, 'app-mode-btn') && kit.hasClass(n, 'active'));
  if (!nodes.some(n => kit.hasClass(n, 'app-mode-btn'))) return { unread: 'no mode button with DOM classes in the helper read' };
  if (lit.length !== 1) return { unread: 'not exactly one mode button is active' };
  const label = nodeText(lit[0]).join(' | '), mode = Object.keys(MODES).find(k => MODES[k].test(label));
  return mode ? { mode } : { unread: 'the active mode button has no known label' };
}
function aboveSelected(nodes) {
  const buttons = nodes.filter(n => kit.hasClass(n, 'icon-btn') && nodeText(n).some(t => ABOVE.test(t)));
  if (buttons.length !== 1) return { unread: 'the Above placement button is not in the helper read' };
  return { on: kit.hasClass(buttons[0], 'active') };
}
async function look(session) {
  const r = await session.input('observe');
  return Array.isArray(r?.nodes) && r.nodes.every(plain) ? r.nodes : null;
}

// ---- what each gesture must do, judged on the layer rows -------------------------------------------------
const eq = (p, i, n, j) => p.k[i] === n.k[j] && p.c[i] === n.c[j];
const PRE = reason => ({ pre: reason }), FAIL = reason => ({ fail: reason });
const JUDGES = {
  // A tool inside the component, on a held cell that has no content yet: that cell gains content and its own key.
  draw(p, n, c) {
    if (p.c[c] !== false) return PRE('selected cell already has content');
    if (n.k.length !== p.k.length) return FAIL('total-frames-changed');
    if (n.c[c] !== true) return FAIL('stroke-did-not-land-in-the-cell');
    if (n.k[c] !== 'key') return FAIL('held-frame-did-not-get-its-own-key');
    for (let i = 0; i < c; i++) if (!eq(p, i, n, i)) return FAIL('earlier-frames-changed');
    for (let i = 0; i < p.k.length; i++) if (i !== c && p.k[i] !== n.k[i]) return FAIL('other-frame-kinds-changed');
    return null;
  },
  // Motion mode on a plain scene layer: the stroke is on the playhead frame (as its own key or inherited from the origin key).
  drawAtPlayhead(p, n, c, first) {
    if (first && p.c[c] !== false) return PRE('selected cell already has content');
    if (n.k.length !== p.k.length) return FAIL('total-frames-changed');
    if (n.c[c] !== true) return FAIL('stroke-did-not-land-at-the-playhead-frame');
    return null;
  },
};
// The public component read of the scene's layers: {layerUid: layerKind}. Pinned to the revision of the history read.
function kindsOf(p, ref, why = {}) {
  const q = plain(p) && plain(p.request) && plain(p.response) && p.request.operation === 'query.document.object.component.read' &&
    plain(p.request.payload) && p.response.ok === true && plain(p.response.result) ? p : null;
  const no = reason => { why.reason = reason; return null; };
  if (!q) return no('kinds is not an ok query.document.object.component.read pair');
  const payload = q.request.payload, sel = payload.selector, res = q.response.result;
  if (payload.atRevision !== ref.revision || payload.documentSnapshotId !== ref.snapshot || !plain(sel) || sel.kind !== 'layers' ||
    sel.contextId !== 'scene-root' || sel.cursor !== 0) return no('kinds is not the layers page of scene-root pinned to the history revision');
  if (q.response.documentId !== ref.documentId || q.response.instanceId !== ref.instanceId || res.kind !== 'layers' || res.contextId !== 'scene-root' ||
    res.atRevision !== ref.revision || res.documentSnapshotId !== ref.snapshot || res.nextCursor !== null || !Array.isArray(res.items) ||
    res.layerCount !== res.items.length) return no('kinds does not answer for this document and revision, or is not one complete page');
  const kinds = {};
  for (const item of res.items) {
    if (!plain(item) || !text(item.layerUid) || !text(item.layerKind) || Object.hasOwn(kinds, item.layerUid)) return no('a kinds item is malformed');
    kinds[item.layerUid] = item.layerKind;
  }
  return same(Object.keys(kinds).sort(), layersOf(ref)) ? kinds : no('kinds lists other layers than the timing reads');
}
const sameDocument = (a, b) => a.documentId === b.documentId && a.instanceId === b.instanceId;
const layersOf = s => Object.keys(s.rows).sort();
const sameContent = (a, b) => a.total === b.total && same(a.rows, b.rows);
const othersSame = (p, n, uid) => p.total === n.total && layersOf(p).every(id => id === uid || same(p.rows[id], n.rows[id]));
const unedited = (p, n) => sameDocument(p, n) && n.revision === p.revision && n.undo === p.undo && n.redo === p.redo && sameContent(p, n);
function oneEdit(p, n) {
  if (!sameDocument(p, n) || !same(layersOf(p), layersOf(n))) return 'document-or-layers-changed';
  if (n.revision !== p.revision + 1) return 'not-exactly-one-revision';
  if (n.undo !== p.undo + 1 || n.redo !== 0) return 'not-exactly-one-history-entry';
  return null;
}

// ---- the operator script ---------------------------------------------------------------------------------
const READS = 'Evidence reply (a JSON object): {observation: one plain sentence of what you saw, productFailure: true only if the app visibly ' +
  'misbehaved (crash, tool refused with a notice, nothing drawn), selection: {layerUid, frame}}. Do NOT attach any MCP reads or document ' +
  'state: the runner reads the document itself right after your reply, so after replying touch nothing in the app. Frames in `selection` are ' +
  '0-based (the UI shows frame N as N+1). Do not add pass/fail fields: the oracle refuses a reply that carries a verdict or any read.';
const SCENE_READS = READS + ' The runner reads context "scene-root".';
const COMP_READS = READS + ' The runner reads the entered component\'s context (the context id you report) and "scene-root".';
const KINDS = ' The runner also reads the layer kinds of "scene-root" itself.';
const ui = frame => 'frame ' + (frame + 1);
const compGesture = (id, frame, tool, how, extra = {}) => ({ id, check: 'component-draw-tools', ctx: 'component', type: 'gesture', judge: 'draw', frame, tool, ...extra,
  how: 'Inside the component click the cell of its layer at ' + ui(frame) + ' so the playhead is on ' + ui(frame) + ' (that cell has no drawing yet). ' +
    how + ' Draw ONE gesture only, in the middle half of the artboard, then stop and do not click anything else. ' + COMP_READS });
const motionGesture = (id, frame, first, tool, how) => ({ id, check: 'component-motion-draw', ctx: 'scene', type: 'gesture', judge: 'drawAtPlayhead', frame, first, tool, mode: 'motion',
  how: 'Select the layer named "Plain" and put the playhead on ' + ui(frame) + '. ' + how + ' Draw ONE gesture only, then stop. ' + SCENE_READS });
const PLAN = [
  { id: 'window-placement', type: 'placement', check: null,
    how: kit.PLACEMENT + ' Reply with {observation: "placed"}.' },
  { id: 'scene-setup', type: 'setup', check: null,
    how: 'Make a clean test document through the ordinary UI: File > New with the defaults (Untitled, 1920x1080, 24 fps), close the tutorial, and ' +
      'make sure Total frames is at least 12. On "Layer 1" draw one stroke on frame 1. Add a second layer, draw one stroke on its frame 1, click its ' +
      'cell at frame 2 and press F7 (blank keyframe), then right-click its row (the Layer menu) and choose "Convert to component". Add a third layer, ' +
      'rename it "Plain" and leave it empty. Add a fourth layer, rename it "Bake", draw one stroke on its frame 1, click its cell at frame 2, press F7 ' +
      '(blank keyframe) and convert it to a component the same way. Stay in Animation 2D and in the Scene tab. Reply with {observation, productFailure, ' +
      'layers: {anchor: <layerUid of Layer 1>, component: <layerUid of the first component layer>, plain: <layerUid of Plain>, bake: <layerUid of Bake>}}. ' +
      SCENE_READS + KINDS },
  { id: 'motion-enter', type: 'mode', mode: 'motion', check: null, failCheck: 'component-motion-draw', ctx: 'scene',
    how: 'Switch to Motion mode with the mode switch at the top. Do not edit anything. ' + SCENE_READS },
  motionGesture('motion-draw', 10, true, 'draw', 'Pick the Draw tool (B) and draw a stroke.'),
  motionGesture('motion-pen', 11, false, 'pen', 'Pick the Pen tool (P), click three points and finish the path (Enter).'),
  motionGesture('motion-shape', 9, false, 'rect', 'Pick the Rectangle tool (R) and drag one rectangle.'),
  { id: 'motion-exit', type: 'mode', mode: 'anim2d', check: null, failCheck: 'component-motion-draw', ctx: 'scene',
    how: 'Switch back to 2D Animation. Stay in the Scene tab, put the playhead on frame 9, pick the Select tool and click on the empty pasteboard ' +
      'outside the artboard so nothing is selected, and move the mouse pointer off the artboard. Do not edit. ' + SCENE_READS },
  // No operator phase: the runner captures the owned window once and takes the region from the canvas's accessibility frame.
  { id: 'instance-before', type: 'shot', check: null, auto: true },
  { id: 'bitmap-baseline', type: 'export-baseline', check: 'component-bitmap-export', ctx: 'scene',
    how: 'In the Scene tab put the playhead on frame 3 (the frame number box shows 3), then export the current frame as a PNG (File > Export) into the private ' +
      'report directory as ' + BASELINE + '. Do not draw, select or move anything. Reply with {observation, productFailure, exportedFile: "' + BASELINE + '"}.' },
  { id: 'component-enter', type: 'enter', check: null, failCheck: 'component-draw-tools', ctx: 'component',
    how: 'Double-click the component layer row to enter the component (the tab bar shows the component beside Scene). Do not edit. Reply also with ' +
      'component: {context: <the entered component\'s context id>, layerUid: <the component\'s layer>}. ' + COMP_READS },
  compGesture('comp-draw', 8, 'draw', 'Pick the Draw tool (B).'),
  compGesture('comp-pen', 7, 'pen', 'Pick the Pen tool (P), click three points and finish the path (Enter).'),
  compGesture('comp-line', 6, 'line', 'Pick the Line tool (U) and drag one line.'),
  compGesture('comp-rect', 5, 'rect', 'Pick the Rectangle tool (R) and drag one rectangle.'),
  compGesture('comp-ellipse', 4, 'ellipse', 'Pick the Ellipse tool (L) and drag one ellipse.'),
  compGesture('comp-fillbrush', 3, 'fillbrush', 'Pick the Fill Brush tool, set Above in its options and paint one stroke.', { above: true }),
  compGesture('comp-bitmap', 2, 'draw', 'Pick the Draw tool (B), turn Bitmap Brush on in the Brush panel (choose any tip) and paint one stroke.', { bitmap: true }),
  { id: 'comp-undo', type: 'undo', check: 'component-draw-undo-redo', ctx: 'component',
    how: 'Press Cmd+Z exactly once inside the component with the app focused (no menu). ' + COMP_READS },
  { id: 'comp-redo', type: 'redo', check: 'component-draw-undo-redo', ctx: 'component',
    how: 'Press Cmd+Shift+Z exactly once. ' + COMP_READS },
  { id: 'component-exit', type: 'exit', check: 'component-instances-pixels', ctx: 'scene',
    how: 'Click the Scene tab. Put the playhead on frame 9 again, pick the Select tool and click on the empty pasteboard outside the artboard so nothing ' +
      'is selected, and move the mouse pointer off the artboard. Do not draw or move anything else. ' + SCENE_READS },
  { id: 'bitmap-export', type: 'export', check: 'component-bitmap-export', ctx: 'scene',
    how: 'In the Scene tab put the playhead on frame 3 again (the frame number box shows 3), then export the current frame as a PNG (File > Export) into the ' +
      'private report directory as ' + EXPORT + '. Reply with {observation, productFailure, exportedFile: "' + EXPORT + '"}.' },
  { id: 'bake-break', type: 'bake', check: 'component-break-apart', ctx: 'scene',
    how: 'In the Scene tab right-click the row of the layer named "Bake" (a component instance) and choose "Break apart the component" in the Layer menu. ' +
      SCENE_READS + KINDS },
  { id: 'bake-undo', type: 'bake-undo', check: 'component-break-apart', ctx: 'scene',
    how: 'Press Cmd+Z exactly once with the app focused (no menu). ' + SCENE_READS + KINDS },
  { id: 'bake-redo', type: 'bake-redo', check: 'component-break-apart', ctx: 'scene',
    how: 'Press Cmd+Shift+Z exactly once. ' + SCENE_READS + KINDS },
  { id: 'component-reenter', type: 'enter', check: null, failCheck: 'component-draw-save-reopen', ctx: 'component',
    how: 'Double-click the component layer row again to enter the component. Do not edit. Reply with the same component object. ' + COMP_READS },
  { id: 'draw-save', type: 'save', check: 'component-draw-save-reopen', ctx: 'component', file: SAVED,
    how: 'Save the document (File > Save As) into the private report directory as ' + SAVED + '. Reply additionally with savedFile: "' + SAVED +
      '" (path relative to the report directory). Stay inside the component for the reads. ' + COMP_READS },
  { id: 'draw-reopen', type: 'reopen', check: 'component-draw-save-reopen', ctx: 'component', file: SAVED,
    how: 'Open that same saved file through File > Open (do not edit anything), then double-click the component layer to enter it again. Reply with ' +
      'the same savedFile. ' + COMP_READS },
];

// ---- the scenario ------------------------------------------------------------------------------------------
function createWorkflow({ reportDirectory }) {
  let used = false;
  return { async run({ session, row }) {
    if (used) throw new Error('component-edit-draw cannot restart');
    used = true;
    if (row.issue !== 2256 || !same(row.checks.map(c => c.id), CLAUSES)) throw new Error('original component-draw clauses changed');
    const outcome = Object.fromEntries(CLAUSES.map(id => [id, 'unrun']));
    const remaining = Object.fromEntries(CLAUSES.map(id => [id, PLAN.filter(s => s.check === id).length]));
    // A clause that could not be fully read stays unrun with its reason; it is never passed on what was read of the rest.
    const held = new Set(), reasons = {};
    const hold = (check, reason) => { held.add(check); reasons[check] = (reasons[check] ? reasons[check] + '; ' : '') + reason; };
    const dir = path.join(reportDirectory, ROW);
    const finish = (stop, reason) => {
      kit.privateEvidence(dir, 'outcome', { schema: 'nemo.component-edit-draw-private/1', outcome, reasons, reason: reason || null });
      return { checks: CLAUSES.map(id => ({ id, status: outcome[id] })), safeToContinue: !stop };
    };
    const owner = await session.runtimeContext();
    let scene = null, comp = null, plainUid = null, bakeUid = null, kinds = null, compLayer = null, componentContext = null, saved = null, before = null, baseline = null;
    const lastEdit = {};
    try {
    for (const step of PLAN) {
      if (!same(await session.identity(), owner.identity)) throw malformed(ROW, step.id);
      const details = { sourceSha: owner.identity.sourceSha, pid: owner.identity.pid, windowId: owner.window.windowId,
        reportDirectory, evidenceRule: 'Raw observations only. Source tests and posted input do not establish PASS.' };
      if (step.auto) {
        // No operator phase: one capture, taken and read by the harness, the canvas region from its own accessibility frame.
        const shot = await kit.shot(session, reportDirectory, BEFORE), canvas = shot && shot.stable && shot.window ? byIdAll(shot.nodes, CANVAS) : [];
        const frame = canvas.length === 1 ? kit.frameOf(canvas[0]) : null, rect = frame && kit.toPixels(shot.image, shot.window, frame);
        kit.privateEvidence(dir, step.id, { schema: 'nemo.component-edit-draw-private/1', sha256: shot?.sha256 || null, stable: !!shot?.stable, window: shot?.window || null, canvas: frame, playhead: shot ? playheadOf(shot.nodes) : null });
        if (!rect || !kit.rectOk(shot.image, rect, 100, 60)) hold('component-instances-pixels', 'the drawing canvas has no usable accessibility frame in a stable capture');
        else if (lumaRange(shot.image, rect) < RANGE_MIN) hold('component-instances-pixels', 'the capture does not show the artboard (uniform canvas); pixels cannot be judged');
        else before = { shot, frame, rect, playhead: playheadOf(shot.nodes) };
        continue;
      }
      // Files are measured by the runner around the step, never taken from what the operator says about them.
      const requestedAt = Date.now(), file = step.file || (step.type === 'export' ? EXPORT : step.type === 'export-baseline' ? BASELINE : null);
      if (file && kit.measure(reportDirectory, file) && step.type !== 'reopen') throw malformed(ROW, step.id);
      const ev = await session.requestEvidence({ row: ROW, phase: step.id, instruction: step.how, details });
      if (!rawOnly(ev) || !text(ev.observation) || typeof ev.productFailure !== 'boolean') throw malformed(ROW, step.id);
      kit.privateEvidence(dir, step.id, ev);
      if (step.type === 'placement') continue;
      if (ev.productFailure) {
        // The person may only report a failure, never a success; a reported failure is taken as stated.
        if (step.check) outcome[step.check] = 'fail';
        return finish(true, step.id + (step.check ? ' failure reported' : ' precondition failed'));
      }
      if (step.type === 'export-baseline' || step.type === 'export') {
        // The PNG is measured by the runner (hash, size, mtime), pinned to the playhead frame the page shows, and judged against
        // the baseline export taken before the bitmap gesture.
        const nodes = await look(session), playhead = nodes && playheadOf(nodes);
        if (ev.exportedFile !== file) throw malformed(ROW, step.id);
        const m = kit.measure(reportDirectory, file), image = m && kit.loadPng(m.bytes);
        if (!m || !image || m.mtimeMs < requestedAt - 2000) throw malformed(ROW, step.id);
        kit.privateEvidence(dir, 'file-' + step.id, { schema: 'nemo.component-edit-draw-private/1', sha256: m.sha256, size: m.size, mtimeMs: m.mtimeMs, requestedAt, playhead });
        if (playhead === null) hold(step.check, 'the playhead frame was not readable when ' + file + ' was exported');
        else if (playhead !== 3) throw malformed(ROW, step.id);
        if (step.type === 'export-baseline') { baseline = { image, m }; remaining[step.check]--; continue; }
        if (!baseline || baseline.image.width !== image.width || baseline.image.height !== image.height || m.mtimeMs < baseline.m.mtimeMs) throw malformed(ROW, step.id);
        // The bitmap-brush record of the component at that frame, read by the harness: the stroke is a bitmap brush stroke with ink.
        const bitmap = await kit.readBitmapScene(session, { contextId: componentContext, frame: 2, record: { dir, name: 'export-bitmap' } });
        const mine = bitmap.entries.filter(e => plain(e) && e.layerUid === compLayer);
        if (bitmap.refused) hold(step.check, 'the bitmap brush read was refused (' + bitmap.refused + ')');
        else if (!mine.some(e => plain(e.ink))) { outcome[step.check] = 'fail'; return finish(true, 'no bitmap brush ink is recorded for the component layer at frame 3'); }
        const changed = changeMap(baseline.image, image, { x: 0, y: 0, w: image.width, h: image.height });
        kit.privateEvidence(dir, 'export-pixels', { schema: 'nemo.component-edit-draw-private/1', ...changed, inkMin: EXPORT_INK_MIN });
        if (changed.total < EXPORT_INK_MIN) { outcome[step.check] = 'fail'; return finish(true, 'the exported frame differs from the baseline export by only ' + changed.total + ' pixels, so it shows no bitmap brush ink'); }
        if (changed.tiles > TILES_MAX) { outcome[step.check] = 'fail'; return finish(true, 'the exported frame changed over ' + changed.tiles + ' tiles, which is not one brush stroke'); }
        if (--remaining[step.check] === 0 && !held.has(step.check)) outcome[step.check] = 'pass';
        continue;
      }
      const inComponent = step.ctx === 'component';
      if (inComponent && step.type !== 'enter' && !componentContext) throw malformed(ROW, step.id);
      // The harness reads the document (never the operator). Reads the oracle cannot interpret are never guessed at:
      // the reason is kept privately, then the batch stops.
      const readContext = inComponent ? componentContext || ev.component?.context : 'scene-root';
      if (inComponent && !(text(readContext) && readContext !== 'scene-root')) throw malformed(ROW, step.id);
      const seen = await kit.readTimeline(session, { contexts: inComponent ? [readContext, 'scene-root'] : ['scene-root'], record: { dir, name: step.id } });
      const reads = { history: seen.history, layers: seen.layers[readContext] }, sceneLayers = seen.layers['scene-root'], scenePage = seen.pages['scene-root'];
      const why = {};
      const next = summarize(reads, readContext, why);
      const nextScene = inComponent ? summarize({ history: reads.history, layers: sceneLayers }, 'scene-root', why) : next;
      if (!next || !nextScene) {
        kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.component-edit-draw-private/1', phase: step.id, reason: why.reason });
        throw malformed(ROW, step.id);
      }
      if (step.type === 'setup') {
        const l = ev.layers;
        if (!plain(l) || !text(l.anchor) || !text(l.component) || !text(l.plain) || new Set([l.anchor, l.component, l.plain]).size !== 3 ||
          ![l.anchor, l.component, l.plain].every(uid => Object.hasOwn(next.rows, uid)) || next.total < 12 ||
          next.rows[l.anchor].c[0] !== true || next.rows[l.plain].c.some(Boolean)) throw malformed(ROW, step.id);
        const k = kindsOf(scenePage, next, why);
        if (!text(l.bake) || !Object.hasOwn(next.rows, l.bake) || ![l.anchor, l.component, l.plain, l.bake].every(uid => text(uid)) || new Set([l.anchor, l.component, l.plain, l.bake]).size !== 4 ||
          !k || k[l.bake] !== 'instance' || next.rows[l.bake].c[0] !== true) {
          kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.component-edit-draw-private/1', phase: step.id, reason: why.reason || 'bake layer is not a component instance' });
          throw malformed(ROW, step.id);
        }
        plainUid = l.plain; bakeUid = l.bake; kinds = k; scene = next; continue;
      }
      if (step.type === 'mode') {
        // A view change: no edit, no history entry, the scene untouched; and the page shows the mode that was asked for.
        if (!unedited(scene, next)) { outcome[step.failCheck] = 'fail'; return finish(true, step.id + ': switching mode changed the document'); }
        const nodes = await look(session), mode = nodes ? activeMode(nodes) : { unread: 'no accessibility read' };
        if (mode.unread) hold(step.failCheck, step.id + ': ' + mode.unread);
        else if (mode.mode !== step.mode) throw malformed(ROW, step.id);
        continue;
      }
      if (step.type === 'enter') {
        const c = ev.component;
        if (!plain(c) || !text(c.context) || c.context === 'scene-root' || !text(c.layerUid) || !Object.hasOwn(next.rows, c.layerUid)) throw malformed(ROW, step.id);
        if (componentContext === null) {
          const r = next.rows[c.layerUid];
          // The component keeps the stroke on frame 1 and the blank key the setup made on frame 2: frames 2.. have no content.
          if (next.total < 12 || r.k[0] !== 'key' || r.c[0] !== true || r.k[1] !== 'key' || r.c.slice(1).some(Boolean) || !sameDocument(scene, next)) throw malformed(ROW, step.id);
        } else if (c.context !== componentContext || c.layerUid !== compLayer) throw malformed(ROW, step.id);
        // Entering is a view change: no edit, no history entry, the Scene and the component untouched.
        // `scene` is the latest document-level state (revision and history), so edits made in the Scene since the last visit are expected.
        if (!sameDocument(scene, next) || next.revision !== scene.revision || next.undo !== scene.undo || next.redo !== scene.redo ||
          !sameContent(scene, nextScene) || (comp && !sameContent(comp, next))) {
          outcome[step.failCheck] = 'fail'; return finish(true, step.id + ': entering the component changed the document');
        }
        componentContext = c.context; compLayer = c.layerUid; comp = next; scene = nextScene;
        continue;
      }
      if (step.type === 'exit') {
        // The Scene tab again: still no edit; then the runner captures the same view and judges the instances' pixels.
        if (!sameDocument(comp, next) || next.revision !== comp.revision || next.undo !== comp.undo || next.redo !== comp.redo || !sameContent(scene, next)) {
          throw malformed(ROW, step.id);
        }
        const after = await kit.shot(session, reportDirectory, AFTER);
        if (!after) throw malformed(ROW, step.id);
        const afterCanvas = after.stable && after.window ? byIdAll(after.nodes, CANVAS) : [], afterFrame = afterCanvas.length === 1 ? kit.frameOf(afterCanvas[0]) : null;
        const afterPlayhead = playheadOf(after.nodes);
        kit.privateEvidence(dir, 'shot-' + AFTER, { schema: 'nemo.component-edit-draw-private/1', sha256: after.sha256, stable: after.stable, canvas: afterFrame, playhead: afterPlayhead });
        const frameMoved = f => !f || !before || ['x', 'y', 'w', 'h'].some(k => Math.abs(f[k] - before.frame[k]) > 1);
        if (!before) { /* held at the baseline capture */ }
        else if (!afterFrame || frameMoved(afterFrame) || Math.abs(after.window.w - before.shot.window.w) >= 1 || Math.abs(after.window.h - before.shot.window.h) >= 1 ||
          after.image.width !== before.shot.image.width || after.image.height !== before.shot.image.height) hold(step.check, 'the canvas or the window moved between the two captures, or the second capture was not stable');
        else if (before.playhead === null || afterPlayhead === null) hold(step.check, 'the playhead frame was not readable in both captures');
        else if (before.playhead !== 9 || afterPlayhead !== 9) throw malformed(ROW, step.id);
        else {
          const map = changeMap(before.shot.image, after.image, before.rect);
          kit.privateEvidence(dir, 'pixels', { schema: 'nemo.component-edit-draw-private/1', ...map, inkMin: INK_MIN, tilesMax: TILES_MAX });
          if (map.total < INK_MIN) { outcome[step.check] = 'fail'; return finish(true, 'the scene instance does not show the component drawing (' + map.total + ' pixels changed)'); }
          if (map.tiles > TILES_MAX) { outcome[step.check] = 'fail'; return finish(true, 'the scene changed over ' + map.tiles + ' of ' + map.tileCount + ' canvas tiles, which is not one drawing'); }
          if (!held.has(step.check)) outcome[step.check] = 'pass';
        }
        continue;
      }
      if (step.type === 'bake' || step.type === 'bake-undo' || step.type === 'bake-redo') {
        // Break apart bakes the instance's frames onto the layer: one edit, the layer is a drawing layer, its keys are the component's keys.
        const k = kindsOf(scenePage, next, why);
        if (!k) {
          kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.component-edit-draw-private/1', phase: step.id, reason: why.reason });
          throw malformed(ROW, step.id);
        }
        const otherKinds = (a, b) => layersOf(scene).every(id => id === bakeUid || a[id] === b[id]);
        let bad = null;
        if (step.type === 'bake') {
          const n = next.rows[bakeUid];
          if (!n || n.k.length === 0) throw malformed(ROW, step.id);
          bad = k[bakeUid] !== 'drawing' ? 'layer-is-still-a-component-instance'
            : n.k.length !== scene.rows[bakeUid].k.length ? 'total-frames-changed'
            : !(n.k[0] === 'key' && n.c[0] === true && n.k[1] === 'key' && n.c[1] === false && n.c.slice(1).every(v => v === false)) ? 'baked-frames-differ-from-the-component'
            : oneEdit(scene, next) || (othersSame(scene, next, bakeUid) ? null : 'other-layers-changed') || (otherKinds(kinds, k) ? null : 'other-layer-kinds-changed');
          if (!bad) lastEdit.bake = { before: scene, beforeKinds: kinds, after: next, afterKinds: k };
        } else {
          const edit = lastEdit.bake, undoing = step.type === 'bake-undo';
          if (!edit || (undoing && scene !== edit.after)) throw malformed(ROW, step.id);
          const want = undoing ? edit.before : edit.after, wantKinds = undoing ? edit.beforeKinds : edit.afterKinds;
          bad = !sameDocument(scene, next) || !sameContent(want, next) || !same(wantKinds, k) ? (undoing ? 'undo-did-not-restore-the-component-instance' : 'redo-did-not-restore-the-break-apart')
            : (undoing ? next.undo !== scene.undo - 1 || next.redo !== scene.redo + 1 : next.undo !== scene.undo + 1 || next.redo !== scene.redo - 1) ? 'not-exactly-one-history-step' : null;
        }
        if (bad) { outcome[step.check] = 'fail'; return finish(true, step.id + ': ' + bad); }
        scene = next; kinds = k;
        if (--remaining[step.check] === 0 && !held.has(step.check)) outcome[step.check] = 'pass';
        continue;
      }
      const prev = inComponent ? comp : scene;
      const adopt = () => { if (inComponent) { comp = next; scene = nextScene; } else scene = next; };
      let failure = null;
      if (step.type === 'gesture') {
        const uid = inComponent ? compLayer : plainUid, sel = ev.selection;
        if (!plain(sel) || sel.layerUid !== uid || sel.frame !== step.frame || !int(sel.frame) || sel.frame >= prev.total || !Object.hasOwn(next.rows, uid)) throw malformed(ROW, step.id);
        // What the page shows after the gesture: the tool that drew, the Fill Brush placement, the mode and the playhead frame.
        const nodes = await look(session), tool = nodes ? activeTool(nodes) : { unread: 'no accessibility read' };
        if (tool.unread) hold(step.check, step.id + ': ' + tool.unread);
        else if (tool.tool !== step.tool) throw malformed(ROW, step.id);
        if (step.above) {
          const above = nodes ? aboveSelected(nodes) : { unread: 'no accessibility read' };
          if (above.unread) hold(step.check, step.id + ': ' + above.unread);
          else if (!above.on) throw malformed(ROW, step.id);
        }
        if (step.mode) {
          const mode = nodes ? activeMode(nodes) : { unread: 'no accessibility read' };
          if (mode.unread) hold(step.check, step.id + ': ' + mode.unread);
          else if (mode.mode !== step.mode) throw malformed(ROW, step.id);
        }
        const playhead = nodes ? playheadOf(nodes) : null;
        if (playhead === null) hold(step.check, step.id + ': the playhead frame was not readable');
        else if (playhead !== step.frame + 1) throw malformed(ROW, step.id);
        const verdict = JUDGES[step.judge](prev.rows[uid], next.rows[uid], step.frame, step.first);
        if (verdict?.pre) throw malformed(ROW, step.id);
        failure = verdict?.fail || oneEdit(prev, next) || (othersSame(prev, next, uid) ? null : 'other-layers-changed');
        if (!failure && inComponent && !sameContent(scene, nextScene)) failure = 'scene-changed-by-component-drawing';
        // A stroke added to a cell that already showed content (a held cell of an earlier key) changes no cell read. An edit
        // that leaves the layer's rows exactly as they were cannot be shown to be this gesture's stroke: held, never passed.
        if (!failure && step.first === false && same(prev.rows[uid], next.rows[uid])) hold(step.check, step.id + ': the layer rows did not change, so the stroke cannot be told from any other edit');
        if (!failure && step.bitmap) {
          const bitmap = await kit.readBitmapScene(session, { contextId: readContext, frame: step.frame, record: { dir, name: step.id + '-bitmap' } });
          if (bitmap.refused) hold(step.check, step.id + ': the bitmap brush read was refused (' + bitmap.refused + ')');
          else if (!bitmap.entries.some(e => plain(e) && e.layerUid === uid)) failure = 'no-bitmap-brush-record-for-the-stroke';
        }
        if (!failure) { lastEdit[step.ctx] = { before: prev, after: next }; adopt(); }
      } else if (step.type === 'undo' || step.type === 'redo') {
        const edit = lastEdit[step.ctx];
        if (!edit || (step.type === 'undo' && prev !== edit.after)) throw malformed(ROW, step.id);
        const undoing = step.type === 'undo', want = undoing ? edit.before : edit.after;
        if (!sameDocument(prev, next) || !sameContent(want, next)) failure = undoing ? 'undo-did-not-restore-the-previous-timeline' : 'redo-did-not-restore-the-drawing';
        else if (undoing ? next.undo !== prev.undo - 1 || next.redo !== prev.redo + 1 : next.undo !== prev.undo + 1 || next.redo !== prev.redo - 1) failure = 'not-exactly-one-history-step';
        else if (inComponent && !sameContent(scene, nextScene)) failure = 'scene-changed-by-component-undo-redo';
        if (!failure) { adopt(); if (undoing) lastEdit[step.ctx] = { before: edit.before, after: edit.after, undone: next }; }
      } else if (step.type === 'save' || step.type === 'reopen') {
        if (ev.savedFile !== step.file) throw malformed(ROW, step.id);
        const f = kit.measure(reportDirectory, step.file), json = kit.parseJson(f?.bytes);
        if (!f || !kit.documentOf(json)) throw malformed(ROW, step.id);
        if (step.type === 'save') {
          // Saving must not edit: the reads taken after Save are exactly the state just verified.
          if (!unedited(prev, next) || !sameContent(scene, nextScene)) throw malformed(ROW, step.id);
          if (f.mtimeMs < requestedAt - 2000) failure = 'saved-file-predates-the-save-step';
          else {
            // The file must agree with the verified layers and keys, the component's own layers included.
            const compared = kit.compareSaved(json, { total: nextScene.total, sceneRows: nextScene.rows, rows: { ...nextScene.rows, ...next.rows } });
            if (compared.fail) failure = compared.fail; else if (compared.unrun) hold(step.check, compared.unrun);
          }
          kit.privateEvidence(dir, 'file-' + step.id, { schema: 'nemo.component-edit-draw-private/1', sha256: f.sha256, size: f.size, mtimeMs: f.mtimeMs, requestedAt });
          saved = { hash: f.sha256, mtimeMs: f.mtimeMs, timeline: next, scene: nextScene };
        } else {
          if (!saved) throw malformed(ROW, step.id);
          if (f.sha256 !== saved.hash || f.mtimeMs !== saved.mtimeMs) failure = 'saved-file-changed-by-opening';
          else if (sameDocument(saved.timeline, next)) failure = 'reopen-did-not-create-a-new-document';
          else if (next.undo !== 0 || next.redo !== 0) failure = 'reopened-history-not-fresh';
          else if (!sameContent(saved.timeline, next) || !sameContent(saved.scene, nextScene)) failure = 'reopened-drawings-differ-from-saved';
          if (!failure) adopt();
        }
      }
      if (failure) { outcome[step.check] = 'fail'; return finish(true, step.id + ': ' + failure); }
      if (--remaining[step.check] === 0 && !held.has(step.check)) outcome[step.check] = 'pass';
    }
    return finish(false);
    } catch (error) {
      // Keep what was established before an unusable reply or an input refusal; the batch still stops.
      try { kit.privateEvidence(dir, 'partial', { schema: 'nemo.component-edit-draw-private/1', outcome, reasons, stoppedBy: error?.code || 'error' }); } catch { /* receipt directory unavailable */ }
      throw error;
    }
  } };
}
function prepare({ config }) { return createWorkflow({ reportDirectory: config.reportDirectory }); }
module.exports = { prepare, createWorkflow, JUDGES, CLAUSES, PLAN, BEFORE, AFTER, BASELINE, EXPORT, SAVED, activeTool, activeMode, aboveSelected, playheadOf, changeMap };
