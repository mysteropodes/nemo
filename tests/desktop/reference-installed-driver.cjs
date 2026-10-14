#!/usr/bin/env node
'use strict';
// Manual #1864 operator. Never included in automated desktop launch discovery.
// Run from committed driver source with a separately pinned clean appRoot at the
// frozen application SHA. Compile this checkout's Swift helper only in its slot;
// its nemo.reference-window-helper/1 proof records sourceHead/sourceSha256 and
// executableSha256 independently of the application's existing build receipt.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const readline = require('node:readline');
const {execFileSync} = require('node:child_process');
const policy = require('./reference-installed-policy.cjs');
const shared = require('./p38-installed-driver.cjs');
const ROOT = path.resolve(__dirname,'../..');
const {check,hash} = policy;

function readJSON(file) { return JSON.parse(policy.readFile(file).bytes); }
function git(root,args) {
  return execFileSync('git',['-C',root,...args],{encoding:'utf8',timeout:10000,maxBuffer:65536}).trim();
}
function frozenModules(config) {
  check(typeof config.appRoot === 'string' && path.isAbsolute(config.appRoot) &&
    fs.realpathSync(config.appRoot) === config.appRoot, 'source-head-refused');
  const common = root => path.resolve(root,git(root,['rev-parse','--git-common-dir']));
  check(common(config.appRoot) === common(ROOT) && git(config.appRoot,['rev-parse','HEAD']) === config.sourceHead &&
    git(config.appRoot,['status','--porcelain','--untracked-files=all']) === '', 'source-head-refused');
  // Load the frozen checkout's launcher/identity, never label the new test-only
  // driver commit as the source used to build the older accepted application.
  const load = file => require(path.join(config.appRoot,file));
  return {harness:load('tests/desktop/native-harness.cjs'),runtime:load('scripts/nemo/lib/native-runtime.cjs'),
    isolation:load('scripts/nemo/lib/isolation.cjs'),identity:load('scripts/nemo/lib/identity.cjs')};
}
function verifyProvenance(config,{driver,source,app,proof,helperProof,helperSourceSha256,helperSha256}) {
  check(!driver.dirty && driver.head===config.driverHead && !source.dirty && source.head===config.sourceHead,'source-head-refused');
  check(/^[a-f0-9]{64}$/.test(config.executableSha256 || '') && app.executableSha256===config.executableSha256 &&
    proof.status==='pass' && proof.details?.source?.head===config.sourceHead && proof.details.source.dirty===false &&
    proof.details.cleanup?.stopped===true && proof.details.cleanup.released===true && proof.artifacts?.some(item=>
      item.sha256===app.executableSha256 && item.path.endsWith('/Contents/MacOS/nemo')),'fresh-build-proof-required');
  check(/^[a-f0-9]{64}$/.test(config.helperSha256 || '') && helperSha256===config.helperSha256 &&
    helperProof.schema==='nemo.reference-window-helper/1' && helperProof.sourceHead===config.driverHead &&
    helperProof.sourceSha256===helperSourceSha256 && helperProof.executableSha256===config.helperSha256,'window-helper-refused');
}
function packageProof(config) {
  policy.authorize(config); policy.verifyInputs(config);
  const driver = require('../../scripts/nemo/lib/identity.cjs').sourceIdentity();
  check(!driver.dirty && driver.head === config.driverHead, 'source-head-refused');
  const modules = frozenModules(config), source = modules.identity.sourceIdentity();
  const app = modules.harness.packagedApp(config.app,modules.runtime);
  const proof = readJSON(config.buildProof);
  const helper = policy.readFile(config.windowHelper), helperProof = readJSON(config.helperProof);
  fs.accessSync(config.windowHelper,fs.constants.X_OK);
  const helperSourceSha256 = require('./p38-owned-window-build.cjs').verifySourceProof(helperProof);
  verifyProvenance(config,{driver,source,app,proof,helperProof,helperSourceSha256,helperSha256:hash(helper.bytes)});
  const driverProof = {sourceHead:config.driverHead,helperSourceSha256,helperSha256:config.helperSha256,
    driverSha256:hash(fs.readFileSync(__filename)),policySha256:hash(fs.readFileSync(path.join(__dirname,'reference-installed-policy.cjs'))),
    transportSha256:hash(fs.readFileSync(path.join(__dirname,'p38-installed-driver.cjs'))),
    helperProtocolSha256:hash(fs.readFileSync(path.join(__dirname,'p38-helper-protocol.cjs')))};
  return {...app,source,build:modules.identity.buildIdentity(),proofHash:hash(policy.readFile(config.buildProof).bytes),driverProof,modules};
}
async function transportReady(read,write,isTTY,nonce=crypto.randomUUID()) {
  check(isTTY === true,'transport-pty-required');
  write({schema:'nemo.reference-transport/1',status:'waiting',nonce,startAttempts:0,journalWrites:0});
  const reply=await read();
  check(reply?.transport === 'reference-ready' && reply.nonce === nonce,'transport-handshake-refused');
  write({schema:'nemo.reference-transport/1',status:'ready',startAttempts:0,journalWrites:0});
}
function journalGrant(config,guard) {
  policy.authorize(config); guard.verify();
  let fd;
  try { fd=fs.openSync(path.join(guard.directory,hash(config.grant.id)+'.json'),
    fs.constants.O_CREAT|fs.constants.O_EXCL|fs.constants.O_WRONLY|fs.constants.O_NOFOLLOW,0o600); }
  catch(error) { if(error.code==='EEXIST')throw Object.assign(Error('grant already attempted'),{code:'grant-already-attempted'}); throw error; }
  try {
    guard.verify(); fs.writeFileSync(fd,JSON.stringify({issue:1864,grant:config.grant.id,task:config.taskId,
      appSource:config.sourceHead,driverSource:config.driverHead})); fs.fsyncSync(fd);
  } finally { fs.closeSync(fd); }
}
function createServices(config,io={input:process.stdin,write:value=>process.stdout.write(JSON.stringify(value)+'\n'),isTTY:process.stdin.isTTY}) {
  policy.authorize(config);
  const input=readline.createInterface({input:io.input,terminal:false}),iterator=input[Symbol.asyncIterator]();
  let app, reportGuard, ready=false; const helpers=new Set();
  const directory=path.join(ROOT,'reports','reference-installed-operators',hash(config.grant.id));
  const window=shared.createWindowTransport(config,directory,()=>reportGuard,helpers);
  async function next() {
    const line=await iterator.next(); check(!line.done && line.value.length <= 8192,'operator-input-ended');
    return JSON.parse(line.value);
  }
  return {
    transport(){return transportReady(next,io.write,io.isTTY);},
    async package(){app=packageProof(config);return app;},
    async journal(){
      const {isolation}=app.modules;
      const journal=shared.safeDirectory(path.dirname(isolation.RUNTIME_ROOT),[path.basename(isolation.RUNTIME_ROOT),'reference-grants']);
      journalGrant(config,journal);
      reportGuard=shared.safeDirectory(ROOT,['reports','reference-installed-operators',hash(config.grant.id)]);
      ready=true;
      const workspace=policy.prepareWorkspace(config,reportGuard);
      // Private operator feedback, not copied into the shareable terminal receipt.
      io.write({schema:'nemo.reference-workspace/1',workspace});
    },
    save(receipt){if(ready)shared.atomicJSON(reportGuard,'receipt.json',receipt);},
    controller(){const {harness,isolation,runtime}=app.modules;
      return harness.createController({root:config.appRoot,app:app.bundle,
        startupDiagnostics:task=>shared.captureStartupDiagnostics(task,isolation,runtime)});
    },
    validate(snapshot,instance){
      policy.verifyInputs(config);
      const {harness,isolation,runtime,identity}=app.modules;
      const current=identity.sourceIdentity();
      check(!current.dirty && current.head===config.sourceHead,'source-head-refused');
      check(hash(policy.readFile(config.windowHelper).bytes)===config.helperSha256,'window-helper-refused');
      harness.validateSnapshot(snapshot,instance,{isolation,runtime,source:app.source,build:app.build,app});
    },
    window,next,feedback:io.write,
    close(){return shared.closeOwnedHelpers(()=>input.close(),[...helpers],shared.cleanupHelper);},
    setTimer:setTimeout,clearTimer:clearTimeout,
  };
}
async function main() {
  if(process.argv.length===3 && process.argv[2]==='--transport-probe') {
    const input=readline.createInterface({input:process.stdin,terminal:false}),iterator=input[Symbol.asyncIterator](); let timer;
    try { await Promise.race([transportReady(async()=>{const line=await iterator.next();
      check(!line.done && line.value.length<=8192,'operator-input-ended'); return JSON.parse(line.value);},
      value=>process.stdout.write(JSON.stringify(value)+'\n'),process.stdin.isTTY),
      new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('transport timeout')),15000);})]); }
    finally { clearTimeout(timer);input.close(); } return;
  }
  check(process.argv.length===3,'reference-config-refused');
  const config=readJSON(process.argv[2]);policy.authorize(config);
  const receipt=await shared.runReferenceOperator(config,createServices(config));
  process.stdout.write(JSON.stringify(receipt)+'\n');
  process.exitCode=receipt.status==='partial'||receipt.cleanup==='incomplete'?1:0;
}
if(require.main===module)main().catch(()=>{process.stderr.write('Reference operator refused; no installed acceptance.\n');process.exitCode=1;});
module.exports={packageProof,verifyProvenance,frozenModules,transportReady,journalGrant,createServices,runOperator:shared.runReferenceOperator};
