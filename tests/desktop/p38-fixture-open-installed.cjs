'use strict';
// Owner-side checkpoint runner. It never launches or controls Nemo; the P38 operator
// inspects the original main window and calls acknowledge only for that observed frame.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { safeWindowEvidence, verifyWindow } = require('./p38-installed-driver.cjs');
const INSPECTION_CODES = Object.freeze(['inspection-budget-refused', 'inspection-transport-refused',
  'inspection-status-refused', 'inspection-identity-refused', 'inspection-projection-refused',
  'inspection-root-refused', 'inspection-completeness-refused', 'inspection-page-refused',
  'inspection-marker-refused', 'inspection-region-refused']);
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
    ['ready','a-opened','b-opened','b-closed'][value.checkpoint]===expected.phase,'probe-checkpoint-refused');
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
  check(expected&&['ready','a-opened','b-opened','b-closed'].includes(expected.phase),'probe-phase-refused');
  check(Number.isSafeInteger(expected.pid)&&expected.pid>0&&expected.pid<=2147483647&&
    Number.isSafeInteger(expected.windowId)&&expected.windowId>0&&expected.windowId<=4294967295,
    'probe-window-pin-refused');
  const result=safeWindowEvidence(snapshot);
  verifyWindow(snapshot, expected.pid, expected.windowId);
  check(result.complete===true&&['truncated','depthClipped','childrenUnreadable','foreignChildren'].every(k=>result[k]===false)&&
    result.nodes.every(n=>n.blocked===undefined)&&new Set(result.nodes.map(n=>n.path)).size===result.nodes.length,'probe-visible-state-refused');
  const region=result.projectTabs,markers=result.nodes.filter(n=>n.control==='Project tabs');
  check(region?.unique===true&&markers.length===1&&markers[0].path===region.path&&
    (expected.phase==='ready'||expected.regionToken===region.token),'probe-visible-state-refused');
  const inside=result.nodes.filter(n=>n.path.startsWith(region.path+'.'));
  const rows=inside.filter(n=>typeof n.selected==='boolean');
  const rowA=rows.filter(n=>['A','A.json'].includes(n.control)),rowB=rows.filter(n=>['B','B.json'].includes(n.control));
  check(rows.length===rowA.length+rowB.length,'probe-visible-state-refused');
  check(inside.filter(n=>['A','B','A.json','B.json'].includes(n.control)).every(n=>rows.some(r=>
    r.control.replace('.json','')===n.control.replace('.json','')&&(n.path===r.path||n.path.startsWith(r.path+'.')))),
    'probe-visible-state-refused');
  const alerts=result.nodes.filter(n=>n.alert==='switch-unavailable');
  if(expected.phase==='ready') check(rows.length===0&&!inside.some(n=>['A','B','A.json','B.json'].includes(n.control)),'probe-visible-state-refused');
  else check(rowA.length===1&&rowA[0].selected===true&&
    (expected.phase==='b-opened'?rowB.length===1&&rowB[0].selected===false&&alerts.length===1:rowB.length===0),'probe-visible-state-refused');
  return result;
}

// Complete bounded discovery establishes uniqueness. Root clipping permits only
// authenticated read-only pages; a fresh retained region is required for ACK.
async function observeProjectTabs(inspect,identity,state={actions:0}) {
  const seen=new Map(),queue=[''];
  const complete=s=>s.complete===true&&['truncated','depthClipped','childrenUnreadable','foreignChildren'].every(k=>s[k]===false);
  async function read(request) {
    check(state.actions++<63,'inspection-budget-refused'); // Leave the existing 64th action for quit.
    let raw,s;
    try {raw=await inspect({type:'inspect',target:'main',pid:identity.pid,windowId:identity.windowId,maxNodes:128,...request});}
    catch(error) {if(error?.observerStage==='inspection')throw error;check(false,'inspection-transport-refused');}
    check(raw?.status==='ok'&&raw.type==='inspect','inspection-status-refused');
    try {verifyWindow(raw,identity.pid,identity.windowId);} catch {check(false,'inspection-identity-refused');}
    try {s=safeWindowEvidence(raw);} catch {check(false,'inspection-projection-refused');}
    check(s.nodes.length>0&&s.nodes.length<=128&&s.nodes[0].path===(request.element||''),'inspection-root-refused');
    check(s.foreignChildren===false&&s.childrenUnreadable===false&&
      s.nodes.every(n=>n.blocked===undefined&&Number.isInteger(n.childCount)),'inspection-completeness-refused');
    return s;
  }
  function collect(nodes) {
    for(const n of nodes) {
      check(!seen.has(n.path)||JSON.stringify(seen.get(n.path))===JSON.stringify(n),'inspection-page-refused');
      seen.set(n.path,n);check(seen.size<=4096,'inspection-budget-refused');
    }
  }
  while(queue.length) {
    const element=queue.shift(),first=await read({element,depth:5});
    if(complete(first)) {collect(first.nodes);continue;}
    check(first.complete===false&&typeof first.depthClipped==='boolean'&&typeof first.truncated==='boolean','inspection-completeness-refused');
    let offset=0,token,count,root,done=false;
    while(!done) {
      const s=await read({element,depth:1,childOffset:offset,...(offset?{pageToken:token}:{})});
      check(s.childOffset===offset&&s.childCount===s.nodes[0].childCount&&
        s.nextChildOffset-offset===s.nodes.length-1&&
        (count===undefined||count===s.childCount&&token===s.pageToken&&JSON.stringify(root)===JSON.stringify(s.nodes[0]))&&
        s.childrenComplete===(s.nextChildOffset===s.childCount),'inspection-page-refused');
      root=s.nodes[0];count=s.childCount;token=s.pageToken;done=s.childrenComplete;
      check(new Set(s.nodes.map(n=>n.path)).size===s.nodes.length&&s.nodes.slice(1).every((n,i)=>
        n.path===(element?element+'.':'')+(offset+i)),'inspection-page-refused');
      collect(s.nodes);
      for(const n of s.nodes.slice(1))if(n.childCount>0)queue.push(n.path);
      offset=s.nextChildOffset;
    }
  }
  const markers=[...seen.values()].filter(n=>n.control==='Project tabs');
  check(markers.length===1,'inspection-marker-refused');
  const region=await read({element:markers[0].path,depth:5,projectTabs:true,...(state.token?{projectTabsToken:state.token}:{})});
  check(complete(region)&&region.projectTabs?.path===markers[0].path&&
    (!state.token||state.token===region.projectTabs.token),'inspection-region-refused');
  state.token=region.projectTabs.token;
  const paths=new Set(region.nodes.map(n=>n.path));
  return {...region,projectTabs:{...region.projectTabs,unique:true},nodes:[...region.nodes,
    ...[...seen.values()].filter(n=>n.alert&&!paths.has(n.path))]};
}
function acknowledge(root, expected, ownerToken, decision, snapshot) {
  check(typeof ownerToken==='string'&&/^[a-f0-9]{32,128}$/.test(ownerToken),'owner-token-refused');
  check(['accepted','refused'].includes(decision),'decision-refused');
  check(expected&&['ready','a-opened','b-opened','b-closed'].includes(expected.phase),'probe-phase-refused');
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
module.exports={receipt,visibleEvidence,acknowledge,observeProjectTabs,INSPECTION_CODES};
