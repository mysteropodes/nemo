'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const {fixture,copy} = require('./helpers/native-custom-brush-fixture.cjs');
const raw = 'raw\0"雪';
function edit(f,id,onApply){f.root.BrushEditor.open(id,onApply);const el=f.popover('editor');assert(el);return el;}
function save(el,label){el.querySelector('#bpe-name').value=label;el.querySelector('#bpe-save').fire('click');}
function picker(f){f.ids['p-brushpreset-btn'].fire('click');return f.popover('picker');}
function customButton(el,id){return el.querySelectorAll('.bp-item-custom').find(row=>row.dataset.key===id);}
test('actual editor draft is transient until one native save; confirmed diameter and raw identity survive',async()=>{
  const f=await fixture([['10',{label:'Ten',savedBrushSize:19}],[raw,{label:'Imported',nibSize:2,customStamp:{segments:[]}}],['2',{}]]);
  assert.deepEqual(copy(f.root.BrushPresetPicker.customKeys()),['10',raw,'2']);
  let applied=0;const el=edit(f,raw,id=>{applied++;f.root.BrushPresetPicker.selectPreset(id);});
  const slider=el.querySelector('.bpe-slider[data-key="nibSize"]');slider.value='3';slider.fire('input');
  assert.equal(f.catalog()[1][1].nibSize,2);assert.equal(f.edits().length,0);
  const release=f.hold();save(el,'Updated');save(el,'Ignored repeated click');
  assert.equal(f.root.state.brushPreset,'none');release();await f.settled();
  assert.equal(f.edits().length,1);assert.equal(f.checkpoints(),1);assert.equal(applied,1);
  assert.equal(f.edits()[0].payload.action.id,raw);assert.equal(f.catalog()[1][1].nibSize,3);
  assert.equal(f.root.state.brushPreset,raw);assert.equal(f.root.state.brushSize,12);assert.equal(f.popover('editor'),null);
  assert.deepEqual(copy(f.root.BrushPresetPicker.customKeys()),['10',raw,'2']);
});
test('native save, update and missing delete use existing history; undo/redo and reopen publish only native records',async()=>{
  const f=await fixture(undefined);assert.equal(f.controller.read().catalog.settings.customBrushPresetsPresent,false);
  const initial=f.controller.read();let result=await f.controller.edit(f.action({kind:'delete',id:'missing'}),initial);await f.settled();
  assert.equal(result.status,'unchanged');assert.equal(f.catalog(),undefined);assert.equal(f.checkpoints(),0);
  let el=edit(f,null,id=>f.root.BrushPresetPicker.selectPreset(id));save(el,'Fresh');await f.settled();
  const id=f.catalog()[0][0];el=edit(f,id);save(el,'Changed');await f.settled();assert.equal(f.checkpoints(),2);
  f.history(-1);await f.settled();assert.equal(f.root.NemoNativeCustomBrushUI.record(f.root,id).label,'Fresh');
  result=await f.controller.edit(f.action({kind:'delete',id:'missing'}),f.controller.read());await f.settled();assert.equal(result.status,'unchanged');
  f.history(1);await f.settled();assert.equal(f.root.NemoNativeCustomBrushUI.record(f.root,id).label,'Changed');
  const reopened=await fixture(f.catalog());assert.deepEqual(reopened.catalog(),f.catalog());assert.equal(reopened.checkpoints(),0);
});
test('both original delete controls wait for confirmed same-view native reconciliation',async()=>{
  for(const route of ['picker','menu']){
    const f=await fixture([[raw,{label:'Raw',savedBrushSize:27}],['2',{}]]);f.root.BrushPresetPicker.selectPreset(raw);
    let el;if(route==='picker')el=picker(f);else{f.root.BrushMenu.toggle(f.ids['p-brushpreset-btn']);el=f.popover('menu');}
    const button=route==='picker'?customButton(el,raw):el.querySelectorAll('.bp-item-custom')[0];assert(button);
    const del=button.querySelector('.bp-item-del'),release=f.hold();del.fire('click');del.fire('click');
    assert.equal(f.root.state.brushPreset,raw);assert.equal(f.catalog().length,2);release();await f.settled();
    assert.equal(f.edits().length,1,route);assert.equal(f.catalog().length,1);assert.equal(f.checkpoints(),1);
    assert.equal(f.root.state.brushPreset,'none');assert.equal(f.root.state.brushSize,27);
  }
});
test('held save completion after cancel, reopen, replacement or retirement never applies stale UI callback',async()=>{
  for(const end of ['cancel','reopen','replace','retire']){
    const f=await fixture([['custom',{label:'Before'}]]);let applied=0;const el=edit(f,'custom',()=>applied++),release=f.hold();save(el,'After');
    await new Promise(resolve=>setImmediate(resolve));
    if(end==='cancel')el.querySelector('#bpe-cancel').fire('click');
    if(end==='reopen'){f.root.BrushEditor.close();f.root.BrushEditor.open(null,()=>applied++);}
    if(end==='replace')f.replace([['other',{label:'Replacement'}]]);
    if(end==='retire')f.retire();
    release();await f.settled();assert.equal(applied,0,end);assert.equal(f.root.state.brushPreset,'none');assert.equal(f.edits().length,1);
    if(end==='replace')assert.equal(f.catalog()[0][0],'other');
  }
});
test('rejection, stale ticket, unavailable record and pinned read failure have honest controls without legacy fallback',async()=>{
  const f=await fixture([['custom',{label:'Before'}]]),el=edit(f,'custom');f.refuse();save(el,'Rejected');await f.settled();
  assert.equal(f.catalog()[0][1].label,'Before');assert.equal(f.checkpoints(),0);assert.equal(f.root.state.brushPreset,'none');assert(f.notices.length);
  const g=await fixture([['custom',{label:'Before'}]]),stale=edit(g,'custom');g.resubscribe();save(stale,'Stale');await g.settled();assert.equal(g.edits().length,0);
  const u=await fixture([['huge',{label:'Visible identity'}]],{unavailableIds:['huge'],favorites:['huge']});
  assert.deepEqual(copy(u.root.BrushPresetPicker.customKeys()),['huge']);assert.equal(u.root.resolveBrushPreset('huge'),null);
  assert.equal(u.root.BrushPresetPicker.selectPreset('huge'),false);assert.equal(u.root.state.brushPreset,'none');
  u.root.BrushEditor.open('huge');assert.equal(u.popover('editor'),null);u.root.BrushPresetPicker.refreshFavorites();
  assert.deepEqual(JSON.parse(u.storage.get('sm-brush-favorites')),['huge']);assert.equal(u.ids['p-brushfav-row'].querySelector('canvas').previewText,'Unavailable');
  const b=await fixture([['custom',{}]],{favorites:['custom']});b.staleRead();b.controller.willProject();await b.controller.presented();
  assert.equal(b.controller.read(),null);assert.equal(b.root.BrushPresetPicker.selectPreset('custom'),false);
  b.root.BrushPresetPicker.refreshFavorites();assert.deepEqual(JSON.parse(b.storage.get('sm-brush-favorites')),['custom']);
});
test('actual hover/menu/apply callbacks refuse before every legacy live selection/history/texture mutation',async()=>{
  const f=await fixture([['custom',{label:'Custom',savedBrushSize:25}]]);let el=picker(f),button=customButton(el,'custom');
  button.fire('mouseenter');button.fire('mouseleave');button.fire('click');assert.equal(f.root.state.brushPreset,'custom');assert.equal(f.root.state.brushSize,25);
  f.root.BrushPresetPicker.close();f.root.BrushMenu.toggle(f.ids['p-brushpreset-btn']);el=f.popover('menu');button=el.querySelectorAll('.bp-item-custom')[0];
  button.fire('mouseenter');button.fire('mouseleave');button.fire('click');assert.equal(f.checkpoints(),0);assert.equal(f.edits().length,0);
  const path=new Proxy({},{get(){throw Error('path read before refusal');},set(){throw Error('path write before refusal');}});
  assert.throws(()=>f.root.applyBrushTexture(path,'custom'),error=>error.code==='unavailable');assert.equal(f.root.applyVectorBrushToSelection('custom'),false);
  const editor=edit(f,'custom');editor.querySelector('#bpe-capture-btn').fire('click');assert.equal(f.edits().length,0);
  assert(f.notices.some(text=>text.includes('source/provider')));
});
test('native built-in-first lookup, local favorites and personal bundles remain separate from catalog writes',async()=>{
  const f=await fixture([['chalk-blunt',{label:'Collision',nibSize:99}],['2',{label:'Two'}]]);
  assert.equal(f.root.resolveBrushPreset('chalk-blunt').nibSize,1.3);
  f.root.BrushPresetPicker.toggleFavorite('2');assert.deepEqual(JSON.parse(f.storage.get('sm-brush-favorites')),['2']);
  const bundles=JSON.parse(f.storage.get('nemo-brush-tool-presets'));assert.equal(bundles.personal.fields.brushSize,8);
  f.ids['btn-toolpreset-save'].fire('click');const saved=JSON.parse(f.storage.get('nemo-brush-tool-presets'));
  assert.equal(Object.keys(saved).length,2);assert.equal(f.edits().length,0);assert.equal(f.catalog().length,2);
  f.ids['toolpreset-grid'].querySelectorAll('.bp-item')[0].fire('click');assert.equal(f.root.state.brushSize,8);assert.equal(f.edits().length,0);
  f.root.BrushEditor.open('chalk-blunt');assert.equal(f.popover('editor').querySelector('.bpe-slider[data-key="nibSize"]').value,1.3);
  f.root.BrushEditor.close();const listenerCount=f.doc.count();edit(f,'2');assert.equal(f.doc.count(),listenerCount+1);
  assert.equal(f.events.count(),1);f.controller.dispose();assert.equal(f.events.count(),0);assert.equal(f.doc.count(),listenerCount);assert.equal(f.popover('editor'),null);
  assert.equal(f.root.BrushPresetPicker.selectPreset('2'),false);assert.equal(f.root.BrushPresetPicker.selectPreset('chalk-blunt'),true);
});
test('empty imported identity remains editable and deletable without becoming the none drawing sentinel',async()=>{
  const f=await fixture([['',{label:'Empty ID',nibSize:2}]]),el=edit(f,'');assert.equal(el.querySelector('.bpe-slider[data-key="nibSize"]').value,2);
  save(el,'Updated empty ID');await f.settled();assert.equal(f.catalog()[0][0],'');assert.equal(f.root.BrushPresetPicker.selectPreset(''),false);
  const pop=picker(f);customButton(pop,'').querySelector('.bp-item-del').fire('click');await f.settled();assert.deepEqual(f.catalog(),[]);
});
test('browser editor and original deletion retain the ordinary legacy dictionary path',async()=>{
  const f=await fixture([['custom',{label:'Browser'}]],{browser:true});assert.equal(f.root.resolveBrushPreset('custom').label,'Browser');
  const el=edit(f,'custom');save(el,'Browser changed');assert.equal(f.root.state.customBrushPresets.custom.label,'Browser changed');
  const pop=picker(f);customButton(pop,'custom').querySelector('.bp-item-del').fire('click');assert.equal(f.root.state.customBrushPresets.custom,undefined);
  assert.equal(f.edits().length,0);
});

test('pages cross the 16-item boundary without Object.keys numeric reordering or identity normalization',async()=>{
  const ids=['10','2','01','','__proto__',raw,...Array.from({length:13},(_,i)=>'custom-'+i)];
  const f=await fixture(ids.map((id,i)=>[id,{label:String(i)}]));
  assert.deepEqual(copy(f.root.BrushPresetPicker.customKeys()),ids);
  const pages=f.calls.filter(q=>q.payload.selector?.kind==='catalogPage');assert.deepEqual(pages.map(q=>q.payload.selector.cursor),[0,16]);
  assert(Object.isFrozen(f.controller.read().catalog.items));assert(Object.isFrozen(f.controller.read().catalog.records[0].preset));
  const el=picker(f);assert.deepEqual(el.querySelectorAll('.bp-item-custom').map(row=>row.dataset.key),ids);
});
test('a later transient preset choice revokes a pending editor use callback without replaying its native save',async()=>{
  const f=await fixture([['custom',{label:'Before'}]]);let applied=0;const el=edit(f,'custom',()=>applied++),release=f.hold();save(el,'Saved');
  f.root.BrushPresetPicker.selectPreset('chalk-blunt');release();await f.settled();assert.equal(f.edits().length,1);
  assert.equal(f.catalog()[0][1].label,'Saved');assert.equal(applied,0);assert.equal(f.root.state.brushPreset,'chalk-blunt');
});
test('busy refusal and resource admission stale identity never become an optimistic delete',async()=>{
  const f=await fixture([['custom',{}]]);const el=picker(f);f.refuse('busy_conflict');
  customButton(el,'custom').querySelector('.bp-item-del').fire('click');await f.settled();assert.equal(f.catalog().length,1);assert.equal(f.checkpoints(),0);
  const g=await fixture([['custom',{}]]);const old=picker(g),del=customButton(old,'custom').querySelector('.bp-item-del');
  g.replace([['other',{}]]);del.fire('click');await g.settled();assert.equal(g.edits().length,0);assert.equal(g.catalog()[0][0],'other');
});

test('real SDK refuses an oversized explicit save before native dispatch and keeps a supplied empty catalog distinct',async()=>{
  const f=await fixture([]);assert.equal(f.controller.read().catalog.settings.customBrushPresetsPresent,true);
  const result=await f.controller.edit(f.action({kind:'upsert',id:'huge',preset:{label:'x'.repeat(4096)}}),f.controller.read());
  await f.settled();assert.equal(result.status,'unconfirmed');assert.equal(f.edits().length,0);assert.deepEqual(f.catalog(),[]);assert.equal(f.checkpoints(),0);
});
test('equal native upsert after undo retains redo rather than adding another checkpoint',async()=>{
  const f=await fixture([['custom',{label:'Before'}]]);
  await f.controller.edit(f.action({kind:'upsert',id:'custom',preset:{label:'After'}}),f.controller.read());await f.settled();
  f.history(-1);await f.settled();const result=await f.controller.edit(f.action({kind:'upsert',id:'custom',preset:{label:'Before'}}),f.controller.read());await f.settled();
  assert.equal(result.status,'unchanged');assert.equal(f.checkpoints(),1);f.history(1);await f.settled();assert.equal(f.catalog()[0][1].label,'After');
});

test('the actual ordinary native freehand provider commits a confirmed custom catalog preset and refuses a key the catalog lacks',async()=>{
  // G16 option C (#2361): native stamps a catalog preset's dabs from the project catalog, as resolveBrushPreset reads it.
  const f=await fixture([['custom',{label:'Stored'}]]);f.root.BrushPresetPicker.selectPreset('custom');
  Object.assign(f.root.state,{tool:'draw',drawMode:'front',vectorBrush:false,strokeEnabled:true,fillEnabled:false,strokeColor:'#112233',
    brushSize:4,opacity:100,smoothing:0,stabilizer:0,strokeCap:'round',strokeJoin:'round',strokeStyle:'solid',brushAngle:0,brushAngleFactor:0});
  const Shape=require('../src/js/adapters/native-object-shape-tool.js');
  const ports=Shape.freehandPorts(f.root,{});
  assert.equal(ports.style().brushPreset,'custom');
  f.root.state.brushPreset='missing';
  assert.throws(()=>ports.style(),/This brush combination is unavailable in native Drawing/);
  assert.equal(f.edits().length,0);assert.equal(f.checkpoints(),0);
});
test('canceling an unsaved draft never materializes absent catalog metadata',async()=>{
  const f=await fixture(undefined),el=edit(f,null);el.querySelector('.bpe-slider[data-key="grain"]').value='1.7';
  el.querySelector('.bpe-slider[data-key="grain"]').fire('input');el.querySelector('#bpe-cancel').fire('click');await f.settled();
  assert.equal(f.catalog(),undefined);assert.equal(f.edits().length,0);assert.equal(f.checkpoints(),0);
});

test('custom-brush uses full project readiness and retains narrow immutable authority pins', async () => {
  const f = await fixture();
  assert.equal(f.projectionReady(f.fullInfo()), true);
  const origin = f.controller.read(); assert.ok(origin);
  assert.deepEqual(Object.keys(origin.info).sort(),
    ['contentRevision', 'documentId', 'documentSnapshotId', 'instanceId', 'lifecycleGeneration']);
  assert.equal(Object.isFrozen(origin.info), true);
  assert.equal(f.projectionReady(origin.info), false, 'identity pins cannot stand in for project metadata');
  const calls = f.calls.length;
  f.drift(); assert.equal(f.controller.read(), null);
  assert.equal(await f.controller.presented(), null); await f.settled();
  assert.equal(f.controller.read(), null);
  assert.equal(f.calls.length, calls, 'invalid full projection refuses before dispatch');
  assert.equal(f.edits().length, 0);
  f.repairProjection(); await f.controller.presented(); await f.settled();
  assert.ok(f.controller.read());
  assert.equal(f.edits().length, 0, 'restoring readiness does not replay a gesture');
});
