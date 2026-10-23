'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const Desktop=require('../src/js/adapters/native-composition-desktop.js');
const Contract=require('../src/js/adapters/native-composition-contract.js');
const clone=value=>JSON.parse(JSON.stringify(value));

// Native authority and Session.info are deliberately independent. Only reconcile
// inspects native head and publishes projected session identity, as production does.
function fixture(options={}) {
  let nativeRevision=0,projected={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,documentSnapshotId:'snapshot-0'};
  let metadata=clone(options.metadata||{markers:[{frame:-2},{frame:30.5,name:'',color:''}]}),ticket=1,pending=0,serial=0,tail=Promise.resolve(),active=true;
  const published=[],calls=[],reconciliations=[],divergence=[];let available=true,currentSession;
  const has=(o,k)=>Object.hasOwn(o,k);
  const shown=info=>({...info,presentation:{...info,owner:'native',status:'presented'}});
  const rawSettings=()=>{const value={markersPresent:has(metadata,'markers'),markerCount:(metadata.markers||[]).length,
    effectiveBpm:has(metadata,'bpm')?metadata.bpm:120,effectiveBpmOffset:has(metadata,'bpmOffset')?metadata.bpmOffset:0,effectiveBpmShow:has(metadata,'bpmShow')?metadata.bpmShow:false};
    for(const k of ['bpm','bpmOffset','bpmShow'])if(has(metadata,k))value[k]=metadata[k];return value;};
  async function dispatch(q) {
    calls.push(clone(q));Contract.payload(q.operation,q.payload);
    const response={apiVersion:2,requestId:q.requestId,instanceId:'i',documentId:'d',contentRevision:nativeRevision,ok:true};
    if(q.operation===Contract.EDIT) {
      assert.equal(q.expectedRevision,nativeRevision);assert.equal(q.payload.action.kind,'setBpm');
      const before=clone(metadata);for(const k of ['bpm','bpmOffset','bpmShow'])if(has(q.payload.action,k))metadata[k]=q.payload.action[k];
      const applied=JSON.stringify(before)!==JSON.stringify(metadata);if(applied)nativeRevision++;
      response.contentRevision=nativeRevision;response.result={applied,historyEntriesAdded:applied?1:0};
      divergence.push({native:nativeRevision,projected:projected.contentRevision});
    } else {
      assert.equal(q.payload.atRevision,nativeRevision,'a stale projected head cannot read the new native head');
      assert.equal(q.payload.documentSnapshotId,'snapshot-'+nativeRevision);
      let value;if(q.payload.selector.kind==='settings')value=rawSettings();
      else {const s=q.payload.selector,markers=metadata.markers||[],end=Math.min(markers.length,s.cursor+(options.pageSize||2));
        value={kind:'markerPage',markersPresent:has(metadata,'markers'),markerCount:markers.length,cursor:s.cursor,
          items:markers.slice(s.cursor,end).map((m,i)=>({ordinal:s.cursor+i+1,...m})),nextCursor:end<markers.length?end:null};}
      response.result={instanceId:'i',documentId:'d',atRevision:nativeRevision,documentSnapshotId:'snapshot-'+nativeRevision,contextId:'scene-root',result:value};
      if(options.onRead)await options.onRead(q,response,h);
    }
    return response;
  }
  const session={isActive:()=>active,info:()=>projected,requestId:()=> 'r'+(++serial),readSubscriptionGeneration:()=>ticket,
    readPresentedObjectSession:()=>pending===0&&active?shown(projected):null,
    runResourceCommand(action){pending++;const work=tail.then(async()=>{const origin=projected;
      return action({readPresentedObjectSession:()=>projected===origin&&pending===1?shown(origin):null,dispatch,
        async reconcile(revision){reconciliations.push(revision);assert.equal(revision,nativeRevision);
          const next={...projected,contentRevision:nativeRevision,documentSnapshotId:'snapshot-'+nativeRevision};
          if(options.reconcile==='missing')return projected;
          if(options.reconcile==='unpublished')return next;
          projected=next;return next;}});
    }).finally(()=>{pending--;});tail=work.catch(()=>{});return work;}};
  currentSession=session;
  const h={session,published,calls,reconciliations,divergence,
    replaceSession(){currentSession={...session};},unready(){available=false;},
    native:()=>({revision:nativeRevision,metadata:clone(metadata)}),projected:()=>clone(projected),
    replace(){projected={...projected,documentId:'replacement',lifecycleGeneration:2,documentSnapshotId:'replacement-0'};},
    resubscribe(){ticket++;},retire(){active=false;}};
  h.controller=Desktop.create({currentSession:()=>currentSession,available:()=>active&&pending===0&&available,
    publish(value){assert.equal(pending,0,'never publish an incomplete queued read');published.push(value);return true;}});
  return h;
}

test('edit receipt advances only native head; explicit reconciliation precedes projected readback',async()=>{
  const h=fixture();const out=await h.controller.edit({kind:'setBpm',bpm:137.5,bpmShow:false});
  assert.deepEqual(h.divergence,[{native:1,projected:0}]);assert.deepEqual(h.reconciliations,[1]);
  assert.equal(out.status,'applied-presented');assert.equal(h.native().revision,1);assert.equal(h.projected().contentRevision,1);
  assert.equal(h.published.length,1);const projection=h.controller.read();assert.strictEqual(projection,out.projection);
  assert.equal(projection.composition.bpm,137.5);assert.equal(hasOwn(projection.composition,'bpmOffset'),false);
  assert.deepEqual(projection.composition.markers,[{ordinal:1,frame:-2},{ordinal:2,frame:30.5,name:'',color:''}]);
  assert(Object.isFrozen(projection.composition.markers[0]));
});
function hasOwn(o,k){return Object.prototype.hasOwnProperty.call(o,k);}

test('missing or unpublishing reconcile cannot pass by automatically advancing modeled UI',async()=>{
  for(const mode of ['missing','unpublished']) {
    const h=fixture({reconcile:mode});const out=await h.controller.edit({kind:'setBpm',bpm:99});
    assert.equal(out.status,'applied-unconfirmed');assert.equal(out.applied,true);
    assert.equal(h.native().revision,1);assert.equal(h.projected().contentRevision,0);
    assert.deepEqual(h.divergence,[{native:1,projected:0}]);assert.equal(h.published.length,0);assert.equal(h.controller.read(),null);
    assert.equal(h.calls.length,1,'no read or render is allowed on the unreconciled projection');
  }
});

test('unchanged scalar intent preserves omitted fields without adding a native revision',async()=>{
  const h=fixture({metadata:{markers:[],bpm:120}});const out=await h.controller.edit({kind:'setBpm',bpm:120});
  assert.equal(out.status,'unchanged');assert.equal(out.applied,false);assert.deepEqual(h.reconciliations,[0]);
  assert.equal(h.native().revision,0);assert.equal(hasOwn(out.projection.composition,'bpmOffset'),false);
});

test('refresh gathers all pages before publishing and preserves raw authored ordering',async()=>{
  const markers=[{frame:30,name:'X'},{frame:-4},{frame:30,name:'X',color:''},{frame:0},{frame:0,name:''}];
  const h=fixture({metadata:{markers},pageSize:2});const out=await h.controller.refresh();
  assert.equal(h.calls.filter(q=>q.payload.selector.kind==='markerPage').length,3);assert.equal(h.published.length,1);
  assert.deepEqual(out.composition.markers,markers.map((m,i)=>({ordinal:i+1,...m})));assert.equal(h.reconciliations.length,0);
});

test('replacement or resubscription during a page discards the entire staged projection',async()=>{
  for(const change of ['replace','resubscribe']) {
    let changed=false;const h=fixture({onRead:async(q,r,h)=>{if(q.payload.selector.kind==='markerPage'&&!changed){changed=true;h[change]();}}});
    await assert.rejects(h.controller.refresh());assert.equal(h.published.length,0);assert.equal(h.controller.read(),null);
  }
});

test('mixed snapshot pages, nonprogress and count-only omissions refuse without partial render',async()=>{
  for(const mode of ['snapshot','nonprogress','missing','count']) {
    const h=fixture({metadata:{markers:[{frame:0},{frame:1},{frame:2}]},onRead:async(q,r)=>{
      if(q.payload.selector.kind!=='markerPage')return;
      if(mode==='snapshot')r.result.documentSnapshotId='other';
      if(mode==='nonprogress')r.result.result.nextCursor=q.payload.selector.cursor;
      if(mode==='missing')r.result.result.items=[];
      if(mode==='count')r.result.result.markerCount=4;
    }});
    await assert.rejects(h.controller.refresh());assert.equal(h.published.length,0);assert.equal(h.controller.read(),null);
  }
});

test('cached projection disappears immediately after identity or subscription change',async()=>{
  for(const change of ['replace','resubscribe','retire']) {
    const h=fixture();await h.controller.refresh();assert(h.controller.read());h[change]();assert.equal(h.controller.read(),null);
  }
});

test('explicit absent markers stay absent in metadata while UI gets an empty derived list',async()=>{
  const h=fixture({metadata:{}}),out=await h.controller.refresh();assert.equal(out.composition.markersPresent,false);
  assert.deepEqual(out.composition.markers,[]);assert.deepEqual(h.native().metadata,{});
  assert.equal(out.composition.effectiveBpm,120);assert.equal(hasOwn(out.composition,'bpm'),false);
});


test('explicit composition refusal observer reports the first predicate and preserves destructive reads',async()=>{
  const cases=[
    ['session-replaced',h=>h.replaceSession()],['inactive',h=>h.retire()],
    ['five-pin-stale',h=>h.replace()],['subscription-stale',h=>h.resubscribe()],
    ['presentation-missing',h=>{h.session.readPresentedObjectSession=()=>null;}],
    ['presentation-mismatch',h=>{h.session.readPresentedObjectSession=()=>({...h.projected(),presentation:{...h.projected(),owner:'native',status:'deferred'}});}],
    ['projection-unready',h=>h.unready()],
    ['exception',h=>{h.session.info=()=>{throw Error('PRIVATE /secret stack');};}]
  ];
  for(const [code,change]of cases){
    const h=fixture(),seen=[];await h.controller.refresh();const shown=h.controller.read(value=>seen.push(value));
    assert.ok(shown);assert.deepEqual(seen,[]);const calls=h.calls.length;change(h);
    assert.equal(h.controller.read(value=>{assert.equal(h.controller.read(),null);seen.push(value);}),null);
    assert.deepEqual(seen,[code]);assert.equal(h.calls.length,calls,'observation never refreshes or dispatches');
    assert.equal(h.controller.read(value=>seen.push(value)),null);assert.deepEqual(seen,[code,'cache-empty']);
  }
  const h=fixture(),seen=[];assert.equal(h.controller.read(value=>seen.push(value)),null);
  await h.controller.refresh();h.controller.invalidate();assert.equal(h.controller.read(value=>seen.push(value)),null);
  await h.controller.refresh();h.controller.dispose();assert.equal(h.controller.read(value=>seen.push(value)),null);
  assert.deepEqual(seen,['cache-empty','cache-empty','cache-empty'],'cache remains the first original predicate after invalidation/disposal');
  const throwing=fixture();await throwing.controller.refresh();throwing.resubscribe();
  assert.equal(throwing.controller.read(()=>{throw Error('PRIVATE observer');}),null);
  assert.equal(throwing.controller.read(),null);
});
