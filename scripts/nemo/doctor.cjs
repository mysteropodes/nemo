#!/usr/bin/env node
'use strict';
// npm run doctor — read-only report of source/build identity, platform,
// tool prerequisites and capabilities. Exit 0 always unless the report
// itself cannot be produced: absence of a tool is information here, it is
// the jobs that need the tool which report `blocked`.
const { parseArgs, runJobs } = require('./lib/cli.cjs');
const args = parseArgs(process.argv.slice(2));
const { receipt } = runJobs('doctor', ['doctor'], { json: args.json, quiet: true });
if (!args.json) {
  const s = receipt.source, b = receipt.build, p = receipt.platform, t = receipt.tools, c = receipt.capabilities;
  const yes = (v) => (v ? 'ok     ' : 'MISSING');
  const line = (k, v) => console.log(`  ${k.padEnd(22)} ${v}`);
  console.log('\nNemo doctor  (read-only)\n');
  console.log('Source');
  line('HEAD', `${s.head} (${s.branch}, ${s.describe})`);
  line('commit', `${s.commitDate}  ${s.subject}`);
  line('worktree', s.worktree);
  line('state', s.dirty ? `DIRTY — ${s.modifiedTracked} modified, ${s.untracked} untracked, digest ${s.dirtyDigest.slice(0, 12)}` : 'clean');
  console.log('Build');
  line('package.json', b.packageVersion);
  line('tauri.conf.json', b.tauriVersion);
  line('index.html fallback', `${b.indexHtmlTitleVersion} (title) / ${b.indexHtmlStatusVersion} (status bar)`);
  line('crates', Object.entries(b.crates).map(([k, v]) => `${k}@${v || '?'}`).join('  '));
  line('geometry_wasm', b.artifacts.geometryWasm.present ? `${b.artifacts.geometryWasm.bytes} B  ${b.artifacts.geometryWasm.sha256.slice(0, 12)}` : 'MISSING');
  line('vectorize_wasm', b.artifacts.vectorizeWasm.present ? `${b.artifacts.vectorizeWasm.bytes} B  ${b.artifacts.vectorizeWasm.sha256.slice(0, 12)}` : 'MISSING');
  line('ffmpeg sidecar', b.artifacts.ffmpegSidecar.present ? `${b.artifacts.ffmpegSidecar.path}  ${b.artifacts.ffmpegSidecar.bytes} B  ${b.artifacts.ffmpegSidecar.sha256.slice(0, 12)}` : `MISSING (${b.artifacts.ffmpegSidecar.path || b.artifacts.ffmpegSidecar.note})`);
  console.log('Platform');
  line('os', `${p.os} ${p.osRelease} ${p.arch}  (${p.osVersion || ''})`);
  line('cpu / memory', `${p.cpuModel} ×${p.cpuCount}, ${(p.memoryBytes / 2 ** 30).toFixed(0)} GiB`);
  line('node', p.node);
  line('rust host', b.hostTriple || 'unknown');
  console.log('Tools');
  for (const [k, v] of Object.entries(t)) line(k, `${yes(v.present)} ${v.version || v.note || ''}`);
  console.log('Capabilities');
  line('rust targets', c.rustTargets.join(' ') || 'none');
  line('wasm32 target', yes(c.wasm32Target));
  line('host target', yes(c.hostTargetInstalled));
  line('node_modules', yes(c.nodeModulesInstalled));
  const sc = c.ffmpegSidecar;
  line('sidecar runs', sc.present ? `${yes(sc.runs)} ${sc.runs ? sc.versionLine : sc.failure}` : 'n/a');
  if (sc.present && sc.licenseLine) line('sidecar license', `${sc.licenseLine.trim()}  gpl=${sc.enableGpl} videotoolbox=${sc.enableVideotoolbox}`);
  if (sc.dynamicLibs) line('sidecar dylibs', `${sc.dynamicLibs.external.length} external, ${sc.externalDylibsMissing} missing on this machine`);
  if (sc.dynamicLibs) for (const e of sc.dynamicLibs.external.filter((x) => !x.present)) line('', `MISSING ${e.path}`);
  line('gpu', c.gpu.status === 'ok' ? c.gpu.gpus.map((g) => `${g.model || '?'} ${g.metal ? '(' + g.metal + ')' : ''}`).join('; ') : `unknown — ${c.gpu.reason || ''}`);
  line('webgpu', 'unknown outside a browser');
  line('browser suite', `runner ${yes(c.browserSuite.runnerPresent)} specs ${yes(c.browserSuite.defined)}`);
  line('desktop suite', `harness ${yes(c.desktopSuite.defined)} app ${c.desktopSuite.appBundle || 'none built'}`);
  line('integration suite', yes(c.integrationSuite.defined));
  line('bench workloads', yes(c.benchWorkloads.defined));
  console.log(`\nreceipt reports/${receipt.runId}/receipt.json\n`);
}
process.exit(0);
