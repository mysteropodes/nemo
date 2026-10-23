'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),vm=require('node:vm'),fs=require('node:fs');
function element(){const events=new Map();return {disabled:false,children:[],dataset:{},title:'',attributes:{},
 style:{setProperty(k,v){this[k]=v;}},classList:{toggle(){}},setAttribute(k,v){this.attributes[k]=v;},
 set textContent(value){this.text=value;this.children=[];},get textContent(){return this.text||'';},
 set innerHTML(value){this.children=[];},appendChild(child){this.children.push(child);},
 addEventListener(k,f){events.set(k,f);},fire(k,extra={}){const fn=events.get(k);if(fn)fn({preventDefault(){},clientX:0,clientY:0,shiftKey:false,dataTransfer:{},...extra});}};}
function fixture(native){
 const elements=Object.fromEntries(['btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke','btn-palette-replace','btn-palette-swap'].map(id=>[id,element()]));
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
 for(const id of ['btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke','btn-palette-replace','btn-palette-swap']){
  assert.equal(h.elements[id].disabled,true);h.elements[id].fire('click');}
 h.root.renderPaletteGrid();h.root.setPaletteHighlight('#AbC');assert.equal(h.notices.length,5);
});
test('previously retained palette callbacks stay closed when native platform admission appears later',()=>{
 const h=fixture(false);h.root.__TAURI__={core:{}};
 for(const id of ['btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke','btn-palette-replace','btn-palette-swap'])h.elements[id].fire('click');
 h.root.renderPaletteGrid();assert.equal(h.notices.length,5);
});

test('rendered swatches, tabs, drag and already-open menu actions cannot reach legacy state after native admission',()=>{
 const elements=Object.fromEntries(['palette-grid','palette-tabs','btn-palette-new','btn-palette-add-fill','btn-palette-add-stroke','btn-palette-replace','btn-palette-swap'].map(id=>[id,element()]));
 const original=[{id:'a',name:'First',colors:['#AbC','#abc']},{id:'b',name:'Second',colors:[]}],state={fillEnabled:true};
 let native=false;Object.defineProperty(state,'palettes',{get(){assert.equal(native,false,'late callback read legacy palettes');return original;},set(){assert.fail('legacy palette write');}});
 Object.defineProperty(state,'activePaletteIdx',{get(){assert.equal(native,false,'late callback read legacy selection');return 0;},set(){assert.fail('legacy palette selection write');}});
 const menus=[],calls=[],root={state,document:{readyState:'complete',getElementById:id=>elements[id]||null,createElement:element,addEventListener(){}},
  SM:{t:x=>x,setFillColor:x=>calls.push(['fill',x]),setStrokeColor:x=>calls.push(['stroke',x]),setFillEnabled:x=>calls.push(['fillEnabled',x])},
  showToast(){},showContextMenu(x,y,items){menus.push(items);},prompt(){assert.fail('late legacy prompt');},pushUndo(){assert.fail('legacy history');}};
 root.window=root;vm.runInNewContext(fs.readFileSync(require.resolve('../src/js/palette-panel.js'),'utf8'),root);
 const swatches=elements['palette-grid'].children.slice(),tabs=elements['palette-tabs'].children.slice();assert.equal(swatches.length,2);assert.equal(tabs.length,2);
 swatches[0].fire('contextmenu');tabs[0].fire('contextmenu');assert.equal(menus.length,2);
 native=true;root.__TAURI__={core:{}};root.renderPaletteGrid();
 for(const swatch of swatches){swatch.fire('click');swatch.fire('click',{shiftKey:true});swatch.fire('contextmenu');swatch.fire('dragstart');swatch.fire('drop');}
 for(const tab of tabs){tab.fire('click');tab.fire('dblclick');tab.fire('contextmenu');}
 for(const menu of menus)for(const item of menu)if(item.action)item.action();
 elements['btn-palette-swap'].fire('click');assert.equal(elements['btn-palette-swap'].disabled,true);
 assert.equal(menus.length,2);assert.deepEqual(calls,[]);assert.deepEqual(original,[{id:'a',name:'First',colors:['#AbC','#abc']},{id:'b',name:'Second',colors:[]}]);
});
