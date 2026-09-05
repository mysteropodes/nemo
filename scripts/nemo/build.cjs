#!/usr/bin/env node
'use strict';

const isolation = require('./lib/isolation.cjs');
const {
  buildHandshake,
  readBuildStatus,
  runBuildLauncher,
} = require('./lib/build-runtime.cjs');

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

async function start(args) {
  const taskId = isolation.resolveTaskId(args.task);
  await runBuildLauncher(taskId, {
    command: args.command,
    args: args.passthrough,
    hostTriple: args['host-triple'],
  }, output);
}

function status(args) {
  if (!args.task || !args.owner) throw new Error('status requires --task ID and --owner TOKEN');
  const result = buildHandshake(args.task, args.owner);
  output(result);
  process.exit(result.ok ? 0 : 1);
}

async function stop(args) {
  if (!args.task || !args.owner) throw new Error('stop requires --task ID and --owner TOKEN');
  const stopped = await isolation.requestStop(args.task, args.owner, {
    timeoutMs: args['timeout-ms'] == null ? 10_000 : Number(args['timeout-ms']),
  });
  const statusBeforeRelease = readBuildStatus(args.task);
  const released = stopped.stopped ? isolation.releaseTask(args.task, args.owner) : null;
  output({ ...stopped, runtime: statusBeforeRelease, released });
  process.exit(stopped.stopped && released && released.released ? 0 : 1);
}

async function main() {
  const args = parse(process.argv.slice(2));
  const command = args._[0] || 'start';
  if (command === 'start') return start(args);
  if (command === 'status') return status(args);
  if (command === 'stop') return stop(args);
  throw new Error('usage: build.cjs start [--task ID] [--command /absolute/test/stub -- args...] | status --task ID --owner TOKEN | stop --task ID --owner TOKEN [--timeout-ms N]');
}

main().catch((err) => { process.stderr.write((err.stack || String(err)) + '\n'); process.exit(1); });
