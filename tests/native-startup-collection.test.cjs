'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const { prepareCollection } = require('./desktop/startup-collection.cjs');
const RUN = '12345678-1234-1234-1234-123456789abc';
function fixture() {
  const base = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'startup-collector-')));
  const task = path.join(base, 'task'), reports = path.join(task, 'reports'), dest = path.join(base, 'retained');
  fs.mkdirSync(reports, { recursive: true }); fs.mkdirSync(dest);
  const pins = { runId: RUN, taskKey: 'a'.repeat(64), pid: 123, window: 'main', loadGeneration: 1,
    sourceSha: 'b'.repeat(40), executableSha256: 'c'.repeat(64), buildProofSha256: 'd'.repeat(64) };
  const native = { schema: 'nemo.startup-observer/1', runId: RUN, taskKey: pins.taskKey, pid: pins.pid, window: 'main',
    status: 'partial', stages: [], accepted: 0, rejected: 0, dropped: 0, sealed: false,
    jsReadiness: 'not-observed', visiblePaint: 'not-observed' };
  const js = { adapterRefused: false, js: { schema: 'nemo.startup-js-native/1', runId: RUN, taskKey: pins.taskKey,
    pid: pins.pid, window: 'main', loadGeneration: 1, nativePhase: 0, status: 'provisional',
    accepted: 0, rejected: 0, rejectionOverflow: false, fence: null, probeRefused: false,
    records: [], sealed: false, visiblePaint: 'not-observed' } };
  function write(name, value) { const bytes = Buffer.from(JSON.stringify(value)); fs.writeFileSync(path.join(reports, name), bytes); return bytes; }
  write('startup-observer.json', native);
  const proof = { schema: 'nemo.startup-stop-proof/1', runId: RUN, taskKey: pins.taskKey, pid: pins.pid,
    appExited: true, launcherExited: true, processGroupExited: true, rootsRemoved: true, slotsReleased: true };
  return { base, task, reports, dest, pins, native, js, proof, write,
    prepare() { return prepareCollection(reports, pins, dest); },
    unlink() { fs.rmSync(task, { recursive: true }); },
    cleanup() { fs.rmSync(base, { recursive: true, force: true }); } };
}
function hash(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }

test('held descriptor reads actual final producer bytes after unlink and remains Partial without seal', async () => {
  const f = fixture(); let handle;
  try {
    handle = f.prepare();
    f.native.status = 'native-load-observed'; f.native.accepted = 3;
    f.native.stages = ['window-created','native-page-started','native-page-finished'].map((stage, i) => ({ sequence: i+1, stage, elapsedMs: i }));
    const final = f.write('startup-observer.json', f.native); f.unlink();
    const receipt = await handle.finalize(async pins => { assert(Object.isFrozen(pins)); return f.proof; });
    assert.equal(receipt.status, 'partial'); assert.equal(receipt.sealed, false); assert.equal(receipt.visiblePaint, 'not-observed');
    assert.equal(receipt.hashes.native, hash(final)); assert.equal(receipt.native.accepted, 3);
    assert(receipt.reasons.includes('js-missing')); assert(receipt.reasons.includes('unsealed-source'));
    assert.equal(handle.closed(), true); assert(!fs.existsSync(f.task));
    assert.deepEqual(JSON.parse(fs.readFileSync(path.join(f.dest, 'startup-collection.json'))), receipt);
    await assert.rejects(handle.finalize(() => f.proof), /startup collection refused/);
  } finally { handle?.close(); f.cleanup(); }
});

test('both actual current scalar schemas correlate identity and final refusal/loss is retained', async () => {
  const f = fixture(); let handle;
  try {
    f.js.js.records = [{ schema: 'nemo.startup-js/1', runId: RUN, loadGeneration: 1, sequence: 1, stage: 'early-script' }];
    f.js.js.accepted = 1; f.write('startup-js.json', f.js); handle = f.prepare();
    f.js.adapterRefused = true; f.js.js.rejected = 1;
    const final = f.write('startup-js.json', f.js); f.unlink();
    const receipt = await handle.finalize(() => f.proof);
    assert.equal(receipt.status, 'partial'); assert.equal(receipt.hashes.js, hash(final));
    assert.equal(receipt.js.adapterRefused, true); assert.equal(receipt.js.js.rejected, 1);
    assert.equal(receipt.sourceSha, f.pins.sourceSha); assert.equal(receipt.executableSha256, f.pins.executableSha256);
  } finally { handle?.close(); f.cleanup(); }
});

test('stop callback cannot be replaced by caller boolean or wrong/incomplete control-plane proof', async () => {
  for (const variant of ['literal','wrong-run','false-app','throw','root-still-present']) {
    const f = fixture(); let handle;
    try {
      handle = f.prepare(); const proof = { ...f.proof };
      if (variant !== 'root-still-present') f.unlink();
      if (variant === 'wrong-run') proof.runId = 'private-forged';
      if (variant === 'false-app') proof.appExited = false;
      const verifier = variant === 'literal' ? true : variant === 'throw' ? () => { throw Error('private-stop'); } : () => proof;
      const receipt = await handle.finalize(verifier);
      assert.equal(receipt.status, 'partial'); assert.equal(receipt.stopProof, 'not-established');
      assert.deepEqual(receipt.hashes, { native: null, js: null }); assert(!JSON.stringify(receipt).includes('private'));
      assert.equal(handle.closed(), true);
    } finally { handle?.close(); f.cleanup(); }
  }
});

test('unknown/private, wrong identity, sealed forgery, duplicate keys and oversized/deep sources refuse safely', async () => {
  for (const variant of ['private','run','task','pid','window','sealed','duplicate','oversize','deep','invalid-utf8']) {
    const f = fixture(); let handle;
    try {
      handle = f.prepare(); let value = { ...f.native };
      if (variant === 'private') value.message = 'private-secret';
      if (variant === 'run') value.runId = 'private-run';
      if (variant === 'task') value.taskKey = 'e'.repeat(64);
      if (variant === 'pid') value.pid = 124;
      if (variant === 'window') value.window = 'private-window';
      if (variant === 'sealed') value.sealed = true;
      let bytes = Buffer.from(JSON.stringify(value));
      if (variant === 'duplicate') bytes = Buffer.from(bytes.toString().replace('{', '{"runId":"private-first",'));
      if (variant === 'oversize') bytes = Buffer.alloc(8193, 120);
      if (variant === 'deep') bytes = Buffer.from('['.repeat(500)+'"private-deep"'+']'.repeat(500));
      if (variant === 'invalid-utf8') bytes = Buffer.from([0xff]);
      fs.writeFileSync(path.join(f.reports, 'startup-observer.json'), bytes); f.unlink();
      const receipt = await handle.finalize(() => f.proof);
      assert.equal(receipt.status, 'refused'); assert.equal(receipt.native, null);
      assert.deepEqual(receipt.hashes, { native: null, js: null }); assert(!JSON.stringify(receipt).includes('private'));
      assert.equal(handle.closed(), true);
    } finally { handle?.close(); f.cleanup(); }
  }
});

test('JS projection rejects unknown properties, pins, sequence, stage shape and raw error values', async () => {
  for (const variant of ['unknown','load','record','counter','raw','stage','null']) {
    const f = fixture(); let handle;
    try {
      f.write('startup-js.json', f.js); handle = f.prepare();
      if (variant === 'unknown') f.js.private = 'private-secret';
      if (variant === 'load') f.js.js.loadGeneration = 2;
      if (variant === 'counter') f.js.js.accepted = 15;
      if (variant === 'record') { f.js.js.records = [{schema:'nemo.startup-js/1',runId:RUN,loadGeneration:1,sequence:2,stage:'early-script'}]; f.js.js.accepted=1; }
      if (['raw','stage','null'].includes(variant)) {
        f.js.js.records = [{schema:'nemo.startup-js/1',runId:RUN,loadGeneration:1,sequence:1,stage:'js-error',category:'private-message'}]; f.js.js.accepted=1;
        if (variant === 'stage') f.js.js.records[0].stage='private-stage';
        if (variant === 'null') { f.js.js.records[0].category='unknown'; f.js.js.records[0].message=null; }
      }
      f.write('startup-js.json', f.js); f.unlink(); const receipt=await handle.finalize(()=>f.proof);
      assert.equal(receipt.status,'refused'); assert.equal(receipt.js,null); assert(!JSON.stringify(receipt).includes('private'));
    } finally { handle?.close(); f.cleanup(); }
  }
});

test('directory/file symlinks, destination escape/foreign file and replacement never overwrite', async () => {
  for (const variant of ['source-symlink','root-symlink','parent-symlink','inside','foreign-output','replace-output']) {
    const f=fixture(); let handle;
    try {
      if (variant==='source-symlink') { fs.unlinkSync(path.join(f.reports,'startup-observer.json')); fs.symlinkSync('/private-not-owned',path.join(f.reports,'startup-observer.json')); }
      if (variant==='root-symlink') { const alias=path.join(f.base,'alias'); fs.symlinkSync(f.reports,alias); assert.throws(()=>prepareCollection(alias,f.pins,f.dest)); continue; }
      if (variant==='parent-symlink') { const alias=path.join(f.base,'task-alias'); fs.symlinkSync(f.task,alias); assert.throws(()=>prepareCollection(path.join(alias,'reports'),f.pins,f.dest)); continue; }
      if (variant==='inside') { assert.throws(()=>prepareCollection(f.reports,f.pins,f.reports)); continue; }
      const output=path.join(f.dest,'startup-collection.json');
      if (variant==='foreign-output') { fs.writeFileSync(output,'foreign-private'); assert.throws(()=>f.prepare()); assert.equal(fs.readFileSync(output,'utf8'),'foreign-private'); continue; }
      if (variant==='source-symlink') { assert.throws(()=>f.prepare()); continue; }
      handle=f.prepare(); fs.unlinkSync(output); fs.writeFileSync(output,'foreign-private'); f.unlink();
      await assert.rejects(handle.finalize(()=>f.proof)); assert.equal(fs.readFileSync(output,'utf8'),'foreign-private'); assert(handle.closed());
    } finally { handle?.close(); f.cleanup(); }
  }
});

test('all descriptors close after finalize, explicit close and prepared-source refusal', async () => {
  const f=fixture(), real=fs.openSync, seen=[]; let handle;
  fs.openSync=function(...args){const fd=real(...args);seen.push(fd);return fd;};
  try { handle=f.prepare(); } finally { fs.openSync=real; }
  try { handle.close(); handle.close(); await assert.rejects(handle.finalize(()=>f.proof));
    for (const fd of seen) assert.throws(()=>fs.fstatSync(fd),{code:'EBADF'});
  } finally { f.cleanup(); }
  const bad=fixture(); fs.writeFileSync(path.join(bad.reports,'startup-js.json'),'x'.repeat(16385)); seen.length=0;
  fs.openSync=function(...args){const fd=real(...args);seen.push(fd);return fd;};
  try { assert.throws(()=>bad.prepare()); } finally { fs.openSync=real; }
  try { for (const fd of seen) assert.throws(()=>fs.fstatSync(fd),{code:'EBADF'}); } finally { bad.cleanup(); }
});

test('macOS standard /var alias and captured expected getter identity are stable', async () => {
  const f=fixture(); let handle;
  try {
    let reads=0; Object.defineProperty(f.pins,'runId',{get(){return reads++ ? 'private-retarget' : RUN;}});
    const root=process.platform==='darwin'?f.reports.replace(/^\/private\/var\//,'/var/'):f.reports;
    handle=prepareCollection(root,f.pins,f.dest); assert.equal(reads,1); f.unlink();
    const receipt=await handle.finalize(()=>f.proof); assert.equal(receipt.runId,RUN); assert.equal(receipt.status,'partial');
  } finally { handle?.close(); f.cleanup(); }
});

test('stalled verifier is bounded and final read instability refuses without leaking bytes', async () => {
  const f=fixture(); let handle;
  try {
    handle=f.prepare(); f.unlink();
    const receipt=await handle.finalize(()=>new Promise(()=>{}),1);
    assert.equal(receipt.status,'partial'); assert(receipt.reasons.includes('stop-unverified')); assert(handle.closed());
  } finally { handle?.close(); f.cleanup(); }
  const r=fixture(); const writer=fs.openSync(path.join(r.reports,'startup-observer.json'),fs.constants.O_WRONLY);
  let reader; const real=fs.readSync;
  try {
    reader=r.prepare(); r.unlink(); let changed=false;
    fs.readSync=function(...args){const n=real(...args);if(!changed){changed=true;fs.ftruncateSync(writer,0);fs.writeSync(writer,'private-race');}return n;};
    const receipt=await reader.finalize(()=>r.proof);
    assert.equal(receipt.status,'refused'); assert(receipt.reasons.includes('source-unstable')); assert(!JSON.stringify(receipt).includes('private'));
    assert(reader.closed());
  } finally { fs.readSync=real; reader?.close(); fs.closeSync(writer); r.cleanup(); }
});

test('distinct 64-bit source and destination inode IDs never alias through Number rounding', async () => {
  const first=9007199254740992n, second=9007199254740993n;
  assert.equal(Number(first),Number(second)); assert.notEqual(first,second);
  for (const kind of ['source','destination']) {
    const f=fixture(); let handle; const lstat=fs.lstatSync,fstat=fs.fstatSync;
    const nativePath=path.join(f.reports,'startup-observer.json'),outPath=path.join(f.dest,'startup-collection.json');
    let nativeFd,outFd; const open=fs.openSync;
    fs.openSync=function(file,...args){const fd=open(file,...args);if(file===nativePath)nativeFd=fd;if(file===outPath)outFd=fd;return fd;};
    fs.lstatSync=function(file,opts){const s=lstat(file,opts);if((kind==='source'&&file===nativePath)||(kind==='destination'&&file===outPath)){assert.equal(opts?.bigint,true);s.ino=first;}return s;};
    fs.fstatSync=function(fd,opts){const s=fstat(fd,opts);if((kind==='source'&&fd===nativeFd)||(kind==='destination'&&fd===outFd)){assert.equal(opts?.bigint,true);s.ino=second;}return s;};
    try {
      if(kind==='source')assert.throws(()=>f.prepare());
      else {handle=f.prepare();f.unlink();await assert.rejects(handle.finalize(()=>f.proof));assert(handle.closed());}
    } finally {fs.openSync=open;fs.lstatSync=lstat;fs.fstatSync=fstat;handle?.close();f.cleanup();}
  }
});

test('same-inode foreign output bytes are refused and preserved without prefix overwrite', async () => {
  const f=fixture();let handle;
  try {
    handle=f.prepare();const file=path.join(f.dest,'startup-collection.json'),before=fs.lstatSync(file,{bigint:true});
    fs.writeFileSync(file,'foreign-private-bytes');const after=fs.lstatSync(file,{bigint:true});
    assert.equal(before.ino,after.ino);assert.equal(before.dev,after.dev);f.unlink();
    await assert.rejects(handle.finalize(()=>f.proof));assert(handle.closed());
    assert.equal(fs.readFileSync(file,'utf8'),'foreign-private-bytes');
  } finally {handle?.close();f.cleanup();}
});
