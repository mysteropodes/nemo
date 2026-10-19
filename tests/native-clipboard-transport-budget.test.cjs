"use strict";
const test = require('node:test');
const assert = require('node:assert/strict');
const Contract = require('../src/js/adapters/native-frame-edit-contract.js');
const SDK = require('../src/js/adapters/native-application.js');
const descriptor = require('../engineering/application/capabilities-v2/native-frame-edit.json');
const schema = require('../engineering/application/native-transport-v2.schema.json');
const P = 'command.document.object.';
const count = v => Buffer.byteLength(JSON.stringify(v), 'utf8');
function request(op, payload) { return {apiVersion:2,requestId:'clipboard-budget',instanceId:'instance-A',documentId:'document-A',expectedRevision:7,operation:op,payload}; }
function q(kind) { return request(P+'frames.clipboard.'+kind,{command:'object.frames.clipboard.'+kind,selection:[]}); }
function reply(req, cells) { return {apiVersion:2,requestId:req.requestId,instanceId:req.instanceId,documentId:req.documentId,contentRevision:7,ok:true,result:{applied:false,historyEntriesAdded:0,clipboard:{instanceId:req.instanceId,documentId:req.documentId,cells}}}; }
function fillRequest(req, length) {
  req.payload.selection=[{layerUid:'',frame:0}];
  const bytes=length-count(req);
  req.payload.selection[0].layerUid='雪'.repeat(Math.floor(bytes/3))+'x'.repeat(bytes%3);
  assert.equal(count(req),length);return req;
}
test('descriptor/schema declare full clipboard envelopes while every other operation remains4096',()=>{
  assert.equal(Contract.DATA_BYTES,4194304);assert.equal(Contract.MESSAGE_BYTES,5246976);
  assert.equal(descriptor.resourceBoundary.maxRequestBytes,Contract.MESSAGE_BYTES);
  for(const op of descriptor.operations) {
    assert.equal(descriptor.resourceBoundary.requestBytesByOperation[op],Contract.requestByteLimit(op));
    assert.equal(descriptor.resourceBoundary.responseBytesByOperation[op],Contract.responseByteLimit(op));
  }
  for(const kind of ['Copy','Cut','Paste']) assert.equal(schema.$defs['ObjectFramesClipboard'+kind+'Payload']['x-maxEncodedRequestBytes'],Contract.MESSAGE_BYTES);
  for(const op of [P+'frames.repeat',P+'layer.key.blank',P+'layer.key.move',P+'frames.clipboard.duplicate']) {
    assert.equal(Contract.requestByteLimit(op),4096);assert.equal(Contract.responseByteLimit(op),4096);
  }
});
test('SDK admits complete encoded Unicode requests near/at new limit and refuses one byte over',()=>{
  for(const kind of ['copy','cut']) for(const size of [4096,4097,Contract.MESSAGE_BYTES-1,Contract.MESSAGE_BYTES]) {
    assert.doesNotThrow(()=>SDK.validateRequest(fillRequest(q(kind),size)));
  }
  assert.throws(()=>SDK.validateRequest(fillRequest(q('copy'),Contract.MESSAGE_BYTES+1)),/5246976/);
  const old=request(P+'layer.key.blank',{command:'object.layer.key.blank',layerUid:'',cell:0});
  old.payload.layerUid='x'.repeat(4096-count(old));assert.equal(count(old),4096);
  assert.doesNotThrow(()=>SDK.validateRequest(old));old.payload.layerUid+='x';assert.equal(count(old),4097);
  assert.throws(()=>SDK.validateRequest(old),/4096/);
});
test('SDK preserves large detached clipboard receipts and same-owner source correlation',async()=>{
  const req=q('copy'),cells=[{relLayer:0,relFrame:0,keyframe:true,records:[{padding:'雪'.repeat(4000)}]}];
  const good=reply(req,cells);assert.ok(count(good)>4096);
  assert.doesNotThrow(()=>SDK.validateResponse(good,req.requestId,req.operation));
  const adapter=response=>SDK.createNativeApplicationAdapter('clipboard-test',{dispatch(){return structuredClone(response);}});
  assert.deepEqual(await adapter(good).dispatch(req),good);
  for(const field of ['instanceId','documentId']) {
    const bad=structuredClone(good);bad.result.clipboard[field]='foreign';
    await assert.rejects(()=>adapter(bad).dispatch(req),/owner mismatch/);
  }
  const forged=structuredClone(good);forged.requestId='different';
  assert.throws(()=>SDK.validateResponse(forged,req.requestId,req.operation),/identity/);
});
test('record plus complete order-array bytes retain native4MiB independently of envelope reserves',()=>{
  const cell={relLayer:0,relFrame:0,keyframe:true,records:[{padding:''}],orderElements:[]};
  const overhead=count(cell.records[0])+count(cell.orderElements);
  cell.records[0].padding='x'.repeat(Contract.DATA_BYTES-overhead);
  const req=q('copy');const good=reply(req,[cell]);assert.ok(count(good)<Contract.MESSAGE_BYTES);
  assert.doesNotThrow(()=>SDK.validateResponse(good,req.requestId,req.operation));
  const paste=request(P+'frames.clipboard.paste',{command:'object.frames.clipboard.paste',instanceId:req.instanceId,documentId:req.documentId,layerUid:'L0',frame:0,cells:[cell]});
  assert.doesNotThrow(()=>SDK.validateRequest(paste));
  cell.records[0].padding+='x';assert.throws(()=>SDK.validateResponse(good,req.requestId,req.operation),/4MiB/);
  assert.throws(()=>SDK.validateRequest(paste),/4MiB/);
});
test('larger envelope never relaxes closed selection, null or unrelated result fields',()=>{
  const req=q('copy');req.payload.selection=[{layerUid:'L0',frame:0},{layerUid:'L0',frame:0}];
  assert.throws(()=>SDK.validateRequest(req),/duplicate/);
  req.payload.selection=null;assert.throws(()=>SDK.validateRequest(req));
  req.payload.selection=[];req.payload.extra='x'.repeat(8192);assert.throws(()=>SDK.validateRequest(req),/unknown/);
  const good=reply(q('copy'),[]);good.result.cycledLayers=0;
  assert.throws(()=>SDK.validateResponse(good,'clipboard-budget',P+'frames.clipboard.copy'),/unknown/);
});

test('native-shaped arrays above4096 records remain paired while the4096 cell bound is fixed',()=>{
  const prototype=require('../engineering/application/examples/native-object-v1/cases.json').records[0];
  const records=Array.from({length:4100},(_,index)=>({...structuredClone(prototype),target:{...prototype.target,strokeId:'record-'+index}}));
  const req=request(P+'frames.clipboard.paste',{command:'object.frames.clipboard.paste',instanceId:'instance-A',documentId:'document-A',layerUid:'L0',frame:0,cells:[{relLayer:0,relFrame:0,keyframe:true,records}]});
  assert.doesNotThrow(()=>SDK.validateRequest(req));
  const cell={relLayer:0,relFrame:0,keyframe:false,records:[]};req.payload.cells=Array.from({length:4097},()=>structuredClone(cell));
  assert.throws(()=>SDK.validateRequest(req),/bounded/);
});

// Source regressions for the actual bare-VM failure; no TextEncoder shim or
// changed font-only/bare-client oracle. Node Buffer is the independent UTF-8 oracle.
test('clipboard data bytes stay exact without browser globals for Unicode and surrogate JSON',()=>{
  const vm=require('node:vm'),fs=require('node:fs'),path=require('node:path');
  const context={};vm.createContext(context);
  vm.runInContext(fs.readFileSync(path.join(__dirname,'../src/js/adapters/native-frame-edit-contract.js'),'utf8'),context);
  assert.equal(vm.runInContext('typeof TextEncoder',context),'undefined');
  const op=P+'frames.clipboard.paste';
  for(const sample of ['ascii','雪','\u{1F600}','\ud800','\udc00','\ud800a\udc00','\ud800\udc00','\0"\\/\n']){
    // ASCII delimiters prevent new surrogate pairs across repeated sample units.
    const unit='x'+sample+'x',overhead=count({padding:''})+count([]);
    const unitBytes=count({padding:unit})-count({padding:''});
    const available=Contract.DATA_BYTES-overhead;
    const padding=unit.repeat(Math.floor(available/unitBytes))+'x'.repeat(available%unitBytes);
    const cell={relLayer:0,relFrame:0,keyframe:true,records:[{padding}],orderElements:[]};
    assert.equal(count(cell.records[0])+count(cell.orderElements),Contract.DATA_BYTES);
    const payload={command:'object.frames.clipboard.paste',instanceId:'instance-A',documentId:'document-A',layerUid:'L0',frame:0,cells:[cell]};
    context.packetJson=JSON.stringify(payload);
    const invoke='NemoNativeFrameEditContract.payload('+JSON.stringify(op)+',JSON.parse(packetJson))';
    assert.doesNotThrow(()=>vm.runInContext(invoke,context));
    cell.records[0].padding+='x';context.packetJson=JSON.stringify(payload);
    assert.equal(count(cell.records[0])+count(cell.orderElements),Contract.DATA_BYTES+1);
    assert.throws(()=>vm.runInContext(invoke,context),/4MiB/);
  }
});
