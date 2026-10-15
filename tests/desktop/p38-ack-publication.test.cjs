'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const vm=require('node:vm');
const {createRequire}=require('node:module');
const test=require('node:test');
const writer=path.join(__dirname,'p38-fixture-open-installed.cjs');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const token='d'.repeat(32),uuid='12345678-1234-1234-1234-123456789abc';

function fixture(t) {
  const base=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'p38-ack-test-')));
  t.after(()=>fs.rmSync(base,{recursive:true,force:true}));
  const parent=path.join(base,'tmp'),root=path.join(parent,'p38-fixture-open-'+uuid);
  fs.mkdirSync(parent);fs.mkdirSync(root);
  const fixtures=['A','B'].map(id=>{
    const file=path.join(root,id+'.json');
    fs.copyFileSync(path.join(__dirname,'../fixtures/p38/native-project',id+'.json'),file);
    const stat=fs.statSync(file);
    return {id,file,sha256:hash(fs.readFileSync(file)),device:stat.dev,inode:stat.ino};
  });
  const pins={schema:'nemo.p38-fixture-open/1',runId:uuid,taskKey:'a'.repeat(64),
    sourceSha:'b'.repeat(40),buildSha:'b'.repeat(40),pid:42,window:'main',
    origin:'tauri://localhost/index.html',fixtures};
  const manifest={...pins,manifestSha256:hash(JSON.stringify(pins)),checkpoint:0,waiting:true,failed:false};
  fs.writeFileSync(path.join(root,'manifest.json'),JSON.stringify(manifest));
  const expected={...pins,phase:'ready',windowId:9};
  const snapshot={pid:42,windowId:9,matchedAX:true,complete:true,truncated:false,
    depthClipped:false,childrenUnreadable:false,foreignChildren:false,
    projectTabs:{path:'0',token:uuid,unique:true},nodes:[{path:'0',role:'AXGroup',control:'Project tabs'}]};
  return {base,parent,root,ack:path.join(root,'ack.json'),expected,snapshot,manifest};
}

// Load the actual writer; only filesystem operation boundaries are instrumented.
function instrument(f,hook=()=>{}) {
  const events=[],fds=new Set();let synced=false,publications=0,temp;
  const sample=op=>{
    let bytes=null,valid=false;
    if(fs.existsSync(f.ack)&&fs.lstatSync(f.ack).isFile()) {
      bytes=fs.readFileSync(f.ack);try{JSON.parse(bytes);valid=true;}catch{}
    }
    events.push({op,present:fs.existsSync(f.ack),bytes:bytes?.length,valid,synced});
  };
  const shim={...fs};
  for(const name of ['openSync','writeFileSync','fsyncSync','closeSync','linkSync','unlinkSync',
    'lstatSync','realpathSync','fstatSync'])shim[name]=(...args)=>{
    hook({name,args,when:'before',f,temp});
    let value;
    if(name==='writeFileSync'&&fds.has(args[0])) {
      const bytes=Buffer.from(args[1]);
      fs.writeSync(args[0],bytes.subarray(0,Math.floor(bytes.length/2)));
      sample('partial-write');
      fs.writeSync(args[0],bytes.subarray(Math.floor(bytes.length/2)));
    } else value=fs[name](...args);
    if(name==='openSync'&&(args[1]&fs.constants.O_CREAT)) {fds.add(value);temp=args[0];}
    if(name==='fsyncSync'&&fds.has(args[0]))synced=true;
    if(name==='linkSync'&&args[1]===f.ack)publications++;
    if(name==='closeSync')fds.delete(args[0]);
    hook({name,args,value,when:'after',f,temp});sample(name);return value;
  };
  const module={exports:{}},localRequire=createRequire(writer);
  vm.runInNewContext(fs.readFileSync(writer,'utf8'),{module,exports:module.exports,Buffer,
    __dirname,__filename:writer,require:name=>name==='node:fs'?shim:
      name==='node:crypto'?{...crypto,randomUUID:()=>uuid}:localRequire(name)},{filename:writer});
  return {run:(decision='accepted')=>module.exports.acknowledge(f.root,f.expected,token,decision,f.snapshot),
    events,get publications(){return publications;},get temp(){return temp;},get openFds(){return fds.size;}};
}
function noTemps(f) {assert.deepEqual(fs.readdirSync(f.root).filter(n=>n.startsWith('.ack-')),[]);}

test('final ACK stays absent until complete fsynced JSON is atomically published',t=>{
  const f=fixture(t),r=instrument(f);
  r.run();
  const visible=r.events.filter(e=>e.present);
  assert.ok(visible.length>0);
  assert.ok(visible.every(e=>e.valid&&e.synced),
    'ACK must stay absent until a complete fsynced payload is published');
  assert.equal(r.publications,1);assert.equal(r.openFds,0);noTemps(f);
  assert.deepEqual(JSON.parse(fs.readFileSync(f.ack)),{
    runId:uuid,taskKey:f.expected.taskKey,sourceSha:f.expected.sourceSha,buildSha:f.expected.buildSha,
    manifestSha256:f.manifest.manifestSha256,phase:'ready',ownerToken:token,decision:'accepted'});
  assert.equal(fs.statSync(f.ack).mode&0o777,0o600);
});

for(const kind of ['file','symlink','directory'])test(`existing ${kind} ACK is never overwritten`,t=>{
  const f=fixture(t),foreign=path.join(f.base,'foreign');fs.writeFileSync(foreign,'foreign');
  if(kind==='file')fs.writeFileSync(f.ack,'existing ACK');
  else if(kind==='symlink')fs.symlinkSync(foreign,f.ack);
  else fs.mkdirSync(f.ack);
  const before=fs.lstatSync(f.ack),r=instrument(f);
  assert.throws(()=>r.run(),{code:'EEXIST'});
  assert.equal(fs.lstatSync(f.ack).ino,before.ino);
  assert.equal(fs.readFileSync(foreign,'utf8'),'foreign');
  if(kind==='file')assert.equal(fs.readFileSync(f.ack,'utf8'),'existing ACK');
  assert.equal(r.publications,0);assert.equal(r.openFds,0);noTemps(f);
});

for(const kind of ['file','symlink','directory'])test(`private temporary ${kind} collision is left untouched`,t=>{
  const f=fixture(t),temporary=path.join(f.root,`.ack-${uuid}.tmp`),foreign=path.join(f.base,'foreign');
  fs.writeFileSync(foreign,'foreign');
  if(kind==='file')fs.writeFileSync(temporary,'existing temporary');
  else if(kind==='symlink')fs.symlinkSync(foreign,temporary);
  else fs.mkdirSync(temporary);
  const before=fs.lstatSync(temporary),r=instrument(f);
  assert.throws(()=>r.run());assert.equal(fs.lstatSync(temporary).ino,before.ino);
  assert.equal(fs.readFileSync(foreign,'utf8'),'foreign');
  if(kind==='file')assert.equal(fs.readFileSync(temporary,'utf8'),'existing temporary');
  assert.equal(fs.existsSync(f.ack),false);assert.equal(r.publications,0);assert.equal(r.openFds,0);
});

for(const operation of ['writeFileSync','fsyncSync','linkSync'])test(`${operation} failure never publishes and cleans owned temporary`,t=>{
  const f=fixture(t),error=new Error(operation+' failed'),r=instrument(f,event=>{
    if(event.name===operation&&event.when==='before')throw error;
  });
  assert.throws(()=>r.run(),value=>value===error);
  assert.equal(fs.existsSync(f.ack),false);assert.equal(r.publications,0);assert.equal(r.openFds,0);noTemps(f);
});

test('a concurrent ACK arriving at publication wins without replacement or retry',t=>{
  const f=fixture(t);let attempts=0;
  const r=instrument(f,({name,when})=>{
    if(name==='linkSync'&&when==='before') {attempts++;fs.writeFileSync(f.ack,'concurrent ACK',{flag:'wx'});}
  });
  assert.throws(()=>r.run(),{code:'EEXIST'});
  assert.equal(attempts,1);assert.equal(fs.readFileSync(f.ack,'utf8'),'concurrent ACK');
  assert.equal(r.publications,0);assert.equal(r.openFds,0);noTemps(f);
});

test('a second acknowledgment refuses without changing the first inode or payload',t=>{
  const f=fixture(t),r=instrument(f);r.run();
  const before=fs.statSync(f.ack),bytes=fs.readFileSync(f.ack);
  assert.throws(()=>r.run('refused'),{code:'EEXIST'});
  assert.equal(fs.statSync(f.ack).ino,before.ino);assert.deepEqual(fs.readFileSync(f.ack),bytes);
  assert.equal(r.publications,1);assert.equal(r.openFds,0);noTemps(f);
});

test('native immediate consumption sees complete correlated JSON and is not retried',t=>{
  const f=fixture(t);let consumed;
  const r=instrument(f,({name,when})=>{
    if(name==='linkSync'&&when==='after') {consumed=JSON.parse(fs.readFileSync(f.ack));fs.unlinkSync(f.ack);}
  });
  r.run('refused');assert.equal(consumed.decision,'refused');assert.equal(consumed.ownerToken,token);
  assert.equal(consumed.manifestSha256,f.manifest.manifestSha256);
  assert.equal(r.publications,1);assert.equal(fs.existsSync(f.ack),false);assert.equal(r.openFds,0);noTemps(f);
});

test('replaced temporary identity refuses publication and never deletes the replacement',t=>{
  const f=fixture(t);let saved,temporary;
  const r=instrument(f,({name,when,temp})=>{
    if(name==='fsyncSync'&&when==='after') {
      temporary=temp;saved=path.join(f.base,'owned-original');fs.renameSync(temp,saved);
      fs.writeFileSync(temp,'foreign temporary');
    }
  });
  assert.throws(()=>r.run(),{code:'probe-ack-identity-refused'});
  assert.equal(fs.existsSync(f.ack),false);assert.equal(fs.readFileSync(temporary,'utf8'),'foreign temporary');
  assert.equal(JSON.parse(fs.readFileSync(saved)).decision,'accepted');assert.equal(r.openFds,0);
});

test('root replacement after fsync refuses publication and cleanup in the foreign root',t=>{
  const f=fixture(t),saved=path.join(f.parent,'original-root');let temporary;
  const r=instrument(f,({name,when,temp})=>{
    if(name==='fsyncSync'&&when==='after') {
      temporary=path.basename(temp);fs.renameSync(f.root,saved);fs.mkdirSync(f.root);
      fs.writeFileSync(path.join(f.root,temporary),'foreign temporary');
    }
  });
  assert.throws(()=>r.run(),{code:'probe-root-refused'});
  assert.equal(fs.existsSync(f.ack),false);assert.equal(fs.readFileSync(path.join(f.root,temporary),'utf8'),'foreign temporary');
  assert.equal(JSON.parse(fs.readFileSync(path.join(saved,temporary))).decision,'accepted');
  assert.equal(r.openFds,0);
});

for(const failure of ['close','unlink'])test(`post-publication ${failure} failure preserves complete ACK and is not retried`,t=>{
  const f=fixture(t),error=new Error(failure+' failed');let attempted=0;
  const r=instrument(f,({name,when,temp,args})=>{
    if(name==='linkSync'&&when==='before')attempted++;
    if(failure==='close'&&name==='closeSync'&&when==='after'&&temp)throw error;
    if(failure==='unlink'&&name==='unlinkSync'&&when==='before'&&args[0]===temp)throw error;
  });
  assert.throws(()=>r.run(),value=>value===error);
  assert.equal(JSON.parse(fs.readFileSync(f.ack)).decision,'accepted');
  assert.equal(attempted,1);assert.equal(r.publications,1);assert.equal(r.openFds,0);
  if(failure==='close')noTemps(f);else assert.equal(fs.existsSync(r.temp),true);
});

test('write error stays primary when close and cleanup also fail',t=>{
  const f=fixture(t),primary=new Error('write failure');
  const r=instrument(f,({name,when,temp})=>{
    if(name==='writeFileSync'&&when==='before')throw primary;
    if(name==='closeSync'&&when==='after'&&temp)throw new Error('close failure');
    if(name==='unlinkSync'&&when==='before')throw new Error('cleanup failure');
  });
  assert.throws(()=>r.run(),value=>value===primary);
  assert.equal(fs.existsSync(f.ack),false);assert.equal(r.publications,0);assert.equal(r.openFds,0);
});

for(const field of ['sourceSha','pid','phase'])test(`invalid ${field} does not create any ACK or temporary`,t=>{
  const f=fixture(t);f.expected[field]=field==='pid'?43:field==='phase'?'a-opened':'e'.repeat(40);
  const r=instrument(f);assert.throws(()=>r.run(),{code:'probe-checkpoint-refused'});
  assert.equal(fs.existsSync(f.ack),false);assert.equal(r.temp,undefined);assert.equal(r.openFds,0);noTemps(f);
});

test('incomplete visible evidence does not create an ACK or temporary',t=>{
  const f=fixture(t);f.snapshot.complete=false;const r=instrument(f);
  assert.throws(()=>r.run(),{code:'probe-visible-state-refused'});
  assert.equal(fs.existsSync(f.ack),false);assert.equal(r.temp,undefined);noTemps(f);
});

test('symlink root is refused without writing through it',t=>{
  const f=fixture(t),original=path.join(f.parent,'original');fs.renameSync(f.root,original);
  fs.symlinkSync(original,f.root);const r=instrument(f);
  assert.throws(()=>r.run(),{code:'probe-root-refused'});
  assert.equal(fs.existsSync(f.ack),false);assert.equal(r.temp,undefined);noTemps(f);
});
