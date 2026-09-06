'use strict';

// This is an opt-in REAL packaged-app gate, never part of the fast stub suite.
// Native process/manifest/storage checks do not prove UI save/reload or pixels.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { test } = require('node:test');
const { isDeepStrictEqual } = require('node:util');
const { requireCheck, until, hash, packagedApp, createController, validateSnapshot, disjoint } = require('./native-harness.cjs');
const root = path.resolve(__dirname, '../..');
const isolation = require('../../scripts/nemo/lib/isolation.cjs');
const runtime = require('../../scripts/nemo/lib/native-runtime.cjs');
const identity = require('../../scripts/nemo/lib/identity.cjs');

const slots = ['desktop-input', 'gpu-reference'];

function markers(instance, targets) {
  const roots = instance.snapshot.runtime.roots;
  const dirs = [roots.temp, roots.cache, roots.reports, instance.snapshot.app.dirs.appData,
    targets.find(target => target.name === 'webkitStore').dir];
  return [...new Set(dirs)].map(dir => {
    fs.mkdirSync(dir, { recursive: true });
    const file = path.join(dir, 'nemo-r06-harness-sentinel.txt');
    const content = crypto.randomBytes(24).toString('hex');
    fs.writeFileSync(file, content, { flag: 'wx', mode: 0o600 });
    return { file, digest: hash(file) };
  });
}

function retained(markersToCheck) {
  requireCheck(markersToCheck.every(marker => fs.existsSync(marker.file) && hash(marker.file) === marker.digest), 'isolated storage sentinel changed or disappeared');
}

function reservationProbe(task, held) {
  for (const slot of slots) {
    const claim = isolation.acquireExclusiveSlot(slot, task, { pid: process.pid });
    if (claim.acquired) claim.release();
    requireCheck(claim.acquired === !held, held ? 'reserved desktop/GPU resource was not exclusive' : 'owned desktop/GPU reservation was not released');
  }
}

async function snapshot(controller, instance, context) {
  const result = await controller.status(instance);
  requireCheck(result.code === 0, 'native status failed');
  const targets = validateSnapshot(result.value, instance, context);
  const webkit = targets.find(target => target.name === 'webkitStore');
  await until(() => fs.existsSync(webkit.dir), 'actual WebKit store was not materialized');
  return targets;
}

function isolated(a, b, aTargets, bTargets) {
  requireCheck(a.snapshot.app.pid !== b.snapshot.app.pid && a.info.pid !== b.info.pid, 'two native instances share process identity');
  requireCheck(a.snapshot.app.identifier !== b.snapshot.app.identifier && a.snapshot.app.dataStoreIdentifier !== b.snapshot.app.dataStoreIdentifier, 'two native instances share application or WebKit identity');
  const own = instance => Object.values(instance.snapshot.runtime.roots);
  disjoint(own(a).map(dir => fs.realpathSync(dir)), own(b).map(dir => fs.realpathSync(dir)), 'two native instances share physical task roots');
  disjoint(aTargets.map(target => target.dir), bTargets.map(target => target.dir), 'two native instances share application state targets');
  const physical = targets => targets.filter(target => fs.existsSync(target.dir)).map(target => fs.realpathSync(target.dir));
  disjoint(physical(aTargets), physical(bTargets), 'two native instances share physical application storage');
}

async function rejectForeignOwner(controller, a, b) {
  const wrongStatus = await controller.status(a, b.info.ownerToken);
  requireCheck(wrongStatus.code !== 0 && wrongStatus.value && wrongStatus.value.ok === false, 'native status accepted another instance owner');
  const wrongStop = await controller.command(['stop', '--task', a.task, '--owner', b.info.ownerToken]);
  requireCheck(wrongStop.code !== 0 && wrongStop.value && wrongStop.value.stopped === false, 'native stop accepted another instance owner');
  requireCheck((await controller.status(a)).value.ok === true && (await controller.status(b)).value.ok === true, 'refused owner operation affected a live instance');
}

function removed(instance, targets) {
  requireCheck(!fs.existsSync(instance.info.roots.root), 'default native stop retained the task root');
  requireCheck(targets.every(target => !fs.existsSync(target.dir)), 'default native stop retained isolated app or WebKit state');
}

test('two real packaged native instances isolate state, retain/relaunch, and release owned resources', { timeout: 240_000 }, async t => {
  let controller;
  let stage = 'prerequisites';
  try {
    const app = packagedApp(process.env.NEMO_DESKTOP_APP, runtime);
    const source = identity.sourceIdentity();
    const build = identity.buildIdentity();
    requireCheck(source.head && !source.dirty, 'desktop prerequisite: commit the candidate before packaged acceptance');
    requireCheck(source.worktree && fs.realpathSync(source.worktree) === fs.realpathSync(root), 'desktop source identity is not this checkout');
    const context = { isolation, runtime, app, source, build };
    const runId = `desktop-${Date.now().toString(36)}-${crypto.randomBytes(6).toString('hex')}`;
    t.diagnostic(JSON.stringify({ schema: 'nemo.desktop-harness-run/1', taskIds: [`${runId}-a`, `${runId}-b`] }));
    controller = createController({ root, app: app.bundle });

    stage = 'concurrent startup and identity';
    let a = await controller.start(`${runId}-a`, slots);
    const b = await controller.start(`${runId}-b`);
    let aTargets = await snapshot(controller, a, context);
    const bTargets = await snapshot(controller, b, context);
    requireCheck(isDeepStrictEqual(a.info.reservations, slots) && b.info.reservations.length === 0, 'native launch did not declare expected reservations');
    isolated(a, b, aTargets, bTargets);
    const aMarkers = markers(a, aTargets);
    const bMarkers = markers(b, bTargets);

    stage = 'foreign owner and reservation refusal';
    await rejectForeignOwner(controller, a, b);
    reservationProbe(`${runId}-probe`, true);
    retained(aMarkers); retained(bMarkers);

    stage = 'same-task retained stop';
    const prior = a;
    const stop = await controller.stop(a, true);
    requireCheck(stop.retainedData === true && stop.appState === null && stop.released && stop.released.released === true && stop.released.retainedData === true, 'native stop did not implement explicit retain-data contract');
    retained(aMarkers); retained(bMarkers);
    await snapshot(controller, b, context);
    reservationProbe(`${runId}-probe`, false);

    stage = 'same-task relaunch';
    a = await controller.start(prior.task, slots);
    aTargets = await snapshot(controller, a, context);
    requireCheck(a.snapshot.app.pid !== prior.snapshot.app.pid && a.info.pid !== prior.info.pid, 'same-task relaunch did not create new native processes');
    requireCheck(a.snapshot.app.dataStoreIdentifier === prior.snapshot.app.dataStoreIdentifier && isDeepStrictEqual(a.snapshot.runtime.roots, prior.snapshot.runtime.roots) && isDeepStrictEqual(a.snapshot.app.dirs, prior.snapshot.app.dirs), 'same-task relaunch changed storage identity');
    retained(aMarkers); retained(bMarkers);
    isolated(a, b, aTargets, bTargets);
    reservationProbe(`${runId}-probe`, true);

    stage = 'independent cleanup';
    await controller.stop(a);
    removed(a, aTargets);
    retained(bMarkers);
    await snapshot(controller, b, context);
    reservationProbe(`${runId}-probe`, false);
    await controller.stop(b);
    removed(b, bTargets);
    requireCheck(hash(app.executable) === app.executableSha256 && isDeepStrictEqual(identity.sourceIdentity(), source) && isDeepStrictEqual(identity.buildIdentity(), build), 'source/build/app bytes changed during native acceptance');

    const report = {
      schema: 'nemo.desktop-harness/1', result: 'pass', sourceSha: source.head,
      executableSha256: app.executableSha256, appVersion: build.tauriVersion,
      platform: process.platform, architecture: process.arch, osRelease: os.release(),
      checks: ['two native process groups', 'owner/source/build/manifest identities', 'actual child process environment', 'disjoint app and materialized WebKit roots', 'foreign owner refusal', 'desktop-input and gpu-reference reservations', 'same-task retained filesystem storage', 'independent owner cleanup'],
      limitations: ['No UI document save/reload or rendering acceptance', 'Launcher source/build identity and executable hash do not establish embedded app source provenance'],
    };
    if (process.env.NEMO_DESKTOP_REPORT_DIR) {
      fs.mkdirSync(process.env.NEMO_DESKTOP_REPORT_DIR, { recursive: true });
      fs.writeFileSync(path.join(process.env.NEMO_DESKTOP_REPORT_DIR, 'packaged-native.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
    }
    t.diagnostic(JSON.stringify(report));
  } catch (error) {
    // Native errors may contain bundle paths. Only named stages are shareable.
    const safe = new Error(`packaged native acceptance failed during ${stage}; inspect the owned run locally`);
    if (stage === 'prerequisites' && error.message.startsWith('desktop prerequisite:')) safe.message = error.message;
    safe.stack = `Error: ${safe.message}`;
    throw safe;
  } finally {
    if (controller) {
      try { await controller.cleanup(); }
      catch {
        const error = new Error('packaged native cleanup incomplete; reconcile this owned run before retrying');
        error.stack = `Error: ${error.message}`;
        throw error;
      }
    }
  }
});
