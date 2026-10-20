'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
function element(){const events=new Map();return {disabled:false,textContent:'',attributes:{},setAttribute(k,v){this.attributes[k]=v;},
 addEventListener(k,f){events.set(k,f);},fire(k){const fn=events.get(k);if(fn)fn({preventDefault(){}});}};}
function fixture(native){
 const elements=Object.fromEntries(['btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke','btn-palette-replace'].map(id=>[id,element()]));
 if(native){elements['palette-grid']=element();elements['palette-tabs']=element();}
 const state={};for(const name of ['palettes','activePaletteIdx','layers'])Object.defineProperty(state,name,{get(){throw Error('legacy metadata read: '+name);},set(){throw Error('legacy metadata write: '+name);}});
 const notices=[],root={state,document:{readyState:'complete',getElementById:id=>elements[id]||null,addEventListener(){}},
 showToast:s=>notices.push(s),prompt(){throw Error('unavailable native palette must not open a legacy prompt');},
 pushUndo(){throw Error('legacy history reached');},saveAllLayerFrames(){throw Error('legacy frames reached');}};
 if(native)root.__TAURI__={core:{}};root.window=root;vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/palette-panel.js'),'utf8'),root);
 return {root,elements,notices};
}
test('unadmitted native palette controls expose unavailable without creating JS metadata or invoking legacy writers',()=>{
 const h=fixture(true);assert.equal(h.elements['palette-grid'].textContent,'Palette editing is unavailable.');
 assert.equal(h.elements['palette-grid'].attributes.role,'status');
 for(const id of ['btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke','btn-palette-replace']){
  assert.equal(h.elements[id].disabled,true);h.elements[id].fire('click');}
 h.root.renderPaletteGrid();h.root.setPaletteHighlight('#AbC');assert.equal(h.notices.length,4);
});
test('previously retained palette callbacks stay closed when native platform admission appears later',()=>{
 const h=fixture(false);h.root.__TAURI__={core:{}};
 for(const id of ['btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke','btn-palette-replace'])h.elements[id].fire('click');
 h.root.renderPaletteGrid();assert.equal(h.notices.length,4);
});
