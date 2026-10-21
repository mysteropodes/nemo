'use strict';
const test=require('node:test'),assert=require('node:assert/strict');
const C=require('../src/js/adapters/native-composition-contract.js');
const edit=action=>({command:'object.project.composition.edit',action});
const read=selector=>({atRevision:7,documentSnapshotId:'snapshot-7',contextId:'scene-root',selector});
const settings=()=>({markersPresent:false,markerCount:0,effectiveBpm:120,effectiveBpmOffset:0,effectiveBpmShow:false});
const request=(payload,operation=C.READ)=>({apiVersion:2,requestId:'read',instanceId:'i',documentId:'d',operation,payload});
const result=value=>({instanceId:'i',documentId:'d',atRevision:7,documentSnapshotId:'snapshot-7',contextId:'scene-root',result:value});
const receipt=value=>({apiVersion:2,requestId:'read',instanceId:'i',documentId:'d',contentRevision:7,ok:true,result:result(value)});

test('composition deltas preserve finite authored domain and supplied optional presence',()=>{
  for(const action of [{kind:'addMarker',frame:-0.5},{kind:'addMarker',frame:3,name:'',color:''},
    {kind:'updateMarker',ordinal:4096,frame:-2,name:'é'.repeat(512)}, {kind:'removeMarker',ordinal:1},
    {kind:'clearMarkers'},{kind:'setBpm'},{kind:'setBpm',bpm:-3.5,bpmOffset:-0.25,bpmShow:false}]) {
    assert.doesNotThrow(()=>C.payload(C.EDIT,edit(action)));
    assert.deepEqual(C.action(action),action);
  }
  const copied=C.action({kind:'setBpm',bpm:140});assert.equal(Object.hasOwn(copied,'bpmOffset'),false);assert(Object.isFrozen(copied));
});

test('composition rejects full replacements, null, nonfinite, unknown and overlong text',()=>{
  for(const action of [[],{kind:'setCompositionMetadata',markers:[]},{kind:'setBpm',bpm:null},
    {kind:'setBpm',bpm:undefined},{kind:'setBpm',bpmShow:0},{kind:'setBpm',bpm:Infinity},
    {kind:'addMarker',frame:0,name:'bad\0name'},{kind:'addMarker',frame:0,name:'é'.repeat(513)},
    {kind:'addMarker',frame:0,color:'\ud800'}, {kind:'updateMarker',ordinal:0},
    {kind:'clearMarkers',extra:true},{kind:'removeMarker',ordinal:4097}])assert.throws(()=>C.payload(C.EDIT,edit(action)));
  let accessed=false;const accessor=Object.defineProperty({},'kind',{enumerable:true,get(){accessed=true;return 'clearMarkers';}});
  assert.throws(()=>C.action(accessor));assert.equal(accessed,false);
});

test('all original private selector forms and bounded pages have closed public shapes',()=>{
  for(const selector of [{kind:'settings'},{kind:'markerAt',index:4097},{kind:'markerNamed',name:''},
    {kind:'markerNearest',frame:-1.5},{kind:'markerPage',cursor:0,limit:1},{kind:'markerPage',cursor:4096,limit:16}])
    assert.doesNotThrow(()=>C.payload(C.READ,read(selector)));
  for(const selector of [[],{kind:'settings',extra:true},{kind:'markerPage',cursor:0,limit:0},
    {kind:'markerPage',cursor:0,limit:17},{kind:'markerPage',cursor:4097,limit:1},
    {kind:'markerNamed',name:null},{kind:'markerAt',index:0},{kind:'markerNearest',frame:NaN}])
    assert.throws(()=>C.payload(C.READ,read(selector)));
});

test('settings retain absence and require effective defaults to match raw fields',()=>{
  C.correlate(request(read({kind:'settings'})),receipt(settings()));
  const raw={...settings(),bpm:-2,bpmOffset:0,bpmShow:false,effectiveBpm:-2};
  assert.doesNotThrow(()=>C.result(C.READ,result(raw)));
  for(const value of [{...raw,effectiveBpm:120},{...settings(),markerCount:1},
    {...settings(),bpm:null},{...settings(),bpmShow:false,effectiveBpmShow:true}])assert.throws(()=>C.result(C.READ,result(value)));
});

test('page correlation rejects mixed pins, nonprogress, omission and substituted selectors',()=>{
  const page={kind:'markerPage',markersPresent:true,markerCount:2,cursor:0,items:[{ordinal:1,frame:-0.5}],nextCursor:1};
  const q=request(read({kind:'markerPage',cursor:0,limit:1}));assert.doesNotThrow(()=>C.correlate(q,receipt(page)));
  for(const r of [receipt({...page,nextCursor:0}),receipt({...page,nextCursor:null}),
    receipt({...page,items:[{ordinal:2,frame:0}]}),receipt({...page,items:[]}),
    receipt(settings()),{...receipt(page),contentRevision:8},
    {...receipt(page),result:{...result(page),documentSnapshotId:'other'}}])assert.throws(()=>C.correlate(q,r));
});

test('composition join preserves original contracts and their larger byte limits',()=>{
  const calls=[],base={admitted:op=>op==='base',requiresRevision:()=>true,requestByteLimit:()=>999999,responseByteLimit:()=>888888,
    payload:(...args)=>calls.push(args),result(){},correlate(){},paths:()=>new Set(['base'])};
  const joined=C.join(base);assert(joined.admitted(C.READ));assert(joined.admitted('base'));
  assert.equal(joined.requiresRevision(C.READ),false);assert.equal(joined.requiresRevision(C.EDIT),true);
  assert.equal(joined.requestByteLimit(C.EDIT),4096);assert.equal(joined.requestByteLimit('base'),999999);
  assert.equal(joined.responseByteLimit('base'),888888);joined.payload('base',{kept:true});assert.deepEqual(calls,[['base',{kept:true},undefined]]);
  assert(joined.paths(C.EDIT).has('payload.action.name'));assert(!joined.paths(C.EDIT).has('payload.action.path'));
});
