#!/usr/bin/env node
'use strict';

const isolation = require('./lib/isolation.cjs');
const { IDENTITY_PATH, startBrowserRuntime } = require('./lib/browser-runtime.cjs');

function parse(argv) {
  const out = { _: [] };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg.startsWith('--')) { out._.push(arg); continue; }
    const key = arg.slice(2); const next = argv[i + 1];
    if (next == null || next.startsWith('--')) out[key] = true;
    else { out[key] = next; i += 1; }
  }
  return out;
}

function output(value) { process.stdout.write(JSON.stringify(value) + '\n'); }

async function start(args) {
  const runtime = await startBrowserRuntime({
    taskId: args.task, host: args.host, port: args.port,
    browser: args.browser, headless: !!args.headless,
  });
  output({
    started: true, taskId: runtime.taskId, pid: process.pid,
    origin: runtime.origin, url: runtime.origin + '/', identityUrl: runtime.origin + IDENTITY_PATH,
    ownerToken: runtime.ownerToken, roots: runtime.roots,
    browser: {
      requested: !!args.browser, integrated: !!runtime.browserChild,
      pid: runtime.browserChild ? runtime.browserChild.pid : null,
      profileDir: runtime.roots.browserProfile, error: runtime.browserError,
    },
  });
  let closing = false;
  const shutdown = async () => {
    if (closing) return; closing = true;
    await runtime.close().catch(() => {}); process.exit(0);
  };
  process.once('SIGTERM', shutdown); process.once('SIGINT', shutdown);
}

async function stop(args) {
  if (!args.task || !args.owner) throw new Error('stop requires --task ID and --owner TOKEN');
  const stopped = await isolation.requestStop(args.task, args.owner, {
    signal: args.signal, timeoutMs: args['timeout-ms'] == null ? undefined : Number(args['timeout-ms']),
  });
  const released = stopped.stopped ? isolation.releaseTask(args.task, args.owner) : null;
  output({ ...stopped, released });
  process.exit(stopped.stopped && released && released.released ? 0 : 1);
}

async function status(args) {
  if (!args.task || !args.owner) throw new Error('status requires --task ID and --owner TOKEN');
  const result = isolation.verifyHandshake(args.task, { ownerToken: args.owner, checkSource: true });
  output({ ok: result.ok, reason: result.reason, taskId: args.task, pid: result.launcher ? result.launcher.pid : null });
  process.exit(result.ok ? 0 : 1);
}

async function main() {
  const args = parse(process.argv.slice(2)); const command = args._[0] || 'start';
  if (command === 'start') return start(args);
  if (command === 'stop') return stop(args);
  if (command === 'status') return status(args);
  throw new Error('usage: browser.cjs start [--task ID] [--host 127.0.0.1|::1] [--port N] [--browser auto|/path] [--headless] | stop --task ID --owner TOKEN | status --task ID --owner TOKEN');
}

main().catch((err) => { process.stderr.write((err.stack || String(err)) + '\n'); process.exit(1); });
