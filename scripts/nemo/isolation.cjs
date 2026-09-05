#!/usr/bin/env node
'use strict';
// node scripts/nemo/isolation.cjs <command> [flags] — inspect/exercise R06
// task-runtime isolation (scripts/nemo/lib/isolation.cjs) from the shell.
//
// This CLI is a thin, one-shot wrapper for debugging and scripting. The
// meaningful, testable behavior — real port binding, root separation, the
// owner/source handshake, non-owner stop refusal — lives in the library and
// is proven in isolation.test.cjs; a one-shot process cannot itself hold a
// long-lived port reservation past its own exit (see `alloc --port` note
// below and engineering/runtime-isolation.md).
const path = require('node:path');
const iso = require('./lib/isolation.cjs');

function parseArgs(argv) {
  const out = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith('--')) {
      const key = a.slice(2);
      const next = argv[i + 1];
      if (next !== undefined && !next.startsWith('--')) { out[key] = next; i++; }
      else out[key] = true;
    } else out._.push(a);
  }
  return out;
}

function printJson(obj) { process.stdout.write(JSON.stringify(obj, null, 2) + '\n'); }

function usage() {
  console.error([
    'usage: isolation.cjs <command> [flags]',
    '',
    'commands:',
    '  alloc     [--task ID] [--label TEXT] [--pid N] [--port]   allocate roots (+ launcher, + a port if --port)',
    '  roots     --task ID                                       print the root paths for an existing/new task id',
    '  handshake --task ID --owner TOKEN [--check-source]      verify a launcher is alive and matches',
    '  stop      --task ID --owner TOKEN [--signal SIG] [--timeout-ms N] stop a task; refused unless --owner matches',
    '  release   --task ID [--owner TOKEN]                       remove task roots only after exit; existing records require their owner',
    '  slot-acquire --slot NAME --task ID [--pid N]              acquire a named exclusive slot (desktop input, GPU bench, ...)',
    '  slot-release --slot NAME --owner TOKEN                    release a named exclusive slot',
  ].join('\n'));
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const cmd = args._[0];
  if (!cmd) { usage(); process.exit(64); }

  if (cmd === 'alloc') {
    const taskId = iso.resolveTaskId(args.task);
    const roots = iso.taskRoots(taskId);
    const launcher = iso.registerLauncher(taskId, {
      pid: args.pid ? Number(args.pid) : process.ppid, // the invoking shell/script, not this short-lived CLI process
      label: args.label || null,
    });
    let port = null;
    if (args.port) {
      // NOTE: this process exits right after printing, which releases the
      // bound socket immediately — useful to confirm a port is free *right
      // now*, not as a standing reservation. A real standing reservation
      // needs `reservePort()` called from the long-lived owning process
      // (see engineering/runtime-isolation.md).
      const reserved = await iso.reservePort(taskId, {});
      port = reserved.port;
      await reserved.release();
    }
    printJson({
      taskId,
      ownerToken: launcher.ownerToken,
      pid: launcher.pid,
      roots: relRoots(roots),
      port,
      env: {
        NEMO_TASK_ID: taskId,
        NEMO_REPORT_DIR: roots.reports,
      },
    });
    return;
  }

  if (cmd === 'roots') {
    if (!args.task) { console.error('roots: --task is required'); process.exit(64); }
    printJson({ taskId: args.task, roots: relRoots(iso.taskRoots(args.task)) });
    return;
  }

  if (cmd === 'handshake') {
    if (!args.task) { console.error('handshake: --task is required'); process.exit(64); }
    const result = iso.verifyHandshake(args.task, { ownerToken: args.owner, checkSource: !!args['check-source'] });
    printJson(result);
    process.exit(result.ok ? 0 : 1);
  }

  if (cmd === 'stop') {
    if (!args.task || !args.owner) { console.error('stop: --task and --owner are required'); process.exit(64); }
    const result = await iso.requestStop(args.task, args.owner, { signal: args.signal, timeoutMs: args['timeout-ms'] === undefined ? undefined : Number(args['timeout-ms']) });
    printJson(result);
    process.exit(result.stopped ? 0 : 1);
  }

  if (cmd === 'release') {
    if (!args.task) { console.error('release: --task is required'); process.exit(64); }
    const result = iso.releaseTask(args.task, args.owner);
    printJson(result);
    process.exit(result.released ? 0 : 1);
    return;
  }

  if (cmd === 'slot-acquire') {
    if (!args.slot || !args.task) { console.error('slot-acquire: --slot and --task are required'); process.exit(64); }
    const result = iso.acquireExclusiveSlot(args.slot, args.task, { pid: args.pid ? Number(args.pid) : process.ppid });
    printJson({ acquired: result.acquired, ownerToken: result.ownerToken || null, reason: result.reason || null, holder: result.holder || null });
    process.exit(result.acquired ? 0 : 1);
  }

  if (cmd === 'slot-release') {
    if (!args.slot || !args.owner) { console.error('slot-release: --slot and --owner are required'); process.exit(64); }
    const result = iso.releaseExclusiveSlot(args.slot, args.owner);
    printJson(result);
    process.exit(result.released ? 0 : 1);
  }

  usage();
  process.exit(64);
}

function relRoots(roots) {
  const out = {};
  for (const [k, v] of Object.entries(roots)) out[k] = path.normalize(v);
  return out;
}

main().catch((err) => {
  console.error(err.stack || String(err));
  process.exit(1);
});
