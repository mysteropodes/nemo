'use strict';
// #2172 clause sync-owner-paths: the real shared-folder publish, check, merge and failure paths.
// The UI32 helper cannot press Publish, cannot check or choose a configured folder, and cannot press a Merge
// row (it only reaches the empty-folder cancel and check), so a person performs those clicks. The person never
// reports an outcome. This module judges raw observations it takes itself:
//   * the disposable shared folder on disk (the scenario writes the second profile's published state into it
//     and reads what the app wrote), never anything the operator copies;
//   * the helper's accessibility observation (folder path, update rows, Merge/Merged buttons, last toast);
//   * read-only Nemo MCP cell reads the HARNESS makes itself (session.read, bound to the candidate) for the
//     document state before and after the merge; the operator never attaches document state.
// Expected behaviour and wording are the baseline's (1065e9d8 project.js publishToShared / checkSharedUpdates /
// pullAndMerge, app.js mergeRemoteSnapshot, timeline.js sync UI). What the first installed run may expose
// differently (toast or row not in the accessibility tree, a snapshot shape that is not layers/frames, a
// document read that does not move) is reported as unrun or failed with its reason, never guessed.
const fs = require('node:fs');
const path = require('node:path');
const { isDeepStrictEqual: same } = require('node:util');
const kit = require('./installed-kit.cjs');
const { summarize } = require('./frame-edit-ui.cjs');
const { plain, text, malformed } = kit;

const SECOND = { id: 'cs7-second-profile', name: 'CS7 Second Profile', color: '#ff8800', role: 'animator', email: '' };
// Exact i18n strings (src/js/i18n.js: en, fr, ja, es); a unit test keeps these equal to the source.
const STR = {
  first: ['Set up a sync folder first', "Configurez un dossier de sync d'abord", '先に同期フォルダーを設定してください', 'Configura primero una carpeta de sincronización'],
  published: ['Published for the team', "Publié pour l'équipe", 'チーム向けに公開しました', 'Publicado para el equipo'],
  failed: ['Publish failed', 'Échec de la publication', '公開に失敗しました', 'Fallo al publicar'],
  configured: ['Sync folder configured', 'Dossier de sync configuré', '同期フォルダを設定しました', 'Carpeta de sincronización configurada'],
  merge: ['Merge', 'Fusionner', '統合', 'Fusionar'],
  merged: ['Merged', 'Fusionné', '統合済み', 'Fusionado'],
};
const NONE = "Aucune mise à jour d'un autre profil."; // hard-coded French in the baseline
const SEARCHING = 'Recherche…';                       // likewise
const MERGE_TOAST = `Fusion de ${SECOND.name} : +1 ajout(s)`; // hard-coded French; suffix only if conflicts
const ROLES = ['animator', 'supervisor', 'producer'];

const idOf = n => n?.identifiers?.resolved || n?.identifier || '';
const valueOf = (nodes, id) => { const m = nodes.filter(n => idOf(n) === id); return m.length === 1 && m[0].value !== undefined ? String(m[0].value) : null; };
const textsOf = nodes => nodes.flatMap(n => [...(Array.isArray(n.labels) ? n.labels.map(String) : []), ...(typeof n.value === 'string' ? [n.value] : [])]);
const has = (texts, set) => texts.some(t => set.some(s => t.includes(s)));
const buttonsLabelled = (nodes, set) => nodes.filter(n => n.role === 'AXButton' && Array.isArray(n.labels) && n.labels.some(l => set.includes(l)));
const real = p => { try { return fs.realpathSync(p); } catch { return p; } };
const snapshotsIn = d => fs.readdirSync(d).filter(n => /^\d+\.json$/.test(n)).sort();
const json = file => { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return null; } };

const READS = 'Do NOT attach any MCP reads or document state: the runner reads the document itself right after your reply, so after ' +
  'replying touch nothing in the app. Do not add pass/fail fields: a reply carrying a verdict or any read is refused.';

// ---- the pieces of the second profile's published state ---------------------------------------
// The second profile's state is derived from the snapshot the app itself published, so its shape is the app's
// real one: the stroke drawn on frame 1 is cloned (new strokeId, no owner) into the blank key at frame 4.
// Returns {snapshot} or {unrun} (shape not the layers/frames shape the baseline merge reads) or {fail}.
function deriveSecondSnapshot(published, marker) {
  if (!plain(published) || !Array.isArray(published.layers) || !published.layers.length) return { unrun: 'published snapshot has no layers array' };
  const layers = published.layers;
  if (!layers.every(L => plain(L) && typeof L.name === 'string' && Array.isArray(L.frames) && L.frames.every(plain))) return { unrun: 'published snapshot layers are not name/frames records' };
  const drawn = layers.filter(L => L.frames.length > 3 && Array.isArray(L.frames[0].strokes) && L.frames[0].strokes.length > 0);
  if (!layers.some(L => L.frames.length && Object.hasOwn(L.frames[0], 'strokes'))) return { unrun: 'published snapshot frames carry no strokes field' };
  if (!drawn.length) return { fail: 'the published snapshot has no layer with the stroke drawn on frame 1 and a frame 4' };
  const layer = drawn[0], blank = layer.frames[3];
  if (!Object.hasOwn(blank, 'isKeyframe')) return { unrun: 'published snapshot frames carry no isKeyframe field' };
  if (blank.isKeyframe !== true || (Array.isArray(blank.strokes) && blank.strokes.length)) return { fail: 'the published snapshot does not show frame 4 as a blank keyframe' };
  const source = layer.frames[0].strokes[0];
  if (!plain(source) || !text(source.strokeId)) return { unrun: 'the drawn stroke carries no strokeId, which the baseline merge needs' };
  const snapshot = JSON.parse(JSON.stringify(published));
  const clone = JSON.parse(JSON.stringify(source));
  clone.strokeId = marker; delete clone.ownerId; delete clone.ownerName; delete clone.ownerColor;
  snapshot.layers.find(L => L.name === layer.name).frames[3].strokes = [clone];
  return { snapshot, layerName: layer.name };
}
function readPublished(shared, profileName) {
  const dirs = fs.readdirSync(shared);
  if (dirs.length !== 1 || !fs.statSync(path.join(shared, dirs[0])).isDirectory()) return { fail: 'Publish did not create exactly one profile folder in the shared folder' };
  const dir = path.join(shared, dirs[0]), profile = json(path.join(dir, '_profile.json'));
  if (!plain(profile) || profile.id !== dirs[0] || !text(profile.name) || typeof profile.color !== 'string' || !ROLES.includes(profile.role)) return { fail: '_profile.json is missing or is not the publisher\'s profile' };
  if (profileName !== null && profile.name !== profileName) return { fail: '_profile.json does not carry the profile name shown in Settings' };
  const files = snapshotsIn(dir), extra = fs.readdirSync(dir).filter(n => n !== '_profile.json' && !files.includes(n));
  if (files.length !== 1 || extra.length) return { fail: 'Publish did not write exactly one timestamped snapshot' };
  const snapshot = json(path.join(dir, files[0]));
  if (!plain(snapshot)) return { fail: 'the published snapshot is not a JSON document' };
  return { dir, profile, file: files[0], snapshot };
}

// ---- the sequence ------------------------------------------------------------------------------
// ctx: session, ask(phase, instruction, extra), observe(), stable(), sleep, dir (private evidence directory),
// profileName (the profile name read in Settings), openSettings(), closeSettings().
async function runSyncOwner(ctx) {
  const { session, ask, observe, stable, sleep, dir, profileName } = ctx;
  const parts = {}, reasons = {};
  const settle = (part, status, why) => { parts[part] = status; if (why) reasons[part] = why; return status; };
  const result = () => {
    const values = Object.values(parts);
    const status = values.includes('fail') ? 'fail' : values.length === 6 && values.every(v => v === 'pass') ? 'pass' : 'unrun';
    const reason = Object.entries(reasons).map(([k, v]) => k + ': ' + v).join('; ') || null;
    kit.privateEvidence(dir, 'sync-owner', { schema: 'nemo.settings-sync-owner-private/1', parts, reasons });
    return { status, reason, parts };
  };
  const shared = path.join(dir, 'shared');
  fs.mkdirSync(shared, { recursive: true, mode: 0o700 });
  const sharedReal = fs.realpathSync(shared);
  // Toasts: #toast keeps its last text after it fades, so a label may be stale. Each judgement therefore also
  // requires that the expected text was NOT already shown before the gesture.
  const toastsNow = async () => textsOf(await observe());
  const afterGesture = async () => { const seen = []; for (let i = 0; i < 4; i++) { seen.push(...textsOf(await observe())); if (i < 3) await sleep(400); } return seen; };
  const document = async phase => {
    const seen = await kit.readTimeline(session, { contexts: ['scene-root'], record: { dir, name: phase } });
    const state = summarize({ history: seen.history, layers: seen.layers['scene-root'] }, 'scene-root');
    if (!state) throw malformed('settings-profile', phase);
    return state;
  };
  const gesture = async (phase, instruction, extra) => {
    const before = await toastsNow();
    const ev = await ask(phase, instruction, extra);
    return { ev, before };
  };

  // 0. the document the merge lands in: a stroke on frame 1 and a blank keyframe at frame 4 of the same layer
  await ctx.closeSettings();
  const setup = await ask('sync-document-setup', 'With Settings closed, on Layer 1 draw one stroke on frame 1 (that cell is a key with content). Then click ' +
    'Layer 1\'s cell at frame 4 only and press the REAL F7 key once so frame 4 becomes a blank key (a key with no content). Do not save and do not ' +
    'open Settings. Reply with {observation, productFailure, layer: {uid: <layerUid of that layer>}}. ' + READS);
  if (setup.productFailure) { settle('document-setup', 'fail', 'drawing or the blank keyframe reported failing'); return result(); }
  const uid = setup.layer?.uid;
  if (!text(uid)) throw malformed('settings-profile', 'sync-document-setup');
  const before = await document('sync-document-setup');
  if (!Object.hasOwn(before.rows, uid)) throw malformed('settings-profile', 'sync-document-setup');
  const row0 = before.rows[uid];
  if (before.total < 5 || row0.k[0] !== 'key' || row0.c[0] !== true || row0.k[3] !== 'key' || row0.c[3] !== false) throw malformed('settings-profile', 'sync-document-setup');

  await ctx.openSettings();
  await session.input('settings.tab', 'Collaboration');
  let n = await stable();
  if (valueOf(n, 'sync-folder-path') !== '') throw malformed('settings-profile', 'sync-baseline');

  // 1. Publish with no folder configured: the baseline answers "Set up a sync folder first" and writes nothing
  {
    const { before: toasts } = await gesture('sync-publish-no-folder', 'In Settings > Collaboration (no shared folder is set) click "Publish" once. Do nothing else. ' +
      'Reply with {observation, productFailure}.');
    const seen = await afterGesture();
    if (fs.readdirSync(shared).length) settle('no-folder-publish', 'fail', 'Publish wrote into the disposable folder although none was configured');
    else if (has(seen, STR.published) && !has(toasts, STR.published)) settle('no-folder-publish', 'fail', 'Publish reported success with no folder configured');
    else if (has(toasts, STR.first)) settle('no-folder-publish', 'unrun', 'the "set up a folder" text was already shown before the click, so it is not fresh');
    else if (has(seen, STR.first)) settle('no-folder-publish', 'pass');
    else settle('no-folder-publish', 'unrun', 'no toast text reached the accessibility read');
    if (parts['no-folder-publish'] === 'fail') return result();
  }

  // 2. Choose the disposable folder in the native picker
  {
    const ev = await ask('sync-choose-folder', 'In Settings > Collaboration click "Choose…". In the folder picker go to exactly ' + sharedReal +
      ' (Cmd+Shift+G types a path), select that folder and confirm. Do not click anything else. Reply with {observation, productFailure}.', { sharedFolder: sharedReal });
    if (ev.productFailure) { settle('choose-folder', 'fail', 'choosing the folder reported failing'); return result(); }
    n = await stable();
    const value = valueOf(n, 'sync-folder-path');
    if (value === null) settle('choose-folder', 'unrun', 'the folder field value is not exposed');
    else if (value === '') { settle('choose-folder', 'fail', 'the folder field stayed empty after the picker was confirmed'); return result(); }
    else if (real(value) !== sharedReal) throw malformed('settings-profile', 'sync-choose-folder');
    else settle('choose-folder', 'pass');
    if (parts['choose-folder'] !== 'pass') return result();
  }

  // 3. Publish: the app writes this profile's folder, _profile.json and one timestamped snapshot
  let own = null, derived = null;
  {
    const start = Date.now() - 2000;
    const { ev, before: toasts } = await gesture('sync-publish', 'In Settings > Collaboration click "Publish" once. Do nothing else. Reply with {observation, productFailure}.');
    if (ev.productFailure) { settle('publish', 'fail', 'Publish reported failing'); return result(); }
    const seen = await afterGesture();
    own = readPublished(shared, profileName);
    if (own.fail) { settle('publish', 'fail', own.fail); return result(); }
    if (fs.statSync(path.join(own.dir, own.file)).mtimeMs < start) { settle('publish', 'fail', 'the snapshot predates the click'); return result(); }
    if (has(seen, STR.failed) && !has(toasts, STR.failed)) { settle('publish', 'fail', 'Publish showed its failure text although files were written'); return result(); }
    settle('publish', 'pass', has(seen, STR.published) ? null : 'the "published" toast was not read; the files are the evidence');
    derived = deriveSecondSnapshot(own.snapshot, 'cs7sync' + Date.now().toString(36));
    if (derived.fail) { settle('publish', 'fail', derived.fail); return result(); }
    if (derived.unrun) {
      reasons.publish = derived.unrun;
      settle('check', 'unrun', 'no second profile could be derived: ' + derived.unrun);
      settle('merge', 'unrun', 'no second profile could be derived: ' + derived.unrun);
    } else {
      const secondDir = path.join(shared, SECOND.id);
      fs.mkdirSync(secondDir, { mode: 0o700 });
      fs.writeFileSync(path.join(secondDir, '_profile.json'), JSON.stringify(SECOND), { mode: 0o600 });
      fs.writeFileSync(path.join(secondDir, Date.now() + '.json'), JSON.stringify(derived.snapshot), { mode: 0o600 });
    }
  }

  // 4./5. Check lists only the OTHER profile; Merge pulls its stroke into the document
  if (!derived.unrun) {
    const ev = await ask('sync-check', 'In Settings > Collaboration click "Vérifier les mises à jour" (Check for updates) once. Do nothing else. Reply with {observation, productFailure}.');
    if (ev.productFailure) { settle('check', 'fail', 'Check reported failing'); return result(); }
    n = await stable();
    const texts = textsOf(n), mergeButtons = buttonsLabelled(n, STR.merge);
    if (has(texts, [SEARCHING])) settle('check', 'fail', 'the check left a stuck search text');
    else if (has(texts, [own.profile.name + ' — '])) settle('check', 'fail', 'the check lists this profile\'s own published state');
    else if (mergeButtons.length > 1) settle('check', 'fail', 'the check lists more than the one other profile');
    else if (mergeButtons.length === 1 && has(texts, [SECOND.name + ' — '])) settle('check', 'pass');
    else if (has(texts, [NONE])) settle('check', 'fail', 'the check reports no update although a second profile published');
    else settle('check', 'unrun', 'the update list is not in the accessibility read (no Merge row and no empty-list text)');
    if (parts.check === 'fail') return result();

    if (parts.check === 'pass') {
      // The state the merge starts from, read by the harness right before the click (nothing has edited the document since).
      const preMerge = await document('sync-merge-before');
      const { ev: mergeEv, before: toasts } = await gesture('sync-merge', 'Click the "Merge" button on the row of "' + SECOND.name + '" once. Reply immediately with ' +
        '{observation, productFailure}. ' + READS);
      if (mergeEv.productFailure) { settle('merge', 'fail', 'Merge reported failing'); return result(); }
      const seen = [], nodes = [];
      for (let i = 0; i < 4; i++) { const o = await observe(); nodes.push(o); seen.push(...textsOf(o)); if (i < 3) await sleep(400); }
      const last = nodes.at(-1);
      const merged = buttonsLabelled(last, STR.merged).length === 1 && buttonsLabelled(last, STR.merge).length === 0;
      const fusion = seen.filter(t => t.includes('Fusion de ') && !toasts.includes(t));
      const after = await document('sync-merge-reads');
      if (fusion.length && !fusion.some(t => t.includes(MERGE_TOAST))) settle('merge', 'fail', 'merge toast is not "' + MERGE_TOAST + '": ' + fusion.join(' | '));
      else if (after.documentId !== preMerge.documentId || after.instanceId !== preMerge.instanceId || after.total !== preMerge.total ||
        !same(after.rows[uid]?.k, row0.k) || !same(Object.keys(after.rows).sort(), Object.keys(preMerge.rows).sort())) settle('merge', 'fail', 'the merge replaced the document or changed its frames');
      else if (after.rows[uid].c.some((c, i) => i !== 3 && c !== row0.c[i])) settle('merge', 'fail', 'the merge changed frames other than the blank keyframe');
      else if (after.rows[uid].c[3] !== true) settle('merge', 'fail', 'the document still has no content on frame 4 after Merge (the toast said: ' + (fusion[0] || 'nothing read') + ')');
      else if (after.revision <= preMerge.revision) settle('merge', 'fail', 'the content appeared without a new document revision');
      else if (after.redo !== 0 || after.undo < preMerge.undo) settle('merge', 'fail', 'the merge disturbed the history (undo ' + preMerge.undo + ' -> ' + after.undo + ', redo ' + after.redo + ')');
      else if (!merged) settle('merge', 'fail', 'the document has the merged stroke but the row did not become "Merged"');
      // Content on frame 4 and a Merged row do not show that the content is the second profile's stroke: only the merge toast, which
      // names that profile and counts what it added, attributes it. Without a fresh toast the clause is unrun, never passed.
      else if (!fusion.length) settle('merge', 'unrun', 'the merge toast was not read, so the content on frame 4 cannot be attributed to the second profile\'s stroke');
      else settle('merge', 'pass');
      kit.privateEvidence(dir, 'sync-merge-history', { schema: 'nemo.settings-sync-owner-private/1', revision: [preMerge.revision, after.revision], undo: [preMerge.undo, after.undo], redo: [preMerge.redo, after.redo] });
      if (parts.merge === 'fail') return result();
    } else settle('merge', 'unrun', 'there was no readable Merge row');
  }

  // 6. Publish into a folder the app cannot write: the baseline writes nothing new and shows "Publish failed"
  {
    const before2 = fs.readdirSync(own.dir).sort(), mode = fs.statSync(own.dir).mode & 0o777;
    try {
      fs.chmodSync(own.dir, 0o500);
      const blocked = (fs.statSync(own.dir).mode & 0o200) === 0 && !(process.getuid && process.getuid() === 0);
      const { ev, before: toasts } = await gesture('sync-publish-unwritable', 'In Settings > Collaboration click "Publish" once more. The runner has made this profile\'s folder in the ' +
        'shared folder read-only. Do nothing else. Reply with {observation, productFailure}.');
      if (ev.productFailure) { settle('unwritable-publish', 'fail', 'the failing Publish reported a crash or hang'); return result(); }
      const seen = await afterGesture();
      const wrote = snapshotsIn(own.dir).some(f => !before2.includes(f));
      if (!blocked) settle('unwritable-publish', 'unrun', 'the shared folder could not be made unwritable on this machine');
      else if (wrote) settle('unwritable-publish', 'fail', 'Publish wrote a snapshot into a read-only folder');
      else if (has(seen, STR.published) && !has(toasts, STR.published)) settle('unwritable-publish', 'fail', 'Publish reported success although nothing could be written');
      else if (has(toasts, STR.failed)) settle('unwritable-publish', 'unrun', 'the failure text was already shown before the click, so it is not fresh');
      else if (has(seen, STR.failed)) settle('unwritable-publish', 'pass');
      else settle('unwritable-publish', 'unrun', 'nothing was written, but no failure text reached the accessibility read');
    } finally { try { fs.chmodSync(own.dir, mode || 0o700); } catch { /* receipt directory removed */ } }
  }
  return result();
}

module.exports = { runSyncOwner, deriveSecondSnapshot, readPublished, STR, SECOND, MERGE_TOAST, NONE, SEARCHING };
