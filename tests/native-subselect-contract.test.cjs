'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const C=require('../src/js/adapters/native-subselect-contract.js'),Q=require('../src/js/adapters/native-object-query-contract.js');
const target={contextId:'scene-root',layerUid:'layer-a',frameScope:{kind:'authored',frame:0},strokeId:'stroke-a'};
const p={atRevision:7,documentSnapshotId:'native-object:document:7',stableTarget:target,subselect:{cursor:0,limit:16}};
const r={atRevision:7,documentSnapshotId:p.documentSnapshotId,subselect:{stableTarget:target,closed:false,total:2,cursor:0,nextCursor:null,preview:false,segments:[{point:[0,0],handleIn:[0,0],handleOut:[0,0]},{point:[10,0],handleIn:[0,0],handleOut:[0,0]}]}};
test('subselect accepts independent baseline intents and closed geometry pages',()=>{
  const oracle=require('../native-engine/tests/fixtures/subselect-baseline/oracle.json');
  for(const c of oracle.cases)assert.equal(C.intent(c.intent),true,JSON.stringify(c.intent));
  Q.payload(p);Q.result(r);assert.equal(Q.correlate(p,r,null,'document'),true);
  assert.throws(()=>Q.payload({...p,view:{kind:'presentation'}}));
  assert.throws(()=>Q.payload({...p,subselect:{...p.subselect,preview:null}}));
  for(const patch of [{cursor:1},{total:3},{nextCursor:0},{preview:true},{stableTarget:{...target,strokeId:'foreign'}}])assert.throws(()=>Q.correlate(p,{...r,subselect:{...r.subselect,...patch}},null,'document'));
  assert.throws(()=>Q.correlate(p,r,null,'replacement'));
});
test('subselect rejects duplicate indices, unknown fields and nonfinite or singular wire values',()=>{
  for(const intent of [{kind:'move',indices:[0,0],delta:[0,0]},{kind:'move',indices:[0],delta:[NaN,0]},{kind:'toggle',index:256},{kind:'toggle',index:1,extra:true},{kind:'insert',curve:0,time:0}])assert.equal(C.intent(intent),false);
});
test('subselect atFrame is an optional bounded integer, with source identity retained separately',()=>{
  const payload={command:'object.subselect.edit',stableTarget:target,intent:{kind:'move',indices:[0],delta:[0,0]}};
  C.payload(C.EDIT,payload);for(const atFrame of [0,5,4294967295])C.payload(C.EDIT,{...payload,atFrame});
  for(const atFrame of [-1,1.5,4294967296,null,'5',NaN])assert.throws(()=>C.payload(C.EDIT,{...payload,atFrame}));
});
test('subselect presentation pages bind native transforms to the exact view and preview disposition',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,limit:8};
  const p={atRevision:7,documentSnapshotId:'native-object:document:7',view};
  const r={...view,atRevision:7,documentSnapshotId:p.documentSnapshotId,total:1,nextCursor:null,preview:false,items:[{stableTarget:target,closed:false,count:2,matrix:[1,.2,.3,1,8,9]}]};delete r.limit;
  Q.payload(p);Q.result(r);Q.correlate(p,r,null,'document');
  for(const patch of [{workId:'native-work-2'},{viewGeneration:2},{frame:1},{preview:true},{total:2}])assert.throws(()=>Q.correlate(p,{...r,...patch},null,'document'));
  assert.throws(()=>Q.result({...r,items:[{...r.items[0],matrix:[1,2,2,4,0,0]}]}));
  const preview={stableTarget:target,intent:{kind:'move',indices:[0],delta:[5,7]}};
  Q.correlate({...p,view:{...view,preview}},{...r,preview:true},null,'document');
  assert.throws(()=>Q.payload({...p,view:{...view,preview:{...preview,extra:1}}}));
});
test('application transport preserves opaque source IDs in native subselect preview and selection pages',()=>{
  const A=require('../src/js/adapters/native-application.js');
  const t={...target,layerUid:'data:opaque layer',strokeId:'blob:opaque stroke'};
  const view={kind:'subselect',contextId:'scene-root',layerUid:t.layerUid,frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,limit:8,
    preview:{stableTarget:t,intent:{kind:'move',indices:[0],delta:[1,2]}}};
  const q={apiVersion:2,requestId:'opaque-subselect',instanceId:'instance',documentId:'document',operation:'query.document.object',payload:{atRevision:7,documentSnapshotId:p.documentSnapshotId,view}};
  const result={...view,atRevision:7,documentSnapshotId:p.documentSnapshotId,total:1,nextCursor:null,preview:true,items:[{stableTarget:t,closed:false,count:2,matrix:[1,0,0,1,0,0]}]};delete result.limit;
  A.validateRequest(q);
  A.validateResponse({apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:7,ok:true,result},q.requestId,q.operation);
});

test('paired tangent endpoint stays closed, finite and limited to broken pairs',()=>{
  const A=require('../src/js/adapters/native-application.js');
  const intent={kind:'handle',index:0,handle:'handleOut',point:[50,40],breakPair:true,pairedPoint:[30,20]};
  assert.equal(C.intent(intent),true);
  A.validateRequest({apiVersion:2,requestId:'paired-tangent',instanceId:'instance',documentId:'document',expectedRevision:7,operation:C.EDIT,payload:{command:'object.subselect.edit',stableTarget:target,intent}});
  for(const patch of [{breakPair:false},{pairedPoint:null},{pairedPoint:[Infinity,0]},{pairedPoint:[0]},{extra:1}])assert.equal(C.intent({...intent,...patch}),false);
  assert.equal(C.intent({kind:'toggle',index:0,pairedPoint:[1,2]}),false);
  for(const row of require('../native-engine/tests/fixtures/subselect-baseline/modifiers.json').cases)for(const step of row.steps)assert.equal(C.intent(step.intent),true,row.name);
});

test('paint pages require explicit correlated opt-in and pressure paint counts stay bounded',()=>{
  const painted={...r,subselect:{...r.subselect,closed:true,paint:true}};
  const query={...p,subselect:{...p.subselect,paint:true}};
  C.readCorrelate(query,painted,'document');
  assert.throws(()=>C.readPayload({...p,subselect:{...p.subselect,paint:false}}));
  assert.throws(()=>C.readCorrelate(p,painted,'document'));
  assert.throws(()=>C.readCorrelate(query,r,'document'));
  for(const patch of [{paint:false},{paint:null},{closed:false}])assert.throws(()=>C.readResult({...painted,subselect:{...painted.subselect,...patch}}));
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,total:1,nextCursor:null,preview:false,atRevision:7,documentSnapshotId:p.documentSnapshotId,
    items:[{stableTarget:target,closed:false,count:2,paintCount:20,matrix:[1,0,0,1,0,0]}]};
  C.viewResult(view);
  for(const paintCount of [null,false,0,1,-1,257,1.5,Number.MAX_SAFE_INTEGER+1])assert.throws(()=>C.viewResult({...view,items:[{...view.items[0],paintCount}]}));
  assert.throws(()=>C.viewResult({...view,items:[{...view.items[0],closed:true}]}));
});
test('a centre read (#2362 sculpt) is a hit-test read marked center, with one width per anchor',()=>{
  const q={...p,subselect:{...p.subselect,hitTest:true,center:true}};
  const a={...r,subselect:{...r.subselect,hitTest:true,center:true,widths:[2,3]}};
  C.readPayload(q);assert.equal(C.readCorrelate(q,a,'document'),true);
  assert.throws(()=>C.readPayload({...p,subselect:{...p.subselect,center:true}}),undefined,'only on a hit-test read');
  assert.throws(()=>C.readCorrelate(q,{...a,subselect:{...a.subselect,widths:[2]}},'document'),undefined,'one width per anchor');
  assert.throws(()=>C.readCorrelate(q,{...a,subselect:{...a.subselect,center:undefined}},'document'));
  const {center,widths,...unmarked}=a.subselect;
  assert.throws(()=>C.readCorrelate(q,{...a,subselect:unmarked},'document'),undefined,'the page must say it is a centreline');
});
test('a hit-test listing item must say whether the record is filled (#2372)',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,hitTest:true};
  const item={stableTarget:target,closed:true,count:4,matrix:[1,0,0,1,0,0],bounds:[0,0,10,10],role:'plain',filled:true};
  const r={...view,atRevision:7,documentSnapshotId:'native-object:document:7',total:1,nextCursor:null,preview:false,items:[item]};
  C.viewResult(r);C.viewResult({...r,items:[{...item,filled:false}]});
  const {filled,...missing}=item;
  for(const bad of [missing,{...item,filled:1},{...item,filled:null},{...item,filled:'true'}])assert.throws(()=>C.viewResult({...r,items:[bad]}),undefined,JSON.stringify(bad.filled));
  const {hitTest,...plain}=r;
  assert.throws(()=>C.viewResult({...plain,items:[{stableTarget:target,closed:false,count:2,matrix:[1,0,0,1,0,0],filled:true}]}),undefined,'an editable page has no filled flag');
});
test('a hit-test listing item names a brush anchor, copy or bitmap brush and the group they share (#2372)',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,hitTest:true};
  const item={stableTarget:target,closed:true,count:4,matrix:[1,0,0,1,0,0],bounds:[0,0,10,10],role:'brush',filled:true};
  const r={...view,atRevision:7,documentSnapshotId:'native-object:document:7',total:1,nextCursor:null,preview:false,items:[item]};
  for(const brush of ['anchor','copy','bitmap'])C.viewResult({...r,items:[{...item,brush,group:'g1'}]});
  C.viewResult({...r,items:[{...item,brush:'anchor'}]});
  for(const bad of [{brush:'dab',group:'g1'},{brush:'anchor',group:''},{brush:'anchor',group:7},{group:'g1'}])assert.throws(()=>C.viewResult({...r,items:[{...item,...bad}]}),undefined,JSON.stringify(bad));
});
test('a hit-test listing item may carry a finite non-negative strokeWidth, and only on a hit-test listing (#2372)',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,hitTest:true};
  const item={stableTarget:target,closed:false,count:2,matrix:[1,0,0,1,0,0],bounds:[0,0,10,10],role:'plain',filled:false};
  const r={...view,atRevision:7,documentSnapshotId:'native-object:document:7',total:1,nextCursor:null,preview:false,items:[item]};
  for(const strokeWidth of [0,1,40,0.5])C.viewResult({...r,items:[{...item,strokeWidth}]});
  C.viewResult(r);
  for(const bad of [-1,NaN,Infinity,'40',null,[40]])assert.throws(()=>C.viewResult({...r,items:[{...item,strokeWidth:bad}]}),undefined,String(bad));
  const {hitTest,...plain}=r;
  assert.throws(()=>C.viewResult({...plain,items:[{stableTarget:target,closed:false,count:2,matrix:[1,0,0,1,0,0],strokeWidth:4}]}),undefined,'an editable page has no strokeWidth');
});
test('a hit-test listing item may carry a finite boxAngle, and only on a hit-test listing (#2372)',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,hitTest:true};
  const item={stableTarget:target,closed:false,count:2,matrix:[1,0,0,1,0,0],bounds:[0,0,10,10],role:'plain',filled:false};
  const r={...view,atRevision:7,documentSnapshotId:'native-object:document:7',total:1,nextCursor:null,preview:false,items:[item]};
  for(const boxAngle of [30,-45.5,359.5])C.viewResult({...r,items:[{...item,boxAngle}]});
  for(const bad of [NaN,Infinity,'30',null,[30]])assert.throws(()=>C.viewResult({...r,items:[{...item,boxAngle:bad}]}),undefined,String(bad));
  const {hitTest,...plain}=r;
  assert.throws(()=>C.viewResult({...plain,items:[{stableTarget:target,closed:false,count:2,matrix:[1,0,0,1,0,0],boxAngle:30}]}),undefined,'an editable page has no boxAngle');
});
test('a hit-test page may be shorter than the limit when the engine shortened it to the byte cap (#2372 SEL-PAGE)',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:2,limit:8,hitTest:true};
  const p={atRevision:7,documentSnapshotId:'native-object:document:7',view};
  const item=n=>({stableTarget:{...target,strokeId:'s'+n},closed:true,count:4,matrix:[1,0,0,1,0,0],bounds:[0,0,10,10],role:'plain',filled:true});
  const {limit,...echo}=view;
  const page=(n,next)=>({...echo,atRevision:7,documentSnapshotId:'native-object:document:7',total:20,nextCursor:next,preview:false,items:Array.from({length:n},(_,i)=>item(i))});
  assert.equal(C.viewCorrelate(p,page(8,10),'document'),true,'a full page');
  for(const n of [1,3,7])assert.equal(C.viewCorrelate(p,page(n,2+n),'document'),true,n+' items, nextCursor follows the items sent');
  assert.throws(()=>C.viewCorrelate(p,page(3,10),'document'),undefined,'nextCursor must follow the items actually sent');
  assert.throws(()=>C.viewCorrelate(p,page(0,2),'document'),undefined,'an empty page before the end makes no progress');
  assert.throws(()=>C.viewCorrelate(p,page(9,11),'document'),undefined,'more than the limit');
  assert.throws(()=>C.viewCorrelate({...p,view:{...view,limit:2}},page(3,5),'document'),undefined,'more than the requested limit');
  const last={...page(3,null),total:5};
  assert.equal(C.viewCorrelate(p,last,'document'),true,'the final short page');
});
test('a hit-test listing item may carry the paramShape of a dynamic shape: its kind and finite members of that kind, never the box (#2372)',()=>{
  const view={kind:'subselect',contextId:'scene-root',layerUid:'layer-a',frame:0,workId:'native-work-1',viewGeneration:1,cursor:0,hitTest:true};
  const item={stableTarget:target,closed:true,count:4,matrix:[1,0,0,1,0,0],bounds:[0,0,10,10],role:'plain',filled:true};
  const r={...view,atRevision:7,documentSnapshotId:'native-object:document:7',total:1,nextCursor:null,preview:false,items:[item]};
  for(const paramShape of [{kind:'rect',tl:4,tr:4,br:0,bl:2.5},{kind:'rect'},{kind:'ellipse',startAngle:0,sweep:359.9,innerRadius:0.25},{kind:'star',pointCount:5,innerRatio:0.5,cornerRadius:0}])C.viewResult({...r,items:[{...item,paramShape}]});
  for(const bad of [{},{kind:'hexagon'},{kind:'rect',sweep:1},{kind:'rect',tl:'4'},{kind:'rect',tl:NaN},{kind:'rect',box:{x1:0,y1:0,x2:1,y2:1}},{kind:'star',pointCount:null},'rect',null,[]])
    assert.throws(()=>C.viewResult({...r,items:[{...item,paramShape:bad}]}),undefined,JSON.stringify(bad));
  const {hitTest,...plain}=r;
  assert.throws(()=>C.viewResult({...plain,items:[{stableTarget:target,closed:true,count:4,matrix:[1,0,0,1,0,0],paramShape:{kind:'rect'}}]}),undefined,'an editable page has no paramShape');
});
