'use strict';
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const assert=require('node:assert/strict'),test=require('node:test'),vm=require('node:vm');
const {EventEmitter}=require('node:events'),{PassThrough,Writable}=require('node:stream');
const {createRequire}=require('node:module');
const owner=require('./p38-fixture-open-owner.cjs');
const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==','base64');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const identity={pid:42,windowId:9};
const reply=index=>({status:'ok',type:'capture',...identity,matchedAX:true,capture:'capture-'+(index+1)+'.png'});
function scratch() {return fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-phase-capture-')));}

test('actual private PNG reader pins phase/window/file, hashes bounded pixels and strips arbitrary facts',()=>{
  const root=scratch();try {
    for(const index of [1,2,3]) {
      const file=path.join(root,reply(index).capture);fs.writeFileSync(file,png,{mode:0o644});
      const result=owner.captureEvidence(root,index,identity,{...reply(index),private:'secret'});
      assert.deepEqual(result,{phase:owner.PHASES[index],...identity,capture:reply(index).capture,sha256:hash(png),width:1,height:1});
      assert.equal(fs.statSync(file).mode&0o777,0o600);assert.doesNotMatch(JSON.stringify(result),/private|secret|\/Users|ownerToken/);
    }
    for(const value of [0,4,true,'2'])assert.throws(()=>owner.captureEvidence(root,value,identity,reply(2)),/observer-failed/);
    for(const pin of [{},{pid:0,windowId:9},{pid:42,windowId:true}])assert.throws(()=>owner.captureEvidence(root,2,pin,reply(2)),/observer-failed/);
    for(const change of [{status:'refused'},{type:'inspect'},{pid:43},{windowId:10},{matchedAX:false},{capture:'../capture-3.png'}])
      assert.throws(()=>owner.captureEvidence(root,2,identity,{...reply(2),...change}),/observer-failed/);
    const file=path.join(root,'capture-3.png');fs.unlinkSync(file);fs.symlinkSync(path.join(root,'capture-2.png'),file);
    assert.throws(()=>owner.captureEvidence(root,2,identity,reply(2)),/observer-failed/);fs.unlinkSync(file);
    for(const data of [Buffer.alloc(0),Buffer.alloc(45),png.subarray(0,24),Buffer.concat([png,Buffer.from('tail')]),
      (()=>{const b=Buffer.from(png);b[45]^=1;return b;})(),
      (()=>{const b=Buffer.from(png);b.writeUInt32BE(0,16);return b;})(),
      (()=>{const b=Buffer.from(png);b.writeUInt32BE(16385,20);return b;})()]) {
      fs.writeFileSync(file,data);assert.throws(()=>owner.captureEvidence(root,2,identity,reply(2)),/observer-failed/);
    }
    fs.truncateSync(file,64*1024*1024+1);assert.throws(()=>owner.captureEvidence(root,2,identity,reply(2)),/observer-failed/);
    fs.unlinkSync(file);assert.throws(()=>owner.captureEvidence(root,2,identity,reply(2)),/observer-failed/);
    const alias=root+'-alias';fs.symlinkSync(root,alias);
    try {assert.throws(()=>owner.captureEvidence(alias,2,identity,reply(2)),/observer-failed/);}finally {fs.unlinkSync(alias);}
  } finally {fs.rmSync(root,{recursive:true,force:true});}
});

test('actual owner transport uses one retained helper and capture group cleanup with no second controller',async()=>{
  const root=scratch(),file=path.join(root,'helper');fs.writeFileSync(file,'owned-helper',{mode:0o700});
  const events=[];let spawns=0,cleanups=0,child;
  const filename=path.join(__dirname,'p38-fixture-open-owner.cjs'),realRequire=createRequire(filename),module={exports:{}};
  const context={module,exports:module.exports,__dirname,__filename:filename,Buffer,process,setTimeout,clearTimeout,
    require(name) {
      if(name==='node:child_process')return {spawn(){
        spawns++;child=new EventEmitter();child.pid=123;child.exitCode=null;child.signalCode=null;child.stdout=new PassThrough();
        child.stdin=new Writable({write(bytes,_encoding,done){
          const request=JSON.parse(bytes.toString());events.push(request);
          if(request.type==='capture') {fs.writeFileSync(path.join(root,request.capture),png);child.stdout.write(JSON.stringify({...reply(Number(request.capture[8])-1)})+'\n');}
          else if(request.type==='quit')child.stdout.write(JSON.stringify({status:'quit',...identity})+'\n');
          done();
        }});
        queueMicrotask(()=>{child.emit('spawn');child.stdout.write(JSON.stringify({status:'pinned',...identity,matchedAX:true})+'\n');});
        return child;
      }};
      if(name==='./p38-installed-driver.cjs')return {...realRequire(name),async cleanupHelper(value){
        assert.equal(value,child);assert.equal(value.p38DetachedGroup,true);assert.equal(value.p38GroupMayContainDescendants,true);cleanups++;
      }};
      return realRequire(name);
    }};
  try {
    vm.runInNewContext(fs.readFileSync(filename,'utf8'),context,{filename});
    const config={taskId:'p38-test',grantId:'p38-grant',sourceHead:'a'.repeat(40),helperPath:file,helperSha256:hash(fs.readFileSync(file)),timeoutMs:1000};
    const services=module.exports.defaultServices(config,{reportDir:root,roots:{temp:root},isolation:{idKey:()=> 'b'.repeat(64)}});
    const helper=await services.startHelper(config,identity);assert.equal((await helper.next()).status,'pinned');
    await assert.rejects(helper.capture(identity,0));assert.equal(events.length,0);
    for(const index of [1,2,3])assert.equal((await helper.capture(identity,index)).phase,owner.PHASES[index]);
    for(let n=3;n<63;n++)await helper.capture(identity,1);
    await assert.rejects(helper.capture(identity,1));assert.equal(events.length,63);
    await helper.quit(identity);await helper.close();await helper.close();
    assert.equal(spawns,1);assert.equal(cleanups,1);
    assert.deepEqual(events.slice(0,3).map(r=>r.capture),['capture-2.png','capture-3.png','capture-4.png']);
    assert.equal(events.at(-1).type,'quit');assert.equal(events.length,64);
    assert.ok(events.every(r=>r.pid===42&&r.windowId===9&&r.target==='main'||r.type==='quit'));
    const second=await services.startHelper(config,identity);await second.next();
    child.stdin._write=(_bytes,_encoding,done)=>{child.stdout.write(JSON.stringify({status:'refused',type:'capture'})+'\n');done();};
    await assert.rejects(second.capture(identity,2));
    await assert.rejects(second.close(),error=>error.observerCode==='helper-cleanup-incomplete');
    assert.equal(cleanups,2); // Even a successful helper/group cleanup cannot certify an unanswered capture.
  } finally {if(child){child.stdout.destroy();child.stdin.destroy();}fs.rmSync(root,{recursive:true,force:true});}
});

function fixture(options={}) {
  const events=[],saved=[],acks=[],config={taskId:'p38-test',grantId:'p38-grant',sourceHead:'a'.repeat(40),
    helperPath:'/tmp/helper',helperSha256:'b'.repeat(64),timeoutMs:1000};
  const manifest={schema:'nemo.p38-fixture-open/1',runId:'12345678-1234-1234-1234-123456789abc',taskKey:'c'.repeat(64),
    sourceSha:config.sourceHead,buildSha:config.sourceHead,pid:42,window:'main',origin:'tauri://localhost/index.html',
    fixtures:[{id:'A'},{id:'B'}],manifestSha256:'d'.repeat(64),checkpoint:0};
  let phase='ready',time=0;
  const helper={next:async()=>({status:'pinned',...identity,matchedAX:true}),inspect:async()=>({pid:42,windowId:9,matchedAX:true,nodes:[]}),
    async capture(_window,index) {
      events.push('capture:'+phase);
      if(options.captureFailure===phase)owner.fail('capture','capture-refused');
      const result={phase:owner.PHASES[index],...identity,capture:reply(index).capture,sha256:hash(png),width:1,height:1};
      return options.mutate?options.mutate(result):result;
    },quit:async()=>events.push('quit'),close:async()=>{events.push('cleanup');if(options.cleanupFailure)owner.fail('cleanup','helper-cleanup-incomplete');}};
  const services={now:()=>time++,taskKey:()=>manifest.taskKey,
    waitForDriver:async()=>({schema:'nemo.p38-operator/1',grant:config.grantId,task:config.taskId,source:config.sourceHead,appPid:42,windowId:9}),
    readOwner:async()=>({taskId:config.taskId,ownerToken:'e'.repeat(32)}),waitForProbe:async()=>({root:'/tmp/p38-fixture-open-test',manifest}),
    startHelper:async()=>helper,waitForCheckpoint:async(_c,_e,_p,p,index)=>{phase=p;events.push('checkpoint:'+p);return {...manifest,checkpoint:index};},
    owner:{receipt:()=>({value:manifest}),visibleEvidence(){if(options.visibilityFailure===phase)throw Error('invisible');},
      acknowledge(_root,expected,_token,decision){events.push('ack:'+phase);acks.push([expected.phase,decision]);if(options.ackFailure)owner.fail('acknowledgment','owner-ack-refused');}},
    save(value){events.push('save:'+phase);if(options.saveFailure===phase)throw Error('private receipt error');saved.push(JSON.parse(JSON.stringify(value)));}};
  if(options.missingCapture)delete helper.capture;
  return {config,services,events,saved,acks};
}
test('actual owner persists A-before/B-held-alert/A-after capture proof BEFORE each accepted ACK',async()=>{
  const f=fixture(),result=await owner.runObserver(f.config,f.services);
  assert.equal(result.status,'observed');assert.equal(result.installedAcceptance,'not-established');
  assert.deepEqual(result.captures.map(c=>c.phase),owner.PHASES.slice(1));
  for(const phase of owner.PHASES.slice(1)) {
    const cap=f.events.indexOf('capture:'+phase),ack=f.events.indexOf('ack:'+phase);
    assert.ok(cap>=0&&cap<ack);assert.ok(f.events.slice(cap+1,ack).includes('save:'+phase));
    assert.ok(f.saved.some(s=>s.captures.some(c=>c.phase===phase)));
  }
});
test('capture/identity/proof/visibility/receipt failure refuses before accepted ACK or later phase',async()=>{
  for(const options of [{captureFailure:'a-opened'},{captureFailure:'b-opened'},{captureFailure:'b-closed'},{missingCapture:true},
    {mutate:r=>({...r,pid:43})},{mutate:r=>({...r,windowId:10})},{mutate:r=>({...r,phase:'b-closed'})},
    {mutate:r=>({...r,capture:'private.png'})},{mutate:r=>({...r,sha256:'bad'})},{mutate:r=>({...r,width:0})},
    {mutate:r=>({...r,height:16385})},{saveFailure:'b-opened'}]) {
    const f=fixture(options),result=await owner.runObserver(f.config,f.services);
    assert.equal(result.status,'partial');assert.ok(result.firstFailure);assert.equal(result.cleanup,'complete');
    assert.equal(f.events.includes('quit'),false);
    const failed=options.captureFailure||options.saveFailure||'a-opened';
    assert.equal(f.acks.some(([phase,decision])=>phase===failed&&decision==='accepted'),false);
    assert.ok(!f.events.includes('checkpoint:'+owner.PHASES[owner.PHASES.indexOf(failed)+1]));
  }
  const f=fixture({visibilityFailure:'b-opened'});await owner.runObserver(f.config,f.services);
  assert.equal(f.events.includes('capture:b-opened'),false);assert.deepEqual(f.acks.at(-1),['b-opened','refused']);
});
test('capture first failure survives cleanup failure and never reports installed acceptance',async()=>{
  const f=fixture({captureFailure:'b-opened',cleanupFailure:true}),result=await owner.runObserver(f.config,f.services);
  assert.equal(result.firstFailure.stage,'capture');assert.equal(result.firstFailure.code,'capture-refused');
  assert.equal(result.cleanup,'incomplete');assert.equal(result.installedAcceptance,'not-established');
});
