/* Native font resources: real consumer/SDK/session modules with fake native boundaries.
 * No installed/native execution is established by these Node tests. */
'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),crypto=require('node:crypto'),fs=require('node:fs'),vm=require('node:vm');
const root='../src/js/adapters/';
const Resources=require(root+'native-font-resources.js'),SDK=require(root+'native-application.js'),Project=require(root+'native-object-project.js'),Assets=require(root+'font-assets.js');
const digest=async bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}
async function fixture(){
  let revision=0,generation=1,documentId='document',installed=false,mode='',hold=null,parseHold=null,parsed=0;
  const requests=[],data=Buffer.alloc(4200);for(let i=0;i<data.length;i++)data[i]=i%251;
  const hash=await digest(data);let assetId=' \n'+('雪'.repeat(81))+'data:x ',name=' Exact\tFont 雪 ';
  let rows=[{assetId,name,sourceKind:'embedded',available:true,mediaType:'font/ttf',encodedBytes:data.length,encodedSha256:hash}],receipt;
  const info=()=>({apiVersion:2,instanceId:'instance',documentId,contentRevision:revision,documentSnapshotId:'snapshot-'+documentId+'-'+revision,lifecycleGeneration:generation,
    documentDigest:'a'.repeat(64),byteCount:100,canvasW:100,canvasH:100,fps:24,totalFrames:2,layers:[],reference:null});
  async function dispatch(r){requests.push(structuredClone(r));const p=r.payload;
    function reply(result,ok=true){return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok,...(ok?{result}:{error:{code:'unavailable',message:'refused'}})};}
    if(r.operation==='command.document.object.asset.font.import'){
      assert.equal(r.expectedRevision,revision);if(mode==='refuse')return reply(null,false);
      if(mode==='missing'){throw Error('transport lost before disposition');}
      function ambiguous(){const response=reply(null,false);response.error.details={disposition:'indeterminate',committed:null,retryExecution:false};return response;}
      if(mode==='envelopeMissing')return ambiguous();
      revision++;assetId=String(revision%10)+assetId.slice(1);rows=[{...rows[0],assetId}];
      const value={applied:true,historyEntriesAdded:1,assetId,mediaType:'font/ttf',encodedBytes:data.length,encodedSha256:hash};
      receipt={status:'committed',forRequestId:r.requestId,requestFingerprintSha256:await digest(Buffer.from(Resources.fingerprintInput(r))),contentRevision:revision,...value};
      delete receipt.applied;delete receipt.historyEntriesAdded;
      if(mode==='lost'||mode==='badFingerprint')throw Error('transport lost after commit');
      if(mode==='envelopeCommitted')return ambiguous();
      if(hold)await hold.promise;return reply(value);
    }
    if(p.action==='receipt')return reply(mode==='missing'||mode==='envelopeMissing'?{status:'not_found',forRequestId:p.forRequestId}:mode==='badFingerprint'?{...receipt,requestFingerprintSha256:'0'.repeat(64)}:receipt);
    assert.equal(p.atRevision,revision);assert.equal(p.documentSnapshotId,info().documentSnapshotId);
    if(p.action==='list')return reply({atRevision:revision,documentSnapshotId:p.documentSnapshotId,offset:p.offset,nextOffset:null,totalFontRecords:rows.length,rows});
    if(p.action==='inspect'){if(p.assetId!==assetId||!rows.length)return reply(null,false);return reply({atRevision:revision,documentSnapshotId:p.documentSnapshotId,font:rows[0]});}
    if(p.action==='read'){
      assert.equal(p.assetId,assetId);assert.ok(p.length<=1536);
      const count=Math.min(p.length,701,data.length-p.offset),bytes=Buffer.from(data.subarray(p.offset,p.offset+count));if(mode==='hash')bytes[0]^=1;
      return reply({atRevision:revision,documentSnapshotId:p.documentSnapshotId,assetId,offset:p.offset+(mode==='offset'?1:0),returnedLength:count,totalLength:data.length,encodedSha256:hash,base64:bytes.toString('base64')});
    }
    throw Error('unexpected boundary');
  }
  const sdk=SDK.createNativeApplicationAdapter('ui',{dispatch});
  const session=Project.create({
    dispatchApplication:r=>sdk.dispatch(r),mapping:()=>({cssBounds:{width:100,height:100},physicalExtent:{width:100,height:100}}),
    invoke:async(command,args)=>{
      if(command==='nemo_native_object_client_status')return {...info(),available:installed};
      const r=JSON.parse(args.requestJson);
      if(command==='nemo_native_object_bootstrap'){if(installed){documentId+='x';generation++;revision=0;}installed=true;return {...info(),requestId:r.requestId,resourceCount:0,viewportAvailable:false};}
      if(command==='nemo_native_object_reference'&&r.action.kind==='inspect')return {...info(),requestId:r.requestId};
      if(command==='nemo_native_object_reference'||command==='nemo_native_object_dispatch'){revision++;if(r.operation==='command.document.object.undo')rows=[];return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok:true,result:{applied:true,historyEntriesAdded:1}};}
      if(command==='nemo_native_object_preview')return {status:'presented',workId:'work',viewGeneration:1};
      throw Error(command);
    }
  });
  await session.openPath('/fake-project');
  const resources=Resources.create({session,digest,parse:async bytes=>{parsed++;assert.deepEqual(Buffer.from(bytes),data);if(parseHold)await parseHold.promise;return {parsed};}});
  const fonts={'Roboto-Regular':{url:'fonts/Roboto-Regular.ttf',label:'Roboto'}};
  const assets=Assets.create({fonts,nativeResources:()=>resources,fetch:()=>{throw Error('forbidden fetch fallback');},invoke:()=>{throw Error('forbidden legacy invoke');}});
  return {assets,resources,session,fonts,requests,get assetId(){return assetId;},data,info,parsed:()=>parsed,mode:v=>mode=v,hold:v=>hold=v,parseHold:v=>parseHold=v,
    replace:()=>session.openPath('/replacement'),close:()=>{session.isActive=()=>false;},rows:v=>rows=v};
}
const bundled={kind:'bundled',fontKey:'Roboto-Regular'};
test('actual ordinary loader imports once and parses complete matching shortened chunks; session revision reconciles',async()=>{
  const f=await fixture();const font=await f.assets.load('Roboto-Regular');assert.equal(await f.assets.load('Roboto-Regular'),font);
  assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,1);assert.equal(f.session.info().contentRevision,1);assert.equal(f.parsed(),1);
  const reads=f.requests.filter(r=>r.payload.action==='read');assert.equal(reads.length,6);assert.deepEqual(reads.map(r=>r.payload.offset),[0,701,1402,2103,2804,3505]);
  for(const r of f.requests)assert.ok(Buffer.byteLength(JSON.stringify(r))<=4096);
});
test('fresh reopen discovers exact opaque ID/name then reads original bytes without imports or guessed descriptor lookup',async()=>{
  const f=await fixture();const rows=await f.assets.discoverNative();assert.equal(rows[0].assetId,f.assetId);assert.equal(rows[0].name,' Exact\tFont 雪 ');
  await f.assets.loadNativeAsset(rows[0].assetId);assert.equal(f.requests.some(r=>r.expectedRevision!==undefined),false);assert.equal(f.parsed(),1);
});
test('lost commit recovers original request receipt fingerprint and never reimports',async()=>{
  const f=await fixture();f.mode('lost');await f.assets.load('Roboto-Regular');assert.equal(f.session.info().contentRevision,1);
  const imports=f.requests.filter(r=>r.expectedRevision!==undefined),receipt=f.requests.find(r=>r.payload.action==='receipt');
  assert.equal(imports.length,1);assert.equal(receipt.payload.forRequestId,imports[0].requestId);assert.equal('expectedRevision' in receipt,false);
});
for(const mode of ['missing','badFingerprint'])test(mode+' receipt remains indeterminate without parser or a second import',async()=>{
  const f=await fixture();f.mode(mode);await assert.rejects(f.assets.load('Roboto-Regular'),e=>e.code==='font_import_indeterminate');
  await assert.rejects(f.assets.load('Roboto-Regular'),e=>e.code==='font_import_indeterminate');
  assert.equal(f.parsed(),0);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,1);
});
for(const mode of ['refuse','hash','offset'])test(mode+' failure preserves accepted font/catalog and blocks corrupt parser input',async()=>{
  const f=await fixture();await f.assets.load('Roboto-Regular');const before=structuredClone(f.fonts);f.mode(mode);
  await assert.rejects(f.assets.googleWeight('Other',700,true));assert.deepEqual(f.fonts,before);assert.equal(f.parsed(),1);
});
test('abort after commit preserves catalog and accepted parsed font; no prewarm Bold import',async()=>{
  const f=await fixture();await f.assets.load('Roboto-Regular');const hold=deferred(),controller=new AbortController();f.hold(hold);
  const pending=f.resources.importSource({kind:'google',family:' Exact Family ',weight:400,italic:false},controller.signal);
  await turn();controller.abort();hold.resolve();await assert.rejects(pending,/cancelled/);assert.equal(f.parsed(),1);
  f.hold(null);await f.assets.addGoogle(' Exact Family ');const imports=f.requests.filter(r=>r.expectedRevision!==undefined);
  assert.equal(imports.length,3);assert.equal(imports.at(-1).payload.source.family,' Exact Family ');assert.equal(imports.at(-1).payload.source.weight,400);
});
test('owner replacement invalidates parsed and ordinary cache and preserves session serialization',async()=>{
  const f=await fixture();await f.assets.load('Roboto-Regular');const first=f.session.info();await f.replace();await f.assets.load('Roboto-Regular');
  assert.notEqual(first.documentId,f.session.info().documentId);assert.equal(f.parsed(),2);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,2);
});
test('dispose while parsing blocks late catalog/cache publication',async()=>{
  const f=await fixture(),wait=deferred();f.parseHold(wait);const pending=f.assets.googleWeight('Late',400,false);await turn();f.assets.dispose();wait.resolve();
  await assert.rejects(pending,/unavailable|disposed/);assert.equal(Object.hasOwn(f.fonts,'Google:Late-Regular'),false);
});
test('real bridge desktop startup creates no import, fetch or legacy invocation',async()=>{
  const reads=[],sandbox={Promise,Map,Uint8Array,atob,__TAURI__:{core:{invoke:()=>{throw Error('legacy');}}},fetch:u=>reads.push(u),opentype:{parse:()=>({})}};
  sandbox.window=sandbox;vm.runInNewContext(fs.readFileSync(__dirname+'/../src/js/adapters/font-assets.js','utf8'),sandbox);
  vm.runInNewContext(fs.readFileSync(__dirname+'/../src/js/vector-text-bridge.js','utf8'),sandbox);await turn();assert.deepEqual(reads,[]);
  await assert.rejects(sandbox.SMVectorText.loadVectorFont('Roboto-Regular'),/unavailable/);assert.deepEqual(reads,[]);
});
test('common SDK rejects closed-contract extras, wrong pins, oversized UTF8 IDs and invalid chunks',async()=>{
  const q={apiVersion:2,requestId:'r',instanceId:'i',documentId:'d',operation:'query.document.object.asset.font',payload:{action:'read',atRevision:0,documentSnapshotId:'s',assetId:'雪'.repeat(85),offset:0,length:1536}};
  SDK.validateRequest(q);assert.throws(()=>SDK.validateRequest({...q,payload:{...q.payload,assetId:'雪'.repeat(86)}}),/opaque/);
  assert.throws(()=>SDK.validateRequest({...q,payload:{...q.payload,length:1537}}));assert.throws(()=>SDK.validateRequest({...q,expectedRevision:0}));
  assert.throws(()=>SDK.validateRequest({...q,payload:{...q.payload,url:'x'}}));
  const adapter=SDK.createNativeApplicationAdapter('ui',{dispatch:async r=>({apiVersion:2,requestId:r.requestId,instanceId:'i',documentId:'d',contentRevision:0,ok:true,result:{atRevision:0,documentSnapshotId:'wrong',assetId:r.payload.assetId,offset:0,returnedLength:1,totalLength:1,encodedSha256:'a'.repeat(64),base64:'AA=='}})});
  await assert.rejects(adapter.dispatch(q),/snapshot/);
});
test('cancel before dispatch and invalid descriptor make no native call; direct indeterminate import stays latched',async()=>{
  const f=await fixture(),controller=new AbortController();controller.abort();
  await assert.rejects(f.assets.load('Roboto-Regular',controller.signal),/cancelled/);assert.equal(f.requests.length,0);
  await assert.rejects(f.resources.importSource({kind:'google',family:'x',weight:1001,italic:false}));assert.equal(f.requests.length,0);
  f.mode('missing');await assert.rejects(f.resources.importSource(bundled),e=>e.code==='font_import_indeterminate');
  f.mode('');await assert.rejects(f.resources.importSource(bundled),e=>e.code==='font_import_indeterminate');assert.equal(f.requests.length,2);
});
test('queued replacement during late parse rejects ordinary publication and cache does not survive replacement',async()=>{
  const f=await fixture(),wait=deferred();f.parseHold(wait);const pending=f.assets.googleWeight('LateOwner',400,false);await turn();
  const replacement=f.replace();wait.resolve();await assert.rejects(pending,/unavailable|changed/);await replacement;
  assert.equal(Object.hasOwn(f.fonts,'Google:LateOwner-Regular'),false);f.parseHold(null);await f.assets.googleWeight('LateOwner',400,false);assert.equal(f.parsed(),2);
});
test('cancellation during parser prevents parsed cache and native pending weight does not lose ordinary loader controls',async()=>{
  const f=await fixture(),wait=deferred(),controller=new AbortController();f.parseHold(wait);
  const pending=f.assets.googleWeight('Pending',700,false,controller.signal);await turn();assert.equal(f.assets.hasLoad('Google:Pending-Bold'),true);
  controller.abort();wait.resolve();await assert.rejects(pending,/cancelled/);assert.equal(f.assets.hasLoad('Google:Pending-Bold'),false);
  assert.equal(Object.hasOwn(f.fonts,'Google:Pending-Bold'),false);f.parseHold(null);await f.resources.loadAsset(f.assetId);assert.equal(f.parsed(),2);
});
test('linked catalog entries remain exact unavailable metadata and never resolve locator or parser',async()=>{
  const f=await fixture();f.rows([{assetId:f.assetId,name:' linked font ',sourceKind:'linked',available:false,reason:'linked_font_unavailable'}]);
  const rows=await f.resources.discover();assert.equal(rows[0].available,false);await assert.rejects(f.resources.loadAsset(f.assetId),/unavailable/);
  assert.equal(f.parsed(),0);assert.equal(f.requests.some(r=>r.payload.action==='read'),false);
});
test('closed owner rejects read after await before parsing or cache publication',async()=>{
  const wait=deferred();let active=true,parses=0;
  const pin={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s'};
  const session={info:()=>pin,isActive:()=>active,requestId:()=> 'r',runFontResource:action=>action({dispatch:async()=>{await wait.promise;return {ok:true,result:{font:{assetId:'font',available:true,sourceKind:'embedded',encodedBytes:1,encodedSha256:'a'.repeat(64)}}};}})};
  const r=Resources.create({session,digest,parse:()=>{parses++;return {};}}),pending=r.loadAsset('font');active=false;wait.resolve();
  await assert.rejects(pending,/unavailable/);assert.equal(parses,0);
});
test('lost refusal recovers refusal instead of retrying import or parsing',async()=>{
  let requests=0,reconciled=0;const pin={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'s'};
  let imported;
  const session={info:()=>pin,isActive:()=>true,requestId:()=> 'r'+(++requests),runFontResource:action=>action({
    dispatch:async r=>{if(r.expectedRevision!==undefined){imported=r;throw Error('response lost');}
      return {ok:true,result:{status:'refused',forRequestId:imported.requestId,requestFingerprintSha256:await digest(Buffer.from(Resources.fingerprintInput(imported))),contentRevision:0,code:'unavailable'}};},
    reconcile:async revision=>{assert.equal(revision,0);reconciled++;}})};
  const resources=Resources.create({session,digest,parse:()=>assert.fail('parser must not run')});await assert.rejects(resources.importSource(bundled),/refused/);
  assert.equal(requests,2);assert.equal(reconciled,1);
});
test('common SDK enforces entire 4096 byte response and exact opaque escaped metadata',()=>{
  const response={apiVersion:2,requestId:'r',instanceId:'i',documentId:'d',contentRevision:0,ok:true,result:{atRevision:0,documentSnapshotId:'s',offset:0,nextOffset:null,totalFontRecords:1,
    rows:[{assetId:'data:'+('\n'.repeat(250)),name:'blob:Exact Name ',available:false,sourceKind:'linked',reason:'linked_font_unavailable'}]}};
  SDK.validateResponse(response,'r','query.document.object.asset.font');assert.equal(response.result.rows[0].name,'blob:Exact Name ');
  const big={...response,result:{...response.result,rows:[response.result.rows[0],response.result.rows[0]]}};assert.throws(()=>SDK.validateResponse(big,'r','query.document.object.asset.font'));
  const oversized={...response,ok:false,error:{code:'unavailable',message:'x'.repeat(4096)}};delete oversized.result;
  assert.throws(()=>SDK.validateResponse(oversized,'r','query.document.object.asset.font'),/4096/);
});
const fingerprintVectors=JSON.parse(fs.readFileSync(__dirname+'/../engineering/application/examples/native-font-v1/fingerprint-vectors.json','utf8'));
for(const vector of fingerprintVectors.vectors)test('independent Python canonical fingerprint vector: '+vector.name,async()=>{
  const actual=Resources.fingerprintInput(vector.request);assert.equal(actual,vector.canonicalJson);
  assert.equal(await digest(Buffer.from(actual,'utf8')),vector.sha256);
  if(vector.request.cancelledBeforeDispatch===false)assert.equal(actual.includes('cancelledBeforeDispatch'),false);
});

test('indeterminate source-failure envelope recovers retained committed receipt and exact chosen bytes',async()=>{
  const f=await fixture();f.mode('envelopeCommitted');await f.assets.googleWeight(' Exact Family ',400,true);
  assert.equal(f.parsed(),1);assert.equal(f.session.info().contentRevision,1);assert.equal(f.fonts['Google: Exact Family -Regular'].label,' Exact Family ');
  const imports=f.requests.filter(r=>r.expectedRevision!==undefined);assert.equal(imports.length,1);
  const receipts=f.requests.filter(r=>r.payload.action==='receipt');assert.equal(receipts.length,1);assert.equal(receipts[0].payload.forRequestId,imports[0].requestId);
  assert.ok(f.requests.some(r=>r.payload.action==='read'));await f.assets.googleWeight(' Exact Family ',400,true);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,1);
});
test('indeterminate source-failure envelope with missing receipt latches and preserves prior accepted font without fallback or reimport',async()=>{
  const f=await fixture();const accepted=await f.assets.load('Roboto-Regular'),before=structuredClone(f.fonts);f.mode('envelopeMissing');
  await assert.rejects(f.assets.googleWeight('Uncertain',400,false),e=>e.code==='font_import_indeterminate');
  assert.deepEqual(f.fonts,before);assert.equal(f.parsed(),1);assert.equal(await f.assets.load('Roboto-Regular'),accepted);
  const requests=f.requests.length;await assert.rejects(f.assets.googleWeight('Uncertain',400,false),e=>e.code==='font_import_indeterminate');
  await assert.rejects(f.resources.importSource({kind:'google',family:'Other',weight:700,italic:false}),e=>e.code==='font_import_indeterminate');
  assert.equal(f.requests.length,requests);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,2);assert.equal(f.session.info().contentRevision,1);
});
test('ordinary resolved font cache refuses current native undo removal without reimporting old descriptor',async()=>{
  const f=await fixture();await f.assets.load('Roboto-Regular');const imports=f.requests.filter(r=>r.expectedRevision!==undefined).length;
  await f.session.historyFromUi('undo');await assert.rejects(f.assets.load('Roboto-Regular'),/refused/);
  await assert.rejects(f.assets.load('Roboto-Regular'),/refused/);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,imports);
  assert.equal(f.requests.at(-1).payload.action,'inspect');assert.equal(f.requests.at(-1).payload.atRevision,f.session.info().contentRevision);assert.equal(f.parsed(),1);
});
test('ordinary parsed cache preserves exact font across native fill only after current pinned catalog inspect',async()=>{
  const f=await fixture(),font=await f.assets.load('Roboto-Regular');await f.session.reference({kind:'setOpacity',opacity:0.5});
  const before=f.requests.length;assert.equal(await f.assets.load('Roboto-Regular'),font);
  const reuse=f.requests.slice(before);assert.equal(reuse.length,1);assert.equal(reuse[0].payload.action,'inspect');assert.equal(reuse[0].payload.atRevision,2);
  assert.equal(f.parsed(),1);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,1);
});
test('aborted second ordinary font waiter cannot receive the first caller accepted parsed font',async()=>{
  const f=await fixture(),hold=deferred(),controller=new AbortController();f.hold(hold);
  const first=f.assets.load('Roboto-Regular');await turn();const second=f.assets.load('Roboto-Regular',controller.signal);await turn();controller.abort();hold.resolve();
  assert.ok(await first);await assert.rejects(second,/cancelled/);assert.equal(f.parsed(),1);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,1);
});
test('Google load key honors its own signal when reusing an accepted promise',async()=>{
  const f=await fixture();await f.assets.addGoogle('Accepted');const controller=new AbortController();
  const pending=f.assets.load('Google:Accepted-Regular',controller.signal);controller.abort();await assert.rejects(pending,/cancelled/);
  assert.equal(f.parsed(),1);assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,1);
});
function actualDesktopBridge(f){
  const sandbox={Promise,Map,Uint8Array,atob,__TAURI__:{core:{invoke:()=>assert.fail('legacy invoke')}},
    NemoNativeFontResources:{desktop:()=>f.resources},fetch:()=>assert.fail('fetch fallback'),opentype:{parse:()=>assert.fail('browser parser')}};
  sandbox.window=sandbox;vm.runInNewContext(fs.readFileSync(__dirname+'/../src/js/adapters/font-assets.js','utf8'),sandbox);
  vm.runInNewContext(fs.readFileSync(__dirname+'/../src/js/vector-text-bridge.js','utf8'),sandbox);return sandbox.SMVectorText;
}
test('actual desktop bridge explicitly loads Google Bold without prewarm and preserves Regular on refused Bold',async()=>{
  const f=await fixture(),bridge=actualDesktopBridge(f);await bridge.addGoogleFont('Choice');
  assert.deepEqual(f.requests.filter(r=>r.expectedRevision!==undefined).map(r=>r.payload.source.weight),[400]);
  const hold=deferred();f.hold(hold);const pending=bridge.buildVectorTextGroup('X','Google:Choice-Regular',12,'#fff','left',null,{x:0,y:0},null,{bold:true});
  await turn();assert.equal(Object.hasOwn(bridge.VECTOR_FONTS,'Google:Choice-Bold'),false);
  assert.deepEqual(f.requests.filter(r=>r.expectedRevision!==undefined).map(r=>r.payload.source.weight),[400,700]);
  hold.resolve();await assert.rejects(pending,/charToGlyph/);assert.equal(Object.hasOwn(bridge.VECTOR_FONTS,'Google:Choice-Bold'),true);
  f.hold(null);const count=f.requests.filter(r=>r.expectedRevision!==undefined).length;
  await assert.rejects(bridge.buildVectorTextGroup('X','Google:Choice-Regular',12,'#fff','left',null,{x:0,y:0},null,{bold:true}),/charToGlyph/);
  assert.equal(f.requests.filter(r=>r.expectedRevision!==undefined).length,count);assert.equal(f.parsed(),2);
  const g=await fixture(),refused=actualDesktopBridge(g);await refused.addGoogleFont('Refused');g.mode('refuse');
  await assert.rejects(refused.buildVectorTextGroup('X','Google:Refused-Regular',12,'#fff','left',null,{x:0,y:0},null,{bold:true}),/refused/);
  assert.equal(Object.hasOwn(refused.VECTOR_FONTS,'Google:Refused-Bold'),false);assert.equal(Object.hasOwn(refused.VECTOR_FONTS,'Google:Refused-Regular'),true);
});
test('actual SDK rejects inconsistent catalog pagination, noncanonical base64 pad bits and future receipt revision',async()=>{
  const base={apiVersion:2,requestId:'r',instanceId:'i',documentId:'d',operation:'query.document.object.asset.font',payload:{action:'list',atRevision:0,documentSnapshotId:'s',offset:0,limit:1}};
  function response(result){return {apiVersion:2,requestId:'r',instanceId:'i',documentId:'d',contentRevision:0,ok:true,result};}
  const badList=response({atRevision:0,documentSnapshotId:'s',offset:0,totalFontRecords:1,rows:[],nextOffset:null});
  await assert.rejects(SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>badList}).dispatch(base),/list/);
  const read={...base,payload:{action:'read',atRevision:0,documentSnapshotId:'s',assetId:'font',offset:0,length:1}};
  const badRead=response({atRevision:0,documentSnapshotId:'s',assetId:'font',offset:0,returnedLength:1,totalLength:1,encodedSha256:'a'.repeat(64),base64:'QR=='});
  await assert.rejects(SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>badRead}).dispatch(read),/chunk/);
  const receipt={...base,payload:{action:'receipt',forRequestId:'original'}};
  const future=response({status:'refused',forRequestId:'original',requestFingerprintSha256:'a'.repeat(64),contentRevision:1,code:'unavailable'});
  await assert.rejects(SDK.createNativeApplicationAdapter('ui',{dispatch:async()=>future}).dispatch(receipt),/receipt/);
  assert.throws(()=>SDK.validateRequest({...base,payload:{...base.payload,offset:1025}}));
  assert.throws(()=>SDK.validateRequest({...read,payload:{...read.payload,offset:16*1024*1024}}));
});
