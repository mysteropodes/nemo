'use strict';
// #2256 installed row: drawing inside an entered component (PRs #2765, #2784, #2786, #2800, #2803), foreground.
// The UI32 helper has no pointer drawing and no double-click, so a person draws. The person never reports a
// verdict: after each gesture they attach the unedited read-only Nemo MCP replies (history depth, per-layer
// cell runs) and this oracle computes the effect itself (revision step, history step, which cell gained
// content, whether the Scene stayed unchanged). Pixels come from the helper's own owned-window capture
// (instances) and from a PNG the person exports into the private report directory (bitmap brush); the person
// only reports rectangles, never whether something "looks right".
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
const EXPORT = 'exports/bitmap-brush.png', SAVED = 'saved/component-draw.json';
const DIFF = 24, INK_MIN = 200, NOISE_MAX = 20, RANGE_MIN = 30, EXPORT_INK = 0.01;

// ---- raw pixel measurements (rectangles come from the person, every number from the image) -------------
const inside = (outer, r) => r.x >= outer.x && r.y >= outer.y && r.x + r.w <= outer.x + outer.w && r.y + r.h <= outer.y + outer.h;
const disjoint = (a, b) => a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
const far = (p, q) => Math.abs(p[0] - q[0]) > DIFF || Math.abs(p[1] - q[1]) > DIFF || Math.abs(p[2] - q[2]) > DIFF;
function changedPixels(a, b, rect) {
  let n = 0;
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) if (far(kit.pixel(a, i, j), kit.pixel(b, i, j))) n++;
  return n;
}
function lumaRange(image, rect) {
  let lo = 255, hi = 0;
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) {
    const p = kit.pixel(image, i, j), l = kit.luma(p[0], p[1], p[2]); if (l < lo) lo = l; if (l > hi) hi = l;
  }
  return hi - lo;
}
function rectMean(image, rect) {
  const sum = [0, 0, 0];
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) { const p = kit.pixel(image, i, j); sum[0] += p[0]; sum[1] += p[1]; sum[2] += p[2]; }
  return sum.map(v => v / (rect.w * rect.h));
}
function farFraction(image, rect, colour) {
  let n = 0;
  for (let j = rect.y; j < rect.y + rect.h; j++) for (let i = rect.x; i < rect.x + rect.w; i++) if (far(kit.pixel(image, i, j), colour)) n++;
  return n / (rect.w * rect.h);
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
  'misbehaved (crash, tool refused with a notice, nothing drawn), selection: {layerUid, frame}, reads: {history: {request, response}, layers: ' +
  '[{request, response}, ...]}}. Each {request, response} is the UNEDITED request and response of a READ-ONLY public Nemo MCP call, never a ' +
  'command.* operation: history is query.document.object.history; layers holds one query.document.object.layer.timing call with payload.view ' +
  '"cells" and payload.contextId set to the context named below for EVERY layer of that context, all pinned (atRevision, documentSnapshotId) to ' +
  'the revision the history read reports. Make the reads right after the gesture and before any other action. Frames in `selection` are 0-based ' +
  '(the UI shows frame N as N+1). Do not add pass/fail fields: the oracle judges the raw reads and refuses a reply that carries a verdict.';
const SCENE_READS = READS + ' Context: "scene-root".';
const COMP_READS = READS + ' Context: the entered component\'s id (the same value in every call). Also attach sceneLayers: the same cells ' +
  'reads with payload.contextId "scene-root" for every scene layer, pinned to the same revision.';
const KINDS = ' Also attach kinds: the UNEDITED request and response of query.document.object.component.read with selector {kind: "layers", ' +
  'contextId: "scene-root", cursor: 0, limit: 16}, pinned to the same revision (it lists each scene layer with its layerKind).';
const ui = frame => 'frame ' + (frame + 1);
const compGesture = (id, frame, how) => ({ id, check: 'component-draw-tools', ctx: 'component', type: 'gesture', judge: 'draw', frame,
  how: 'Inside the component click the cell of its layer at ' + ui(frame) + ' so the playhead is on ' + ui(frame) + ' (that cell has no drawing yet). ' +
    how + ' Draw ONE gesture only, in the middle half of the artboard, then stop and do not click anything else. ' + COMP_READS });
const motionGesture = (id, frame, first, how) => ({ id, check: 'component-motion-draw', ctx: 'scene', type: 'gesture', judge: 'drawAtPlayhead', frame, first,
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
      'layers: {anchor: <layerUid of Layer 1>, component: <layerUid of the first component layer>, plain: <layerUid of Plain>, bake: <layerUid of Bake>}, ' +
      'reads, kinds}. ' + SCENE_READS + KINDS },
  { id: 'motion-enter', type: 'mode', check: null, failCheck: 'component-motion-draw', ctx: 'scene',
    how: 'Switch to Motion mode with the mode switch at the top. Do not edit anything. ' + SCENE_READS },
  motionGesture('motion-draw', 10, true, 'Pick the Draw tool (B) and draw a stroke.'),
  motionGesture('motion-pen', 11, false, 'Pick the Pen tool (P), click three points and finish the path (Enter).'),
  motionGesture('motion-shape', 9, false, 'Pick the Rectangle tool (R) and drag one rectangle.'),
  { id: 'motion-exit', type: 'mode', check: null, failCheck: 'component-motion-draw', ctx: 'scene',
    how: 'Switch back to 2D Animation. Stay in the Scene tab, put the playhead on frame 9, pick the Select tool and click on the empty pasteboard ' +
      'outside the artboard so nothing is selected, and move the mouse pointer off the artboard. Do not edit. ' + SCENE_READS },
  { id: 'instance-before', type: 'shot', check: null,
    how: 'The runner has just saved an owned-window capture of the Scene at frame 9. Open the PNG named in details.capturePath and measure, in pixels ' +
      'of that image, three rectangles: canvas (the whole artboard), drawZone (the middle half of the artboard) and outside (a strip of the artboard, ' +
      'at least 40x20 pixels, that you will never draw on, away from every stroke). Reply with {observation, productFailure, rects: {canvas, ' +
      'drawZone, outside}} (each {x,y,w,h}). Do not judge anything else.' },
  { id: 'component-enter', type: 'enter', check: null, failCheck: 'component-draw-tools', ctx: 'component',
    how: 'Double-click the component layer row to enter the component (the tab bar shows the component beside Scene). Do not edit. Reply also with ' +
      'component: {context: <component id used in the reads>, layerUid: <the component\'s layer>}. ' + COMP_READS },
  compGesture('comp-draw', 8, 'Pick the Draw tool (B).'),
  compGesture('comp-pen', 7, 'Pick the Pen tool (P), click three points and finish the path (Enter).'),
  compGesture('comp-line', 6, 'Pick the Line tool (L) and drag one line.'),
  compGesture('comp-rect', 5, 'Pick the Rectangle tool (R) and drag one rectangle.'),
  compGesture('comp-ellipse', 4, 'Pick the Ellipse tool (E) and drag one ellipse.'),
  compGesture('comp-fillbrush', 3, 'Pick the Fill Brush tool, set Above in its options and paint one stroke.'),
  compGesture('comp-bitmap', 2, 'Pick the Draw tool (B), turn Bitmap Brush on in the Brush panel (choose any tip) and paint one stroke.'),
  { id: 'comp-undo', type: 'undo', check: 'component-draw-undo-redo', ctx: 'component',
    how: 'Press Cmd+Z exactly once inside the component with the app focused (no menu). ' + COMP_READS },
  { id: 'comp-redo', type: 'redo', check: 'component-draw-undo-redo', ctx: 'component',
    how: 'Press Cmd+Shift+Z exactly once. ' + COMP_READS },
  { id: 'component-exit', type: 'exit', check: 'component-instances-pixels', ctx: 'scene',
    how: 'Click the Scene tab. Put the playhead on frame 9 again, pick the Select tool and click on the empty pasteboard outside the artboard so nothing ' +
      'is selected, and move the mouse pointer off the artboard. Do not draw or move anything else. ' + SCENE_READS },
  { id: 'bitmap-export', type: 'export', check: 'component-bitmap-export', ctx: 'scene',
    how: 'In the Scene tab put the playhead on frame 3, then export the current frame as a PNG (File > Export) into the private report directory as ' +
      EXPORT + '. Open the exported PNG and measure, in pixels of that image, stroke (a rectangle around the bitmap brush stroke you painted at frame 3, ' +
      'containing nothing else) and background (an empty part of the picture, at least 20x10 pixels). Reply with {observation, productFailure, ' +
      'exportedFile: "' + EXPORT + '", rects: {stroke, background}}. Report rectangles only.' },
  { id: 'bake-break', type: 'bake', check: 'component-break-apart', ctx: 'scene',
    how: 'In the Scene tab right-click the row of the layer named "Bake" (a component instance) and choose "Break apart the component" in the Layer menu. ' +
      SCENE_READS + KINDS },
  { id: 'bake-undo', type: 'bake-undo', check: 'component-break-apart', ctx: 'scene',
    how: 'Press Cmd+Z exactly once with the app focused (no menu). ' + SCENE_READS + KINDS },
  { id: 'bake-redo', type: 'bake-redo', check: 'component-break-apart', ctx: 'scene',
    how: 'Press Cmd+Shift+Z exactly once. ' + SCENE_READS + KINDS },
  { id: 'component-reenter', type: 'enter', check: null, failCheck: 'component-draw-save-reopen', ctx: 'component',
    how: 'Double-click the component layer row again to enter the component. Do not edit. Reply with the same component object. ' + COMP_READS },
  { id: 'draw-save', type: 'save', check: 'component-draw-save-reopen', ctx: 'component',
    how: 'Save the document (File > Save As) into the private report directory as ' + SAVED + '. Reply additionally with savedFile: "' + SAVED +
      '" (path relative to the report directory). Stay inside the component for the reads. ' + COMP_READS },
  { id: 'draw-reopen', type: 'reopen', check: 'component-draw-save-reopen', ctx: 'component',
    how: 'Open that same saved file through File > Open (do not edit anything), then double-click the component layer to enter it again. Reply with ' +
      'the same savedFile and the reads. ' + COMP_READS },
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
    const reasons = {};
    const dir = path.join(reportDirectory, ROW);
    const finish = (stop, reason) => {
      kit.privateEvidence(dir, 'outcome', { schema: 'nemo.component-edit-draw-private/1', outcome, reasons, reason: reason || null });
      return { checks: CLAUSES.map(id => ({ id, status: outcome[id] })), safeToContinue: !stop };
    };
    const owner = await session.runtimeContext();
    let scene = null, comp = null, plainUid = null, bakeUid = null, kinds = null, compLayer = null, componentContext = null, saved = null, rects = null;
    const lastEdit = {};
    try {
    for (const step of PLAN) {
      if (!same(await session.identity(), owner.identity)) throw malformed(ROW, step.id);
      const details = { sourceSha: owner.identity.sourceSha, pid: owner.identity.pid, windowId: owner.window.windowId,
        reportDirectory, evidenceRule: 'Raw observations only. Source tests and posted input do not establish PASS.' };
      if (step.type === 'shot') {
        const captured = await session.input('capture', BEFORE);
        if (!plain(captured) || captured.capture !== BEFORE) throw malformed(ROW, step.id);
        details.capturePath = path.join(reportDirectory, BEFORE);
      }
      const ev = await session.requestEvidence({ row: ROW, phase: step.id, instruction: step.how, details });
      if (!rawOnly(ev) || !text(ev.observation) || typeof ev.productFailure !== 'boolean') throw malformed(ROW, step.id);
      kit.privateEvidence(dir, step.id, ev);
      if (step.type === 'placement') continue;
      if (ev.productFailure) {
        // The person may only report a failure, never a success; a reported failure is taken as stated.
        if (step.check) outcome[step.check] = 'fail';
        return finish(true, step.id + (step.check ? ' failure reported' : ' precondition failed'));
      }
      if (step.type === 'shot') {
        const image = kit.loadPng(kit.readInside(reportDirectory, BEFORE)), r = ev.rects;
        if (!image || !plain(r) || !kit.rectOk(image, r.canvas, 100, 60) || !kit.rectOk(image, r.drawZone, 40, 30) || !kit.rectOk(image, r.outside, 40, 20) ||
          !inside(r.canvas, r.drawZone) || !inside(r.canvas, r.outside) || !disjoint(r.drawZone, r.outside)) throw malformed(ROW, step.id);
        rects = { canvas: r.canvas, drawZone: r.drawZone, outside: r.outside };
        continue;
      }
      if (step.type === 'export') {
        const image = ev.exportedFile === EXPORT ? kit.loadPng(kit.readInside(reportDirectory, EXPORT)) : null, r = ev.rects;
        if (!image || !plain(r) || !kit.rectOk(image, r.stroke, 20, 10) || !kit.rectOk(image, r.background, 20, 10) || !disjoint(r.stroke, r.background)) throw malformed(ROW, step.id);
        const colour = rectMean(image, r.background);
        if (farFraction(image, r.background, colour) > 0.01) throw malformed(ROW, step.id);
        const ink = farFraction(image, r.stroke, colour);
        if (ink < EXPORT_INK) { outcome[step.check] = 'fail'; return finish(true, 'the exported frame shows no bitmap brush ink (' + ink.toFixed(4) + ')'); }
        outcome[step.check] = 'pass'; continue;
      }
      const inComponent = step.ctx === 'component';
      if (inComponent && step.type !== 'enter' && !componentContext) throw malformed(ROW, step.id);
      // Reads the oracle cannot interpret are never guessed at: the reason is kept privately, then the batch stops as operator input.
      const why = {};
      const next = summarize(ev.reads, inComponent ? componentContext || ev.component?.context : 'scene-root', why);
      const nextScene = inComponent ? summarize({ history: ev.reads?.history, layers: ev.sceneLayers }, 'scene-root', why) : next;
      if (!next || !nextScene) {
        kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.component-edit-draw-private/1', phase: step.id, reason: why.reason });
        throw malformed(ROW, step.id);
      }
      if (step.type === 'setup') {
        const l = ev.layers;
        if (!plain(l) || !text(l.anchor) || !text(l.component) || !text(l.plain) || new Set([l.anchor, l.component, l.plain]).size !== 3 ||
          ![l.anchor, l.component, l.plain].every(uid => Object.hasOwn(next.rows, uid)) || next.total < 12 ||
          next.rows[l.anchor].c[0] !== true || next.rows[l.plain].c.some(Boolean)) throw malformed(ROW, step.id);
        const k = kindsOf(ev.kinds, next, why);
        if (!text(l.bake) || !Object.hasOwn(next.rows, l.bake) || ![l.anchor, l.component, l.plain, l.bake].every(uid => text(uid)) || new Set([l.anchor, l.component, l.plain, l.bake]).size !== 4 ||
          !k || k[l.bake] !== 'instance' || next.rows[l.bake].c[0] !== true) {
          kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.component-edit-draw-private/1', phase: step.id, reason: why.reason || 'bake layer is not a component instance' });
          throw malformed(ROW, step.id);
        }
        plainUid = l.plain; bakeUid = l.bake; kinds = k; scene = next; continue;
      }
      if (step.type === 'mode') {
        // A view change: no edit, no history entry, the scene untouched.
        if (!unedited(scene, next)) { outcome[step.failCheck] = 'fail'; return finish(true, step.id + ': switching mode changed the document'); }
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
        const captured = await session.input('capture', AFTER);
        if (!plain(captured) || captured.capture !== AFTER) throw malformed(ROW, step.id);
        const before = kit.loadPng(kit.readInside(reportDirectory, BEFORE)), after = kit.loadPng(kit.readInside(reportDirectory, AFTER));
        if (!before || !after || !rects || before.width !== after.width || before.height !== after.height) throw malformed(ROW, step.id);
        if (lumaRange(before, rects.canvas) < RANGE_MIN) {
          reasons[step.check] = 'the capture does not show the artboard (uniform canvas); pixels cannot be judged';
          continue;
        }
        const ink = changedPixels(before, after, rects.drawZone), noise = changedPixels(before, after, rects.outside);
        kit.privateEvidence(dir, 'pixels', { schema: 'nemo.component-edit-draw-private/1', ink, noise, inkMin: INK_MIN, noiseMax: NOISE_MAX });
        if (ink < INK_MIN) { outcome[step.check] = 'fail'; return finish(true, 'the scene instance does not show the component drawing (' + ink + ' pixels changed)'); }
        if (noise > NOISE_MAX) { outcome[step.check] = 'fail'; return finish(true, 'the scene changed outside the drawing zone (' + noise + ' pixels)'); }
        outcome[step.check] = 'pass'; continue;
      }
      if (step.type === 'bake' || step.type === 'bake-undo' || step.type === 'bake-redo') {
        // Break apart bakes the instance's frames onto the layer: one edit, the layer is a drawing layer, its keys are the component's keys.
        const k = kindsOf(ev.kinds, next, why);
        if (!k) {
          kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.component-edit-draw-private/1', phase: step.id, reason: why.reason });
          throw malformed(ROW, step.id);
        }
        const otherKinds = (a, b) => layersOf(scene).every(id => id === bakeUid || a[id] === b[id]);
        let bad = null;
        if (step.type === 'bake') {
          const n = next.rows[bakeUid];
          if (n.k.length === 0) throw malformed(ROW, step.id);
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
        if (--remaining[step.check] === 0) outcome[step.check] = 'pass';
        continue;
      }
      const prev = inComponent ? comp : scene;
      const adopt = () => { if (inComponent) { comp = next; scene = nextScene; } else scene = next; };
      let failure = null;
      if (step.type === 'gesture') {
        const uid = inComponent ? compLayer : plainUid, sel = ev.selection;
        if (!plain(sel) || sel.layerUid !== uid || sel.frame !== step.frame || !int(sel.frame) || sel.frame >= prev.total || !Object.hasOwn(next.rows, uid)) throw malformed(ROW, step.id);
        const verdict = JUDGES[step.judge](prev.rows[uid], next.rows[uid], step.frame, step.first);
        if (verdict?.pre) throw malformed(ROW, step.id);
        failure = verdict?.fail || oneEdit(prev, next) || (othersSame(prev, next, uid) ? null : 'other-layers-changed');
        if (!failure && inComponent && !sameContent(scene, nextScene)) failure = 'scene-changed-by-component-drawing';
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
        const bytes = ev.savedFile === SAVED ? kit.readInside(reportDirectory, SAVED) : null;
        let json = null;
        try { json = bytes ? JSON.parse(bytes.toString('utf8')) : null; } catch { json = null; }
        const doc = plain(json) ? (int(json.totalFrames) ? json : json.document) : null;
        if (!bytes || !plain(doc) || !int(doc.totalFrames) || !Array.isArray(doc.layers)) throw malformed(ROW, step.id);
        const hash = kit.sha256(bytes);
        if (step.type === 'save') {
          // Saving must not edit: the reads taken after Save are exactly the state just verified.
          if (!unedited(prev, next) || !sameContent(scene, nextScene)) throw malformed(ROW, step.id);
          failure = doc.totalFrames === nextScene.total ? null : 'saved-total-frames-differs';
          saved = { hash, timeline: next, scene: nextScene };
        } else {
          if (!saved) throw malformed(ROW, step.id);
          if (hash !== saved.hash) failure = 'saved-file-changed-by-opening';
          else if (next.undo !== 0 || next.redo !== 0) failure = 'reopened-history-not-fresh';
          else if (!sameContent(saved.timeline, next) || !sameContent(saved.scene, nextScene)) failure = 'reopened-drawings-differ-from-saved';
          if (!failure) adopt();
        }
      }
      if (failure) { outcome[step.check] = 'fail'; return finish(true, step.id + ': ' + failure); }
      if (--remaining[step.check] === 0) outcome[step.check] = 'pass';
    }
    return finish(false);
    } catch (error) {
      // Keep what was established before an unusable reply or an input refusal; the batch still stops.
      try { kit.privateEvidence(dir, 'partial', { schema: 'nemo.component-edit-draw-private/1', outcome, stoppedBy: error?.code || 'error' }); } catch { /* receipt directory unavailable */ }
      throw error;
    }
  } };
}
function prepare({ config }) { return createWorkflow({ reportDirectory: config.reportDirectory }); }
module.exports = { prepare, createWorkflow, JUDGES, CLAUSES, PLAN, BEFORE, AFTER, EXPORT, SAVED };
