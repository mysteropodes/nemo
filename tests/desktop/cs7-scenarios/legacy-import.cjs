'use strict';
// #2355 installed row: a baseline project opens as a NEW unsaved native document (PRs #2772, #2774, #2777).
// The native file picker, Cmd+S/Save As and F5 are outside the UI32 helper, so a person performs them. The person
// never reports a verdict. The scenario itself:
//   * copies small baseline-format fixtures into a private source folder and records their sha256, size and mtime,
//     so "the old file is byte-identical afterwards" is a measurement it repeats after every phase;
//   * derives what the opened document must contain (scene frames, component frames, layer names) from the
//     fixture file, then compares it with read-only Nemo MCP replies the HARNESS reads itself (session.read,
//     bound to the candidate); the person never attaches document state;
//   * reads the saved files from disk (their hash and mtime are measured by the runner around each step, and their layers and
//     frame count are compared with the document the app reports) and the toast text from the helper's accessibility
//     observation (a toast or dialog that cannot be read leaves its clause unrun with the reason, it is never guessed).
// How a file was picked or saved (the native picker, Cmd+S, F5) cannot be observed: those surfaces are operator-attested.
// Expected behaviour: baseline (1065e9d8) project.js shows "Opened: <name>" for an opened project; the native
// import adds that it is a new project, the original is never modified and the first Save goes through Save As
// (src/js/adapters/native-legacy-import.js notice and samePath guard). A field with no native owner refuses the
// open naming its path, and the document that was open stays as it was (PR #2777).
const fs = require('node:fs');
const path = require('node:path');
const { isDeepStrictEqual: same } = require('node:util');
const kit = require('./installed-kit.cjs');
const { summarize, JUDGES } = require('./frame-edit-ui.cjs');
const { plain, int, text, malformed, rawOnly } = kit;

const ROW = 'legacy-import';
const FIXTURES = path.resolve(__dirname, '../../fixtures/legacy-import');
const SOURCES = 'legacy-source';
const CLAUSES = ['open-as-new-document', 'original-untouched', 'first-save-save-as', 'edit-undo-redo', 'save-reopen', 'media-text-open', 'refused-field-path'];
const OPENED = ' — imported as a new project; the original file is not modified. Use Save As to keep your work.';
const REFUSED_SAVE = 'The original file is never overwritten. Choose another name.';
const REFUSED_OPEN = 'Could not open native project';
const SAVED = 'saved/legacy-animation.json';
const OP = { history: 'query.document.object.history', component: 'query.document.object.component.read' };

// The fixture files: the repository's own baseline-format projects, plus two derived refusals (a field with no native owner).
const FILES = {
  animation: { from: 'animation.json', name: 'animation.json', base: 'animation' },
  media: { from: 'media.json', name: 'media.json', base: 'media' },
  expr: { from: 'single-frame.json', name: 'refused-expr.json', base: 'refused-expr', patch: { exprGlobals: 'var a = 1;' }, field: 'exprGlobals' },
  roles: { from: 'single-frame.json', name: 'refused-roles.json', base: 'refused-roles', patch: { trackRoles: { r: { name: 'a' } } }, field: 'trackRoles' },
};
const statOf = file => { const bytes = fs.readFileSync(file), s = fs.statSync(file); return { sha: kit.sha256(bytes), size: bytes.length, mtimeMs: s.mtimeMs }; };
function prepareSources(reportDirectory, fixtures) {
  const dir = path.join(reportDirectory, SOURCES);
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
  const out = {};
  for (const [key, spec] of Object.entries(FILES)) {
    const json = JSON.parse(fs.readFileSync(path.join(fixtures, spec.from), 'utf8'));
    const file = path.join(dir, spec.name);
    fs.writeFileSync(file, JSON.stringify({ ...json, ...(spec.patch || {}) }, null, 1) + '\n', { flag: 'wx', mode: 0o600 });
    out[key] = { file, json: JSON.parse(fs.readFileSync(file, 'utf8')), stat: statOf(file), spec };
  }
  return out;
}
// The layer kind the read model reports (object_component_read.rs) for a layer the file declares. A kind whose mapping is not certain
// (an effector layer) is left out rather than guessed.
function kindOfLayer(layer) {
  if (layer.isEffectorLayer || layer.isEffectLayer) return null;
  return layer.isWidgetLayer ? 'widget' : layer.isNullLayer ? 'null' : layer.isFolderLayer ? 'folder' : layer.symbolId ? 'instance' : 'drawing';
}
function expectedOf(json) {
  const symbols = plain(json.symbols) ? Object.values(json.symbols) : [];
  const kinds = Object.fromEntries(json.layers.map(l => [l.name, kindOfLayer(l)]).filter(([, k]) => k));
  return { frames: json.totalFrames, names: json.layers.map(l => l.name).sort(), kinds, comps: symbols.map(s => s.totalFrames).sort((a, b) => a - b) };
}

// ---- raw MCP replies -> document summary ---------------------------------------------------------------
function pair(p, operation) {
  return plain(p) && plain(p.request) && plain(p.response) && p.request.operation === operation && plain(p.request.payload) &&
    p.response.ok === true && plain(p.response.result) && text(p.response.documentId) && text(p.response.instanceId) ? p : null;
}
// reads: {history, contexts, layers}. contexts and layers are the public component read (selector kinds "contexts" and "layers" of
// "scene-root") from cursor 0, one complete page, pinned to the revision the history read reports.
function summarizeDoc(reads, why = {}) {
  const no = reason => { why.reason = reason; return null; };
  if (!plain(reads)) return no('reads is not an object');
  const h = pair(reads.history, OP.history);
  if (!h) return no('history is not an ok query.document.object.history request/response pair');
  const r = h.response.result;
  if (![r.atRevision, r.undoDepth, r.redoDepth].every(int) || !text(r.documentSnapshotId)) return no('history result lacks atRevision, undoDepth, redoDepth or documentSnapshotId');
  const page = (p, kind) => {
    const q = pair(p, OP.component);
    if (!q) return null;
    const payload = q.request.payload, selector = payload.selector, result = q.response.result;
    if (payload.atRevision !== r.atRevision || payload.documentSnapshotId !== r.documentSnapshotId || !plain(selector) || selector.kind !== kind || selector.cursor !== 0) return null;
    if (q.response.documentId !== h.response.documentId || q.response.instanceId !== h.response.instanceId) return null;
    if (result.kind !== kind || result.atRevision !== r.atRevision || result.documentSnapshotId !== r.documentSnapshotId ||
      result.cursor !== 0 || result.nextCursor !== null || !Array.isArray(result.items)) return null;
    return { selector, result };
  };
  const cx = page(reads.contexts, 'contexts'), ly = page(reads.layers, 'layers');
  if (!cx || cx.result.contextCount !== cx.result.items.length) return no('contexts is not one complete pinned component.read page of kind "contexts"');
  if (!ly || ly.selector.contextId !== 'scene-root' || ly.result.contextId !== 'scene-root' || ly.result.layerCount !== ly.result.items.length) {
    return no('layers is not one complete pinned component.read page of kind "layers" for scene-root');
  }
  const contexts = [], layers = [];
  for (const c of cx.result.items) {
    if (!plain(c) || !text(c.contextId) || !['scene', 'component'].includes(c.contextKind) || !int(c.totalFrames) || !int(c.layerCount)) return no('a context item is malformed');
    contexts.push({ contextId: c.contextId, contextKind: c.contextKind, totalFrames: c.totalFrames, layerCount: c.layerCount });
  }
  for (const l of ly.result.items) {
    if (!plain(l) || !text(l.layerUid) || !(l.name === null || typeof l.name === 'string')) return no('a layer item is malformed');
    layers.push({ layerUid: l.layerUid, name: l.name ?? '', layerKind: l.layerKind });
  }
  return { documentId: h.response.documentId, instanceId: h.response.instanceId, revision: r.atRevision, snapshot: r.documentSnapshotId,
    undo: r.undoDepth, redo: r.redoDepth, contexts, layers };
}
const sceneOf = s => s.contexts.find(c => c.contextId === 'scene-root' && c.contextKind === 'scene');
const sameDocument = (a, b) => a.documentId === b.documentId && a.instanceId === b.instanceId;
const sameContent = (a, b) => same(a.contexts, b.contexts) && same(a.layers, b.layers);
// What a document read must show for the baseline file it was opened from.
function matches(summary, expected) {
  const scene = sceneOf(summary);
  if (!scene || scene.totalFrames !== expected.frames) return 'scene-total-frames-differ-from-the-file';
  if (scene.layerCount !== expected.names.length) return 'scene-layer-count-differs-from-the-file';
  const comps = summary.contexts.filter(c => c.contextKind === 'component').map(c => c.totalFrames).sort((a, b) => a - b);
  if (!same(comps, expected.comps)) return 'component-contexts-differ-from-the-file';
  if (!same(summary.layers.map(l => l.name).sort(), expected.names)) return 'scene-layer-names-differ-from-the-file';
  if (summary.layers.some(l => Object.hasOwn(expected.kinds, l.name) && l.layerKind !== expected.kinds[l.name])) return 'scene-layer-kinds-differ-from-the-file';
  return null;
}
function withFrames(summary, delta) {
  return { ...summary, contexts: summary.contexts.map(c => (c.contextId === 'scene-root' ? { ...c, totalFrames: c.totalFrames + delta } : c)) };
}
function oneEdit(p, n) {
  if (!sameDocument(p, n)) return 'document-changed';
  if (n.revision !== p.revision + 1) return 'not-exactly-one-revision';
  if (n.undo !== p.undo + 1 || n.redo !== 0) return 'not-exactly-one-history-entry';
  return null;
}
const unedited = (p, n) => sameDocument(p, n) && n.revision === p.revision && n.undo === p.undo && n.redo === p.redo && sameContent(p, n);
// The saved file against the document the app reports: same native document, scene frames, layer count, names and uids.
function savedAgrees(json, summary) {
  const doc = kit.documentOf(json), scene = sceneOf(summary);
  if (!doc) return 'saved-file-holds-no-native-object-document';
  if (!scene || doc.totalFrames !== scene.totalFrames) return 'saved-total-frames-differ-from-the-document';
  if (!same(doc.layers.map(l => l?.layerUid).sort(), summary.layers.map(l => l.layerUid).sort())) return 'saved-layers-differ-from-the-document';
  if (!same(doc.layers.map(l => (typeof l?.name === 'string' ? l.name : '')).sort(), summary.layers.map(l => l.name).sort())) return 'saved-layer-names-differ-from-the-document';
  return null;
}
// A name dialog, as the page's owned window shows it: a sheet/dialog node, or a Save button beside a Cancel button.
function nameDialog(nodes) {
  const buttons = nodes.filter(n => n.role === 'AXButton').flatMap(kit.labelsOfNode);
  return nodes.some(n => n.role === 'AXSheet' || n.role === 'AXDialog') || (buttons.some(l => /^(Cancel|Annuler)$/i.test(l)) && buttons.some(l => /^(Save|Enregistrer)$/i.test(l)));
}

// ---- toast text from the helper's accessibility observation -----------------------------------------------
const textsOf = nodes => nodes.flatMap(n => [...(Array.isArray(n?.labels) ? n.labels.map(String) : []), ...(typeof n?.value === 'string' ? [n.value] : [])]);
const freshOf = (before, after) => (after ? after.filter(t => !(before || []).includes(t)) : []);

// ---- the operator script -----------------------------------------------------------------------------------
const READS = 'Evidence reply (a JSON object): {observation: one plain sentence of what you saw, productFailure: true only if the app visibly ' +
  'misbehaved (crash, hang, an unexpected dialog you could not dismiss)}. Do NOT attach any MCP reads or document state: the runner reads the ' +
  'document itself right after your reply (history, contexts, layers), so after replying touch nothing in the app. Do not add pass/fail ' +
  'fields: the oracle refuses a reply that carries a verdict or any read.';
const PICK = 'In the native picker press Cmd+Shift+G, paste the path named in details.legacyFile, press Return, then Open. ';
const PLAN = [
  { id: 'window-placement', type: 'placement', check: null, how: kit.PLACEMENT + ' Reply with {observation: "placed"}.' },
  { id: 'open-animation', type: 'open', file: 'animation', check: 'open-as-new-document',
    how: 'From the Start screen choose File > Open (Cmd+O). ' + PICK + 'Let the project open, dismiss nothing else, and do not click in the document. ' + READS },
  { id: 'save-first-press', type: 'save-press', check: 'first-save-save-as',
    how: 'Press Cmd+S once and then touch NOTHING else: leave any dialog that appears open. Reply immediately with nameDialogOpened: true if a dialog ' +
      'asking for a file name and place is on screen (and false if the app saved or did nothing without asking). ' + READS },
  { id: 'save-first-name', type: 'save-first', check: 'first-save-save-as',
    how: 'If the name dialog from the previous step is still open, save to saved/legacy-animation.json inside the private report directory ' +
      '(details.reportDirectory); if no dialog is open, press Cmd+S again and do the same. Reply with savedFile: "' + SAVED + '" (path relative to the ' +
      'report directory). Do not touch the original file. ' + READS },
  { id: 'save-original-refused', type: 'refuse-save', file: 'animation', check: 'first-save-save-as',
    how: 'Choose File > Save As and pick the ORIGINAL file (details.legacyFile) as the destination; confirm Replace if the system asks. If the app lets ' +
      'you continue anyway, stop there. Reply with {observation, productFailure}.' },
  { id: 'legacy-edit', type: 'edit', delta: 1, check: 'edit-undo-redo',
    how: 'Click the cell of the layer named "Shapes" at frame 1 so exactly that cell is selected, then press the REAL F5 key once. ' + READS },
  { id: 'legacy-undo', type: 'undo', delta: -1, check: 'edit-undo-redo', how: 'Press Cmd+Z exactly once with the app focused (no menu). ' + READS },
  { id: 'legacy-redo', type: 'redo', delta: 1, check: 'edit-undo-redo', how: 'Press Cmd+Shift+Z exactly once. ' + READS },
  { id: 'save-again', type: 'save-again', check: 'save-reopen',
    how: 'Press Cmd+S once: the document now saves to saved/legacy-animation.json without asking again. If a dialog asks for a name, reply with ' +
      'nameDialogOpened: true and leave it open; otherwise reply with nameDialogOpened: false and savedFile: "' + SAVED + '". ' + READS },
  { id: 'reopen-saved', type: 'reopen', check: 'save-reopen',
    how: 'Choose File > Open and open the saved file saved/legacy-animation.json in the private report directory. ' + PICK.replace('details.legacyFile', 'details.savedPath') +
      'Do not edit anything. Reply with the same savedFile. ' + READS },
  { id: 'open-media', type: 'open', file: 'media', check: 'media-text-open',
    how: 'Choose File > Open (Cmd+O). ' + PICK + 'Let the project open and do not click in the document. ' + READS },
  { id: 'refuse-expr', type: 'refuse-open', file: 'expr', check: 'refused-field-path',
    how: 'Choose File > Open (Cmd+O) and open the file named in details.legacyFile. The app is expected to refuse it with a message. ' + PICK +
      'Do not click anything else afterwards. ' + READS },
  { id: 'refuse-roles', type: 'refuse-open', file: 'roles', check: 'refused-field-path',
    how: 'Choose File > Open (Cmd+O) and open the file named in details.legacyFile. The app is expected to refuse it with a message. ' + PICK +
      'Do not click anything else afterwards. ' + READS },
];

// ---- the scenario ------------------------------------------------------------------------------------------
// The attempts that must be SEEN (a notice, a refusal) before "the baseline files are untouched" may be claimed: an operator who
// never opened or saved anything leaves the files untouched too.
const ATTEMPTS = ['open-animation', 'open-media', 'save-original-refused', 'refuse-expr', 'refuse-roles'];
function createWorkflow({ reportDirectory, fixtureDirectory = FIXTURES }) {
  let used = false;
  return { async run({ session, row }) {
    if (used) throw new Error('legacy-import cannot restart');
    used = true;
    if (row.issue !== 2355 || !same(row.checks.map(c => c.id), CLAUSES)) throw new Error('original legacy-import clauses changed');
    const outcome = Object.fromEntries(CLAUSES.map(id => [id, 'unrun']));
    const remaining = Object.fromEntries(CLAUSES.map(id => [id, PLAN.filter(s => s.check === id).length]));
    const held = new Set(), reasons = {}, attempted = new Set();
    const dir = path.join(reportDirectory, 'legacy-import');
    const finish = (stop, reason) => {
      kit.privateEvidence(dir, 'outcome', { schema: 'nemo.legacy-import-private/1', outcome, reasons, attempted: [...attempted], reason: reason || null });
      return { checks: CLAUSES.map(id => ({ id, status: outcome[id] })), safeToContinue: !stop };
    };
    const hold = (check, reason) => { held.add(check); reasons[check] = (reasons[check] ? reasons[check] + '; ' : '') + reason; };
    const files = prepareSources(reportDirectory, fixtureDirectory);
    // The old files are measured again after every phase; any change is a failure of the "byte-identical" promise.
    const changed = () => Object.entries(files).filter(([, f]) => !same(statOf(f.file), f.stat)).map(([key]) => key);
    const owner = await session.runtimeContext();
    const lookNodes = async () => {
      try { const o = await session.input('observe'); return Array.isArray(o?.nodes) && o.nodes.every(plain) ? o.nodes : null; } catch { return null; }
    };
    let doc = null, firstSave = null, secondSave = null, cells = null;
    // The Shapes layer's cell runs (so F5 is shown to be a held cell inserted after frame 1), read by the harness on top of the document.
    const readCells = async (name, summary) => {
      const layer = summary.layers.find(l => l.name === 'Shapes');
      if (!layer) return null;
      const seen = await kit.readLayerCells(session, { contextId: 'scene-root', layerUid: layer.layerUid, record: { dir, name } });
      return summarize({ history: seen.history, layers: [seen.cells] }, 'scene-root')?.rows[layer.layerUid] || null;
    };
    try {
    for (const step of PLAN) {
      if (!same(await session.identity(), owner.identity)) throw malformed(ROW, step.id);
      const target = step.file ? files[step.file] : null;
      const details = { sourceSha: owner.identity.sourceSha, pid: owner.identity.pid, windowId: owner.window.windowId, reportDirectory,
        ...(target ? { legacyFile: target.file } : {}), ...(step.type === 'reopen' ? { savedPath: path.join(reportDirectory, SAVED) } : {}),
        evidenceRule: 'Raw observations only. Source tests and posted input do not establish PASS.' };
      const beforeNodes = await lookNodes(), before = beforeNodes && textsOf(beforeNodes);
      // Saved files are measured by the runner around the step: the first save must create the file after the request.
      const requestedAt = Date.now(), savedBefore = ['save-first', 'save-again', 'reopen'].includes(step.type) ? kit.measure(reportDirectory, SAVED) : null;
      if (step.type === 'save-first' && savedBefore) throw malformed(ROW, step.id);
      const ev = await session.requestEvidence({ row: ROW, phase: step.id, instruction: step.how, details });
      const afterNodes = await lookNodes(), after = afterNodes && textsOf(afterNodes);
      if (!rawOnly(ev) || !text(ev.observation) || typeof ev.productFailure !== 'boolean') throw malformed(ROW, step.id);
      kit.privateEvidence(dir, step.id, ev);
      if (step.type === 'placement') continue;
      const fresh = freshOf(before, after);
      kit.privateEvidence(dir, 'texts-' + step.id, { schema: 'nemo.legacy-import-private/1', readable: after !== null, fresh: fresh.slice(0, 40).map(t => t.slice(0, 300)) });
      const moved = changed();
      if (moved.length) { outcome['original-untouched'] = 'fail'; if (step.check) outcome[step.check] = 'fail'; return finish(true, step.id + ': a baseline file changed on disk (' + moved.join(', ') + ')'); }
      if (ev.productFailure) {
        // The person may only report a failure, never a success; a reported failure is taken as stated.
        if (step.check) outcome[step.check] = 'fail';
        return finish(true, step.id + ' failure reported');
      }
      if (step.type === 'refuse-save') {
        // No reads: nothing may be written. The original is measured above; the refusal wording is read from the toast.
        if (fresh.some(t => t.includes(REFUSED_SAVE))) attempted.add(step.id);
        else if (fresh.some(t => t.startsWith('Saved:'))) { outcome[step.check] = 'fail'; return finish(true, step.id + ': Save As over the original reported success'); }
        else hold(step.check, 'the refusal text for saving over the original was not read');
        remaining[step.check]--; if (!remaining[step.check] && !held.has(step.check)) outcome[step.check] = 'pass';
        continue;
      }
      const why = {}, next = summarizeDoc(await kit.readDocument(session, { record: { dir, name: step.id } }), why);
      if (!next) {
        kit.privateEvidence(dir, 'mismatch-' + step.id, { schema: 'nemo.legacy-import-private/1', phase: step.id, reason: why.reason });
        throw malformed(ROW, step.id);
      }
      let failure = null;
      if (step.type === 'open') {
        const expected = expectedOf(target.json), base = target.spec.base;
        if (next.undo !== 0 || next.redo !== 0) failure = 'opened-history-not-fresh';
        else if (doc && sameDocument(doc, next)) failure = 'open-did-not-create-a-new-document';
        else failure = matches(next, expected);
        // The native notice must be read, fresh, and be the whole notice: any "not carried over" suffix means the import was lossy.
        const notice = fresh.find(t => t.includes('Opened: ' + base));
        if (!failure && notice) {
          if (/not carried over/.test(notice)) failure = 'open-notice-reports-items-not-carried-over';
          else if (!notice.endsWith('Opened: ' + base + OPENED)) failure = 'open-notice-wording';
        }
        if (!failure && !notice) hold(step.check, 'the open notice for ' + base + ' was not read');
        if (!failure && notice) attempted.add(step.id);
        if (!failure) {
          doc = next;
          if (step.file === 'animation') { const rows = await readCells('cells-' + step.id, next); cells = rows && { rows, edit: null }; if (!cells) hold('edit-undo-redo', 'the Shapes layer cell runs could not be read'); }
        }
      } else if (step.type === 'save-press') {
        if (typeof ev.nameDialogOpened !== 'boolean') throw malformed(ROW, step.id);
        if (!unedited(doc, next)) failure = 'pressing-save-changed-the-document';
        else if (!ev.nameDialogOpened) failure = 'first-save-did-not-ask-for-a-name';
        else {
          // The operator's word is not proof of a dialog: the harness looks for one in the owned window's accessibility read.
          if (!afterNodes) hold(step.check, 'the accessibility read was unavailable, so the name dialog was not observed');
          else if (!nameDialog(afterNodes)) hold(step.check, 'the name dialog was reported but is not visible in the accessibility read');
        }
      } else if (step.type === 'save-first') {
        const f = kit.measure(reportDirectory, SAVED), json = kit.parseJson(f?.bytes);
        if (ev.savedFile !== SAVED || !f || !plain(json)) throw malformed(ROW, step.id);
        if (typeof json.format !== 'string' || !json.format.startsWith('nemo.native-')) failure = 'saved-file-is-not-a-native-project';
        else if (f.sha256 === files.animation.stat.sha) failure = 'saved-file-is-the-original';
        else if (f.mtimeMs < requestedAt - 2000) failure = 'saved-file-predates-the-save-step';
        else if (!unedited(doc, next)) failure = 'saving-changed-the-document';
        else failure = savedAgrees(json, next);
        if (!failure) firstSave = { hash: f.sha256, mtimeMs: f.mtimeMs };
        kit.privateEvidence(dir, 'file-' + step.id, { schema: 'nemo.legacy-import-private/1', sha256: f.sha256, size: f.size, mtimeMs: f.mtimeMs, requestedAt });
      } else if (step.type === 'edit' || step.type === 'undo' || step.type === 'redo') {
        const wanted = withFrames(doc, step.delta);
        if (step.type === 'edit') {
          failure = oneEdit(doc, next) || (sameContent(wanted, next) ? null : 'the-edit-is-not-visible-in-the-document');
        } else if (!sameDocument(doc, next) || !sameContent(wanted, next)) failure = step.type === 'undo' ? 'undo-did-not-restore-the-file-as-opened' : 'redo-did-not-restore-the-edit';
        else if (step.type === 'undo' ? next.undo !== doc.undo - 1 || next.redo !== doc.redo + 1 : next.undo !== doc.undo + 1 || next.redo !== doc.redo - 1) failure = 'not-exactly-one-history-step';
        // F5 on the Shapes cell at frame 1 is one held cell inserted after it, not a change of the Total frames field or an edit elsewhere.
        if (!failure && cells) {
          const now = await readCells('cells-' + step.id, next);
          if (!now) hold(step.check, step.id + ': the Shapes layer cell runs could not be read');
          else {
            if (step.type === 'edit') { failure = JUDGES.insertFrame(cells.rows, now, 0)?.fail || null; if (!failure) cells = { rows: now, edit: { before: cells.rows, after: now } }; }
            else if (!cells.edit || !same(step.type === 'undo' ? cells.edit.before : cells.edit.after, now)) failure = step.type === 'undo' ? 'undo-did-not-restore-the-shapes-cells' : 'redo-did-not-restore-the-shapes-cells';
            else cells = { ...cells, rows: now };
          }
        }
        if (!failure) doc = next;
      } else if (step.type === 'save-again' || step.type === 'reopen') {
        const f = kit.measure(reportDirectory, SAVED), json = kit.parseJson(f?.bytes);
        if (ev.savedFile !== SAVED && step.type === 'reopen') throw malformed(ROW, step.id);
        if (step.type === 'save-again') {
          if (ev.nameDialogOpened === true) failure = 'save-asked-for-a-name-again';
          else if (typeof ev.nameDialogOpened !== 'boolean' || ev.savedFile !== SAVED) throw malformed(ROW, step.id);
        }
        if (!failure && !firstSave) throw malformed(ROW, step.id);
        if (!failure && step.type === 'save-again') {
          if (!f || !plain(json)) failure = 'save-did-not-write-the-edit';
          else if (f.sha256 === firstSave.hash || f.mtimeMs <= firstSave.mtimeMs || f.mtimeMs < requestedAt - 2000) failure = 'save-did-not-write-the-edit';
          else if (!unedited(doc, next)) failure = 'saving-changed-the-document';
          else failure = savedAgrees(json, next);
          if (!failure && afterNodes && nameDialog(afterNodes)) failure = 'save-asked-for-a-name-again';
          if (!failure && !afterNodes) hold(step.check, 'the accessibility read was unavailable, so a repeat name dialog was not observed');
          if (!failure) secondSave = { hash: f.sha256, mtimeMs: f.mtimeMs, summary: next };
          if (!failure) kit.privateEvidence(dir, 'file-' + step.id, { schema: 'nemo.legacy-import-private/1', sha256: f.sha256, size: f.size, mtimeMs: f.mtimeMs, requestedAt });
        } else if (!failure && step.type === 'reopen') {
          if (!f || !plain(json)) throw malformed(ROW, step.id);
          if (!secondSave) throw malformed(ROW, step.id);
          else if (f.sha256 !== secondSave.hash || f.mtimeMs !== secondSave.mtimeMs) failure = 'saved-file-changed-by-opening';
          else if (sameDocument(secondSave.summary, next)) failure = 'reopen-did-not-create-a-new-document';
          else if (next.undo !== 0 || next.redo !== 0) failure = 'reopened-history-not-fresh';
          else if (!sameContent(secondSave.summary, next)) failure = 'reopened-document-differs-from-saved';
          if (!failure) doc = next;
        }
      } else if (step.type === 'refuse-open') {
        // The document that was open stays exactly as it was, and the refusal names the field path.
        if (!doc || !same(next, doc)) failure = 'refused-open-changed-the-open-document';
        else {
          const refusal = fresh.find(t => t.includes(REFUSED_OPEN));
          if (refusal && !refusal.includes(target.spec.field)) failure = 'refusal-does-not-name-the-field-path (' + target.spec.field + ')';
          else if (!refusal) hold(step.check, 'the refusal text for ' + target.spec.field + ' was not read');
          else attempted.add(step.id);
        }
      }
      if (failure) { outcome[step.check] = 'fail'; return finish(true, step.id + ': ' + failure); }
      if (--remaining[step.check] === 0 && !held.has(step.check)) outcome[step.check] = 'pass';
    }
    // Every phase measured the old files again; reaching here means none of them ever changed. That alone proves nothing unless the
    // writes it promises not to make were attempted and seen: the opens by their notice, the Save As onto the original and the refused
    // opens by their refusal text. Otherwise the clause stays unrun and says which attempt was not observed.
    if (!changed().length) {
      const missing = ATTEMPTS.filter(id => !attempted.has(id));
      if (!missing.length) outcome['original-untouched'] = 'pass';
      else reasons['original-untouched'] = 'the files are unchanged, but the attempt was not observed for: ' + missing.join(', ');
    }
    return finish(false);
    } catch (error) {
      try { kit.privateEvidence(dir, 'partial', { schema: 'nemo.legacy-import-private/1', outcome, reasons, stoppedBy: error?.code || 'error' }); } catch { /* receipt directory unavailable */ }
      throw error;
    }
  } };
}
function prepare({ config }) { return createWorkflow({ reportDirectory: config.reportDirectory }); }
module.exports = { prepare, createWorkflow, summarizeDoc, matches, expectedOf, savedAgrees, nameDialog, CLAUSES, PLAN, FILES, SAVED, OPENED, REFUSED_SAVE, REFUSED_OPEN, ATTEMPTS };
