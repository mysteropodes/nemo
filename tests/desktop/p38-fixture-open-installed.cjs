'use strict';
// Owner-side checkpoint runner. It never launches or controls Nemo; the P38 operator
// inspects the original main window and calls acknowledge only for that observed frame.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { safeWindowEvidence, verifyWindow } = require('./p38-installed-driver.cjs');
const hashes = Object.freeze({
  A:'e1bc63b0e284f521e170d8236296392bdcf19d4b1f13b4e04683727844fa71a0',
  B:'a438bda3b5c25ec3e8797d1c95fb8ade1da7b0bda675c2279499c659ece46bf6'
});
function check(value, code) { if (!value) throw Object.assign(new Error(code), {code}); }
function hash(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }
function pinRoot(root) {
  check(path.isAbsolute(root) && path.normalize(root) === root, 'probe-root-refused');
  const stat = fs.lstatSync(root);
  check(stat.isDirectory() && !stat.isSymbolicLink() && fs.realpathSync(root) === root &&
    path.basename(path.dirname(root)) === 'tmp' && path.basename(root).startsWith('p38-fixture-open-'), 'probe-root-refused');
  return {dev:stat.dev, ino:stat.ino, verify() {
    const now=fs.lstatSync(root);
    check(now.isDirectory()&&!now.isSymbolicLink()&&now.dev===stat.dev&&now.ino===stat.ino&&fs.realpathSync(root)===root,'probe-root-refused');
  }};
}
function readNoFollow(pathname, guard, limit, identity) {
  guard.verify();
  const named=fs.lstatSync(pathname);
  check(named.isFile()&&!named.isSymbolicLink()&&named.size<=limit,'probe-file-refused');
  const fd=fs.openSync(pathname,fs.constants.O_RDONLY|fs.constants.O_NOFOLLOW);
  try {
    const opened=fs.fstatSync(fd);
    check(opened.isFile()&&opened.size<=limit&&opened.dev===named.dev&&opened.ino===named.ino,
      'probe-file-identity-refused');
    if(identity) check(Number.isSafeInteger(identity.device)&&Number.isSafeInteger(identity.inode)&&
      opened.dev===identity.device&&opened.ino===identity.inode,'probe-fixture-identity-refused');
    const chunks=[], buffer=Buffer.alloc(Math.min(65536,limit+1)); let total=0;
    for(;;) {
      const count=fs.readSync(fd,buffer,0,Math.min(buffer.length,limit+1-total),null);
      if(!count) break;
      total+=count; check(total<=limit,'probe-file-bound-exceeded'); chunks.push(Buffer.from(buffer.subarray(0,count)));
    }
    const after=fs.lstatSync(pathname); guard.verify();
    check(after.isFile()&&!after.isSymbolicLink()&&after.dev===opened.dev&&after.ino===opened.ino,
      'probe-file-identity-refused');
    return Buffer.concat(chunks,total);
  } finally { fs.closeSync(fd); }
}

function receipt(root, expected) {
  const guard=pinRoot(root); guard.verify();
  const file=path.join(root,'manifest.json'), stat=fs.lstatSync(file);
  check(stat.isFile()&&!stat.isSymbolicLink()&&stat.size<=2048,'probe-receipt-refused');
  const value=JSON.parse(readNoFollow(file,guard,2048).toString('utf8'));
  check(value.schema==='nemo.p38-fixture-open/1'&&value.runId===expected.runId&&
    value.taskKey===expected.taskKey&&value.sourceSha===expected.sourceSha&&value.buildSha===expected.buildSha&&
    value.pid===expected.pid&&value.window==='main'&&value.waiting===true&&value.failed===false&&
    ['ready','a-opened','b-opened'][value.checkpoint]===expected.phase,'probe-checkpoint-refused');
  check(value.origin==='tauri://localhost/index.html'&&Array.isArray(value.fixtures)&&value.fixtures.length===2,
    'probe-receipt-refused');
  for(const [index,id] of ['A','B'].entries()) {
    const fixture=path.join(root,`${id}.json`), item=value.fixtures[index], bytes=readNoFollow(fixture,guard,1048576,item);
    check(item&&item.id===id&&item.file===fixture&&item.sha256===hashes[id]&&hash(bytes)===hashes[id],
      'probe-fixture-refused');
  }
  const pins={schema:value.schema,runId:value.runId,taskKey:value.taskKey,sourceSha:value.sourceSha,
    buildSha:value.buildSha,pid:value.pid,window:value.window,origin:value.origin,fixtures:value.fixtures};
  check(/^[a-f0-9]{64}$/.test(value.manifestSha256)&&hash(Buffer.from(JSON.stringify(pins)))===value.manifestSha256,
    'probe-manifest-hash-refused');
  return {guard,value};
}
function visibleEvidence(snapshot, expected) {
  check(expected&&['ready','a-opened','b-opened'].includes(expected.phase),'probe-phase-refused');
  check(Number.isSafeInteger(expected.pid)&&expected.pid>0&&expected.pid<=2147483647&&
    Number.isSafeInteger(expected.windowId)&&expected.windowId>0&&expected.windowId<=4294967295,
    'probe-window-pin-refused');
  const result=safeWindowEvidence(snapshot);
  verifyWindow(snapshot, expected.pid, expected.windowId);
  const controls=new Set(result.nodes.map(node=>node.control).filter(Boolean));
  const alerts=new Set(result.nodes.map(node=>node.alert).filter(Boolean));
  if(expected.phase==='ready') check(!controls.has('A.json')&&!controls.has('B.json'),'probe-visible-state-refused');
  if(expected.phase==='a-opened') check(controls.has('A.json')&&alerts.has('switch-unavailable'),'probe-visible-state-refused');
  if(expected.phase==='b-opened') check(controls.has('B.json')&&alerts.has('switch-unavailable'),'probe-visible-state-refused');
  return result;
}
function acknowledge(root, expected, ownerToken, decision, snapshot) {
  check(typeof ownerToken==='string'&&/^[a-f0-9]{32,128}$/.test(ownerToken),'owner-token-refused');
  check(['accepted','refused'].includes(decision),'decision-refused');
  check(expected&&['ready','a-opened','b-opened'].includes(expected.phase),'probe-phase-refused');
  check(Number.isSafeInteger(expected.pid)&&expected.pid>0&&expected.pid<=2147483647&&
    Number.isSafeInteger(expected.windowId)&&expected.windowId>0&&expected.windowId<=4294967295,
    'probe-window-pin-refused');
  const {guard,value}=receipt(root,expected);
  if(decision==='accepted') visibleEvidence(snapshot,expected);
  const ack={runId:value.runId,taskKey:value.taskKey,sourceSha:value.sourceSha,buildSha:value.buildSha,
    manifestSha256:value.manifestSha256,phase:expected.phase,ownerToken,decision};
  const file=path.join(root,'ack.json'), fd=fs.openSync(file,fs.constants.O_CREAT|fs.constants.O_EXCL|fs.constants.O_WRONLY|fs.constants.O_NOFOLLOW,0o600);
  try { guard.verify(); fs.writeFileSync(fd,JSON.stringify(ack)); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  guard.verify();
  return {phase:expected.phase,decision,windowId:expected.windowId};
}
module.exports={receipt,visibleEvidence,acknowledge};
