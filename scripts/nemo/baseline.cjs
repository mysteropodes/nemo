#!/usr/bin/env node
'use strict';
// P02 — adopt and check the current-state comparison manifest.
//
//   node scripts/nemo/baseline.cjs --adopt [--from <runId|path>,...] [--note "..."]
//                                          [--ignore-worktree]
//   node scripts/nemo/baseline.cjs --check [--from <runId|path>] [--json]
//
// --ignore-worktree records the committed tree only, for adopting a baseline
// in the same change that introduces it. It is recorded, not hidden:
// `references.source.worktreeIgnored` stays in the manifest.
//
// --adopt records the selected current state as the comparison baseline in
// engineering/inventory/baseline-manifest.json. --check classifies a receipt
// against it: new regression vs unchanged known failure vs environment
// mismatch, with every immutable reference verified first.
//
// Exit: 0 nothing to answer for, 1 a blocking verdict (new/changed/newly
// observed failure), 2 inconclusive (stale reference, environment mismatch,
// or an uncomparable job).
const fs = require('node:fs');
const path = require('node:path');
const { ROOT, exists, readJson } = require('./lib/util.cjs');
const baseline = require('./lib/baseline.cjs');

function reportsDir() {
  return process.env.NEMO_REPORT_DIR || path.join(ROOT, 'reports');
}

// Accepts a runId, a report directory, or a direct path to a receipt.json.
function resolveReceipt(ref) {
  const candidates = [ref, path.join(ref, 'receipt.json'), path.join(reportsDir(), ref, 'receipt.json'), path.join(ROOT, ref), path.join(ROOT, ref, 'receipt.json')];
  for (const c of candidates) {
    if (exists(c) && fs.statSync(c).isFile()) return path.resolve(c);
  }
  throw new Error(`no receipt found for "${ref}"`);
}

function latestReceipt() {
  const dir = reportsDir();
  if (!exists(dir)) throw new Error(`no reports directory at ${path.relative(ROOT, dir)} — run \`npm run verify\` first`);
  const runs = fs.readdirSync(dir).filter((d) => exists(path.join(dir, d, 'receipt.json'))).sort();
  if (!runs.length) throw new Error('no receipts found; run `npm run verify` first');
  return path.join(dir, runs[runs.length - 1], 'receipt.json');
}

function parse(argv) {
  const out = { adopt: false, check: false, from: null, note: null, json: false, manifest: null, out: null, ignoreWorktree: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--adopt') out.adopt = true;
    else if (a === '--check') out.check = true;
    else if (a === '--json') out.json = true;
    else if (a === '--ignore-worktree') out.ignoreWorktree = true;
    else if (a === '--from') out.from = argv[++i];
    else if (a.startsWith('--from=')) out.from = a.slice(7);
    else if (a === '--note') out.note = argv[++i];
    else if (a.startsWith('--note=')) out.note = a.slice(7);
    else if (a === '--manifest') out.manifest = argv[++i];
    else if (a.startsWith('--manifest=')) out.manifest = a.slice(11);
    else if (a === '--out') out.out = argv[++i];
    else if (a.startsWith('--out=')) out.out = a.slice(6);
    else { console.error(`unknown argument "${a}"`); process.exit(64); }
  }
  return out;
}

function main() {
  const args = parse(process.argv.slice(2));
  if (args.adopt === args.check) {
    console.error('usage: baseline.cjs --adopt [--from a,b] [--note "..."] | --check [--from run] [--json]');
    process.exit(64);
  }

  if (args.adopt) {
    const refs = args.from ? args.from.split(',').map((s) => s.trim()).filter(Boolean) : [latestReceipt()];
    const inputs = refs.map((r) => {
      const receiptPath = resolveReceipt(r);
      return { receipt: readJson(receiptPath), receiptPath };
    });
    const manifest = baseline.adopt(inputs, {
      note: args.note,
      references: baseline.currentReferences({ ignoreWorktree: args.ignoreWorktree }),
    });
    const target = args.out ? path.resolve(args.out) : baseline.MANIFEST_PATH;
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, JSON.stringify(manifest, null, 2) + '\n');
    if (args.json) { process.stdout.write(JSON.stringify(manifest, null, 2) + '\n'); return 0; }
    console.log(`adopted ${manifest.entries.length} entr${manifest.entries.length === 1 ? 'y' : 'ies'} at ${String(manifest.references.source.head).slice(0, 12)} -> ${path.relative(ROOT, target)}`);
    for (const e of manifest.entries) {
      console.log(`  ${e.status.padEnd(8)} ${e.job.padEnd(18)} ${e.carriedOver ? '[carried over] ' : ''}${e.case.reason || ''}`);
    }
    const carried = manifest.summary.carriedOver;
    if (carried.length) console.log(`\n${carried.length} entr${carried.length === 1 ? 'y' : 'ies'} carried over from another source: ${carried.join(', ')}`);
    return 0;
  }

  const manifest = baseline.loadManifest(args.manifest ? path.resolve(args.manifest) : null);
  const receiptPath = args.from ? resolveReceipt(args.from) : latestReceipt();
  const receipt = readJson(receiptPath);
  const cmp = baseline.compare(manifest, receipt, { receiptPath });
  process.stdout.write(args.json ? JSON.stringify(cmp, null, 2) + '\n' : baseline.renderComparison(cmp));
  return cmp.summary.exitCode;
}

try {
  process.exit(main());
} catch (e) {
  console.error(String((e && e.message) || e));
  process.exit(64);
}
