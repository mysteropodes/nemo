'use strict';
// One receipt per run. Schema `nemo.receipt/1`:
//   { schema, runId, command, profile, startedAt, finishedAt, durationMs,
//     source, build, platform, tools?, capabilities?, jobs: [Job], summary }
//   Job: { name, required, status: pass|fail|blocked|not-run, reason,
//          exitCode, durationMs, command, artifacts: [{path, sha256?, bytes?}],
//          limitations: [string], details?, log? }
// Status semantics (03_TESTING_AND_DEBUGGING.md):
//   pass     the job ran and its own success criterion held
//   fail     the job ran and it did not
//   blocked  a required environment/tool/artifact is missing; NEVER a skip
//   not-run  intentionally not attempted (no suite defined, not in profile)
const fs = require('node:fs');
const path = require('node:path');
const { ROOT, nowIso, compactStamp } = require('./util.cjs');
const identity = require('./identity.cjs');

const SCHEMA = 'nemo.receipt/1';
const STATUS = { PASS: 'pass', FAIL: 'fail', BLOCKED: 'blocked', NOT_RUN: 'not-run' };
const EXIT = { pass: 0, fail: 1, blocked: 2, 'not-run': 0 };

function create(command, opts = {}) {
  const source = identity.sourceIdentity();
  const build = identity.buildIdentity();
  const platform = identity.platformIdentity();
  const started = new Date();
  const runId = compactStamp(started) + '-' + (source.head ? source.head.slice(0, 7) : 'nogit') + (source.dirty ? '-dirty' : '');
  return {
    schema: SCHEMA,
    runId,
    command,
    profile: opts.profile || null,
    startedAt: started.toISOString(),
    finishedAt: null,
    durationMs: null,
    source, build, platform,
    tools: null,
    capabilities: null,
    jobs: [],
    limitations: [],
    summary: null,
  };
}

function reportDir(receipt) {
  return path.join(process.env.NEMO_REPORT_DIR || path.join(ROOT, 'reports'), receipt.runId);
}

function finalize(receipt) {
  const counts = { pass: 0, fail: 0, blocked: 0, 'not-run': 0 };
  for (const j of receipt.jobs) counts[j.status] = (counts[j.status] || 0) + 1;
  let overall = STATUS.PASS;
  if (receipt.jobs.some((j) => j.status === STATUS.FAIL)) overall = STATUS.FAIL;
  else if (receipt.jobs.some((j) => j.status === STATUS.BLOCKED && j.required)) overall = STATUS.BLOCKED;
  else if (receipt.jobs.length && receipt.jobs.every((j) => j.status === STATUS.NOT_RUN)) overall = STATUS.NOT_RUN;
  receipt.finishedAt = nowIso();
  receipt.durationMs = new Date(receipt.finishedAt) - new Date(receipt.startedAt);
  receipt.summary = { overall, exitCode: EXIT[overall], counts };
  return receipt;
}

function write(receipt) {
  const dir = reportDir(receipt);
  fs.mkdirSync(dir, { recursive: true });
  const stripped = JSON.parse(JSON.stringify(receipt));
  for (const j of stripped.jobs) {
    if (j.log != null) {
      const logFile = path.join(dir, j.name.replace(/[^a-z0-9_-]+/gi, '_') + '.log');
      fs.writeFileSync(logFile, j.log);
      j.logPath = path.relative(ROOT, logFile);
      delete j.log;
    }
  }
  const jsonPath = path.join(dir, 'receipt.json');
  fs.writeFileSync(jsonPath, JSON.stringify(stripped, null, 2) + '\n');
  const mdPath = path.join(dir, 'receipt.md');
  fs.writeFileSync(mdPath, renderMarkdown(stripped));
  return { dir, jsonPath, mdPath };
}

function shortSha(s) { return s ? s.slice(0, 12) : 'n/a'; }

function renderMarkdown(r) {
  const s = r.source, b = r.build, p = r.platform;
  const lines = [];
  lines.push(`# Nemo receipt \`${r.runId}\``);
  lines.push('');
  lines.push(`- Command: \`${r.command}\`${r.profile ? ` (profile \`${r.profile}\`)` : ''}`);
  lines.push(`- Overall: **${r.summary ? r.summary.overall : 'incomplete'}** (exit ${r.summary ? r.summary.exitCode : '?'}) — ${r.summary ? Object.entries(r.summary.counts).map(([k, v]) => `${k} ${v}`).join(', ') : ''}`);
  lines.push(`- Started: ${r.startedAt}; finished: ${r.finishedAt}; ${r.durationMs} ms`);
  lines.push(`- Source: \`${shortSha(s.head)}\` on \`${s.branch}\` (${s.describe})${s.dirty ? ` — **dirty**, ${s.modifiedTracked} modified, ${s.untracked} untracked, digest \`${shortSha(s.dirtyDigest)}\`` : ' — clean'}`);
  lines.push(`- Build: package \`${b.packageVersion}\`, tauri.conf \`${b.tauriVersion}\`, index.html fallback \`${b.indexHtmlTitleVersion}\`/\`${b.indexHtmlStatusVersion}\`; geometry_wasm \`${shortSha(b.artifacts.geometryWasm.sha256)}\`, sidecar ${b.artifacts.ffmpegSidecar.present ? '`' + shortSha(b.artifacts.ffmpegSidecar.sha256) + '`' : 'absent'}`);
  lines.push(`- Platform: ${p.os} ${p.osRelease} ${p.arch}, ${p.cpuModel} ×${p.cpuCount}, ${(p.memoryBytes / 2 ** 30).toFixed(0)} GiB, node ${p.node}`);
  lines.push('');
  lines.push('| Job | Required | Result | Reason / evidence | Limitation |');
  lines.push('|---|---|---|---|---|');
  for (const j of r.jobs) {
    const ev = [j.reason, j.logPath ? `log \`${j.logPath}\`` : null].concat((j.artifacts || []).map((a) => `\`${a.path}\`${a.sha256 ? ' ' + shortSha(a.sha256) : ''}`)).filter(Boolean).join('; ');
    lines.push(`| \`${j.name}\` | ${j.required ? 'yes' : 'no'} | **${j.status}** | ${ev.replace(/\|/g, '\\|')} | ${(j.limitations || []).join('; ').replace(/\|/g, '\\|')} |`);
  }
  if (r.limitations && r.limitations.length) {
    lines.push('');
    lines.push('Run limitations:');
    for (const l of r.limitations) lines.push(`- ${l}`);
  }
  lines.push('');
  return lines.join('\n');
}

function printSummary(receipt, written) {
  const r = receipt;
  const pad = (s, n) => String(s).padEnd(n);
  console.log('');
  console.log(`nemo ${r.command}${r.profile ? ' --profile ' + r.profile : ''}  run ${r.runId}`);
  console.log(`source ${shortSha(r.source.head)} ${r.source.branch}${r.source.dirty ? ' (dirty ' + shortSha(r.source.dirtyDigest) + ')' : ' (clean)'}  build ${r.build.packageVersion}  ${r.platform.os}/${r.platform.arch}`);
  console.log('');
  for (const j of r.jobs) {
    console.log(`  ${pad(j.status.toUpperCase(), 8)} ${pad(j.name, 18)} ${((j.durationMs || 0) / 1000).toFixed(1).padStart(7)}s  ${j.reason || ''}`);
    for (const l of j.limitations || []) console.log(`           ${pad('', 18)}          ! ${l}`);
  }
  console.log('');
  console.log(`overall ${r.summary.overall.toUpperCase()} (exit ${r.summary.exitCode})  pass ${r.summary.counts.pass}  fail ${r.summary.counts.fail}  blocked ${r.summary.counts.blocked}  not-run ${r.summary.counts['not-run']}`);
  if (written) console.log(`receipt ${path.relative(ROOT, written.jsonPath)}`);
}

module.exports = { SCHEMA, STATUS, EXIT, create, finalize, write, renderMarkdown, printSummary, reportDir };
