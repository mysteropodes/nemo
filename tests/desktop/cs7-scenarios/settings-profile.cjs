'use strict';
// #2172 installed settings/profile row. The UI32 helper drives what its closed vocabulary reaches (settings
// entries, the eight tabs, close/backdrop, profile name/email/role with restore, tool buttons, the empty
// sync-folder open-cancel/check) and `observe` returns the raw accessibility facts the oracle judges.
// A person performs only what the helper cannot reach:
//   * File > New (the helper's entry.* commands need an entry mode the installed batch does not set),
//   * the colour swatch and its alpha (no colour command; judged from the helper's own window captures, with the swatch
//     rectangle taken from the helper's accessibility frame, never from the person),
//   * rebinding a shortcut and "Reset all" (no key capture, `shortcuts.reset` is denied by the helper).
//   * the sync-owner clause (settings-sync-owner.cjs): choosing a shared folder in the native picker, Publish,
//     Check, a Merge row and the failing Publish. The helper reaches only the empty-folder cancel/check, so a
//     person clicks and the oracle judges the disposable shared folder on disk (which it also seeds with a
//     second profile's published state), the accessibility read and the document reads the harness makes itself.
// Not covered by the helper: the backdrop press is attempted through the helper and recorded unrun if the
// helper refuses it (the helper then ends, so it is the last helper step).
const path = require('node:path');
const { isDeepStrictEqual: same } = require('node:util');
const kit = require('./installed-kit.cjs');
const { runSyncOwner } = require('./settings-sync-owner.cjs');
const { OperatorInputError } = require('../cs7-runner.cjs');
const { plain, text, malformed, rawOnly } = kit;

const ROW = 'settings-profile';
const CLAUSES = ['settings-entries-tabs', 'profile-persist-restore', 'profile-role-guard', 'profile-color-alpha',
  'shortcuts-reset', 'sync-empty-folder', 'sync-owner-paths'];
const TABS = ['General', 'Updates', 'Collaboration', 'Feedback', 'Figma', 'Shortcuts', 'Labs', 'About'];
const REQUESTED = { name: 'UI-validation', email: 'ui-validation@example.invalid' };
// Baseline timeline.js: an emptied name is stored as the literal 'Animateur' (not localized, no suffix; the
// suffixed 'Animateur XXXX' is only the first-install default).
const FALLBACK = 'Animateur';
const LOADING = /(Recherche…|Searching…|検索中|Buscando…)/;
// Baseline timeline.js setTool: showToast('Profil "Producteur" : lecture seule + commentaires'), a literal.
const GUARD_TOAST = 'Profil "Producteur" : lecture seule + commentaires';
const BRUSH = /^(Brush|Pinceau|ブラシ|Pincel)/, HAND = /^(Hand|Main|手のひら|Mano)/;
const CAPTURES = { baseline: 'capture-11.png', opaque: 'capture-12.png', alpha: 'capture-13.png', restored: 'capture-14.png' };
const OPAQUE = [0x33, 0x66, 0x99];
const WELL = 'profile-color-well';
// The element only one Settings pane renders (hidden panes are not in the accessibility tree): its presence names the pane.
const ANCHOR = { General: 'profile-name', Updates: 'app-check-update', Collaboration: 'sync-folder-path', Feedback: 'fb-pull',
  Figma: 'figma-token', Shortcuts: 'shortcuts-reset', Labs: 'labs-tools-list', About: 'app-version-txt-about' };
// The baseline's completion text for an update check that finds no other profile (a hard-coded French string).
const NONE = require('./settings-sync-owner.cjs').NONE;
const textsOf = nodes => nodes.flatMap(n => [...(Array.isArray(n.labels) ? n.labels.map(String) : []), ...(typeof n.value === 'string' ? [n.value] : [])]);
// The ground behind the swatch: four samples just outside its rectangle, which must agree (a plain panel), else unknown.
function groundAround(image, well) {
  const d = Math.max(3, Math.round(image.width / 200)), cx = well.x + (well.w >> 1), cy = well.y + (well.h >> 1);
  const points = [[well.x - d, cy], [well.x + well.w + d, cy], [cx, well.y - d], [cx, well.y + well.h + d]];
  if (points.some(([x, y]) => x < 0 || y < 0 || x >= image.width || y >= image.height)) return null;
  const colours = points.map(([x, y]) => kit.meanColour(image, x, y));
  const mean = [0, 1, 2].map(i => colours.reduce((t, c) => t + c[i], 0) / colours.length);
  return colours.every(c => near(c, mean, 10)) ? mean : null;
}

const idOf = n => n?.identifiers?.resolved || n?.identifier || '';
const byId = (nodes, id) => nodes.filter(n => idOf(n) === id);
const labelsOf = nodes => nodes.flatMap(n => (Array.isArray(n.labels) ? n.labels : []).map(String)).sort();
const valueOf = (nodes, id) => { const m = byId(nodes, id); return m.length === 1 && m[0].value !== undefined ? String(m[0].value) : null; };
const near = (a, b, tolerance) => a.every((v, i) => Math.abs(v - b[i]) <= tolerance);

function createWorkflow({ reportDirectory, sleep = ms => new Promise(resolve => setTimeout(resolve, ms)) }) {
  let used = false;
  return { async run({ session, row }) {
    if (used) throw new Error('settings-profile cannot restart');
    used = true;
    if (row.issue !== 2172 || !same(row.checks.map(c => c.id), CLAUSES)) throw new Error('original settings clauses changed');
    const outcome = Object.fromEntries(CLAUSES.map(id => [id, 'unrun']));
    const reasons = {};
    let sync = null;
    const dir = path.join(reportDirectory, ROW);
    const finish = stop => {
      kit.privateEvidence(dir, 'outcome', { schema: 'nemo.settings-profile-private/1', outcome, reasons,
        sync });
      return { checks: CLAUSES.map(id => ({ id, status: outcome[id] })), safeToContinue: !stop };
    };
    const fail = (id, why) => { outcome[id] = 'fail'; reasons[id] = why; return finish(true); };
    const owner = await session.runtimeContext();
    async function ask(phase, instruction, extra = {}) {
      const ev = await session.requestEvidence({ row: ROW, phase, instruction, details: { sourceSha: owner.identity.sourceSha,
        pid: owner.identity.pid, windowId: owner.window.windowId, reportDirectory, ...extra,
        evidenceRule: 'Raw observations only. Source tests and posted input do not establish PASS.' } });
      if (!rawOnly(ev) || !text(ev.observation) || typeof ev.productFailure !== 'boolean') throw malformed(ROW, phase);
      kit.privateEvidence(dir, phase, ev);
      return ev;
    }
    const observe = async () => {
      const r = await session.input('observe');
      if (!plain(r) || !Array.isArray(r.nodes) || !r.nodes.every(plain)) throw malformed(ROW, 'observe');
      return r.nodes;
    };
    // Toasts and loading text come and go: read until two consecutive observations carry the same labels.
    async function stable() {
      let last = await observe();
      for (let i = 0; i < 4; i++) {
        await sleep(1500);
        const now = await observe();
        if (same(labelsOf(now), labelsOf(last))) return now;
        last = now;
      }
      return last;
    }
    const openMenu = async () => { await session.input('settings.menu.open'); await session.input('settings.menu.entry'); };
    const openGear = () => session.input('settings.open');
    const isOpen = nodes => byId(nodes, 'settings-close').length === 1;
    const close = async () => { await session.input('settings.close'); return stable(); };
    const general = async () => { await session.input('settings.tab', 'General'); return stable(); };
    async function reopen(route) { await (route === 'menu' ? openMenu() : openGear()); return general(); }
    try {
      await ask('window-placement', kit.PLACEMENT + ' Reply with {observation: "placed", productFailure: false}.');
      const start = await observe();               // the helper's first command: the strict Start observation
      void start;
      const made = await ask('new-project', 'Through the ordinary UI choose File > New (or the Start screen New Project card) with the defaults, ' +
        'Create, then close the tutorial. Do not open Settings. Reply with {observation, productFailure}.');
      if (made.productFailure) return finish(true);
      if (!byId(await stable(), 'project-tabs-settings').length) throw malformed(ROW, 'new-project');

      // 1 settings-entries-tabs (everything except the backdrop, which is last so a refusal there loses nothing else)
      await openMenu();
      let n = await stable();
      if (!isOpen(n)) return fail('settings-entries-tabs', 'app menu Settings entry did not open the modal');
      const panes = [], unreadPanes = [];
      for (const tab of TABS) {
        await session.input('settings.tab', tab);
        n = await stable();
        if (!n.some(x => x.role === 'AXButton' && Array.isArray(x.labels) && x.labels.includes(tab))) return fail('settings-entries-tabs', 'tab button ' + tab + ' not observed');
        panes.push(labelsOf(n));
        if (panes.length > 1 && same(panes.at(-1), panes.at(-2))) return fail('settings-entries-tabs', 'tab ' + tab + ' did not change the pane');
        // The pane itself, not just a changed label set: this tab's own element is there and no other tab's element is.
        const shown = TABS.filter(t => byId(n, ANCHOR[t]).length > 0);
        const other = shown.find(t => t !== tab);
        if (other) return fail('settings-entries-tabs', 'tab ' + tab + ' showed the pane of ' + other);
        if (!shown.includes(tab)) unreadPanes.push(tab);
      }
      if (unreadPanes.length) reasons['settings-entries-tabs'] = 'the pane element of ' + unreadPanes.join(', ') + ' was not in the accessibility read';
      if (isOpen(await close())) return fail('settings-entries-tabs', 'close left the modal open');
      await openGear();
      if (!isOpen(await stable())) return fail('settings-entries-tabs', 'gear entry did not open the modal');
      if (isOpen(await close())) return fail('settings-entries-tabs', 'close after the gear entry left the modal open');

      // 2 profile-persist-restore: write through the menu route, read back after close and the OTHER entry route
      n = await reopen('menu');
      const name0 = valueOf(n, 'profile-name'), email0 = valueOf(n, 'profile-email');
      if (name0 === null || email0 === null) throw malformed(ROW, 'profile-baseline');
      await session.input('profile.name', undefined, REQUESTED.name);
      await session.input('profile.email', undefined, REQUESTED.email);
      n = await stable();
      if (valueOf(n, 'profile-name') !== REQUESTED.name || valueOf(n, 'profile-email') !== REQUESTED.email) {
        return fail('profile-persist-restore', 'typed name/email not read back in the open dialog');
      }
      await close();
      n = await reopen('gear');
      if (valueOf(n, 'profile-name') !== REQUESTED.name || valueOf(n, 'profile-email') !== REQUESTED.email) {
        return fail('profile-persist-restore', 'name/email not persisted across close and the other entry route');
      }
      await session.input('profile.name', undefined, '');
      await close();
      n = await reopen('menu');
      const fallback = valueOf(n, 'profile-name');
      if (fallback !== FALLBACK) return fail('profile-persist-restore', 'empty name did not fall back to the baseline default "' + FALLBACK + '" (read: ' + JSON.stringify(fallback) + ')');
      await session.input('profile.name.restore');
      await session.input('profile.email.restore');
      n = await stable();
      if (valueOf(n, 'profile-name') !== name0 || valueOf(n, 'profile-email') !== email0) return fail('profile-persist-restore', 'baseline profile not restored');
      outcome['profile-persist-restore'] = 'pass';

      // 3 profile-role-guard
      const role0 = valueOf(n, 'profile-role');
      if (role0 === null) throw malformed(ROW, 'role-baseline');
      const hint = nodes => labelsOf(byId(nodes, 'profile-role-hint'));
      const tools = nodes => {
        const buttons = nodes.filter(x => x.role === 'AXButton' && Array.isArray(x.labels));
        return { brush: buttons.find(x => x.labels.some(l => BRUSH.test(l))), hand: buttons.find(x => x.labels.some(l => HAND.test(l))) };
      };
      // Calibration with the baseline role: the tool buttons only count as evidence if pressing Brush then Hand
      // is seen to move their accessibility `selected` state. If it is not, only the literal toast can speak.
      await close();
      await session.input('tool.brush');
      const onBrush = tools(await stable());
      await session.input('tool.hand');
      const onHand = tools(await stable());
      const calibrated = onBrush.brush?.selected === true && onBrush.hand?.selected !== true &&
        onHand.hand?.selected === true && onHand.brush?.selected !== true;
      await reopen('gear');
      await session.input('profile.role', undefined, 'Supervisor');
      n = await stable();
      const supervisorHint = hint(n);
      if (valueOf(n, 'profile-role') !== 'Supervisor') return fail('profile-role-guard', 'Supervisor not read back');
      await session.input('profile.role', undefined, 'Producer');
      n = await stable();
      if (valueOf(n, 'profile-role') !== 'Producer') return fail('profile-role-guard', 'Producer not read back');
      if (!hint(n).length || same(hint(n), supervisorHint)) return fail('profile-role-guard', 'role hint did not change for Producer');
      await close();
      const staleToast = labelsOf(await observe()).some(l => l.includes(GUARD_TOAST));
      await session.input('tool.brush');
      const seenToast = []; let after = null;
      for (let i = 0; i < 3; i++) { after = await observe(); seenToast.push(labelsOf(after).some(l => l.includes(GUARD_TOAST))); if (i < 2) await sleep(500); }
      const toast = seenToast.some(Boolean), now = tools(after);
      let guard = 'unknown';
      if (calibrated && now.brush?.selected === true) guard = 'bypassed';
      else if (calibrated && now.brush?.selected !== true && now.hand?.selected === true) guard = 'held';
      else if (!calibrated && toast && !staleToast) guard = 'held';
      n = await reopen('gear');
      await session.input('profile.role.restore');
      n = await stable();
      if (guard === 'bypassed') return fail('profile-role-guard', 'Producer role let the Brush tool become active');
      if (valueOf(n, 'profile-role') !== role0) return fail('profile-role-guard', 'baseline role not restored');
      if (guard === 'held') {
        outcome['profile-role-guard'] = 'pass';
        if (!toast) reasons['profile-role-guard'] = 'tool state held; the read-only toast text was not read';
      } else reasons['profile-role-guard'] = 'role read back, but ' + (calibrated ? 'the tool buttons did not show Hand selected' :
        'the tool buttons do not expose their selected state and the baseline read-only toast was ' + (staleToast ? 'already shown before the press' : 'not read'));

      // 6 sync-empty-folder
      await session.input('settings.tab', 'Collaboration');
      n = await stable();
      if (valueOf(n, 'sync-folder-path') !== '') throw malformed(ROW, 'sync-baseline');
      await session.input('sync.open-cancel');
      await session.input('sync.panel-observe');
      await session.input('sync.cancel-only');
      await session.input('sync.complete');
      n = await stable();
      if (valueOf(n, 'sync-folder-path') !== '' || !isOpen(n)) return fail('sync-empty-folder', 'cancelling the folder picker changed the folder or closed Settings');
      const noneBefore = textsOf(await observe()).some(t => t.includes(NONE));
      await session.input('sync.check-empty');
      // The check completes when the baseline's "no update from another profile" text appears and the search text is gone.
      let loading = true, none = false;
      for (let i = 0; i < 6; i++) {
        n = await observe();
        loading = labelsOf(n).some(l => LOADING.test(l)); none = textsOf(n).some(t => t.includes(NONE));
        if (!loading && none) break;
        if (!loading && i >= 2) break;
        await sleep(1500);
      }
      if (valueOf(n, 'sync-folder-path') !== '' || loading) return fail('sync-empty-folder', 'empty-folder check left a stale folder or a stuck search');
      if (none && !noneBefore) outcome['sync-empty-folder'] = 'pass';
      else reasons['sync-empty-folder'] = none ? 'the completion text was already shown before the check, so it is not fresh' :
        'the completion text of the empty-folder check was not read, so the check cannot be shown to have run';

      // 5 shortcuts-reset: label sets before rebind, after the person rebinds, after "Reset all"
      // The baseline is taken AFTER a Reset all, never from whatever the tab held: an earlier row (frame-edit-ui binds
      // Duplicate keyframe) may have left a binding, and Reset all restores the true defaults.
      await session.input('settings.tab', 'Shortcuts');
      const first = await ask('shortcut-reset-first', 'In Settings > Shortcuts press "Reset all" (confirm if asked) so the tab shows the true defaults, whatever an earlier ' +
        'step bound. Do not close Settings. Reply with {observation, productFailure}.');
      if (first.productFailure) return fail('shortcuts-reset', 'the first Reset all reported failing');
      const defaults = labelsOf(await stable());
      const rebind = await ask('shortcut-rebind', 'In Settings > Shortcuts click the key box of "Insert frame" and press F8 so its binding changes. Do not close Settings. ' +
        'Reply with {observation, productFailure}.');
      if (rebind.productFailure) return fail('shortcuts-reset', 'rebinding reported failing');
      const rebound = labelsOf(await stable());
      if (same(rebound, defaults)) throw malformed(ROW, 'shortcut-rebind');
      const reset = await ask('shortcut-reset', 'Press "Reset all" in the Shortcuts tab (confirm if asked). Do not close Settings. Reply with {observation, productFailure}.');
      if (reset.productFailure) return fail('shortcuts-reset', 'reset reported failing');
      if (!same(labelsOf(await stable()), defaults)) return fail('shortcuts-reset', 'reset did not return the Shortcuts tab to its defaults');
      outcome['shortcuts-reset'] = 'pass';

      // 4 profile-color-alpha: owned-window captures the harness takes, against the swatch's own accessibility frame
      n = await general();
      const wellOf = shot => { const m = byId(shot.nodes, WELL); return m.length === 1 ? kit.frameOf(m[0]) : null; };
      const shoot = async name => {
        const r = await kit.shot(session, reportDirectory, name);
        if (!r) throw malformed(ROW, name);
        kit.privateEvidence(dir, 'shot-' + name.replace(/\.png$/, ''), { schema: 'nemo.settings-profile-private/1', sha256: r.sha256, bytes: r.bytes, stable: r.stable, window: r.window, well: wellOf(r) });
        return r;
      };
      const base = await shoot(CAPTURES.baseline), wellFrame = base.stable && base.window ? wellOf(base) : null;
      const well = wellFrame && kit.toPixels(base.image, base.window, wellFrame), ground0 = well && groundAround(base.image, well);
      if (!well || !kit.rectOk(base.image, well, 6, 6) || !ground0) {
        reasons['profile-color-alpha'] = !wellFrame ? 'the swatch has no accessibility frame in the helper read (or the capture was not stable)' :
          'the swatch rectangle is too small or the panel around it is not plain, so its colour cannot be measured';
      } else {
        const sameWell = shot => shot.stable && shot.window && Math.abs(shot.window.w - base.window.w) < 1 && Math.abs(shot.window.h - base.window.h) < 1 &&
          shot.image.width === base.image.width && shot.image.height === base.image.height && (() => { const f = wellOf(shot); return !!f && ['x', 'y', 'w', 'h'].every(k => Math.abs(f[k] - wellFrame[k]) <= 1); })();
        const centre = image => kit.meanColour(image, well.x + (well.w >> 1), well.y + (well.h >> 1));
        const original = centre(base.image);
        const setColour = async (phase, instruction, name) => {
          const ev = await ask(phase, instruction + ' Leave Settings open on the General tab. Reply with {observation, productFailure}.');
          if (ev.productFailure) return { failed: true };
          const r = await shoot(name);
          return sameWell(r) ? { image: r.image } : { moved: true };
        };
        const colourSteps = [];
        const run = async (phase, instruction, name) => { const r = await setColour(phase, instruction, name); colourSteps.push(r); return r; };
        const opaque = await run('profile-color-opaque', 'Click the colour swatch next to Name, set the colour to hex 336699 with alpha 100%, then close the picker.', CAPTURES.opaque);
        if (opaque.failed) return fail('profile-color-alpha', 'setting the colour reported failing');
        if (opaque.moved) reasons['profile-color-alpha'] = 'the swatch moved or the capture was unstable after setting the colour';
        else {
          if (!near(centre(opaque.image), OPAQUE, 16)) return fail('profile-color-alpha', 'swatch is not the chosen opaque colour');
          const translucent = await run('profile-color-alpha', 'Open the colour picker again and set the SAME hex 336699 but alpha 50%, then close the picker.', CAPTURES.alpha);
          if (translucent.failed) return fail('profile-color-alpha', 'setting the alpha reported failing');
          if (translucent.moved) reasons['profile-color-alpha'] = 'the swatch moved or the capture was unstable after setting the alpha';
          else {
            const solid = centre(opaque.image), half = centre(translucent.image), ground = groundAround(translucent.image, well);
            if (!ground) reasons['profile-color-alpha'] = 'the panel around the swatch is not plain in the alpha capture';
            else {
              const blended = solid.map((v, i) => (v + ground[i]) / 2);
              if (!near(half, blended, 16) || near(half, solid, 12)) return fail('profile-color-alpha', 'swatch does not show the half-transparent colour');
              const restored = await run('profile-color-restore', 'Restore the swatch to its ORIGINAL colour and alpha (the one in the first capture), then close the picker.', CAPTURES.restored);
              if (restored.failed) return fail('profile-color-alpha', 'restoring the colour reported failing');
              if (restored.moved) reasons['profile-color-alpha'] = 'the swatch moved or the capture was unstable after the restore';
              else if (!near(centre(restored.image), original, 10)) return fail('profile-color-alpha', 'original colour not restored');
              else outcome['profile-color-alpha'] = 'pass';
            }
          }
        }
      }

      // 7 sync-owner-paths: the folder picker, Publish, Check, Merge and the failing Publish. A person clicks; the
      // oracle judges the disposable shared folder, the accessibility read and the document reads the harness makes itself.
      const sync0 = await runSyncOwner({ session, ask, observe, stable, sleep, dir, profileName: name0,
        openSettings: () => reopen('gear'), closeSettings: close });
      sync = sync0;
      outcome['sync-owner-paths'] = sync0.status;
      if (sync0.reason) reasons['sync-owner-paths'] = sync0.reason;

      // 1 (rest): the backdrop dismisses the dialog; last, because a refusal here ends the helper (which is why
      // the sync-owner observations come first). A refusal is recorded as unrun, not guessed as a failure.
      const failed = Object.values(outcome).includes('fail');
      if (!isOpen(await stable())) { reasons['settings-entries-tabs'] = 'Settings was not open for the backdrop press'; return finish(true); }
      try { await session.input('settings.backdrop'); }
      catch (error) {
        if (!(error instanceof OperatorInputError)) throw error;
        reasons['settings-entries-tabs'] = 'the helper refused the backdrop press (' + (error.code || 'refused') + '); the other entry/tab/close observations stand';
        return finish(true);
      }
      if (isOpen(await stable())) return fail('settings-entries-tabs', 'backdrop did not dismiss Settings');
      if (!unreadPanes.length) outcome['settings-entries-tabs'] = 'pass';
      return finish(failed);
    } catch (error) {
      // Keep what was established before an input refusal or unusable evidence; the batch still stops.
      try { kit.privateEvidence(dir, 'partial', { schema: 'nemo.settings-profile-private/1', outcome, reasons, stoppedBy: error?.code || 'error' }); } catch { /* receipt directory unavailable */ }
      throw error;
    }
  } };
}
function prepare({ config }) { return createWorkflow({ reportDirectory: config.reportDirectory }); }
module.exports = { prepare, createWorkflow, CLAUSES, TABS };
