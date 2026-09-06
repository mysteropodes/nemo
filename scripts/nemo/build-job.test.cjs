'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const { once } = require('node:events');

const source = path.resolve(__dirname, '../..');
function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-build-job-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const files = ['build.cjs', 'build-job.cjs', 'job.cjs', ...['build-job', 'build-runtime', 'native-runtime',
    'native-process', 'isolation', 'identity', 'util', 'jobs', 'receipt', 'capabilities', 'cli'].map(name => `lib/${name}.cjs`)];
  for (const name of files) {
    const dest = path.join(dir, 'scripts/nemo', name);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.copyFileSync(path.join(source, 'scripts/nemo', name), dest);
  }
  fs.mkdirSync(path.join(dir, 'src-tauri'), { recursive: true });
  fs.mkdirSync(path.join(dir, 'src'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'package.json'), '{"version":"0.7.0"}');
  fs.writeFileSync(path.join(dir, 'src-tauri/tauri.conf.json'), '{"version":"0.7.0","productName":"Nemo","identifier":"fixture.app"}');
  fs.writeFileSync(path.join(dir, 'src/index.html'), '<title>Nemo v0.7.0</title>');
  fs.writeFileSync(path.join(dir, 'src/tracked.txt'), 'original source');
  fs.writeFileSync(path.join(dir, '.gitignore'), '.runtime/\n.reports/\nnode_modules/\nobserved-build.json\nsrc-tauri/target/\n');
  const stub = path.join(dir, 'builder.cjs');
  fs.writeFileSync(stub, String.raw`
    const fs=require('node:fs'),path=require('node:path');
    const mode=process.env.NEMO_FIXTURE_MODE;
    fs.writeFileSync('observed-build.json',JSON.stringify({target:process.env.CARGO_TARGET_DIR,
      temp:process.env.TMPDIR,cache:process.env.XDG_CACHE_HOME,reports:process.env.NEMO_REPORT_DIR}));
    console.log('fixture build stdout'); console.error('fixture build stderr');
    if(mode==='failure')process.exit(7);
    if(mode==='no-output')process.exit(0);
    if(mode==='source-drift')fs.appendFileSync('src/tracked.txt',' changed during build');
    const app=path.join(process.env.CARGO_TARGET_DIR,process.env.NEMO_FIXTURE_TRIPLE||'fixture-target','release/bundle/macos/Selected app.app');
    fs.mkdirSync(path.join(app,'Contents/MacOS'),{recursive:true});
    fs.writeFileSync(path.join(app,'Contents/Info.plist'),'<plist><dict><key>CFBundleExecutable</key><string>actual-engine</string></dict></plist>');
    fs.writeFileSync(path.join(app,'Contents/MacOS/actual-engine'),'new isolated executable',{mode:0o700});
    fs.writeFileSync(path.join(app,'Contents/MacOS/ffmpeg'),'fixture sidecar',{mode:0o700});
  `);
  const git = args => {
    const r = spawnSync('git', args, { cwd: dir, encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
  };
  git(['init', '-q']); git(['add', '.']);
  git(['-c', 'user.name=Nemo test fixture', '-c', 'user.email=fixture@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '-qm', 'Synthetic isolated-build fixture']);
  return { dir, stub, reports: path.join(dir, '.reports'), runtime: path.join(dir, '.runtime') };
}

function execute(f, mode = 'success', options = {}) {
  const script = `
    const fs=require('node:fs'),path=require('node:path');
    const {runBuildJob}=require('./scripts/nemo/lib/build-job.cjs');
    runBuildJob({reportDir:process.env.NEMO_FIXTURE_REPORTS,taskId:'controlled-build',
      command:process.execPath,args:[process.env.NEMO_FIXTURE_STUB],hostTriple:'fixture-target',
      ...JSON.parse(process.env.NEMO_FIXTURE_OPTIONS),finalizePackage(app){
        if(!app.startsWith(process.env.NEMO_FIXTURE_REPORTS+path.sep))throw new Error('finalizer escaped preserved package');
        fs.writeFileSync(path.join(app,'prepared.txt'),'dylib finalizer ran on copy');
        return {status:process.env.NEMO_FIXTURE_MODE==='finalize-failure'?9:0,stdout:'fixture finalization',stderr:''};
      }}).then(result=>{console.log(JSON.stringify(result));process.exit(0)}).catch(error=>{console.error(error.stack);process.exit(1)});
  `;
  const r = spawnSync(process.execPath, ['-e', script], {
    cwd: f.dir, encoding: 'utf8', timeout: 30_000,
    env: { ...process.env, NEMO_ISOLATION_ROOT: f.runtime, NEMO_FIXTURE_REPORTS: f.reports,
      NEMO_FIXTURE_STUB: f.stub, NEMO_FIXTURE_MODE: mode, NEMO_FIXTURE_OPTIONS: JSON.stringify(options) },
  });
  assert.equal(r.status, 0, r.stderr);
  assert.doesNotMatch(r.stdout, /ownerToken/);
  return { result: JSON.parse(r.stdout), stdout: r.stdout };
}

function absolute(f, artifact) { return path.resolve(f.dir, artifact.path); }
async function holder(f, task) {
  const child = spawn(process.execPath, ['-e', `
    const iso=require('./scripts/nemo/lib/isolation.cjs');
    const build=require('./scripts/nemo/lib/build-runtime.cjs');
    const record=${task ? "iso.registerLauncher('controlled-build')" : "iso.acquireExclusiveSlot(build.worktreeBuildSlot(),'foreign-build')"};
    console.log('ready'); process.on('SIGTERM',()=>{if(record.release)record.release();process.exit(0)});setInterval(()=>{},1000);
  `], { cwd: f.dir, env: { ...process.env, NEMO_ISOLATION_ROOT: f.runtime }, stdio: ['ignore', 'pipe', 'pipe'] });
  await once(child.stdout, 'data');
  return async () => { child.kill('SIGTERM'); await once(child, 'exit'); };
}

test('isolated build preserves selected package and reports before owner cleanup', t => {
  const f = fixture(t);
  const { result } = execute(f);
  assert.equal(result.status, 'pass', result.reason);
  assert.deepEqual([result.details.cleanup.stopped, result.details.cleanup.released], [true, true]);
  assert.equal(result.details.handshake.ok, true);
  const app = result.artifacts.find(a => a.path.endsWith('.app'));
  assert.ok(app);
  assert.equal(fs.readFileSync(path.join(absolute(f, app), 'Contents/MacOS/actual-engine'), 'utf8'), 'new isolated executable');
  assert.ok(result.artifacts.some(a => a.path.endsWith('actual-engine') && a.sha256));
  assert.equal(result.artifacts.some(a => /MacOS\/Nemo$/.test(a.path)), false);
  assert.ok(fs.existsSync(path.join(absolute(f, app), 'prepared.txt')));
  const observed = JSON.parse(fs.readFileSync(path.join(f.dir, 'observed-build.json')));
  assert.ok(observed.target.includes('.runtime/tasks/'));
  for (const value of Object.values(observed)) assert.equal(fs.existsSync(value), false, 'disposable runtime removed after preservation');
  const logs = absolute(f, result.artifacts.find(a => a.path.endsWith('build-logs')));
  assert.match(fs.readFileSync(path.join(logs, 'desktop-build.stdout.log'), 'utf8'), /fixture build stdout/);
  assert.match(fs.readFileSync(path.join(logs, 'desktop-build.stderr.log'), 'utf8'), /fixture build stderr/);
  const proof = JSON.parse(fs.readFileSync(absolute(f, result.artifacts.find(a => a.path.endsWith('build-proof.json')))));
  assert.equal(proof.status, 'pass'); assert.equal(proof.details.cleanup.released, true);
  assert.doesNotMatch(JSON.stringify(proof), /ownerToken/);
});

test('nonzero build preserves failure logs and fails the job', t => {
  const f = fixture(t); const { result } = execute(f, 'failure');
  assert.equal(result.status, 'fail'); assert.equal(result.details.buildExitCode, 7);
  assert.equal(result.details.cleanup.released, true);
  assert.equal(result.artifacts.some(a => a.path.endsWith('.app')), false);
  assert.ok(result.artifacts.some(a => a.path.endsWith('build-logs')));
});

test('source drift cannot pass a successful isolated compiler exit', t => {
  const f = fixture(t); const { result } = execute(f, 'source-drift');
  assert.equal(result.status, 'fail'); assert.match(result.reason, /source moved/);
  assert.equal(result.details.handshake.ok, false);
  assert.ok(result.artifacts.some(a => a.path.endsWith('.app')), 'unaccepted output remains inspectable');
});

test('missing task output cannot select an older shared-target app', t => {
  const f = fixture(t);
  const stale = path.join(f.dir, 'src-tauri/target/fixture-target/release/bundle/macos/Nemo.app');
  fs.mkdirSync(stale, { recursive: true }); fs.writeFileSync(path.join(stale, 'stale.txt'), 'shared artifact');
  const { result } = execute(f, 'no-output');
  assert.equal(result.status, 'fail'); assert.match(result.reason, /did not produce exactly one/);
  assert.equal(result.artifacts.some(a => a.path.endsWith('.app')), false);
  assert.equal(fs.readFileSync(path.join(stale, 'stale.txt'), 'utf8'), 'shared artifact');
});

test('same-worktree reservation refuses a build without stopping its foreign holder', async t => {
  const f = fixture(t); const stop = await holder(f, false);
  try {
    const { result } = execute(f);
    assert.equal(result.status, 'blocked'); assert.match(result.reason, /same-worktree desktop build unavailable/);
    assert.equal(fs.existsSync(path.join(f.dir, 'observed-build.json')), false);
  } finally { await stop(); }
});

test('an existing task owner is refused before its records or data are touched', async t => {
  const f = fixture(t); const stop = await holder(f, true);
  try {
    const tasks = fs.readdirSync(path.join(f.runtime, 'tasks'));
    const record = path.join(f.runtime, 'tasks', tasks[0], 'launcher.json');
    const bytes = fs.readFileSync(record);
    const { result, stdout } = execute(f);
    assert.equal(result.status, 'blocked'); assert.match(result.reason, /runtime already exists/);
    assert.deepEqual(fs.readFileSync(record), bytes);
    assert.equal(stdout.includes(JSON.parse(bytes).ownerToken), false);
  } finally { await stop(); }
});

test('a package finalization failure is retained and cannot become a pass', t => {
  const f = fixture(t); const { result } = execute(f, 'finalize-failure');
  assert.equal(result.status, 'fail'); assert.match(result.reason, /bundle-ffmpeg-dylibs.py failed \(9\)/);
  assert.ok(result.artifacts.some(a => a.path.endsWith('.app')));
  assert.equal(result.details.cleanup.released, true);
});

test('report placement inside disposable state is blocked before starting', t => {
  const f = fixture(t);
  const key = require('node:crypto').createHash('sha256').update('controlled-build').digest('hex');
  const { result } = execute(f, 'success', { reportDir: path.join(f.runtime, 'tasks', key, 'reports') });
  assert.equal(result.status, 'blocked');
  assert.equal(fs.existsSync(path.join(f.dir, 'observed-build.json')), false);
});

test('the named build job reaches the isolated controller with a harmless local Tauri executable', t => {
  const f = fixture(t);
  const bin = path.join(f.dir, 'node_modules/.bin');
  fs.mkdirSync(bin, { recursive: true });
  fs.writeFileSync(path.join(bin, 'tauri'), '#!/usr/bin/env node\n' +
    "process.env.NEMO_FIXTURE_TRIPLE=process.argv[process.argv.indexOf('--target')+1];require('../../builder.cjs');\n", { mode: 0o700 });
  fs.writeFileSync(path.join(f.dir, 'scripts/bundle-ffmpeg-dylibs.py'),
    "import pathlib,sys\npathlib.Path(sys.argv[1], 'prepared.txt').write_text('fixture dylib preparation')\n");
  const desktop = path.join(f.dir, 'tests/desktop');
  fs.mkdirSync(desktop, { recursive: true });
  fs.writeFileSync(path.join(desktop, 'selected.test.cjs'), `
    require('node:test')('the package built in this run is selected',()=>{
      const fs=require('node:fs'),path=require('node:path');
      require('node:assert/strict').equal(fs.readFileSync(path.join(process.env.NEMO_DESKTOP_APP,'Contents/MacOS/actual-engine'),'utf8'),'new isolated executable');
    });
  `);
  const stale = path.join(f.dir, 'src-tauri/target/stale/release/bundle/macos/Nemo.app');
  fs.mkdirSync(stale, { recursive: true });
  const run = spawnSync(process.execPath, ['-e', `
    const jobs=require('./scripts/nemo/lib/jobs.cjs');
    const identity=require('./scripts/nemo/lib/identity.cjs');
    const ctx={reportDir:process.env.NEMO_FIXTURE_REPORTS,receipt:{jobs:[],build:identity.buildIdentity()}};
    const built=jobs.execute('build:desktop',ctx);
    const desktop=built.status==='pass'?jobs.execute('test:desktop',ctx):null;
    console.log(JSON.stringify({built,desktop}));
  `], { cwd: f.dir, encoding: 'utf8', timeout: 30_000, env: { ...process.env,
    NEMO_ISOLATION_ROOT: f.runtime, NEMO_FIXTURE_REPORTS: f.reports, NEMO_FIXTURE_MODE: 'success', NEMO_DESKTOP_APP: stale } });
  assert.equal(run.status, 0, run.stderr);
  const { built: result, desktop: tested } = JSON.parse(run.stdout);
  if (process.platform !== 'darwin') {
    assert.equal(result.status, 'blocked'); assert.match(result.reason, /macOS/); return;
  }
  assert.equal(result.status, 'pass', result.reason);
  assert.equal(tested.status, 'pass', tested.reason || tested.log);
  const observed = JSON.parse(fs.readFileSync(path.join(f.dir, 'observed-build.json')));
  assert.ok(observed.target.includes('.runtime/tasks/'), 'standard command selected an isolated Cargo output');
  assert.ok(result.artifacts.some(a => a.path.endsWith('actual-engine') && a.sha256));
  assert.equal(result.details.cleanup.released, true);
  assert.doesNotMatch(run.stdout, /ownerToken/);
});

test('the actual named CLI exits blocked when a required build prerequisite is absent', t => {
  const f = fixture(t);
  const run = spawnSync(process.execPath, ['scripts/nemo/job.cjs', 'build:desktop', '--json'], {
    cwd: f.dir, encoding: 'utf8', timeout: 30_000,
    env: { ...process.env, NEMO_REPORT_DIR: f.reports, NEMO_ISOLATION_ROOT: f.runtime },
  });
  assert.equal(run.status, 2, run.stderr || run.stdout);
  const receipt = JSON.parse(run.stdout);
  assert.equal(receipt.summary.overall, 'blocked');
  assert.equal(receipt.jobs[0].required, true);
  assert.equal(receipt.jobs[0].status, 'blocked');
  assert.match(receipt.jobs[0].reason, /node_modules\/\.bin\/tauri/);
  assert.equal(fs.existsSync(path.join(f.dir, 'observed-build.json')), false);
});

function alive(pid) {
  try { process.kill(pid, 0); return true; } catch (error) { return error.code !== 'ESRCH'; }
}

test('a silent startup helper that ignores SIGTERM is reaped instead of returning a false blocked result', t => {
  const f = fixture(t);
  fs.writeFileSync(path.join(f.dir, 'scripts/nemo/build.cjs'), `
    const fs=require('node:fs');
    fs.writeFileSync('observed-build.json',JSON.stringify({pid:process.pid}));
    process.on('SIGTERM',()=>{});setInterval(()=>{},1000);setTimeout(()=>process.exit(0),5000);
  `);
  const { result } = execute(f, 'success', { readyTimeoutMs: 300, startupStopTimeoutMs: 100 });
  const helper = JSON.parse(fs.readFileSync(path.join(f.dir, 'observed-build.json')));
  assert.equal(result.status, 'fail'); assert.equal(result.exitCode, 1);
  assert.match(result.reason, /readiness timed out/);
  assert.equal(result.details.cleanup.helperStopped, true);
  assert.equal(alive(helper.pid), false, 'only the controller-created helper was forcibly reaped');
});

test('missing ready output recovers only its exact late owner and uses owned group cleanup with retained data', t => {
  const f = fixture(t);
  fs.writeFileSync(path.join(f.dir, 'scripts/nemo/build.cjs'), `
    const task=process.argv[process.argv.indexOf('--task')+1];
    setTimeout(()=>process.exit(0),8000);
    require('./lib/build-runtime.cjs').runBuildLauncher(task,{
      command:process.execPath,args:[require('node:path').resolve('builder.cjs')],hostTriple:'fixture-target'
    },()=>{}).catch(error=>{console.error(error.message);process.exit(1)});
  `);
  const { result, stdout } = execute(f, 'success', { readyTimeoutMs: 1200, startupStopTimeoutMs: 100 });
  assert.equal(result.status, 'fail'); assert.match(result.reason, /readiness timed out/);
  assert.equal(result.details.recoveredStartup, true);
  assert.equal(result.details.cleanup.stopped, true);
  assert.equal(result.details.cleanup.released, true);
  assert.equal(result.details.cleanup.retainedData, true);
  const key = require('node:crypto').createHash('sha256').update('controlled-build').digest('hex');
  const root = path.join(f.runtime, 'tasks', key);
  const status = JSON.parse(fs.readFileSync(path.join(root, 'reports/build-runtime.json')));
  assert.equal(alive(status.launcherPid), false);
  assert.equal(alive(status.childPid), false);
  assert.equal(status.processTree.stopped, true);
  assert.equal(fs.existsSync(path.join(root, 'launcher.json')), false);
  assert.ok(fs.existsSync(path.join(root, 'build')), 'startup-failure artifacts remain inspectable');
  assert.doesNotMatch(stdout, /ownerToken/);
});

test('startup reconciliation never adopts a foreign PID record or removes its data', async t => {
  const f = fixture(t);
  const foreign = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore' });
  try {
    fs.writeFileSync(path.join(f.dir, 'scripts/nemo/build.cjs'), `
      const fs=require('node:fs'),path=require('node:path');
      const task=process.argv[process.argv.indexOf('--task')+1];
      const root=require('./lib/isolation.cjs').taskRoots(task).root;
      fs.writeFileSync(path.join(root,'launcher.json'),JSON.stringify({taskId:task,pid:${foreign.pid},ownerToken:'foreign-owner-token'}));
      fs.writeFileSync(path.join(root,'retained.txt'),'foreign state');
      fs.writeFileSync('observed-build.json',JSON.stringify({pid:process.pid,root}));
      process.on('SIGTERM',()=>{});setInterval(()=>{},1000);setTimeout(()=>process.exit(0),5000);
    `);
    const { result, stdout } = execute(f, 'success', { readyTimeoutMs: 300, startupStopTimeoutMs: 100 });
    const helper = JSON.parse(fs.readFileSync(path.join(f.dir, 'observed-build.json')));
    assert.equal(result.status, 'fail'); assert.match(result.reason, /cleanup incomplete/);
    assert.equal(result.details.cleanup.helperStopped, true);
    assert.equal(result.details.cleanup.released, false);
    assert.equal(result.details.cleanup.taskId, 'controlled-build');
    assert.equal(alive(helper.pid), false); assert.equal(alive(foreign.pid), true);
    assert.equal(fs.readFileSync(path.join(helper.root, 'retained.txt'), 'utf8'), 'foreign state');
    assert.equal(JSON.parse(fs.readFileSync(path.join(helper.root, 'launcher.json'))).pid, foreign.pid);
    assert.equal(stdout.includes('foreign-owner-token'), false);
  } finally {
    if (foreign.exitCode === null && foreign.signalCode === null) { foreign.kill('SIGKILL'); await once(foreign, 'exit'); }
  }
});
