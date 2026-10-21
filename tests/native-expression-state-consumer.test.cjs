'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {createRequire}=require('node:module');
const SDK=require('../src/js/adapters/native-application.js');
const fixturePath=path.join(__dirname,'native-motion-property-panel.test.cjs');
const fixtureSource=fs.readFileSync(fixturePath,'utf8');
const {host,tree,tick}=new Function('require',fixtureSource.slice(0,fixtureSource.indexOf('\ntest('))+';return {host,tree,tick};')(createRequire(fixturePath));
const rows=require('../engineering/application/examples/native-property-metadata-v1/cases.json').propertyPageOracle;
function query(){return {apiVersion:2,requestId:'expression-state',instanceId:'instance',documentId:'document',operation:'query.document.object.property.read',payload:{atRevision:7,documentSnapshotId:'native-object:document:7',view:{kind:'expressionState',target:{contextId:'scene-root',layerUid:'L0'},property:'opacity',frame:2}}};}
function reply(q,evaluation){return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:q.payload.atRevision,ok:true,result:{instanceId:q.instanceId,documentId:q.documentId,atRevision:q.payload.atRevision,documentSnapshotId:q.payload.documentSnapshotId,kind:q.payload.view.kind,target:q.payload.view.target,property:q.payload.view.property,dimension:1,frame:q.payload.view.frame,expression:{present:true,enabled:false},evaluation}};}
test('existing SDK accepts typed native diagnostic states and rejects forged correlations',async()=>{
 const q=query();
 for(const diagnostic of [null,{kind:'timeout'},{kind:'tooDeep'},{kind:'mustReturnNumber'},{kind:'mustReturnNumberOrXY'},{kind:'runtime',message:'data:literal <b>error</b>',line:0},{kind:'syntax',message:'bad',line:null}]){
  const r=reply(q,{state:'evaluated',diagnostic});const sdk=SDK.createNativeApplicationAdapter('state',{dispatch:()=>r});assert.deepEqual(await sdk.readProperty(q),r);
 }
 for(const mutate of [r=>r.result.frame++,r=>r.result.atRevision++,r=>r.result.documentSnapshotId='foreign',r=>r.result.expression.present=false,r=>r.result.evaluation.diagnostic={kind:'timeout',extra:true},r=>r.result.evaluation.diagnostic={kind:'runtime',message:'x',line:-1},r=>r.result.evaluation={state:'unavailable',reason:'not-evaluated',diagnostic:null}]){
  const r=reply(q,{state:'evaluated',diagnostic:{kind:'timeout'}});mutate(r);const sdk=SDK.createNativeApplicationAdapter('state',{dispatch:()=>r});await assert.rejects(sdk.readProperty(q));
 }
});
function mount(evaluation,delay){
 const h=host(),calls=[],waiting=[];let nonce=0,currentEvaluation=evaluation,delayReads=delay;
 const sdk=SDK.createNativeApplicationAdapter('row',{dispatch(q){calls.push(q);let r=reply(q,currentEvaluation);const d=rows.find(d=>d.property===q.payload.view.property);r.result.dimension=d.dimension;
  if(q.payload.view.kind==='rawSample'){delete r.result.expression;delete r.result.evaluation;r.result.value=Array(d.dimension).fill(17);}
  if(delayReads&&q.payload.view.kind==='expressionState')return new Promise(resolve=>waiting.push(()=>resolve(r)));return Promise.resolve(r);
 }});
 h.ports.readProperty=async selector=>{const response=await sdk.readProperty({apiVersion:2,requestId:'expression-row-'+(++nonce),instanceId:selector.instanceId,documentId:selector.documentId,operation:'query.document.object.property.read',payload:{atRevision:selector.atRevision,documentSnapshotId:selector.documentSnapshotId,view:selector.view}});return response.result;};
 return {...h,calls,waiting,setEvaluation(value){currentEvaluation=structuredClone(value);},setDelay(value){delayReads=value;},errors:()=>tree(h.nodes['motion-props-body']).filter(n=>n.className==='motion-expr-error'&&!n.hidden)};
}
test('actual native Motion rows display text and line from the native receipt without an HTML or document writer',async()=>{
 const h=mount({state:'evaluated',diagnostic:{kind:'runtime',message:'literal <b>error</b>',line:3}});h.paint();await h.settle();await tick();
 assert.ok(h.errors().length);assert.ok(h.errors().every(n=>n.textContent==='literal <b>error</b>'&&n.dataset.errorLine==='3'&&n.children.length===0));assert.ok(h.calls.every(q=>q.operation.startsWith('query.')));h.retire();
});
test('actual clear hides previous errors and unvisited states remain explicitly unavailable',async()=>{
 for(const [evaluation,expected] of [[{state:'evaluated',diagnostic:null},null],[{state:'unavailable',reason:'not-evaluated'},'Expression has not been evaluated for this frame.']]){
  const h=mount(evaluation);h.paint();await h.settle();await tick();if(expected)assert.ok(h.errors().every(n=>n.textContent===expected)&&h.errors().length);else assert.equal(h.errors().length,0);h.retire();
 }
});
test('delayed native diagnostics cannot paint after frame, owner, transport or view retirement',async()=>{
 for(const retire of [h=>{h.root.state.currentFrame++;},h=>h.replace(),h=>h.transport(),h=>h.retire()]){
  const h=mount({state:'evaluated',diagnostic:{kind:'runtime',message:'late error',line:2}},true);h.paint();await h.settle();await tick();assert.ok(h.waiting.length);retire(h);h.waiting.forEach(resolve=>resolve());await tick();assert.equal(h.errors().length,0);h.retire();
 }
});

test('one actual native inspector lifetime clears a prior displayed error on the next real native receipt',async()=>{
 const h=mount({state:'evaluated',diagnostic:{kind:'runtime',message:'prior error',line:4}});
 h.paint();await h.settle();await tick();assert.ok(h.errors().some(n=>n.textContent==='prior error'&&n.dataset.errorLine==='4'));
 const before=h.calls.filter(q=>q.payload.view.kind==='expressionState').length;
 const sameRow=h.rows()[0],draft=tree(sameRow).find(n=>n.tagName==='INPUT'&&n.type==='number');if(draft)draft.value='123';
 h.setEvaluation({state:'evaluated',diagnostic:null});h.paint();await h.settle();await tick();
 assert.ok(h.calls.filter(q=>q.payload.view.kind==='expressionState').length>before);
 assert.equal(h.errors().length,0);assert.strictEqual(h.rows()[0],sameRow);if(draft)assert.equal(draft.value,'123');assert.ok(h.calls.every(q=>q.operation.startsWith('query.')));h.retire();
});

test('paints during an in-flight diagnostic batch drain one latest frame without stale text or overlapping reads',async()=>{
 const h=mount({state:'evaluated',diagnostic:{kind:'runtime',message:'old frame error',line:2}});
 h.paint();await h.settle();await tick();assert.ok(h.errors().length);
 h.setDelay(true);h.paint();await h.settle();await tick();const first=h.waiting.splice(0);assert.ok(first.length);
 const before=h.calls.filter(q=>q.payload.view.kind==='expressionState').length;
 h.root.state.currentFrame=1;h.setEvaluation({state:'evaluated',diagnostic:null});
 for(let i=0;i<5;i++)h.paint();await h.settle();await tick();
 assert.equal(h.errors().length,0);assert.equal(h.waiting.length,0);
 assert.equal(h.calls.filter(q=>q.payload.view.kind==='expressionState').length,before);
 first.forEach(resolve=>resolve());await tick();await tick();const latest=h.waiting.splice(0);
 assert.equal(latest.length,first.length);assert.ok(h.calls.filter(q=>q.payload.view.kind==='expressionState').slice(-latest.length).every(q=>q.payload.view.frame===1));
 latest.forEach(resolve=>resolve());await tick();await tick();assert.equal(h.errors().length,0);assert.equal(h.waiting.length,0);
 assert.equal(h.calls.filter(q=>q.payload.view.kind==='expressionState').length,before+latest.length);h.retire();
});
test('retired diagnostic rows release display and never drain queued reads through the retired API',async()=>{
 const h=mount({state:'evaluated',diagnostic:{kind:'runtime',message:'private old text',line:1}},true);
 h.paint();await h.settle();await tick();h.paint();await h.settle();await tick();
 const before=h.calls.length;h.retire();h.waiting.splice(0).forEach(resolve=>resolve());await tick();await tick();
 assert.equal(h.calls.length,before);assert.equal(h.rows().length,0);assert.equal(h.errors().length,0);
});
