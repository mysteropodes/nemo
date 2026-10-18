'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Contract=require('../src/js/adapters/native-text-read-contract.js');
const Reader=require('../src/js/adapters/native-text-read.js');
const SDK=require('../src/js/adapters/native-application.js');
const target={contextId:'scene-root',layerUid:'native layer / 雪',frameScope:{kind:'authored',frame:2},strokeId:'native-text:1:0'};
const scope={contextId:target.contextId,layerUid:target.layerUid,frameScope:target.frameScope};
const metadata={font:{kind:'asset',assetId:' native font / 雪 '},size:48,color:'#102030',align:'left',anchorTopLeft:{x:-10,y:20},fixedWidth:null,bold:false};
const clone=v=>JSON.parse(JSON.stringify(v));
function fixture(text='Hi 雪🦊\n"\\'){let pin={instanceId:'instance',documentId:'document',lifecycleGeneration:4,contentRevision:7,documentSnapshotId:'native-object:document:7'},serial=0,calls=[],override=null,queue=Promise.resolve();
  function reply(q){const v=q.payload.view,base={instanceId:pin.instanceId,documentId:pin.documentId,atRevision:pin.contentRevision,documentSnapshotId:pin.documentSnapshotId};let result;
    if(v.kind==='targets')result={...base,kind:'targets',scope:clone(v.scope),offset:v.offset,total:1,items:v.offset===0?[{target:clone(target)}]:[],nextOffset:null};
    else if(v.kind==='inspect')result={...base,kind:'inspect',target:clone(target),textBytes:Buffer.byteLength(text),block:clone(metadata)};
    else {const bytes=Buffer.from(text);let end=Math.min(v.offset+v.limit,bytes.length);while(end>v.offset&&(bytes[end]&0xc0)===0x80)end--;result={...base,kind:'text',target:clone(target),textBytes:bytes.length,offset:v.offset,text:bytes.subarray(v.offset,end).toString(),nextOffset:end<bytes.length?end:null};}
    return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:pin.contentRevision,ok:true,result};
  }
  const sdk=SDK.createNativeApplicationAdapter('text-fixture',{dispatch:async q=>{calls.push(clone(q));return override?override(q,reply(q)):reply(q);}});
  const session={info:()=>pin,isActive:()=>true,requestId:label=>`${label}-${++serial}`,runResourceCommand:action=>queue.then(()=>action({dispatch:q=>sdk.dispatch(q)}))};
  return {session,reader:Reader.create(session),pin:()=>clone(pin),move:p=>{pin={...pin,...p};},calls,override:f=>{override=f;},queue:p=>{queue=p;}};
}
test('actual shared SDK and serialized read client return frozen native targets and lossless block/font facts',async()=>{
  const f=fixture();const page=await f.reader.listPage(scope,0,8,f.pin());assert.deepEqual(page.items,[{target}]);assert.ok(Object.isFrozen(page.items[0].target));
  const block=await f.reader.readBlock(target,f.pin());assert.deepEqual(block.block,{...metadata,text:'Hi 雪🦊\n"\\'});assert.deepEqual(block.target,target);assert.deepEqual(block.pin,f.pin());assert.ok(Object.isFrozen(block.block));
  assert.deepEqual(f.calls.map(q=>q.payload.view.kind),['targets','inspect','text']);assert.ok(f.calls.every(q=>!('expectedRevision'in q)&&q.operation===Contract.OP));
});
test('large Unicode text is chunked under the existing complete reply budget without dropping characters',async()=>{
  const text='雪🦊"\\\n'.repeat(300),f=fixture(text);const block=await f.reader.readBlock(target,f.pin());assert.equal(block.block.text,text);
  const reads=f.calls.filter(q=>q.payload.view.kind==='text');assert.ok(reads.length>2);assert.ok(reads.every(q=>q.payload.view.limit===1024));assert.equal(reads[0].payload.view.offset,0);
});
test('stale queued snapshot is refused before the shared route dispatches',async()=>{
  const f=fixture();let release;f.queue(new Promise(r=>release=r));const job=f.reader.readBlock(target,f.pin());await Promise.resolve();f.move({contentRevision:8,documentSnapshotId:'new'});release();
  await assert.rejects(job,e=>e.code==='stale_revision');assert.equal(f.calls.length,0);
});
test('owner replacement between request and result cannot publish old text',async()=>{
  const f=fixture();f.override((q,r)=>{f.move({documentId:'replacement',lifecycleGeneration:5});return r;});await assert.rejects(f.reader.readBlock(target,f.pin()),e=>e.code==='stale_revision');assert.equal(f.calls.length,1);
});
test('caller mutating its target after submission cannot retarget a queued read',async()=>{
  const f=fixture();let release;f.queue(new Promise(r=>release=r));const mutable=clone(target);const job=f.reader.readBlock(mutable,f.pin());mutable.strokeId='foreign';release();
  assert.deepEqual((await job).target,target);assert.ok(f.calls.every(q=>q.payload.view.target.strokeId===target.strokeId));
});
test('synchronous busy route refusal becomes a rejected promise with no dispatch',async()=>{
  const f=fixture();f.session.runResourceCommand=()=>{throw new Error('busy');};await assert.rejects(f.reader.readBlock(target,f.pin()),/busy/);assert.equal(f.calls.length,0);
});
for(const [label,mutate]of [
  ['request id',r=>{r.requestId='forged';}],['snapshot',r=>{r.result.documentSnapshotId='forged';}],['outer revision',r=>{r.contentRevision++;}],
  ['inner owner',r=>{r.result.instanceId='other';}],['target',r=>{r.result.target.strokeId='other';}],['metadata text injection',r=>{r.result.block.text='injected';}],
  ['metadata unknown member',r=>{r.result.block.rawFontBytes=[1];}],['unrecognized font',r=>{r.result.block.font={kind:'google',family:'Arial'};}],
])test('forged '+label+' is rejected by actual SDK/DTO before publication',async()=>{const f=fixture();f.override((q,r)=>{mutate(r);return r;});await assert.rejects(f.reader.readBlock(target,f.pin()));assert.equal(f.calls.length,1);});
test('missing and cross-scope targets cannot be admitted by list result shape',async()=>{
  const f=fixture();f.override((q,r)=>{r.result.items[0].target.layerUid='other';return r;});await assert.rejects(f.reader.listPage(scope,0,8,f.pin()));
});
test('forged page progression and repeated identities cannot create infinite discovery loops',async()=>{
  for(const edit of [r=>{r.result.nextOffset=0;},r=>{r.result.items.push(clone(r.result.items[0]));r.result.total=2;}]){
    const f=fixture();f.override((q,r)=>{edit(r);return r;});await assert.rejects(f.reader.listPage(scope,0,8,f.pin()));
  }
});
test('changed lengths, backward chunk cursors and premature terminators are rejected',async()=>{
  for(const edit of [r=>{r.result.textBytes++;},r=>{r.result.nextOffset=0;},r=>{r.result.text='short';r.result.nextOffset=null;}]){
    const f=fixture('雪'.repeat(900));f.override((q,r)=>{if(q.payload.view.kind==='text')edit(r);return r;});await assert.rejects(f.reader.readBlock(target,f.pin()));assert.equal(f.calls.length,2);
  }
});
test('UTF-16 lone surrogate chunks cannot replace native scalar values',()=>{
  const f=fixture(),q={apiVersion:2,requestId:'q',instanceId:'instance',documentId:'document',operation:Contract.OP,payload:{atRevision:7,documentSnapshotId:'native-object:document:7',view:{kind:'text',target,offset:0,limit:1024}}};
  assert.throws(()=>Contract.result(Contract.OP,{instanceId:q.instanceId,documentId:q.documentId,atRevision:7,documentSnapshotId:q.payload.documentSnapshotId,kind:'text',target,textBytes:3,offset:0,text:'\ud800',nextOffset:null}));assert.equal(f.calls.length,0);
});
test('closed request variants refuse extra data, invalid limits and guessed unsupported contexts',()=>{
  const p={atRevision:7,documentSnapshotId:'native-object:document:7',view:{kind:'inspect',target}};
  for(const bad of [{...p,extra:true},{...p,view:['inspect',target]},{...p,view:{kind:'inspect',target,extra:true}},
    {...p,view:{kind:'text',target,offset:0,limit:1025}},{...p,view:{kind:'targets',scope,offset:0,limit:0}},
    {...p,view:{kind:'inspect',target:{...target,contextId:'guessed'}}}])assert.throws(()=>Contract.payload(Contract.OP,bad));
});
test('original metadata own keys reject literal __proto__ before any sentinel merge',async()=>{
  const f=fixture();f.override((q,r)=>{r.result.block=JSON.parse(JSON.stringify(r.result.block).slice(0,-1)+',"__proto__":{"arbitraryUnknown":"injected"}}');return r;});
  await assert.rejects(f.reader.readBlock(target,f.pin()),/metadata fields invalid/);
});
test('native positive dimensions are repeated in the read DTO while null dimensions remain valid',async()=>{
  for(const [field,value]of [['size',-1],['size',0],['fixedWidth',0],['fixedHeight',-1],['lineHeightMult',0]]){
    const f=fixture();f.override((q,r)=>{r.result.block[field]=value;return r;});await assert.rejects(f.reader.readBlock(target,f.pin()),/positive dimension/);
  }
  const f=fixture();f.override((q,r)=>{if(q.payload.view.kind==='inspect')r.result.block.fixedHeight=null;return r;});assert.equal((await f.reader.readBlock(target,f.pin())).block.fixedHeight,null);
});
test('only genuine actual-owner text refusals retain their typed disposition',async()=>{
  for(const code of ['wrong_instance','wrong_document']){
    const f=fixture();f.override((q,r)=>({apiVersion:2,requestId:q.requestId,instanceId:code==='wrong_instance'?'actual-instance':q.instanceId,documentId:'actual-document',contentRevision:3,ok:false,
      error:{code,message:'Actual native owner refusal',...(code==='wrong_document'?{details:{requestedDocumentId:q.documentId}}:{})}}));
    await assert.rejects(f.reader.readBlock(target,f.pin()),e=>e.code===code);
  }
  for(const error of [{code:'unavailable',message:'foreign'},{code:'wrong_document',message:'missing requested document'},{code:'wrong_document',message:'wrong requested document',details:{requestedDocumentId:'forged'}}]){
    const f=fixture();f.override((q,r)=>({apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:'actual-document',contentRevision:3,ok:false,error}));await assert.rejects(f.reader.readBlock(target,f.pin()));
  }
});
