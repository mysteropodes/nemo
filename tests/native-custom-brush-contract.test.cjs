'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const C = require('../src/js/adapters/native-custom-brush-contract.js');
const numbers = ['bristleCount','dashGap','edgeNoise','fixedAngle','grain','nibSize','noise','opacity','opacityJitter','polySides',
  'pressureEnd','pressureMid','pressureStart','rotationJitter','roughness','roundness','scatter','scribbleAngleSpread','scribbleCount',
  'scribbleLen','scribbleLenJitter','scribbleSpread','scribbleWidth','sharpness','sizeJitter','spaceJitter','spacing','tipCorner','valueJitter','savedBrushSize'];
const strings = ['rotationMode','scatterDistribution','tipShape','label'];
const query = selector => ({apiVersion:2,requestId:'read',instanceId:'i',documentId:'d',operation:C.READ,
  payload:{atRevision:7,documentSnapshotId:'s7',contextId:'scene-root',selector}});
const reply = result => ({apiVersion:2,requestId:'read',instanceId:'i',documentId:'d',contentRevision:7,ok:true,
  result:{instanceId:'i',documentId:'d',atRevision:7,documentSnapshotId:'s7',contextId:'scene-root',result}});
test('all 36 optional fields preserve raw supplied values and refuse null, unknowns and accessors', () => {
  const preset = Object.fromEntries(numbers.map(key => [key,-0.5]));
  strings.forEach(key => preset[key] = 'data:raw\0雪'); preset.markerTip = false;
  preset.customStamp = {segments:[{point:[0,-1],handleIn:[2,3],handleOut:[4,5]}],closed:false,pointCount:0};
  assert.equal(Object.keys(preset).length,36); C.preset(preset);
  for (const id of ['', '10', '2', '__proto__', 'raw\0"雪']) {
    C.payload(C.EDIT,{command:'object.project.customBrushPreset.edit',action:{kind:'upsert',id,preset}});
    C.action({kind:'delete',id});
  }
  for (const key of Object.keys(preset)) assert.throws(() => C.preset({[key]:null}));
  for (const value of [{unexpected:1},{opacity:Infinity},{customStamp:{segments:[{point:[1]}]}},
    {customStamp:{segments:[],closed:null}},{label:'\ud800'}]) assert.throws(() => C.preset(value));
  const value = {}; Object.defineProperty(value,'label',{enumerable:true,get(){throw Error('accessor executed');}});
  assert.throws(() => C.preset(value),/Invalid native/); C.preset({}); C.preset({label:'',customStamp:{segments:[]}});
});
test('ordered pages retain numeric and empty IDs, label presence and strict progress', () => {
  const q = query({kind:'catalogPage',cursor:0,limit:2}); C.payload(q.operation,q.payload);
  const value = {kind:'customBrushPresetPage',customBrushPresetsPresent:true,presetCount:3,cursor:0,
    items:[{ordinal:1,id:'10',label:''},{ordinal:2,id:'2'}],nextCursor:2};
  C.correlate(q,reply(value)); assert.deepEqual(value.items.map(row => row.id),['10','2']);
  for (const changes of [{nextCursor:1},{items:[]},{cursor:1},{items:[{ordinal:1,id:'2'},{ordinal:2,id:'2'}]}]) {
    assert.throws(() => C.correlate(q,reply({...value,...changes})));
  }
  for (const limit of [0,17,1.5]) assert.throws(() => C.payload(C.READ,{...q.payload,selector:{kind:'catalogPage',cursor:0,limit}}));
  const empty = {kind:'customBrushPresetPage',customBrushPresetsPresent:true,presetCount:0,cursor:0,items:[],nextCursor:null};
  C.correlate(q,reply(empty)); C.correlate(q,reply({...empty,customBrushPresetsPresent:false}));
});
test('record correlation distinguishes missing optional fields, stale pins and edit noop receipts', () => {
  const q = query({kind:'record',id:''}), body = {kind:'customBrushPresetRecord',id:'',preset:{savedBrushSize:0,label:''}};
  C.correlate(q,reply(body)); assert.throws(() => C.correlate(q,reply({...body,id:'other'})));
  const stale = reply(body); stale.result.documentSnapshotId = 'old'; assert.throws(() => C.correlate(q,stale));
  const e = {...q,operation:C.EDIT,expectedRevision:7,payload:{command:'object.project.customBrushPreset.edit',action:{kind:'delete',id:'missing'}}};
  for (const applied of [false,true]) {
    const r = {...reply(null),contentRevision:7+Number(applied),result:{applied,historyEntriesAdded:Number(applied)}};
    C.correlate(e,r); r.contentRevision++; assert.throws(() => C.correlate(e,r));
  }
  assert(C.paths(C.EDIT).has('payload.action.id')); assert(C.paths(C.EDIT).has('payload.action.preset.label'));
  assert(!C.paths(C.EDIT).has('payload.action.preset.geometry'));
});
