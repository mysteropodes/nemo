#!/usr/bin/env node
'use strict';
// One-shot CLI for the native (Tauri) task-instance launcher — R06, #902.
// Same shape as build.cjs: `start` is long-lived and prints one JSON line
// containing the owner token; `status` and `stop` are run by the controlling
// process that kept that token.

const isolation = require('./lib/isolation.cjs');
const {
  nativeHandshake,
  readAppManifest,
  readNativeStatus,
  releaseNativeState,
  runNativeLauncher,
} = require('./lib/native-runtime.cjs');
const { launcherProcessIdentity, nativeProcessTreeStopped } = require('./lib/native-process.cjs');

function parse(argv) {
  const out = { _: [], passthrough: [] };
  let passthrough = false;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (passthrough) { out.passthrough.push(arg); continue; }
    if (arg === '--') { passthrough = true; continue; }
    if (!arg.startsWith('--')) { out._.push(arg); continue; }
    const key = arg.slice(2); const next = argv[i + 1];
    if (next == null || next.startsWith('--')) out[key] = true;
    else { out[key] = next; i += 1; }
  }
  return out;
}

function output(value) { process.stdout.write(JSON.stringify(value) + '\n'); }

function reservations(value) {
  if (value == null || value === true) return [];
  return String(value).split(',').map((slot) => slot.trim()).filter(Boolean);
}

async function start(args) {
  const taskId = isolation.resolveTaskId(args.task);
  await runNativeLauncher(taskId, {
    app: args.app === true ? undefined : args.app,
    executable: args.executable === true ? undefined : args.executable,
    args: args.passthrough,
    hostTriple: args['host-triple'],
    reserve: reservations(args.reserve),
    manifestTimeoutMs: args['manifest-timeout-ms'] == null ? undefined : Number(args['manifest-timeout-ms']),
  }, output);
}

function status(args) {
  if (!args.task || !args.owner) throw new Error('status requires --task ID and --owner TOKEN');
  const result = nativeHandshake(args.task, args.owner);
  output(result);
  process.exit(result.ok ? 0 : 1);
}

async function stop(args) {
  if (!args.task || !args.owner) throw new Error('stop requires --task ID and --owner TOKEN');
  if (Object.hasOwn(args, 'retain-data') && args['retain-data'] !== true) {
    throw new Error('--retain-data is a flag and takes no value');
  }
  // Read the app's own manifest BEFORE stopping: it names the directories the
  // instance actually resolved, and it lives in the tauri-data root that the
  // release below removes.
  const disclosed = readAppManifest(args.task);
  const statusBeforeStop = readNativeStatus(args.task);
  const stopped = await isolation.requestStop(args.task, args.owner, {
    timeoutMs: args['timeout-ms'] == null ? 10_000 : Number(args['timeout-ms']),
    retainRecord: true,
    verifyProcess: (record) => statusBeforeStop && statusBeforeStop.taskId === args.task
      && statusBeforeStop.launcherPid === record.pid && !!statusBeforeStop.launcherIdentity
      && statusBeforeStop.launcherIdentity === launcherProcessIdentity(record.pid),
  });
  const statusBeforeRelease = readNativeStatus(args.task) || statusBeforeStop;
  const retainedData = args['retain-data'] === true;
  let appState = null; let released = null; let processTree = null;
  if (stopped.stopped) {
    try {
      released = isolation.releaseTask(args.task, args.owner, {
        requireOwner: true, retainData: retainedData, releaseManagedSlots: true,
        beforeRelease: (record) => {
          processTree = nativeProcessTreeStopped(args.task, record, statusBeforeRelease);
          if (!processTree.stopped) throw new Error(processTree.reason);
          if (retainedData) return;
          appState = releaseNativeState(args.task, disclosed && disclosed.valid ? disclosed.manifest : null,
            statusBeforeRelease && statusBeforeRelease.app ? statusBeforeRelease.app.bundle : null);
          if (appState.removed.some((item) => !item.removed && item.reason !== 'already absent')) {
            throw new Error('app state cleanup incomplete; ownership and task records retained for retry');
          }
        },
      });
    } catch (err) { released = { released: false, reason: err.message }; }
  }
  const ok = stopped.stopped && released && released.released;
  output({ ...stopped, stopped: !!ok, reason: ok ? stopped.reason : (released ? released.reason : stopped.reason),
    runtime: statusBeforeRelease, processTree, retainedData: !!ok && retainedData, appState, released });
  process.exit(ok ? 0 : 1);
}

async function main() {
  const args = parse(process.argv.slice(2));
  const command = args._[0] || 'start';
  if (command === 'start') return start(args);
  if (command === 'status') return status(args);
  if (command === 'stop') return stop(args);
  throw new Error('usage: native.cjs start [--task ID] [--app /path/Nemo.app | --executable /absolute/path] [--reserve desktop-input,gpu-reference] [--manifest-timeout-ms N] [-- args...] | status --task ID --owner TOKEN | stop --task ID --owner TOKEN [--timeout-ms N] [--retain-data]');
}

main().catch((err) => { process.stderr.write((err.stack || String(err)) + '\n'); process.exit(1); });
