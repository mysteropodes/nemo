#!/usr/bin/env node
'use strict';
// Manual T12 / #2232 point 8 installed video operator. Never part of automated
// desktop discovery. Product (appRoot at the frozen protected SHA) and driver/helper
// source identities stay separate, as in reference-/png-ui-installed-driver.cjs.
// Usage:
//   node tests/desktop/video-installed-driver.cjs --make-fixtures /abs/ffmpeg /abs/fixture-dir
//   node tests/desktop/video-installed-driver.cjs /abs/config.json
const fs=require('node:fs'),path=require('node:path'),readline=require('node:readline'),crypto=require('node:crypto');
const {spawnSync}=require('node:child_process');
const policy=require('./video-installed-policy.cjs');
const {check,hash}=policy;
const {frozenModules}=require('./reference-installed-driver.cjs');
// The existing owned-window helper transport (--png-persistent, nemo.png-ui-window-helper/1);
// no video-specific helper mode exists, and its label allowlist covers this workflow.
const {helperTransport}=require('./png-ui-installed-driver.cjs');
const ROOT=path.resolve(__dirname,'../..');
const SIDECAR='src-tauri/binaries/ffmpeg-aarch64-apple-darwin';
function json(file){return JSON.parse(policy.readFile(file,16384).bytes);}
function decodeWith(ffmpeg){
  return file=>{
    const r=spawnSync(ffmpeg,['-v','error','-i',file,'-f','rawvideo','-pix_fmt','rgba','-'],{stdio:['ignore','pipe','pipe'],timeout:60000,maxBuffer:4*1024*1024});
    check(!r.error&&r.status===0&&Buffer.isBuffer(r.stdout),'video-fixture-refused');return r.stdout;
  };
}
// Generate both lossless fixtures exactly as native_media_host_tests.rs does, then
// prove every decoded texel against the formula before printing the config pins.
function makeFixtures(ffmpeg,dir){
  check(path.isAbsolute(ffmpeg)&&path.isAbsolute(dir)&&fs.realpathSync(dir)===dir,'video-fixture-refused');
  const out={};
  for(const label of ['A','B']){
    const spec=policy.SOURCES[label],file=path.join(dir,spec.file);
    check(!fs.existsSync(file),'video-fixture-refused');
    const r=spawnSync(ffmpeg,['-v','error','-y','-f','lavfi','-i',spec.lavfi,'-c:v','png','-f','mov',file],{stdio:['ignore','ignore','pipe'],timeout:60000});
    check(!r.error&&r.status===0,'video-fixture-refused');
    fs.chmodSync(file,0o400);
    out[label]={path:file,sha256:hash(policy.readFile(file).bytes),...policy.verifyFixtureFrames(label,decodeWith(ffmpeg)(file))};
  }
  return {schema:'nemo.video-installed-fixtures/1',fixtures:out};
}
function packageProof(c){
  policy.verifyInputs(c);
  const driver=require('../../scripts/nemo/lib/identity.cjs').sourceIdentity();
  check(!driver.dirty&&driver.head===c.driverHead,'video-driver-source-refused');
  let modules;try{modules=frozenModules(c);}catch{check(false,'video-app-source-refused');}
  const source=modules.identity.sourceIdentity(),app=modules.harness.packagedApp(c.app,modules.runtime),proof=json(c.buildProof);
  check(app.executableSha256===c.executableSha256&&proof.status==='pass'&&proof.details?.source?.head===c.sourceHead&&
    proof.details.source.dirty===false&&proof.details.cleanup?.stopped&&proof.details.cleanup.released&&
    proof.artifacts?.some(x=>x.sha256===c.executableSha256&&x.path.endsWith('/Contents/MacOS/nemo')),'video-build-refused');
  const helper=policy.readFile(c.windowHelper),hp=json(c.helperProof);
  const sourceSha256=require('./p38-owned-window-build.cjs').verifySourceProof(hp);
  check(hash(helper.bytes)===c.helperSha256&&hp.schema==='nemo.png-ui-window-helper/1'&&hp.sourceHead===c.driverHead&&
    hp.sourceSha256===sourceSha256&&hp.executableSha256===c.helperSha256,'video-helper-refused');
  fs.accessSync(c.windowHelper,fs.constants.X_OK);
  const ffmpeg=path.join(c.appRoot,SIDECAR);fs.accessSync(ffmpeg,fs.constants.X_OK);
  const driverProof={driverSha256:hash(fs.readFileSync(__filename)),policySha256:hash(fs.readFileSync(path.join(__dirname,'video-installed-policy.cjs'))),
    sidecarSha256:hash(fs.readFileSync(ffmpeg))};
  return {...app,source,build:modules.identity.buildIdentity(),modules,ffmpeg,helperProof:hp,driverProof,buildProofSha256:hash(policy.readFile(c.buildProof).bytes)};
}
function atomic(root,name,value){
  const stat=fs.lstatSync(root);check(stat.isDirectory()&&!stat.isSymbolicLink()&&fs.realpathSync(root)===root,'video-root-refused');
  const tmp=path.join(root,'.'+name+'-'+crypto.randomUUID());
  fs.writeFileSync(tmp,JSON.stringify(value,null,2)+'\n',{flag:'wx',mode:0o600});fs.renameSync(tmp,path.join(root,name));
}
// One attempt per grant, recorded before any app start.
function journal(c,isolation){
  policy.authorize(c);const parent=isolation.RUNTIME_ROOT;
  fs.mkdirSync(parent,{recursive:true,mode:0o700});check(fs.realpathSync(parent)===parent,'video-root-refused');
  const root=path.join(parent,'video-ui-grants');try{fs.mkdirSync(root,{mode:0o700});}catch(e){if(e.code!=='EEXIST')throw e;}
  const stat=fs.lstatSync(root);check(stat.isDirectory()&&!stat.isSymbolicLink()&&fs.realpathSync(root)===root,'video-root-refused');
  let fd;try{fd=fs.openSync(path.join(root,hash(c.grant.id)+'.json'),fs.constants.O_CREAT|fs.constants.O_EXCL|fs.constants.O_WRONLY|fs.constants.O_NOFOLLOW,0o600);}
  catch(e){if(e.code==='EEXIST')check(false,'video-grant-already-attempted');throw e;}
  try{fs.writeFileSync(fd,JSON.stringify({issue:policy.ISSUE,grant:c.grant.id,task:c.taskId,appSource:c.sourceHead,driverSource:c.driverHead}));fs.fsyncSync(fd);}finally{fs.closeSync(fd);}
}
async function runOperator(c,s){
  policy.authorize(c);Object.freeze(c.fixtures.A);Object.freeze(c.fixtures.B);Object.freeze(c.fixtures);Object.freeze(c.grant);Object.freeze(c);
  const receipt={schema:'nemo.video-ui-operator/1',issue:policy.ISSUE,grant:c.grant.id,source:c.sourceHead,driver:c.driverHead,task:c.taskId,
    status:'partial',installedAcceptance:'not-established',workflowCoverage:'partial',steps:policy.STEPS,covered:policy.COVERED,blocked:policy.BLOCKED,
    plan:policy.PLAN,startAttempts:0,actions:[],artifacts:[],cleanup:'not-required',helperCleanup:'not-required'};
  let app,controller,instance,helper,timer,windowId,pid,last=null;const sheet={token:null};let workspaceReady=false,completed=false;
  let abort;const deadline=new Promise((_,reject)=>{abort=()=>reject(Object.assign(Error('video-watchdog'),{code:'video-watchdog'}));});
  const bound=x=>Promise.race([x,deadline]);
  const save=()=>{if(workspaceReady)s.save(receipt);};
  try{
    timer=s.setTimer(abort,c.timeoutMs);
    await bound(s.transport());app=await bound(s.package());await bound(s.journal(app));
    const w=await bound(s.workspace(app));workspaceReady=true;receipt.workspace={projectSha256:w.projectSha256,fixtures:w.fixtures};
    if(app.driverProof)receipt.driverProof=app.driverProof;save();
    s.feedback({schema:'nemo.video-ui-workspace/1',project:'A.json',saved:'Saved.json',output:'output',blocked:policy.BLOCKED.map(x=>x.step)});
    controller=s.controller(app);receipt.startAttempts=1;save();instance=await bound(controller.start(c.taskId,['desktop-input','gpu-reference']));
    const snapshot=await bound(controller.status(instance));s.validate(snapshot.value,instance,app);policy.bindCache(c,snapshot.value.app);
    pid=snapshot.value?.app?.pid;check(Number.isInteger(pid)&&pid>0&&snapshot.value.runtime?.childPid===pid,'video-pid-refused');receipt.pid=pid;save();
    helper=await bound(s.helper(pid));const pin=await bound(helper.pin());windowId=pin.windowId;policy.identity(pin,pid,windowId);receipt.windowId=windowId;save();
    for(;;){
      check(receipt.actions.length<96,'video-action-budget');const a=await bound(s.next());policy.verifyAction(c,a,sheet,last);
      const status=await bound(controller.status(instance));s.validate(status.value,instance,app);check(status.value.app.pid===pid,'video-pid-changed');policy.bindCache(c,status.value.app);
      const guard=await bound(helper.action(windowId,{type:'check'}));policy.identity(guard,pid,windowId);check(guard.status==='ok','video-window-refused');
      if(a.type==='stop'){completed=true;break;}
      if(a.type==='artifacts'){
        const result=policy.artifacts(c,a.phase,status.value.app);receipt.artifacts.push(result);receipt.actions.push({type:a.type,phase:a.phase});save();
        s.feedback({phase:result.phase,frames:result.frames,savedSha256:result.savedSha256});continue;
      }
      const selected=last?.nodes?.find(n=>n.path===a.element);
      receipt.inFlight={type:a.type,target:a.target||'main',...(selected?{role:selected.role,control:selected.control,element:selected.path}:{})};save();
      const request={...a};
      if(a.type==='capture'){request.capture='capture-'+(receipt.actions.length+1)+'.png';delete request.name;check(!fs.existsSync(path.join(c.reportRoot,request.capture)),'video-capture-reused');}
      if(a.type==='press'){request.expectedRole=selected.role;request.expectedControl=selected.control;}
      if(a.type==='go-to-folder')sheet.locationAttempted=true;
      const result=await bound(helper.action(windowId,request));policy.identity(result,pid,windowId);check(result.status==='ok'&&result.type===a.type,'video-action-refused');
      policy.observeSheet(a,sheet,result);policy.witnessAction(c,a,last);const clean=policy.sanitizeHelper(result,a.target||'main');last=a.type==='inspect'?{...clean,target:a.target||'main'}:null;
      if(a.type==='capture'){check(result.capture===request.capture,'video-capture-refused');clean.captureSha256=hash(policy.readFile(path.join(c.reportRoot,request.capture)).bytes);clean.evidenceOnly=true;}
      receipt.orderedUiWitnesses=policy.witnessSummary(c);
      receipt.actions.push({...receipt.inFlight,evidence:clean});delete receipt.inFlight;save();s.feedback(clean);
    }
    const phases=new Set(receipt.artifacts.map(x=>x.phase));
    check(['saved','exported'].every(x=>phases.has(x)),'video-workflow-incomplete');
    // Covered steps only; BLOCKED steps keep #2232 point 8 open whatever happens here.
    receipt.status='operator-complete';receipt.ownerReview='required';
  }catch(e){receipt.reason=/^video-[a-z-]{1,64}$/.test(e?.code||'')?e.code:'video-operation-refused';}
  finally{
    s.clearTimer(timer);
    if(helper){try{await helper.close();receipt.helperCleanup='complete';}catch{receipt.helperCleanup='incomplete';}}
    if(controller){try{await controller.cleanup();receipt.cleanup='complete';}catch{receipt.cleanup='incomplete';}}
    if(!completed||receipt.cleanup==='incomplete'||receipt.helperCleanup==='incomplete'){receipt.status='partial';receipt.installedAcceptance='not-established';}
    try{await s.close();receipt.inputCleanup='complete';}
    catch{receipt.inputCleanup='incomplete';receipt.status='partial';receipt.reason||='video-input-close-refused';}
    try{save();}catch{receipt.evidenceSave='incomplete';receipt.status='partial';receipt.reason||='video-terminal-save-refused';}
  }
  return receipt;
}
function services(c,io={input:process.stdin,write:v=>process.stdout.write(JSON.stringify(v)+'\n'),isTTY:process.stdin.isTTY}){
  const input=readline.createInterface({input:io.input,terminal:false}),lines=input[Symbol.asyncIterator]();let app;
  async function next(){const x=await lines.next();check(!x.done&&x.value.length<=8192,'video-input-eof');return JSON.parse(x.value);}
  return {async transport(){check(io.isTTY===true,'video-transport-refused');const nonce=crypto.randomUUID();io.write({schema:'nemo.video-ui-transport/1',status:'waiting',nonce});
      const r=await next();check(r.transport==='video-ui-ready'&&r.nonce===nonce,'video-transport-refused');},
    package(){app=packageProof(c);return app;},journal(){journal(c,app.modules.isolation);},
    workspace(){return policy.prepareWorkspace(c,decodeWith(app.ffmpeg));},
    controller(){return app.modules.harness.createController({root:c.appRoot,app:app.bundle});},
    validate(snapshot,instance){policy.verifyWorkspace(c);const driver=require('../../scripts/nemo/lib/identity.cjs').sourceIdentity();
      check(!driver.dirty&&driver.head===c.driverHead&&hash(policy.readFile(c.windowHelper).bytes)===c.helperSha256,'video-driver-source-refused');
      const source=app.modules.identity.sourceIdentity();check(!source.dirty&&source.head===c.sourceHead,'video-app-source-refused');
      app.modules.harness.validateSnapshot(snapshot,instance,{isolation:app.modules.isolation,runtime:app.modules.runtime,source:app.source,build:app.build,app});},
    helper(pid){return helperTransport(c,pid,c.reportRoot);},next,feedback:io.write,
    save(r){atomic(c.reportRoot,'receipt.json',r);},setTimer:setTimeout,clearTimer:clearTimeout,async close(){input.close();}};
}
async function main(){
  check(process.platform==='darwin','video-cli-refused');
  if(process.argv.length===5&&process.argv[2]==='--make-fixtures'){process.stdout.write(JSON.stringify(makeFixtures(process.argv[3],process.argv[4]))+'\n');return;}
  check(process.argv.length===3,'video-cli-refused');
  const c=json(path.resolve(process.argv[2]));policy.authorize(c);const r=await runOperator(c,services(c));
  process.stdout.write(JSON.stringify(r)+'\n');process.exitCode=r.status==='operator-complete'?0:1;
}
if(require.main===module)main().catch(()=>{process.stderr.write('Video UI operator refused; no installed acceptance.\n');process.exitCode=1;});
module.exports={SIDECAR,ROOT,decodeWith,makeFixtures,packageProof,journal,runOperator,services};
