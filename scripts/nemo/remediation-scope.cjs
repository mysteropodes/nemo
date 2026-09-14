'use strict';

const fs = require('node:fs');
const path = require('node:path');
const build = require('./lib/remediation-scope-build.cjs');
const { digest, verifyScope } = require('./lib/remediation-scope-verify.cjs');

const INDEX = 'engineering/inventory/remediation-scope.json';
const USAGE = 'usage: remediation-scope.cjs [--integrity-only] [--source FULL_SHA]\n'
  + '       remediation-scope.cjs --refreeze FULL_SHA --id LEAF --issue N --reason TEXT';

function parseArgs(args) {
  const options = { flags: new Set(), values: {} };
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === '--integrity-only') {
      if (options.flags.has(arg)) throw new Error(USAGE);
      options.flags.add(arg);
    } else if (['--source', '--refreeze', '--id', '--issue', '--reason'].includes(arg)) {
      if (i + 1 >= args.length || arg in options.values) throw new Error(USAGE);
      options.values[arg] = args[i + 1];
      i += 1;
    } else throw new Error(USAGE);
  }
  const refreeze = '--refreeze' in options.values;
  if (refreeze && (options.flags.size || '--source' in options.values
    || !['--id', '--issue', '--reason'].every((key) => key in options.values))) throw new Error(USAGE);
  if (!refreeze && ['--id', '--issue', '--reason'].some((key) => key in options.values)) throw new Error(USAGE);
  return options;
}

function readIndex(root) {
  return JSON.parse(fs.readFileSync(path.join(root, INDEX), 'utf8'));
}

function runRefreeze(root, values) {
  const previous = readIndex(root);
  const issue = Number(values['--issue']);
  if (!Number.isInteger(issue) || issue <= 0) throw new Error('--issue must be a positive integer');
  const scope = build.refreeze(root, previous, { commit: values['--refreeze'], id: values['--id'], issue, reason: values['--reason'] });
  fs.writeFileSync(path.join(root, INDEX), `${JSON.stringify(scope, null, 2)}\n`);
  const amendment = scope.amendments[scope.amendments.length - 1];
  const known = new Set(previous.files.map((entry) => entry.path));
  const classified = scope.files.filter((entry) => !known.has(entry.path))
    .map((entry) => ({ path: entry.path, classification: entry.classification }));
  const result = build.inspect(root, scope);
  process.stdout.write(`${JSON.stringify({ refreeze: amendment.commit, added: amendment.added.length,
    removed: amendment.removed.length, modified: amendment.modified.length, classified, ...result }, null, 2)}\n`);
  return result.integrity === 'pass' ? 0 : 1;
}

function main(args = process.argv.slice(2), root = path.resolve(__dirname, '../..')) {
  try {
    const { flags, values } = parseArgs(args);
    if ('--refreeze' in values) return runRefreeze(root, values);
    const result = build.inspect(root, readIndex(root), values['--source']);
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    return result.integrity === 'pass' && (flags.has('--integrity-only') || result.complete) ? 0 : 1;
  } catch (error) {
    process.stderr.write(`remediation scope check failed: ${error.message}\n`);
    return 1;
  }
}

if (require.main === module) process.exitCode = main();
module.exports = { digest, trackedSource: build.trackedSource, verifyScope, inspect: build.inspect, refreeze: build.refreeze, main };
