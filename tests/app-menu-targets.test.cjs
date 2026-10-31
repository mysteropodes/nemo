'use strict';
// U04 (#2370): the real app menu (timeline.js initAppMenu) against its declared desktop targets (app-menu-targets.js).
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Targets=require('../src/js/application/app-menu-targets.js');
const timeline=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');
function source(name){const start=timeline.indexOf('function '+name+'(');assert.ok(start>=0,name);let at=timeline.indexOf('{',start),depth=1,end=at+1;
  while(depth){if(timeline[end]==='{')depth++;if(timeline[end]==='}')depth--;end++;}return timeline.slice(start,end);}

// Builds the menu once with every target recorded; returns the items as shown and what each click reached.
function menu({desktop=true,targets=true,kitsu=false,scriptApi=false,documentOpen=true,frozen3d=true}={}){
  const clicked=[],calls=[],notices=[],menus=[],listeners={};
  const el=(id)=>({id,click(){clicked.push(id);},addEventListener(k,f){listeners[id+':'+k]=f;},getBoundingClientRect:()=>({left:0,bottom:0}),classList:{toggle(){}}});
  const call=(name)=>()=>calls.push(name);
  const win={
    SM:{t:(k)=>k},showContextMenu:(x,y,items)=>menus.push(items),showToast:(m)=>notices.push(m),
    SMProject:{getCurrentLabel:()=>'Untitled',open:call('SMProject.open'),save:call('SMProject.save'),saveAs:call('SMProject.saveAs')},
    SMScript:{openFile:call('SMScript.openFile')},SMPlugin:{openFile:call('SMPlugin.openFile')},SMSecondViewer:{open:call('SMSecondViewer.open')},
    SMSvgImport:{openFile:call('SMSvgImport.openFile')},SMRenderManager:{open:call('SMRenderManager.open')},
    SMKitsu:{getCurrentShot:()=>kitsu?{projectName:'P',shotName:'S'}:null},SM_FROZEN_IN_DEV:{import3d:frozen3d},
    NemoNativeObjectProject:{desktop:()=>({blocksLegacy:()=>documentOpen})}};
  if(desktop)win.__TAURI__={core:{}};
  if(targets)win.NemoAppMenuTargets=Targets;
  if(scriptApi)win.NemoScriptNativeApi={};
  win.window=win;
  win.document={readyState:'complete',getElementById:el,querySelector:()=>null,addEventListener(){}};
  const ctx=vm.createContext(win);
  vm.runInContext(source('clickEl')+source('openObjReference')+source('initAppMenu')+';initAppMenu();',ctx);
  listeners['app-menu-btn:click']({stopPropagation(){}});
  return {items:menus[0]||null,clicked,calls,notices};
}
const actions=(items)=>Array.from(items).filter((i)=>i&&!i.sep&&!i.disabled&&typeof i.action==='function');

test('every app-menu action has a stable id and a declared desktop target',()=>{
  const {items}=menu({kitsu:true,frozen3d:false});
  const ids=actions(items).map((i)=>i.id);
  assert.equal(new Set(ids).size,ids.length,'ids are unique');
  assert.ok(ids.every(Boolean),'every action has an id');
  assert.deepEqual(ids.filter((id)=>!Object.prototype.hasOwnProperty.call(Targets.targets,id)),[],'an unclassified action must be declared');
  assert.deepEqual(Object.keys(Targets.targets).filter((id)=>!ids.includes(id)),[],'no declared target is stale');
});

test('on desktop an unavailable action shows its reason and reaches nothing; native and presentation actions run',()=>{
  const m=menu({kitsu:true});
  const byId=Object.fromEntries(actions(m.items).map((i)=>[i.id,i]));
  for(const id of ['ctx-nemo-plugin','ctx-new-view','ctx-kitsu-publish','ctx-nemo-script']){
    byId[id].action();
  }
  assert.deepEqual(m.calls,[],'no legacy script, plugin or viewer entry runs');
  assert.deepEqual(m.clicked,[],'no Kitsu button is pressed');
  assert.deepEqual(m.notices,['Plugins are unavailable in native Nemo.','A second viewer is unavailable in native Nemo.',
    'Kitsu publishing is unavailable in native Nemo.','Scripts are unavailable in native Nemo.']);
  byId['ctx-open'].action();byId['ctx-save'].action();byId['ctx-new-project'].action();byId['ctx-export'].action();byId['ctx-import-img'].action();byId['ctx-kitsu-open'].action();
  assert.deepEqual(m.calls,['SMProject.open','SMProject.save']);
  assert.deepEqual(m.clicked,['project-tab-add','btn-export','btn-import-img','btn-kitsu-open'],'Kitsu open is a native route now');
});

test('conditional targets: scripts follow the native script API, media imports need an open native document',()=>{
  const withApi=menu({scriptApi:true}),s=actions(withApi.items).find((i)=>i.id==='ctx-nemo-script');
  s.action();assert.deepEqual(withApi.calls,['SMScript.openFile'],'the native script API owns the run');
  const none=menu({documentOpen:false}),byId=Object.fromEntries(actions(none.items).map((i)=>[i.id,i]));
  byId['ctx-import-img'].action();byId['ctx-import-video'].action();byId['ctx-import-psd'].action();
  assert.deepEqual(none.clicked,['btn-import-psd']);
  assert.deepEqual(none.notices,['Open or create a native project first.','Open or create a native project first.']);
});

test('without the target table the desktop menu refuses to open; the browser menu is unchanged',()=>{
  const missing=menu({targets:false});
  assert.equal(missing.items,null);assert.deepEqual(missing.notices,['The app menu is unavailable.']);
  const browser=menu({desktop:false,kitsu:true});
  const v=actions(browser.items).find((i)=>i.id==='ctx-new-view');v.action();
  assert.deepEqual(browser.calls,['SMSecondViewer.open'],'outside the desktop host every action runs as before');
  assert.deepEqual(browser.notices,[]);
});

test('an action missing from the table is unavailable on desktop, not run',()=>{
  const items=[{label:'x',id:'ctx-unknown',action(){throw new Error('ran');}},{sep:true},{label:'d',disabled:true}];
  const notices=[],g=Targets.guard({__TAURI__:{core:{}},showToast:(m)=>notices.push(m)},items);
  g[0].action();assert.deepEqual(notices,['This menu action is unavailable in native Nemo.']);
  assert.equal(g[1],items[1]);assert.equal(g[2],items[2]);
});
