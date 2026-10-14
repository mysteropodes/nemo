#!/usr/bin/env node
'use strict';
// Manual #1959 operator. Product and helper source identities remain separate.
const fs=require('node:fs'),path=require('node:path'),readline=require('node:readline'),crypto=require('node:crypto');
const {spawn,execFileSync}=require('node:child_process');
const policy=require('./png-ui-installed-policy.cjs');
const {check,hash}=policy;
const {cleanupHelper}=require('./p38-helper-protocol.cjs');
const {frozenModules}=require('./reference-installed-driver.cjs');
const ROOT=path.resolve(__dirname,'../..');
function json(file){return JSON.parse(policy.readFile(file,16384).bytes);}
function packageProof(c){
  policy.verifyInputs(c);
  const driver=require('../../scripts/nemo/lib/identity.cjs').sourceIdentity();
  check(!driver.dirty&&driver.head===c.driverHead,'png-driver-source-refused');
  const modules=frozenModules(c),source=modules.identity.sourceIdentity(),app=modules.harness.packagedApp(c.app,modules.runtime),proof=json(c.buildProof);
  check(app.executableSha256===c.executableSha256&&proof.status==='pass'&&proof.details.source.head===c.sourceHead&&
    proof.details.source.dirty===false&&proof.details.cleanup.stopped&&proof.details.cleanup.released&&
    proof.artifacts.some(x=>x.sha256===c.executableSha256&&x.path.endsWith('/Contents/MacOS/nemo')),'png-build-refused');
  const sourceBytes=fs.readFileSync(path.join(__dirname,'p38-owned-window.swift')),helper=policy.readFile(c.windowHelper),hp=json(c.helperProof);
  check(hash(helper.bytes)===c.helperSha256&&hp.schema==='nemo.png-ui-window-helper/1'&&hp.sourceHead===c.driverHead&&
    hp.sourceSha256===hash(sourceBytes)&&hp.executableSha256===c.helperSha256,'png-helper-refused');
  fs.accessSync(c.windowHelper,fs.constants.X_OK);
  return {...app,source,build:modules.identity.buildIdentity(),modules,helperProof:hp,buildProofSha256:hash(policy.readFile(c.buildProof).bytes)};
}
function atomic(root,name,value){
  const stat=fs.lstatSync(root);check(stat.isDirectory()&&!stat.isSymbolicLink()&&fs.realpathSync(root)===root,'png-root-refused');
  const tmp=path.join(root,'.'+name+'-'+crypto.randomUUID());
  fs.writeFileSync(tmp,JSON.stringify(value,null,2)+'\n',{flag:'wx',mode:0o600});fs.renameSync(tmp,path.join(root,name));
}
function journal(c){
  policy.authorize(c);const I=require('../../scripts/nemo/lib/isolation.cjs'),parent=I.RUNTIME_ROOT;
  fs.mkdirSync(parent,{recursive:true,mode:0o700});check(fs.realpathSync(parent)===parent,'png-root-refused');
  const root=path.join(parent,'png-ui-grants');try{fs.mkdirSync(root,{mode:0o700});}catch(e){if(e.code!=='EEXIST')throw e;}
  const stat=fs.lstatSync(root);check(stat.isDirectory()&&!stat.isSymbolicLink()&&fs.realpathSync(root)===root,'png-root-refused');
  const fd=fs.openSync(path.join(root,hash(c.grant.id)+'.json'),fs.constants.O_CREAT|fs.constants.O_EXCL|fs.constants.O_WRONLY|fs.constants.O_NOFOLLOW,0o600);
  try{fs.writeFileSync(fd,JSON.stringify({issue:1959,grant:c.grant.id,task:c.taskId,appSource:c.sourceHead,driverSource:c.driverHead}));fs.fsyncSync(fd);}finally{fs.closeSync(fd);}
}
function helperTransport(c,pid,reportRoot){
  const pinned=policy.readFile(c.windowHelper);check(hash(pinned.bytes)===c.helperSha256,'png-helper-refused');
  const child=spawn(c.windowHelper,['--png-persistent',String(pid)],{detached:true,stdio:['pipe','pipe','ignore'],
    env:{...process.env,NEMO_PNG_UI_GRANT:c.grant.id,NEMO_PNG_UI_REPORT_ROOT:reportRoot,
      NEMO_P38_GRANT:'',NEMO_REFERENCE_GRANT:''}});
  let failed=null;child.on('error',e=>{failed=e;});
  const input=readline.createInterface({input:child.stdout,terminal:false}),lines=input[Symbol.asyncIterator]();let closed=false;
  async function reply(){const next=await lines.next();check(!failed&&!next.done&&next.value.length<=65536,'png-helper-eof');return JSON.parse(next.value);}
  return {child,async pin(){const after=policy.readFile(c.windowHelper);check(after.dev===pinned.dev&&after.ino===pinned.ino&&hash(after.bytes)===c.helperSha256,'png-helper-refused');
      const r=await reply();check(r.status==='pinned'&&r.pid===pid&&r.matchedAX===true&&Number.isInteger(r.windowId)&&r.windowId>0,'png-window-refused');return r;},
    async action(windowId,a){check(child.stdin.write(JSON.stringify({...a,pid,windowId})+'\n'),'png-helper-backpressure');return reply();},
    async close(){if(closed)return;closed=true;input.close();child.stdin.end();await cleanupHelper(child);}};
}
async function runOperator(c,s){
  policy.authorize(c);
  Object.freeze(c.grant);Object.freeze(c);
  const receipt={schema:'nemo.png-ui-operator/1',issue:1959,grant:c.grant.id,source:c.sourceHead,driver:c.driverHead,task:c.taskId,
    status:'partial',installedAcceptance:'not-established',startAttempts:0,actions:[],artifacts:[],cleanup:'not-required',helperCleanup:'not-required'};
  let app,controller,instance,helper,timer,windowId,pid,last=null;const sheet={token:null};let journaled=false,workspaceReady=false,completed=false;
  let abort;const deadline=new Promise((_,reject)=>{abort=()=>reject(Object.assign(Error('png-watchdog'),{code:'png-watchdog'}));});
  const bound=x=>Promise.race([x,deadline]);
  const save=()=>{if(journaled&&workspaceReady)s.save(receipt);};
  try{
    timer=s.setTimer(abort,c.timeoutMs);
    await bound(s.transport());app=await bound(s.package());await bound(s.journal());journaled=true;
    await bound(s.workspace());workspaceReady=true;save();s.feedback({schema:'nemo.png-ui-workspace/1',fixture:'A.json',before:'Saved.json',after:'SavedAfter.json',output:'output'});
    controller=s.controller(app);receipt.startAttempts=1;save();instance=await bound(controller.start(c.taskId,['desktop-input','gpu-reference']));
    const snapshot=await bound(controller.status(instance));s.validate(snapshot.value,instance,app);
    pid=snapshot.value?.app?.pid;check(Number.isInteger(pid)&&pid>0&&snapshot.value.runtime?.childPid===pid,'png-pid-refused');receipt.pid=pid;save();
    helper=await bound(s.helper(pid));const pin=await bound(helper.pin());windowId=pin.windowId;policy.identity(pin,pid,windowId);receipt.windowId=windowId;save();
    for(;;){
      check(receipt.actions.length<64,'png-action-budget');const a=await bound(s.next());policy.verifyAction(c,a,sheet,last);
      const status=await bound(controller.status(instance));s.validate(status.value,instance,app);check(status.value.app.pid===pid,'png-pid-changed');
      const guard=await bound(helper.action(windowId,{type:'check'}));policy.identity(guard,pid,windowId);check(guard.status==='ok','png-window-refused');
      if(a.type==='stop'){completed=true;break;}
      if(a.type==='artifacts'){
        const result=policy.artifacts(c,a.phase,app);receipt.artifacts.push(result);receipt.actions.push({type:a.type,phase:a.phase});save();s.feedback(result);continue;
      }
      const selected=last?.nodes?.find(n=>n.path===a.element);
      receipt.inFlight={type:a.type,target:a.target||'main',...(selected?{role:selected.role,control:selected.control,element:selected.path}:{})};save();
      const request={...a};if(a.type==='capture'){request.capture='capture-'+(receipt.actions.length+1)+'.png';delete request.name;check(!fs.existsSync(path.join(c.reportRoot,request.capture)),'png-capture-reused');}
      if(a.type==='go-to-folder')sheet.locationAttempted=true;
      const result=await bound(helper.action(windowId,request));policy.identity(result,pid,windowId);check(result.status==='ok'&&result.type===a.type,'png-action-refused');
      policy.observeSheet(a,sheet,result);policy.witnessAction(c,a,last);const clean=policy.sanitizeHelper(result,a.target||'main');last=a.type==='inspect'?{...clean,target:a.target||'main'}:null;
      if(a.type==='capture'){check(result.capture===request.capture,'png-capture-refused');clean.captureSha256=hash(policy.readFile(path.join(c.reportRoot,request.capture)).bytes);}
      receipt.orderedUiWitnesses=policy.witnessSummary(c);
      receipt.actions.push({...receipt.inFlight,evidence:clean});delete receipt.inFlight;save();s.feedback(clean);
    }
    const phases=new Set(receipt.artifacts.map(x=>x.phase));
    check(['baseline','cancelled','exported','after-save'].every(x=>phases.has(x)),'png-workflow-incomplete');
    receipt.status='operator-complete';receipt.ownerReview='required'; // Independent owner decides installed acceptance.
  }catch(e){receipt.reason=/^png-[a-z-]{1,64}$/.test(e?.code||'')?e.code:'png-operation-refused';}
  finally{
    s.clearTimer(timer);
    if(helper){try{await helper.close();receipt.helperCleanup='complete';}catch{receipt.helperCleanup='incomplete';}}
    if(controller){try{await controller.cleanup();receipt.cleanup='complete';}catch{receipt.cleanup='incomplete';}}
    if(!completed||receipt.cleanup==='incomplete'||receipt.helperCleanup==='incomplete'){receipt.status='partial';receipt.installedAcceptance='not-established';}
    save();await s.close();
  }
  return receipt;
}
function services(c,io={input:process.stdin,write:v=>process.stdout.write(JSON.stringify(v)+'\n'),isTTY:process.stdin.isTTY}){
  const input=readline.createInterface({input:io.input,terminal:false}),lines=input[Symbol.asyncIterator]();let app;
  async function next(){const x=await lines.next();check(!x.done&&x.value.length<=8192,'png-input-eof');return JSON.parse(x.value);}
  return {async transport(){check(io.isTTY===true,'png-transport-refused');const nonce=crypto.randomUUID();io.write({schema:'nemo.png-ui-transport/1',status:'waiting',nonce});
      const r=await next();check(r.transport==='png-ui-ready'&&r.nonce===nonce,'png-transport-refused');},
    package(){app=packageProof(c);return app;},journal(){journal(c);},workspace(){return policy.prepareWorkspace(c);},
    controller(){return app.modules.harness.createController({root:c.appRoot,app:app.bundle});},
    validate(snapshot,instance){policy.verifyWorkspace(c);const driver=require('../../scripts/nemo/lib/identity.cjs').sourceIdentity();
      check(!driver.dirty&&driver.head===c.driverHead&&hash(policy.readFile(c.windowHelper).bytes)===c.helperSha256,'png-driver-source-refused');
      const source=app.modules.identity.sourceIdentity();check(!source.dirty&&source.head===c.sourceHead,'png-app-source-refused');app.modules.harness.validateSnapshot(snapshot,instance,
      {isolation:app.modules.isolation,runtime:app.modules.runtime,source:app.source,build:app.build,app});},
    helper(pid){return helperTransport(c,pid,c.reportRoot);},next,feedback:io.write,
    save(r){atomic(c.reportRoot,'receipt.json',r);},setTimer:setTimeout,clearTimer:clearTimeout,async close(){input.close();}};
}
async function main(){check(process.platform==='darwin'&&process.argv.length===3,'png-cli-refused');
  const c=json(path.resolve(process.argv[2]));policy.authorize(c);const r=await runOperator(c,services(c));
  process.stdout.write(JSON.stringify(r)+'\n');process.exitCode=r.status==='operator-complete'?0:1;}
if(require.main===module)main().catch(()=>{process.stderr.write('PNG UI operator refused; no installed acceptance.\n');process.exitCode=1;});
module.exports={packageProof,journal,helperTransport,runOperator,services};
