'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const Reference=require('../src/js/adapters/native-object-reference.js');
function queued(){
  let current=null,tail=Promise.resolve(),serial=0;const actions=[];
  return {actions,requestId:()=> 'ref-'+(++serial),info:()=>({reference:current}),
    reference(make){const next=tail.then(()=>{const action=typeof make==='function'?make(current):make;actions.push(action);
      if(action.kind==='attach')current={id:action.id,name:'image.png',visible:true,opacity:0.5,offsetFrames:0};
      if(action.kind==='settings')current={...current,...action};if(action.kind==='remove')current=null;return action;
    });tail=next.catch(()=>{});return next;}};
}
test('replacement keeps identity and sends only chosen PNG path; picker cancel/unsupported file have no native effects',async()=>{
  const s=queued();let path='/reference.png';const ui=Reference.create(s,{pick:async()=>path});
  await ui.importPath();const first=ui.read().id;path='/replace.PNG';await ui.importPath();assert.equal(ui.read().id,first);
  assert.deepEqual(s.actions[1],{kind:'attach',id:first,sourcePath:'/replace.PNG'});
  path=null;assert.equal(await ui.importPath(),false);path='/clip.mp4';await assert.rejects(ui.importPath(),/one PNG/);
  assert.equal(s.actions.length,2);
});
test('queued controls merge against native metadata at execution, preserving preceding changes',async()=>{
  const s=queued(),ui=Reference.create(s,{pick:async()=>'/image.png'});await ui.importPath();
  await Promise.all([ui.settings({opacity:0.25}),ui.settings({visible:false}),ui.settings({offsetFrames:8})]);
  assert.equal(ui.read().opacity,0.25);assert.equal(ui.read().visible,false);assert.equal(ui.read().offsetFrames,8);
  assert.equal(s.actions[3].opacity,0.25);assert.equal(s.actions[3].visible,false);
  await ui.remove();assert.equal(ui.read(),null);await assert.rejects(ui.settings({opacity:1}),/No reference/);
});
test('production reference DOM handlers route import/settings/removal and disable legacy reference media/upload',async()=>{
  const s=queued(),ui=Reference.create(s,{pick:async()=>'/image.png'}),elements=new Map(),notices=[];
  function el(){return {style:{},value:'',checked:false,addEventListener(k,f){this[k]=f;}};}
  const document={readyState:'complete',getElementById(id){if(!elements.has(id))elements.set(id,el());return elements.get(id);}};
  const state={refMedia:{type:'image',src:'old-legacy-bytes'},currentFrame:0},root={NemoNativeObjectReference:{desktop:()=>ui},__TAURI__:{},SM:{t:k=>k}};
  const context={window:root,document,state,SM:root.SM,showToast:m=>notices.push(m)};
  vm.runInNewContext(fs.readFileSync('src/js/reference-bridge.js','utf8'),context);
  elements.get('btn-ref-import').click();await new Promise(setImmediate);assert.equal(s.actions[0].kind,'attach');
  const opacity=elements.get('p-ref-opacity');opacity.value='30';opacity.input();
  const visible=elements.get('p-ref-on');visible.checked=false;visible.change();await new Promise(setImmediate);
  assert.equal(ui.read().opacity,0.3);assert.equal(ui.read().visible,false);
  assert.equal(root.SMReference.buildRefItem(()=>assert.fail('legacy upload')),null);
  root.SMReference.onFrameChanged(0,true);assert.equal(state.refMedia.src,'old-legacy-bytes');
  root.SMReference.importFiles([{name:'image.png'}]);assert.match(notices.at(-1),/Import Reference/);
  elements.get('btn-ref-remove').click();await new Promise(setImmediate);assert.equal(ui.read(),null);
});
