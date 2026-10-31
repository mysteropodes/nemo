'use strict';
// CS8-SDK (#2354): the native script API reads a pinned model, queues writes, drains them
// through the ordinary UI entry points, and refuses unbound rows before any write.
const test=require('node:test'),assert=require('node:assert/strict');
const Api=require('../src/js/application/script-native-api.js');
function harness(layers){
  let info={instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:0,fps:24,canvasW:320,canvasH:180,totalFrames:12,layers:layers.map(l=>({...l}))};
  const calls=[],frames=[],toasts=[];let refuse=null;
  const write={};
  for(const key of ['visible','locked','shy'])write[key]=index=>{calls.push([key,index]);if(refuse===key)return Promise.resolve(false);
    const l=info.layers[index];info={...info,contentRevision:info.contentRevision+1,layers:info.layers.map((x,i)=>i===index?{...x,[key]:key==='visible'?x.visible===false:!(x[key]===true)}:x)};
    void l;return Promise.resolve(true);};
  const ports={info:()=>info,write,goToFrame:f=>frames.push(f),selection:()=>[1],toast:m=>toasts.push(m)};
  return {ports,calls,frames,toasts,get info(){return info;},set info(v){info=v;},refuse(k){refuse=k;}};
}
const L=[{layerUid:'a',name:'A',visible:true},{layerUid:'b',name:'B',visible:false,locked:true},{layerUid:'c',visible:true,shy:true}];

test('reads are detached, layer handles follow layerUid, and the vocabulary is preserved',()=>{
  const h=harness(L),run=Api.createRun(h.ports),n=run.nemo;
  assert.equal(n.layerCount,3);assert.equal(n.fps,24);assert.equal(n.frameCount,12);
  assert.deepEqual(n.layers().map(l=>[l.name,l.visible,l.locked,l.shy]),[['A',true,false,false],['B',false,true,false],['Layer 3',true,false,true]]);
  assert.equal(n.layer('B').layerUid,'b');assert.equal(n.layer(2).layerUid,'c');assert.equal(n.activeLayer().layerUid,'b');
  assert.deepEqual(n.selection.layers().map(l=>l.layerUid),['b']);
  h.info.layers[0].name='mutated after the run started';assert.equal(n.layer(0).name,'A');
  n.goToFrame(5);n.toast('hi');assert.deepEqual(h.frames,[5]);assert.deepEqual(h.toasts,['hi']);assert.deepEqual(n.log(),['toast']);
  assert.ok(n.help().includes('Layer.visible'));
  assert.throws(()=>n.layer('missing'),/no layer/);
});

test('writes are queued with read-your-writes, cancel out, and drain in order through the UI entries',async()=>{
  const h=harness(L),run=Api.createRun(h.ports),n=run.nemo;
  const a=n.layer('A'),b=n.layer('B');
  a.visible=false;assert.equal(a.visible,false);a.visible=true;a.visible=true;
  b.locked=false;b.visible=true;n.layer('c').shy=false;
  assert.deepEqual(h.calls,[],'nothing is written while the script runs');
  assert.deepEqual(await run.commit(),{ok:true,applied:3,pending:0});
  assert.deepEqual(h.calls,[['locked',1],['visible',1],['shy',2]]);
  assert.equal(h.info.layers[1].locked,false);assert.equal(h.info.layers[1].visible,true);assert.equal(h.info.layers[2].shy,false);
});

test('unbound and unavailable rows refuse with a typed error and never fall back',()=>{
  const n=Api.createRun(harness(L).ports).nemo,layer=n.layer(0);
  const typed=(fn,row,reason)=>assert.throws(fn,e=>e.code==='unavailable'&&e.row===row&&e.reason===reason);
  typed(()=>layer.blendMode,'sdk:nemo-script.Layer.blendMode','no-native-blend-matte');
  typed(()=>{layer.blendMode='add';},'sdk:nemo-script.Layer.blendMode','no-native-blend-matte');
  typed(()=>layer.get('position'),'sdk:nemo-script.Layer.get','not-yet-bound');
  typed(()=>layer.keys,'sdk:nemo-script.Layer.keys','not-yet-bound');
  typed(()=>n.isKeyframe(0,1),'sdk:nemo-script.isKeyframe','not-yet-bound');
  typed(()=>n.undoGroup(()=>{}),'sdk:nemo-script.undoGroup','script-run-grouping');
  typed(()=>n.markers.list(),'sdk:nemo-script.markers.list','not-yet-bound');
  typed(()=>n.camera.keys(),'sdk:nemo-script.camera.keys','no-native-camera');
  typed(()=>n.comp.setWorkArea(0,1),'sdk:nemo-script.comp.setWorkArea','no-native-work-area');
});

test('a replaced document, a removed layer or a refused write stops the drain (mutation-policy negative controls)',async()=>{
  let h=harness(L),run=Api.createRun(h.ports);run.nemo.layer(0).visible=false;
  h.info={...h.info,documentId:'other'};
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:1,error:'document-replaced'});assert.deepEqual(h.calls,[]);
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer(0).visible=false;h.info={...h.info,lifecycleGeneration:2};
  assert.equal((await run.commit()).error,'document-replaced');
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('A').visible=false;h.info={...h.info,layers:h.info.layers.slice(1)};
  assert.equal((await run.commit()).error,'layer-gone');assert.deepEqual(h.calls,[]);
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('B').locked=false;run.nemo.layer('A').visible=false;h.refuse('locked');
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:2,error:'write-refused'});assert.deepEqual(h.calls,[['locked',1]]);
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('A').visible=false;
  h.info={...h.info,layers:[h.info.layers[2],h.info.layers[0],h.info.layers[1]]};
  assert.deepEqual(await run.commit(),{ok:true,applied:1,pending:0});assert.deepEqual(h.calls,[['visible',1]],'resolved by layerUid after a reorder');
  h=harness(L);run=Api.createRun(h.ports);run.nemo.layer('A').visible=false;run.discard();
  assert.deepEqual(await run.commit(),{ok:true,applied:0,pending:0});assert.deepEqual(h.calls,[]);
  assert.throws(()=>Api.createRun({...h.ports,info:()=>null}),/no native document/);
});

test('undeclared members throw, frame routes to presentation, and the active layer is the active index',()=>{
  const h=harness(L),n=Api.createRun({...h.ports,frame:()=>3,activeIndex:()=>2}).nemo;
  const undeclared=(fn,pattern)=>assert.throws(fn,e=>e.code==='unavailable'&&e.reason==='undeclared'&&pattern.test(e.message));
  undeclared(()=>n.backgrond,/backgrond is not part/);undeclared(()=>{n.typo=1;},/typo/);
  undeclared(()=>n.layer(0).opacityy,/Layer\.opacityy/);undeclared(()=>{n.layer(0).custom=1;},/Layer\.custom/);
  undeclared(()=>n.comp.workAreaa,/comp\.workAreaa/);undeclared(()=>n.selection.everything(),/selection\.everything/);
  assert.equal(n.frame,3);n.frame=7;assert.deepEqual(h.frames,[7]);
  assert.equal(n.activeLayer().layerUid,'c');assert.equal(typeof n.then,'undefined','a returned API is not mistaken for a thenable');
  assert.equal(n.layer(0).name,'A');assert.ok(n.help().includes('Layer.name'));
  assert.throws(()=>{n.layer(0).name='x';},e=>e.code==='unavailable'&&e.reason==='native-entry-absent'&&e.row==='sdk:nemo-script.Layer.name');
  assert.throws(()=>Api.createRun(harness([{layerUid:'x'},{layerUid:'x'}]).ports),/duplicate layer id x/);
});

test('a write the native read model does not confirm is reported, not counted',async()=>{
  const h=harness(L),run=Api.createRun({...h.ports,write:{...h.ports.write,visible:()=>Promise.resolve(true)}});
  run.nemo.layer('A').visible=false;
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:1,error:'write-unconfirmed'});
});

// ---- queued structural, property and marker writes (CS8-SDK-A) ----
// A fake owner: each entry mutates the read model the way the native owner would, and records the call it received.
function owner(layers,hooks={}){
  const h=harness(layers),calls=[];let minted=0;
  const revise=fn=>{h.info=fn({...h.info,contentRevision:h.info.contentRevision+1});};
  const row=uid=>h.info.layers.findIndex(l=>l.layerUid===uid);
  const insert=(at,l)=>revise(i=>({...i,layers:[...i.layers.slice(0,at),l,...i.layers.slice(at)]}));
  const done=(name,extra)=>{ if(hooks[name])return hooks[name](extra,h); return null; };
  const ops={
    add:()=>{calls.push(['add']);const early=done('add');if(early)return early;const uid='n'+(++minted);insert(h.info.layers.length,{layerUid:uid,name:'Layer '+(h.info.layers.length+1),visible:true});return Promise.resolve({ok:true,applied:true,layerUid:uid});},
    remove:uid=>{calls.push(['remove',uid]);const early=done('remove',uid);if(early)return early;revise(i=>({...i,layers:i.layers.filter(l=>l.layerUid!==uid)}));return Promise.resolve({ok:true,applied:true});},
    duplicate:uid=>{calls.push(['duplicate',uid]);const early=done('duplicate',uid);if(early)return early;const at=row(uid),id='n'+(++minted);insert(at+1,{...h.info.layers[at],layerUid:id,name:h.info.layers[at].name+' copy'});return Promise.resolve({ok:true,applied:true,layerUid:id});},
    split:(index,frame)=>{calls.push(['split',index,frame]);const early=done('split',index);if(early)return early;const id='n'+(++minted);insert(index+1,{...h.info.layers[index],layerUid:id,name:h.info.layers[index].name+' (2)'});return Promise.resolve({ok:true,applied:true});},
    rename:(uid,name)=>{calls.push(['rename',uid,name]);const early=done('rename',uid);if(early)return early;revise(i=>({...i,layers:i.layers.map(l=>l.layerUid===uid?{...l,name}:l)}));return Promise.resolve({ok:true,applied:true});},
    property:spec=>{calls.push(['property',spec]);const early=done('property',spec);if(early)return early;revise(i=>i);return Promise.resolve({ok:true,applied:true});},
    track:spec=>{calls.push(['track',spec]);const early=done('track',spec);if(early)return early;revise(i=>i);return Promise.resolve({ok:true,applied:true});},
    marker:spec=>{calls.push(['marker',spec]);const early=done('marker',spec);if(early)return early;revise(i=>i);return Promise.resolve({ok:true,applied:true});},
  };
  return {h,calls,ops,run:(extra={})=>Api.createRun({...h.ports,ops,frame:()=>4,...extra})};
}

test('every write row without its native entry is refused at the call, before anything is queued, and help omits it',async()=>{
  const h=harness(L),n=Api.createRun(h.ports).nemo,layer=n.layer('A');
  const absent=(fn,row)=>assert.throws(fn,e=>e.code==='unavailable'&&e.reason==='native-entry-absent'&&e.row===row,row);
  absent(()=>n.addLayer('x'),'sdk:nemo-script.addLayer');absent(()=>layer.remove(),'sdk:nemo-script.Layer.remove');
  absent(()=>layer.duplicate(),'sdk:nemo-script.Layer.duplicate');absent(()=>layer.splitAt(2),'sdk:nemo-script.Layer.splitAt');
  absent(()=>{layer.name='x';},'sdk:nemo-script.Layer.name');absent(()=>layer.set('position',[1,2]),'sdk:nemo-script.Layer.set');
  absent(()=>layer.key('position',1,[1,2]),'sdk:nemo-script.Layer.key');absent(()=>layer.hold('position',1),'sdk:nemo-script.Layer.hold');
  absent(()=>layer.clearKeys('position'),'sdk:nemo-script.Layer.clearKeys');
  absent(()=>n.markers.add(1),'sdk:nemo-script.markers.add');absent(()=>n.markers.removeAt(1),'sdk:nemo-script.markers.removeAt');
  const help=n.help();for(const row of ['addLayer','Layer.remove','Layer.duplicate','Layer.splitAt','Layer.set','Layer.key','Layer.hold','Layer.clearKeys','markers.add','markers.removeAt'])assert.ok(!help.includes(row),row);
  assert.ok(help.includes('Layer.visible')&&help.includes('Layer.name'));
  assert.deepEqual(await Api.createRun(h.ports).commit(),{ok:true,applied:0,pending:0});assert.deepEqual(h.calls,[]);
});

test('structural writes drain in order, handles on new layers resolve through the minted ids, and the local model reads your writes',async()=>{
  const o=owner(L),run=o.run(),n=run.nemo;
  assert.ok(['addLayer','Layer.remove','Layer.duplicate','Layer.splitAt','Layer.set','Layer.key','Layer.hold','Layer.clearKeys','markers.add','markers.removeAt'].every(r=>n.help().includes(r)));
  const ink=n.addLayer('Ink');
  assert.equal(n.layerCount,4);assert.equal(ink.name,'Ink');assert.equal(ink.index,3);assert.equal(n.layer('Ink').layerUid,ink.layerUid);
  ink.visible=false;
  const twin=ink.duplicate();twin.name='Ink 2';assert.equal(n.layerCount,5);assert.equal(twin.index,4);assert.equal(twin.visible,false);
  const tail=twin.splitAt(3);assert.equal(tail.index,5);assert.equal(tail.name,'Ink 2 (2)');
  n.layer('A').remove();assert.equal(n.layerCount,5);assert.throws(()=>n.layer('A'),/no layer/);
  assert.deepEqual(o.calls,[],'nothing is written while the script runs');
  assert.deepEqual(await run.commit(),{ok:true,applied:7,pending:0});
  assert.deepEqual(o.calls.slice(0,2),[['add'],['rename','n1','Ink']]);
  assert.deepEqual(o.h.calls,[['visible',3]],'the flag write reaches the new layer by its minted id and current index');
  assert.deepEqual(o.calls.slice(2),[['duplicate','n1'],['rename','n2','Ink 2'],['split',4,3],['remove','a']]);
  assert.deepEqual(o.h.info.layers.map(l=>l.name),['B',undefined,'Ink','Ink 2','Ink 2 (2)']);
  assert.deepEqual(o.h.info.layers.map(l=>l.layerUid),['b','c','n1','n2','n3']);
});

test('property and marker writes carry literal specs for the native routes: set, key with an ease, hold, clearKeys, markers',async()=>{
  const o=owner(L),run=o.run(),n=run.nemo,a=n.layer('A');
  assert.equal(a.set('position',[10,20]),a);a.set('opacity',50);a.key('rotation',8,90,'easeOut');a.key('scale',2,[100,100]);a.hold('rotation',8);a.clearKeys('position');
  assert.equal(n.markers.add(6,'Beat'),n.markers);n.markers.add();n.markers.removeAt(6);
  assert.deepEqual(await run.commit(),{ok:true,applied:11,pending:0});
  const at=(property,frame,extra)=>({layerUid:'a',property,frame,...extra});
  assert.deepEqual(o.calls,[
    ['property',at('position',4,{action:{kind:'setValue',values:[10,20]}})],['property',at('opacity',4,{action:{kind:'setValue',values:[50]}})],
    ['property',at('rotation',8,{action:{kind:'setKey',values:[90]}})],['track',at('rotation',8,{preset:'easeOut'})],
    ['property',at('scale',2,{action:{kind:'setKey',values:[100,100]}})],['track',at('scale',2,{preset:'ease'})],
    ['track',at('rotation',8,{preset:'hold'})],['property',at('position',4,{action:{kind:'setAnimated',animated:false}})],
    ['marker',{action:'add',frame:6,name:'Beat'}],['marker',{action:'add',frame:4,name:''}],['marker',{action:'remove',frame:6,name:''}]]);
});

test('script-time validation fails the run before anything is queued: units, properties, eases, frames and the last layer',()=>{
  const o=owner(L),n=o.run().nemo,a=n.layer('A');
  const fails=(fn,pattern)=>assert.throws(fn,e=>e.code!=='unavailable'&&pattern.test(e.message));
  fails(()=>a.set('position',5),/position takes 2 finite/);fails(()=>a.set('opacity',[1,2]),/opacity takes 1 finite/);fails(()=>a.set('opacity',NaN),/finite/);
  fails(()=>a.set('colour',1),/unknown property "colour"/);fails(()=>a.key('opacity',1,1,'bouncy'),/unknown ease "bouncy"/);
  fails(()=>a.splitAt(12),/outside the composition/);fails(()=>a.splitAt(-1),/outside the composition/);fails(()=>n.markers.add(-3),/before the composition/);
  const typed=(fn,row,reason)=>assert.throws(fn,e=>e.code==='unavailable'&&e.row===row&&e.reason===reason);
  typed(()=>a.set('timeRemap',[1]),'sdk:nemo-script.Layer.set','no-native-time-remap');typed(()=>a.key('timeRemap',1,[1]),'sdk:nemo-script.Layer.key','no-native-time-remap');
  typed(()=>a.key('opacity',1,1,'strong'),'sdk:nemo-script.Layer.key','no-native-ease-strong');
  const solo=owner([{layerUid:'x',name:'X'}]).run().nemo;fails(()=>solo.layer(0).remove(),/keeps at least one layer/);
  assert.equal(o.calls.length,0);
});

test('a stale handle fails at script time, a layer removed by the owner stops the drain, and a failing script sends nothing', async()=>{
  let o=owner(L),run=o.run(),n=run.nemo;const b=n.layer('B');b.remove();
  assert.throws(()=>b.visible,/layer b is not in this document/);assert.throws(()=>b.duplicate(),/layer b is not in this document/);assert.throws(()=>{b.name='z';},/layer b is not in this document/);
  o=owner(L);run=o.run();run.nemo.layer('B').duplicate();run.nemo.layer('A').set('opacity',5);
  o.h.info={...o.h.info,layers:o.h.info.layers.filter(l=>l.layerUid!=='b')};
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:2,error:'layer-gone'});assert.deepEqual(o.calls,[]);
  o=owner(L);run=o.run();run.nemo.addLayer('x');try{run.nemo.layer('missing');}catch(e){run.discard();}
  assert.deepEqual(await run.commit(),{ok:true,applied:0,pending:0});assert.deepEqual(o.calls,[]);
});

test('the pin is checked before every step: a document replaced mid-drain stops the rest and nothing more is sent',async()=>{
  const o=owner(L,{add:(_,h)=>{h.info={...h.info,documentId:'other',layers:[...h.info.layers,{layerUid:'n1',name:'Layer 4',visible:true}]};return Promise.resolve({ok:true,applied:true,layerUid:'n1'});}});
  const run=o.run(),n=run.nemo;n.addLayer('x');n.layer('A').set('opacity',5);n.markers.add(1);
  assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:4,error:'write-unconfirmed'},'a pin that moved under the write is not confirmed');
  assert.deepEqual(o.calls,[['add']]);
  const p=owner(L,{rename:(_,h)=>{h.info={...h.info,lifecycleGeneration:9};return Promise.resolve({ok:true,applied:true});}});
  const run2=p.run();run2.nemo.layer('A').name='Z';run2.nemo.layer('B').set('opacity',5);
  assert.equal((await run2.commit()).error,'write-unconfirmed');assert.deepEqual(p.calls.map(c=>c[0]),['rename']);
  const q=owner(L),run3=q.run();run3.nemo.layer('A').set('opacity',5);q.h.info={...q.h.info,instanceId:'else'};
  assert.deepEqual(await run3.commit(),{ok:false,applied:0,pending:1,error:'document-replaced'});assert.deepEqual(q.calls,[]);
});

test('a refused, rejected or unconfirmed native write is reported with its place in the queue and stops the drain',async()=>{
  const cases=[
    ['add',{add:()=>Promise.resolve({ok:false,code:'busy'})},n=>n.addLayer('x'),'write-refused'],
    ['rejected',{remove:()=>Promise.reject(new Error('transport'))},n=>n.layer('A').remove(),'write-refused'],
    ['remove not shown',{remove:()=>Promise.resolve({ok:true,applied:true})},n=>n.layer('A').remove(),'write-unconfirmed'],
    ['duplicate mints two',{duplicate:(_,h)=>{h.info={...h.info,layers:[...h.info.layers,{layerUid:'p'},{layerUid:'q'}]};return Promise.resolve({ok:true,applied:true});}},n=>n.layer('A').duplicate(),'write-unconfirmed'],
    ['duplicate id disagrees',{duplicate:(_,h)=>{h.info={...h.info,layers:[...h.info.layers,{layerUid:'p'}]};return Promise.resolve({ok:true,applied:true,layerUid:'zzz'});}},n=>n.layer('A').duplicate(),'write-unconfirmed'],
    ['split in the wrong place',{split:(_,h)=>{h.info={...h.info,layers:[...h.info.layers,{layerUid:'p'}]};return Promise.resolve({ok:true,applied:true});}},n=>n.layer('A').splitAt(2),'write-unconfirmed'],
    ['rename not shown',{rename:()=>Promise.resolve({ok:true,applied:true})},n=>{n.layer('A').name='Z';},'write-unconfirmed'],
    ['property applied without a new revision',{property:()=>Promise.resolve({ok:true,applied:true})},n=>n.layer('A').set('opacity',5),'write-unconfirmed'],
    ['track revision moved though unapplied',{track:(_,h)=>{h.info={...h.info,contentRevision:h.info.contentRevision+1};return Promise.resolve({ok:true,applied:false});}},n=>n.layer('A').hold('opacity',2),'write-unconfirmed'],
    ['marker refused',{marker:()=>Promise.resolve({ok:false})},n=>n.markers.add(2),'write-refused'],
  ];
  for(const [label,hooks,script,error] of cases){
    const o=owner(L,hooks),run=o.run();script(run.nemo);run.nemo.layer('B').set('opacity',1);
    const queued=label==='add'?3:2; // addLayer('x') queues the add and its rename
    assert.deepEqual(await run.commit(),{ok:false,applied:0,pending:queued,error},label);assert.equal(o.calls.length,1,label+': the next step is never sent');
  }
});

test('an unchanged property or marker write is a no-op, not an applied write; a rename to the same name never leaves the script',async()=>{
  const o=owner(L,{property:()=>Promise.resolve({ok:true,applied:false}),marker:()=>Promise.resolve({ok:true,applied:false})});
  const run=o.run(),n=run.nemo;n.layer('A').set('opacity',100);n.markers.removeAt(40);n.layer('A').name='A';
  assert.deepEqual(await run.commit(),{ok:true,applied:0,pending:0});assert.deepEqual(o.calls.map(c=>c[0]),['property','marker']);
});

test('consecutive renames of one layer collapse into one native write',async()=>{
  const o=owner(L),run=o.run(),n=run.nemo,a=n.layer('A');a.name='One';a.name='Two';n.layer('B').name='B2';
  assert.deepEqual(await run.commit(),{ok:true,applied:2,pending:0});
  assert.deepEqual(o.calls,[['rename','a','Two'],['rename','b','B2']]);assert.deepEqual(o.h.info.layers.map(l=>l.name).slice(0,2),['Two','B2']);
});
