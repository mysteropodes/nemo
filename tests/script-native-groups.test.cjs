'use strict';
// CS8-SDK (#2354, I33): one successful script run is one undo group; a stale or foreign pin always falls back to one entry.
const test=require('node:test'),assert=require('node:assert/strict');
const Groups=require('../src/js/application/script-native-groups.js');
const id={instanceId:'i',documentId:'d',lifecycleGeneration:1};
const pin=(rev,over={})=>({...id,contentRevision:rev,...over});

test('a recorded run undoes and redoes as its own step count, only at the revision it ended on',()=>{
  const g=Groups.create();
  assert.equal(g.record(id,{start:4,end:7,steps:3}),true);
  assert.equal(g.plan('undo',pin(7)),3);
  assert.equal(g.plan('redo',pin(7)),1,'nothing was undone yet');
  assert.equal(g.moved('undo',pin(7),pin(8),3),true);
  assert.equal(g.plan('redo',pin(8)),3);assert.equal(g.plan('undo',pin(8)),1,'the group sits above the head now');
  assert.equal(g.moved('redo',pin(8),pin(9),3),true);
  assert.equal(g.plan('undo',pin(9)),3,'a redo returns the group to the head, at its new revision');
});

test('runs that follow each other form a chain: undoing one leaves the next one below it grouped',()=>{
  const g=Groups.create();
  g.record(id,{start:0,end:2,steps:2});g.record(id,{start:2,end:5,steps:3});
  assert.equal(g.depth(),2);
  assert.equal(g.plan('undo',pin(5)),3);g.moved('undo',pin(5),pin(6),3);
  assert.equal(g.plan('undo',pin(6)),2,'the earlier run is still one step');g.moved('undo',pin(6),pin(7),2);
  assert.equal(g.plan('undo',pin(7)),1,'below the first run there is nothing recorded');
  assert.equal(g.plan('redo',pin(7)),2);g.moved('redo',pin(7),pin(8),2);assert.equal(g.plan('redo',pin(8)),3);
});

test('any other write, history move or foreign identity drops the group: the fallback is one entry, never extra entries',()=>{
  const g=Groups.create();
  g.record(id,{start:0,end:3,steps:3});
  assert.equal(g.plan('undo',pin(4)),1,'a write after the run moved the revision');assert.equal(g.depth(),0,'and the chain is gone');
  g.record(id,{start:0,end:3,steps:3});
  assert.equal(g.plan('undo',pin(3,{documentId:'other'})),1);assert.equal(g.plan('undo',pin(3,{instanceId:'x'})),1);assert.equal(g.plan('undo',pin(3,{lifecycleGeneration:2})),1);
  g.record(id,{start:0,end:3,steps:3});
  assert.equal(g.moved('undo',pin(3),pin(4),1),false,'a single-entry move at a group head is not the group move');assert.equal(g.plan('undo',pin(4)),1);
  g.record(id,{start:0,end:3,steps:3});
  assert.equal(g.moved('undo',pin(3),pin(4,{documentId:'other'}),3),false);
  g.record(id,{start:0,end:3,steps:3});assert.equal(g.plan('clear',pin(3)),1);assert.equal(g.plan('undo',null),1);
});

test('a run that does not start where the chain ended starts a new chain; a write after an undo clears the undone groups',()=>{
  const g=Groups.create();
  g.record(id,{start:0,end:2,steps:2});g.record(id,{start:9,end:11,steps:2});
  assert.equal(g.depth(),1);assert.equal(g.plan('undo',pin(11)),2);
  g.moved('undo',pin(11),pin(12),2);
  g.record(id,{start:12,end:14,steps:2});
  assert.equal(g.depth(),1,'the undone group is replaced');assert.equal(g.plan('redo',pin(14)),1);
  g.record({...id,documentId:'second'},{start:14,end:16,steps:2});
  assert.equal(g.plan('undo',pin(14)),1,'a run on another document replaces the chain');
});

test('record refuses an empty or malformed run, and forget drops everything',()=>{
  const g=Groups.create();
  for(const bad of [null,{start:0,end:1,steps:0},{start:0,end:1,steps:1.5},{start:-1,end:1,steps:1},{start:0,end:'x',steps:1}])assert.equal(g.record(id,bad),false);
  assert.equal(g.record(null,{start:0,end:1,steps:1}),false);assert.equal(g.depth(),0);
  g.record(id,{start:0,end:1,steps:1});g.forget();assert.equal(g.depth(),0);
  assert.equal(typeof Groups.shared.plan,'function');
});
