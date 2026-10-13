'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),zlib=require('node:zlib');
const {spawnSync}=require('node:child_process');
const policy=require('./desktop/reference-installed-policy.cjs');
const driver=require('./desktop/reference-installed-driver.cjs');
const shared=require('./desktop/p38-installed-driver.cjs');
// Original independently frozen PNG corpus and literal straight-alpha pixels.
const RGBA=Buffer.from('89504e470d0a1a0a0000000d49484452000000030000000108060000001be014b40000001549444154789c63105432feef129ad650de319301001b41047d056017fe0000000049454e44ae426082','hex');
const RGB=Buffer.from('89504e470d0a1a0a0000000d4948445200000002000000020802000000fdd49a730000001649444154789c63e051b2389162c4c0cec1c9c5cd0300124401fe0da78d180000000049454e44ae426082','hex');
const SAVED={drawing:{color:'blue'},reference:'second'},HEAD='a'.repeat(40),DRIVER='d'.repeat(40),HASH='b'.repeat(64);
const OUTER='11111111-1111-4111-8111-111111111111',INNER='22222222-2222-4222-8222-222222222222';
function setup(t) {
  const root=fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()),'nemo-reference-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const write=(name,bytes)=>{const file=path.join(root,name);fs.writeFileSync(file,bytes);return {path:file,sha256:policy.hash(bytes)};};
  const config={workflow:'P37_REFERENCE',grant:{issue:1864,confirmed:true,id:'reference-test'},taskId:'reference-owned',
    sourceHead:HEAD,driverHead:DRIVER,timeoutMs:1000,
    fixtures:[{label:'A',...write('a.json',JSON.stringify({A:true}))},{label:'B',...write('b.json',JSON.stringify({B:true}))}],
    references:[{label:'ReferenceA',...write('first.png',RGBA)},{label:'ReferenceB',...write('second.png',RGB)}],
    savedCanonicalSha256:policy.hash(policy.canonical(SAVED)),exportOracle:{width:3,height:1,pixels:[
      {x:0,y:0,rgba:[17,34,51,255]},{x:1,y:0,rgba:[68,85,102,128]},{x:2,y:0,rgba:[119,136,153,0]}]}};
  const report=shared.safeDirectory(root,['report']),journal=shared.safeDirectory(root,['journal']);
  return {root,config,report,journal,prepare:()=>policy.prepareWorkspace(config,report)};
}
for(const fault of ['none','no-proof','same-token','wrong-pid','wrong-window','old-inner','early-dismissal','repeat',
  'search','mismatch','arbitrary-keys','outer-while-inner','eof','timeout']) {
  test('Reference owned location lifecycle is terminal and bounded: '+fault,async t=>{
    let index=0,commands=0,sets=0;const feedback=[];
    const outer={target:'sheet',sheetToken:OUTER},inner={...outer,locationToken:INNER};
    const f=modeled(t,{window(mode,pid,wid,action){
      const result={pid,windowId:45,matchedAX:true};
      if(mode==='pin-sheet')return {...result,...outer};
      if(mode==='go-to-folder'){
        commands++;return {...result,...outer,commandPosted:true,locationReady:fault!=='no-proof',
          locationToken:fault==='same-token'?OUTER:INNER,...(fault==='wrong-pid'?{pid:999}:{}),...(fault==='wrong-window'?{windowId:46}:{})};
      }
      if(mode==='set')sets++;
      if(mode==='sheet-dismissed')return {...result,dismissed:true};
      return {...result,...(action?.target==='sheet'?outer:{}),...(action?.locationToken?{locationToken:action.locationToken}:{}),
        ...(mode==='location-dismissed'?{locationDismissed:fault!=='early-dismissal'}:{}),
        ...(mode==='set'?{actionDiagnostics:{stage:'axtext-confirm',role:'AXTextField',textFieldKind:fault==='search'?'search':'plain',enabled:true,
          valueSettableQueryError:0,valueSettable:true,valueWriteError:0,valueReadbackError:0,valueMatches:fault!=='mismatch',
          confirmQueryError:0,confirmAdvertised:true,confirmError:0}}:{})};
    },next(workspace,fire){
      const actions=[{type:'pin-sheet'},{type:'go-to-folder',...outer,...(fault==='arbitrary-keys'?{keyCode:5}:{})},
        {type:'inspect',...inner,...(fault==='old-inner'?{locationToken:OUTER}:{}),...(fault==='outer-while-inner'?{locationToken:undefined}:{})},
        {type:'set',...inner,element:'0.1',value:path.join(workspace.directory,'A.json')},
        {type:'location-dismissed',...inner},{type:fault==='repeat'?'go-to-folder':'inspect',...outer},
        {type:'press',...outer,element:'0.10'},{type:'sheet-dismissed',sheetToken:OUTER},{type:'stop'}];
      if(index===2 && fault==='eof')throw Object.assign(Error('EOF'),{code:'operator-input-ended'});
      if(index===2 && fault==='timeout'){fire();return new Promise(()=>{});}
      return actions[index++];
    }});
    f.services.feedback=value=>feedback.push(value);
    const result=await driver.runOperator(f.config,f.services);
    assert.equal(result.status,fault==='none'?'operator-complete':'partial',JSON.stringify(result));
    assert.equal(result.installedAcceptance,'not-established');assert.equal(result.cleanup,'complete');assert.equal(result.helperCleanup,'complete');
    assert.equal(commands,fault==='arbitrary-keys'?0:1);assert.ok(sets<=1);
    if(fault==='none'){
      assert.deepEqual(feedback,[{target:'sheet',ordinal:1,token:OUTER},{target:'sheet',ordinal:2,token:OUTER,locationToken:INNER}]);
      assert.equal(result.actions[1].locationReady,true);assert.equal(result.actions[4].locationDismissed,true);
      assert.equal(result.actions[3].actionDiagnostics.valueMatches,true);
    } else assert.ok(result.firstFailure);
    if(['old-inner','outer-while-inner','eof','timeout','arbitrary-keys'].includes(fault))assert.equal(sets,0);
  });
}
function outputs(workspace) {
  fs.writeFileSync(workspace.saved,JSON.stringify({reference:'second',drawing:{color:'blue'}},null,2));
  const published=path.join(workspace.exportDirectory,'nemo-native-opacity-1791918000000-1');fs.mkdirSync(published);
  fs.writeFileSync(path.join(published,'frame_0001.png'),RGBA);
}
function modeled(t,change={}) {
  const fixture=setup(t),{config,report,journal}=fixture,events=[],receipts=[];
  let fire,workspace,attempts=0,cleaned=0;
  const instance={task:config.taskId},snapshot={ok:true,taskId:config.taskId,source:{matches:true,startup:{head:HEAD}},
    build:{matches:true},runtime:{childPid:123},app:{pid:123,taskId:config.taskId,isolated:true,state:'active'}};
  let actions=[{type:'inspect'},{type:'capture'},{type:'stop'}];
  const services={
    async transport(){events.push('transport');if(change.transport)return change.transport(fire);},
    async package(){events.push('package');policy.verifyInputs(config);return {executableSha256:HASH,proofHash:HASH,
      driverProof:{sourceHead:DRIVER,helperSha256:HASH}};},
    async journal(){events.push('journal');driver.journalGrant(config,journal);workspace=fixture.prepare();if(change.prepared)change.prepared(workspace);},
    save(value){receipts.push(structuredClone(value));},
    controller(){return {
      async start(){events.push('start');attempts++;return instance;},
      async status(){return {value:change.snapshot?change.snapshot(structuredClone(snapshot)):structuredClone(snapshot)};},
      async cleanup(){events.push('cleanup');cleaned++;if(change.cleanup)throw Error('private cleanup');}
    };},
    validate(){policy.verifyInputs(config);},
    async window(mode,pid,windowId,action){events.push('window-'+mode);
      if(change.window)return change.window(mode,pid,windowId,action,fire);
      return {pid,windowId:45,matchedAX:true,...(mode==='capture'?{captureSha256:HASH}:{})};},
    async next(){if(change.next)return change.next(workspace,fire);return actions.shift();},
    feedback(){events.push('feedback');},
    async close(){events.push('helpers-close');if(change.close)throw Error('private helper');},
    setTimer(fn){fire=fn;return 0;},clearTimer(){events.push('timer-clear');}
  };
  return {...fixture,services,events,receipts,get attempts(){return attempts;},get cleaned(){return cleaned;},
    get workspace(){return workspace;},set actions(value){actions=value;}};
}
test('Reference and P38 grants cannot substitute for each other before any service effect',async t=>{
  const f=modeled(t);
  for(const patch of [{workflow:'P38'},{grant:{issue:1645,confirmed:true,id:'p38'}},{grant:{issue:1864,confirmed:false,id:'reference'}},{grant:null}]) {
    await assert.rejects(driver.runOperator({...f.config,...patch},f.services),/new-reference-grant-required/);
  }
  await assert.rejects(shared.runOperator(f.config,f.services),/new-p38-grant-required/);
  assert.deepEqual(f.events,[]);assert.equal(f.attempts,0);
});
test('real journal is exclusive, namespaces the issue and rejects reuse before a second launch',async t=>{
  const f=modeled(t),first=await driver.runOperator(f.config,f.services);
  assert.equal(first.status,'operator-complete');assert.equal(first.schema,'nemo.reference-operator/1');
  assert.equal(first.issue,1864);assert.equal(first.source,HEAD);assert.equal(first.driver.sourceHead,DRIVER);
  assert.equal(first.appPid,123);assert.equal(first.windowId,45);assert.equal(first.installedAcceptance,'not-established');
  assert.equal(first.cleanup,'complete');assert.equal(first.helperCleanup,'complete');
  const second=await driver.runOperator(f.config,f.services);
  assert.equal(second.reason,'grant-already-attempted');assert.equal(f.attempts,1);
  const journal=JSON.parse(fs.readFileSync(path.join(f.journal.directory,policy.hash(f.config.grant.id)+'.json')));
  assert.equal(journal.issue,1864);assert.equal(journal.appSource,HEAD);assert.equal(journal.driverSource,DRIVER);
});
test('staged immutable fixtures allow only exact inputs and owned Save As/export destinations',t=>{
  const f=setup(t),w=f.prepare();
  assert.deepEqual(w.inputs.map(v=>path.basename(v.path)),['A.json','B.json','ReferenceA.png','ReferenceB.png']);
  for(const value of [...w.inputs.map(v=>v.path),w.saved,w.exportDirectory])
    assert.doesNotThrow(()=>policy.verifySet(f.config,{target:'sheet',value}));
  for(const value of [f.config.fixtures[0].path,path.join(w.directory,'../escape.json'),'elsewhere.png','../Saved.json','Saved.json','Export'])
    assert.throws(()=>policy.verifySet(f.config,{target:'sheet',value}),/reference-path-refused/);
  assert.throws(()=>policy.verifySet(f.config,{target:'main',value:w.saved}),/reference-path-refused/);
  assert.throws(()=>f.prepare(),/reference-workspace-reused/);
  assert.equal(fs.existsSync(w.saved),false);assert.deepEqual(fs.readdirSync(w.exportDirectory),[]);
});
test('bounded descriptor reads reject FIFO, directory and over-capacity input without blocking',t=>{
  const f=setup(t),fifo=path.join(f.root,'input.json');
  assert.equal(spawnSync('mkfifo',[fifo]).status,0);
  const child=spawnSync(process.execPath,['-e',
    'try{require(process.argv[1]).readFile(process.argv[2]);process.exit(2);}catch(e){if(e.code!=="reference-input-refused")process.exit(3);}',
    path.resolve('tests/desktop/reference-installed-policy.cjs'),fifo],{timeout:2000,encoding:'utf8'});
  assert.equal(child.status,0);assert.equal(child.error,undefined);
  assert.throws(()=>policy.readFile(f.root),/reference-input-refused/);
  const large=path.join(f.root,'large');fs.writeFileSync(large,Buffer.alloc(1));fs.truncateSync(large,4*1024*1024+1);
  assert.throws(()=>policy.readFile(large),/reference-input-refused/);
});
for(const fault of ['hash','source-symlink','staged-symlink','directory-swap','output-symlink','duplicate-asset']) {
  test('input/output ownership refusal: '+fault,t=>{
    const f=setup(t),w=f.prepare();
    if(fault==='hash')fs.writeFileSync(f.config.references[0].path,'changed');
    if(fault==='source-symlink'){fs.renameSync(f.config.fixtures[0].path,f.config.fixtures[0].path+'.old');fs.symlinkSync(f.config.fixtures[0].path+'.old',f.config.fixtures[0].path);}
    if(fault==='staged-symlink'){fs.unlinkSync(w.inputs[0].path);fs.symlinkSync(f.config.fixtures[0].path,w.inputs[0].path);}
    if(fault==='directory-swap'){fs.renameSync(w.directory,w.directory+'.old');fs.mkdirSync(w.directory);}
    if(fault==='output-symlink')fs.symlinkSync(f.config.fixtures[0].path,w.saved);
    if(fault==='duplicate-asset')f.config.references[1].sha256=f.config.references[0].sha256;
    assert.throws(()=>policy.verifySet(f.config,{target:'sheet',value:w.saved}));
  });
}
test('independent literal PNG corpus and semantic saved bytes pass exact output oracles',t=>{
  const f=setup(t),w=f.prepare();outputs(w);
  assert.equal(policy.crc32(Buffer.from('IEND')),0xae426082);
  const result=policy.artifacts(f.config);
  assert.equal(result.savedSha256,policy.hash(fs.readFileSync(w.saved)));
  assert.equal(result.pngSha256,policy.hash(RGBA));assert.deepEqual(result.pixels,f.config.exportOracle.pixels);
  assert.equal(result.width,3);assert.equal(result.height,1);
  fs.writeFileSync(w.saved,JSON.stringify({...SAVED,reference:'first'}));
  assert.throws(()=>policy.artifacts(f.config),/reference-save-mismatch/);
});
test('generated UI reference ID normalization preserves asset binding, payload, and repeated-save identity',t=>{
  const f=setup(t),w=f.prepare();outputs(w);
  const expected={drawing:{color:'blue'},viewportReference:{kind:'still-image',assetId:'@GENERATED_REFERENCE_ID',visible:true},
    assets:[{id:'@GENERATED_REFERENCE_ID',data:RGBA.toString('base64')}]};
  f.config.normalizeGeneratedReferenceId=true;f.config.savedCanonicalSha256=policy.hash(policy.canonical(expected));
  const actual=()=>JSON.parse(JSON.stringify(expected).replaceAll('@GENERATED_REFERENCE_ID','project-ref-17'));
  fs.writeFileSync(w.saved,JSON.stringify(actual()));assert.equal(policy.artifacts(f.config).referenceId,'project-ref-17');
  for(const mutate of [v=>{v.assets[0].id='project-ref-18';},v=>{v.assets[0].data=RGB.toString('base64');},
    v=>{v.viewportReference.assetId=v.assets[0].id='project-ref-18';},v=>{v.viewportReference.visible=false;}]) {
    const v=actual();mutate(v);fs.writeFileSync(w.saved,JSON.stringify(v));
    assert.throws(()=>policy.artifacts(f.config),/reference-save-mismatch/);
  }
});
for(const fault of ['extra-directory','wrong-name','directory-symlink','frame-symlink','export-root-swap']) {
  test('actual native export directory contract refuses '+fault,t=>{
    const f=setup(t),w=f.prepare();outputs(w);
    const published=path.join(w.exportDirectory,'nemo-native-opacity-1791918000000-1'),frame=path.join(published,'frame_0001.png');
    if(fault==='extra-directory')fs.mkdirSync(path.join(w.exportDirectory,'nemo-native-opacity-1791918000000-2'));
    if(fault==='wrong-name')fs.renameSync(frame,path.join(published,'Artwork.png'));
    if(fault==='directory-symlink'){fs.renameSync(published,path.join(f.root,'foreign'));fs.symlinkSync(path.join(f.root,'foreign'),published);}
    if(fault==='frame-symlink'){fs.unlinkSync(frame);fs.symlinkSync(f.config.references[0].path,frame);}
    if(fault==='export-root-swap'){fs.renameSync(w.exportDirectory,w.exportDirectory+'.old');fs.mkdirSync(w.exportDirectory);}
    assert.throws(()=>policy.artifacts(f.config));
  });
}
test('PNG oracle rejects changed pixels, CRC, dimensions, profile, truncation and trailing bytes',t=>{
  const f=setup(t),expected=f.config.exportOracle;
  assert.throws(()=>policy.pngPixels(RGBA,{...expected,width:4}),/reference-png-refused/);
  assert.throws(()=>policy.pngPixels(RGBA,{...expected,pixels:[{x:0,y:0,rgba:[0,0,0,0]}]}),/reference-pixel-mismatch/);
  for(const bytes of [RGBA.subarray(0,-1),Buffer.concat([RGBA,Buffer.from([0])]),RGB])
    assert.throws(()=>policy.pngPixels(bytes,expected),/reference-png-refused/);
  const corrupt=Buffer.from(RGBA);corrupt[50]^=1;
  assert.throws(()=>policy.pngPixels(corrupt,expected),/reference-png-refused/);
  const compressed=RGBA.subarray(41,62),raw=zlib.inflateSync(compressed);
  function idat(data){const size=Buffer.alloc(4);size.writeUInt32BE(data.length);const body=Buffer.concat([Buffer.from('IDAT'),data]);
    const crc=Buffer.alloc(4);crc.writeUInt32BE(policy.crc32(body));return Buffer.concat([size,body,crc]);}
  for(const data of [Buffer.concat([compressed,Buffer.from('trailing')]),zlib.deflateSync(Buffer.concat([raw,Buffer.from([0])]))]) {
    assert.throws(()=>policy.pngPixels(Buffer.concat([RGBA.subarray(0,33),idat(data),RGBA.subarray(-12)]),expected));
  }
});
for(const fault of ['foreign-pid','foreign-window','bad-sheet','eof','timeout','cleanup','helper-cleanup']) {
  test('operator preserves exact owner and terminal cleanup on '+fault,async t=>{
    const change={};
    if(fault==='foreign-pid')change.snapshot=s=>{s.app.pid=999;return s;};
    if(fault==='foreign-window')change.window=(mode,pid)=>({pid,windowId:mode==='discover'?45:46,matchedAX:true});
    if(fault==='bad-sheet')change.next=()=>({type:'press',target:'sheet',sheetToken:'foreign',element:'0.1'});
    if(fault==='eof')change.next=()=>{throw Object.assign(Error('EOF'),{code:'operator-input-ended'});};
    if(fault==='timeout')change.next=(_,fire)=>{fire();return new Promise(()=>{});};
    if(fault==='cleanup')change.cleanup=true;
    if(fault==='helper-cleanup')change.close=true;
    const f=modeled(t,change),result=await driver.runOperator(f.config,f.services);
    assert.equal(result.status,'partial');assert.equal(result.installedAcceptance,'not-established');
    assert.equal(f.attempts,1);assert.equal(f.cleaned,1);assert.ok(result.firstFailure);
    assert.ok(f.events.indexOf('helpers-close')<f.events.indexOf('cleanup'));
    assert.equal(JSON.stringify(result).includes('private helper'),false);
    if(['foreign-pid','foreign-window','bad-sheet'].includes(fault))assert.equal(f.events.includes('window-press'),false);
  });
}
test('artifact action checks bytes/pixels under same pinned live window without claiming visual acceptance',async t=>{
  const f=modeled(t,{prepared:outputs});f.actions=[{type:'artifacts'},{type:'stop'}];
  const result=await driver.runOperator(f.config,f.services);
  assert.equal(result.status,'operator-complete');assert.equal(result.artifacts.pngSha256,policy.hash(RGBA));
  assert.deepEqual(result.artifacts.pixels,f.config.exportOracle.pixels);
  assert.equal(f.events.includes('window-artifacts'),false);assert.equal(f.events.includes('window-check'),true);
  assert.equal(result.installedAcceptance,'not-established');
});
test('frozen app and newer driver/helper proofs are independently bound, never relabelled',t=>{
  const f=setup(t),config={...f.config,executableSha256:HASH,helperSha256:'c'.repeat(64)};
  const facts={driver:{head:DRIVER,dirty:false},source:{head:HEAD,dirty:false},app:{executableSha256:HASH},
    proof:{status:'pass',details:{source:{head:HEAD,dirty:false},cleanup:{stopped:true,released:true}},
      artifacts:[{sha256:HASH,path:'/scoped/Nemo.app/Contents/MacOS/nemo'}]},
    helperProof:{schema:'nemo.reference-window-helper/1',sourceHead:DRIVER,sourceSha256:HASH,executableSha256:config.helperSha256},
    helperSourceSha256:HASH,helperSha256:config.helperSha256};
  assert.doesNotThrow(()=>driver.verifyProvenance(config,facts));
  for(const mutate of [v=>{v.driver.head=HEAD;},v=>{v.source.head=DRIVER;},v=>{v.driver.dirty=true;},
    v=>{v.source.dirty=true;},v=>{v.proof.details.source.head=DRIVER;},v=>{v.proof.details.cleanup.stopped=false;},
    v=>{v.app.executableSha256='e'.repeat(64);},v=>{v.helperProof.schema='nemo.p38-window-helper/1';},
    v=>{v.helperProof.sourceHead=HEAD;},v=>{v.helperSourceSha256='f'.repeat(64);},v=>{v.helperSha256=HASH;}]) {
    const wrong=structuredClone(facts);mutate(wrong);assert.throws(()=>driver.verifyProvenance(config,wrong));
  }
});
test('Reference transport requires its own fresh nonce and PTY before any launch',async()=>{
  const frames=[];
  await assert.rejects(driver.transportReady(async()=>({transport:'p38-ready',nonce:'n'}),v=>frames.push(v),true,'n'),/transport-handshake-refused/);
  assert.equal(frames[0].schema,'nemo.reference-transport/1');assert.equal(frames[0].startAttempts,0);
  await assert.rejects(driver.transportReady(async()=>({}),()=>{},false),/transport-pty-required/);
  await driver.transportReady(async()=>({transport:'reference-ready',nonce:'fresh'}),v=>frames.push(v),true,'fresh');
  assert.equal(frames.at(-1).status,'ready');
  const result=spawnSync(process.execPath,['tests/desktop/reference-installed-driver.cjs','--transport-probe'],{encoding:'utf8',timeout:5000});
  assert.equal(result.status,1);assert.match(result.stderr,/Reference operator refused/);
});
test('P38 default fixture and label vocabulary remain closed to Reference inputs',t=>{
  const f=setup(t),w=f.prepare(),node={pid:123,windowId:45,matchedAX:true,nodes:[{path:'0',role:'AXButton',control:'Reference…'}]};
  assert.equal(shared.safeWindowEvidence(node).nodes[0].control,undefined);
  assert.equal(shared.safeWindowEvidence(node,true).nodes[0].control,'Reference…');
  node.nodes[0]={path:'0',role:'AXPopUpButton',control:'PNG Sequence'};
  assert.equal(shared.safeWindowEvidence(node).nodes[0].role,'other');
  assert.equal(shared.safeWindowEvidence(node,true).nodes[0].role,'AXPopUpButton');
  assert.equal(shared.safeWindowEvidence(node,true).nodes[0].control,'PNG Sequence');
  assert.throws(()=>shared.verifyFixtures({...f.config,fixtures:f.config.references}),/fixture-refused/);
  assert.throws(()=>policy.authorize({...f.config,workflow:'P38'}),/new-reference-grant-required/);
});
test('real child transport chooses Reference entrypoint/environment and retires only its helper',async t=>{
  const f=setup(t);f.prepare();const helpers=new Set();
  const helper=path.join(f.root,'modeled-helper');
  fs.writeFileSync(helper,'#!'+process.execPath+'\n'+
    'if(process.argv[2]!=="--reference-persistent"||!process.env.NEMO_REFERENCE_GRANT||process.env.NEMO_P38_GRANT)process.exit(3);\n'+
    'const pid=Number(process.argv[3]);const emit=v=>process.stdout.write(JSON.stringify(v)+"\\n");\n'+
    'emit({status:"pinned",pid,windowId:45,matchedAX:true});\n'+
    'require("node:readline").createInterface({input:process.stdin}).on("line",line=>{const v=JSON.parse(line);'+
    'emit({status:"ok",type:v.type,pid,windowId:45,matchedAX:true,nodes:[{path:"0",role:"AXButton",control:"Reference…"}]});});\n',{mode:0o700});
  f.config.windowHelper=helper;
  const transport=shared.createWindowTransport(f.config,f.report.directory,()=>f.report,helpers);
  try {
    const pinned=await transport('discover',123);assert.equal(pinned.windowId,45);
    await transport('inspect',123,45);
    const projection=JSON.parse(fs.readFileSync(path.join(f.report.directory,'ax-1.json')));
    assert.equal(projection.nodes[0].control,'Reference…');
    await assert.rejects(transport('check',999,45),/owned-window-refused/);
  } finally {await shared.closeOwnedHelpers(()=>{},[...helpers],shared.cleanupHelper);}
  assert.equal(helpers.size,0);
});

for(const fault of ['none','missing','mismatch','confirm','identity','search']) {
  test('Reference sheet set requires categorical exact readback proof: '+fault,async t=>{
    const token='11111111-1111-4111-8111-111111111111';let n=0,sets=0;
    const diagnostics={stage:'axtext-confirm',elementPath:'0.7',role:'AXTextField',textFieldKind:'plain',enabled:true,
      focusSettableQueryError:-25205,focusSettable:false,focusedQueryError:0,focusedMatches:false,
      valueSettableQueryError:0,valueSettable:true,valueWriteError:0,valueReadbackError:0,valueMatches:true,
      confirmQueryError:0,confirmAdvertised:true,confirmError:0};
    const f=modeled(t,{window(mode,pid,wid,action){
      const result={pid,windowId:45,matchedAX:true};
      if(mode==='pin-sheet')return {...result,target:'sheet',sheetToken:token};
      if(mode==='sheet-dismissed')return {...result,dismissed:true};
      if(mode!=='set')return result;
      sets++;const d={...diagnostics};
      if(fault==='mismatch')d.valueMatches=false;
      if(fault==='confirm')d.confirmError=-25204;
      if(fault==='search'){d.stage='axtext-role';d.textFieldKind='search';}
      if(fault==='identity')return {...result,pid:999};
      return {...result,target:'sheet',sheetToken:token,...(fault==='missing'?{}:{actionDiagnostics:{...d,value:'/private/secret'}})};
    }});
    f.services.next=async()=>[{type:'pin-sheet'},{type:'set',target:'sheet',sheetToken:token,element:'0.7',
      value:path.join(f.workspace.directory,'A.json')},{type:'sheet-dismissed',sheetToken:token},{type:'stop'}][n++];
    const result=await driver.runOperator(f.config,f.services);
    assert.equal(sets,1);assert.equal(result.status,fault==='none'?'operator-complete':'partial',JSON.stringify(result));
    if(fault==='none')assert.deepEqual(result.actions[1].actionDiagnostics,diagnostics);
    else assert.ok(result.firstFailure);
    assert.equal(result.installedAcceptance,'not-established');assert.equal(result.cleanup,'complete');
    assert.equal(JSON.stringify(result).includes('/private/secret'),false);
  });
}
