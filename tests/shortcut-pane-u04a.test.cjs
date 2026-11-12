'use strict';
// U04a (#2370): the Settings > Shortcuts pane presenter (shortcut-pane.js) over the registry, against a fake DOM.
// The markup and the notices are baseline 1065e9d8's; the capture hygiene (one listener, modifiers ignored, window capture,
// closing the settings modal) and the live menu/status hints are what this change adds.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const Pane=require('../src/js/application/shortcut-pane.js'),Registry=require('../src/js/application/shortcut-registry.js');
const Rail=require('../src/js/application/tool-rail.js'),Prefs=require('../src/js/adapters/settings-preferences-ui.js');
const timeline=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');

const STR={shortcutSearchPlaceholder:'Search',shortcutNotRebindableTitle:'Fixed',shortcutClickThenPressTitle:'Click then press a key',
  shortcutKeyClashToast:'Key already used by « {label} »',shortcutNoMatch:'No match',shortcutCatTools:'Tools',shortcutCatNav:'Navigation',
  shortcutCatFrames:'Frames',shortcutCatLayers:'Layers',shortcutCatView:'View',shortcutCatEdit:'Edit',shortcutCmdInsertFrame:'Insert frame',
  shortcutCmdInsertKey:'Insert keyframe',shortcutCmdPrevKey:'Previous key'};
const tr=k=>STR[k]!==undefined?STR[k]:k;

function bus(){const m={};return {m,addEventListener(t,f,c){(m[t]=m[t]||[]).push({f,c});},removeEventListener(t,f,c){m[t]=(m[t]||[]).filter(x=>!(x.f===f&&x.c===c));},
  count(t){return (m[t]||[]).length;},
  // capture listeners first, like a window/document dispatch, stop honoured for the immediate case
  emit(t,e){let stopped=false;e.stopImmediatePropagation=()=>{stopped=true;e.immediate=true;};e.stopPropagation=()=>{e.stopped=true;};
    for(const x of (m[t]||[]).slice()){if(stopped)break;x.f(e);}return e;}};}
function el(tag){const e=Object.assign(bus(),{tag,children:[],style:{cssText:''},className:'',_text:'',value:'',title:'',
  classList:{set:new Set(),add(c){this.set.add(c);},remove(c){this.set.delete(c);},contains(c){return this.set.has(c);}}});
  Object.defineProperty(e,'textContent',{get(){return e._text;},set(v){e._text=String(v);}});
  Object.defineProperty(e,'innerHTML',{get(){return '';},set(v){e.children=[];}});
  e.appendChild=c=>{e.children.push(c);return c;};e.click=()=>{(e.m.click||[]).slice().forEach(x=>x.f({}));};return e;}
function walk(n,f){f(n);n.children.forEach(c=>walk(c,f));}
function rig(initial){
  const store={};if(initial)store['nemo-shortcuts']=JSON.stringify(initial);
  const storage={getItem:k=>(k in store?store[k]:null),setItem:(k,v)=>{store[k]=String(v);},removeItem:k=>{delete store[k];}};
  const win=bus(),doc=Object.assign(bus(),{createElement:el});
  const toasts=[],changed=[],resets=[];
  const registry=Registry.create({storage,onOverrideChanged:a=>changed.push(a),onReset:()=>resets.push(1)});
  const pane=Pane.create({doc,win,registry,translate:tr,notify:t=>toasts.push(t)});
  const list=el('div');pane.render(list);
  const key=ev=>Object.assign({key:'x',preventDefault(){this.prevented=true;}},ev);
  const button=label=>{let found=null;walk(list,n=>{if(n.tag==='div'&&n.children.length===2&&n.children[0]._text===label)found=n.children[1];});assert.ok(found,'row '+label);return found;};
  return {store,storage,win,doc,toasts,changed,resets,registry,pane,list,button,press:k=>win.emit('keydown',key(typeof k==='string'?{key:k}:k))};
}

test('remapping Insert Frame: the registry holds it, it is persisted, the row and the hint show it, and a fresh context reads it back',()=>{
  const r=rig(),b=r.button('Insert frame');
  assert.equal(b._text,'F5');assert.equal(r.registry.hintFor('cmdInsertFrame'),'F5');
  b.click();assert.equal(b._text,'…');assert.ok(b.classList.contains('ac'));assert.equal(r.win.count('keydown'),1,'one capture listener on the window');
  const e=r.press('y');assert.equal(e.prevented,true);assert.equal(e.immediate,true,'no other key handler also reacts');
  assert.equal(b._text,'y');assert.equal(b.classList.contains('ac'),false);assert.equal(r.win.count('keydown'),0);
  assert.equal(r.registry.keyFor('cmdInsertFrame'),'y');assert.equal(r.registry.hintFor('cmdInsertFrame'),'Y');
  assert.deepEqual(JSON.parse(r.store['nemo-shortcuts']),{cmdInsertFrame:'y'});assert.deepEqual(r.changed,['cmdInsertFrame']);
  const fresh=Registry.create({storage:r.storage});assert.equal(fresh.keyFor('cmdInsertFrame'),'y','survives a new preference context');
  assert.equal(fresh.keyFor('cmdInsertKey'),'F6','the others keep their defaults');
});

test('a key already held by another action is refused with the baseline notice; the row and storage keep what they had',()=>{
  const r=rig(),b=r.button('Insert keyframe');b.click();r.press('b');
  assert.deepEqual(r.toasts,['Key already used by « Draw »']);assert.equal(b._text,'F6');assert.equal(r.registry.keyFor('cmdInsertKey'),'F6');
  assert.equal(r.store['nemo-shortcuts'],undefined);assert.deepEqual(r.changed,[]);assert.equal(r.win.count('keydown'),0);
  const c=r.button('Insert frame');c.click();r.press('J');
  assert.equal(r.toasts[1],'Key already used by « Previous key »','a command holder is named through translate');
  assert.equal(r.registry.clashFor('cmdInsertFrame','B').action,'draw','case-insensitive clash');
});

test('keys hard-wired outside the tables (Space, arrows, Enter, Delete, Tab, F9) are refused and never stored (#2370 review)',()=>{
  for(const k of [' ','ArrowLeft','ArrowRight','Enter','Delete','Backspace','Tab','F9']){
    const r=rig(),b=r.button('Insert frame');b.click();r.press(k);
    assert.equal(r.registry.keyFor('cmdInsertFrame'),'F5',JSON.stringify(k)+' keeps the default');
    assert.equal(r.store['nemo-shortcuts'],undefined);assert.match(r.toasts[0]||'',/reserved/);
  }
  const r=rig();assert.equal(r.registry.setKey('cmdInsertFrame',' '),false,'setKey itself refuses a reserved key');assert.equal(r.registry.keyFor('cmdInsertFrame'),'F5');
});

test('Escape cancels; a modifier press alone is ignored and the capture keeps waiting',()=>{
  const r=rig(),b=r.button('Insert frame');b.click();
  for(const m of ['Shift','Control','Alt','Meta']){const e=r.press(m);assert.equal(e.prevented,undefined,m+' is not consumed');}
  assert.equal(r.pane.capturing(),true);assert.equal(b._text,'…');
  r.press('Escape');assert.equal(b._text,'F5');assert.equal(r.pane.capturing(),false);assert.equal(r.store['nemo-shortcuts'],undefined);
  b.click();r.press('F8');assert.equal(r.registry.keyFor('cmdInsertFrame'),'f8','a function key can be assigned once a real key is pressed');
});

test('arming a second row cancels the first: one key press assigns exactly one action',()=>{
  const r=rig(),a=r.button('Insert frame'),b=r.button('Insert keyframe');
  a.click();b.click();assert.equal(a._text,'F5','the first row is restored');assert.equal(r.win.count('keydown'),1);
  r.press('x');assert.equal(r.registry.keyFor('cmdInsertKey'),'x');assert.equal(r.registry.keyFor('cmdInsertFrame'),'F5');
  assert.deepEqual(r.changed,['cmdInsertKey']);
});

test('re-rendering, dispose and the settings-modal close paths drop the capture so the next key is not swallowed',()=>{
  let r=rig();r.button('Insert frame').click();r.pane.render(el('div'));assert.equal(r.win.count('keydown'),0);
  r=rig();r.button('Insert frame').click();r.pane.dispose();assert.equal(r.win.count('keydown'),0);assert.equal(r.press('q').prevented,undefined);
  // The modal's close button, backdrop click and tab switch cancel an armed capture before hiding.
  const ids={};['btn-settings','settings-modal','settings-close','settings-tabs','shortcuts-reset'].forEach(id=>{ids[id]=el('x');ids[id].style={};});
  const tab=el('tab');tab.dataset={tab:'t'};ids['settings-tabs'].querySelectorAll=()=>[tab];
  const sdoc={getElementById:id=>ids[id]||null,querySelectorAll:()=>[]};let cancels=0;
  Prefs.create({document:sdoc,readProfile:()=>null,renderShortcuts(){},resetShortcuts(){},showToast(){},translate:k=>k,renderLabs(){},refreshFeedback(){},cancelShortcutCapture(){cancels++;}}).initSettingsModal();
  const click=(n,e)=>(n.m.click||[]).slice().forEach(x=>x.f(e||{}));
  click(ids['settings-close']);assert.equal(cancels,1);assert.equal(ids['settings-modal'].style.display,'none');
  click(ids['settings-modal'],{target:ids['settings-modal']});assert.equal(cancels,2);
  click(ids['settings-modal'],{target:{}});assert.equal(cancels,2,'a click inside the modal body does not');
  click(tab);assert.equal(cancels,3);
});

test('reset restores every default, the registry fires onReset, and a re-render shows the default key',()=>{
  const r=rig({cmdInsertFrame:'y',draw:'u9'});
  assert.equal(r.button('Insert frame')._text,'y');assert.equal(r.registry.hintFor('cmdInsertFrame'),'Y');
  r.registry.reset();r.pane.render(r.list);
  assert.equal(r.resets.length,1);assert.equal(r.store['nemo-shortcuts'],undefined,'persisted overrides cleared');
  assert.equal(r.button('Insert frame')._text,'F5');assert.equal(r.registry.keyFor('draw'),'b');assert.equal(r.registry.hintFor('cmdInsertFrame'),'F5');
});

test('rail badge text follows a remap and a reset through the registry ports',()=>{
  const badges={};const doc={querySelector:s=>{const m=/data-tool="(\w+)"/.exec(s);return m&&badges[m[1]]?{querySelector:()=>badges[m[1]]}:null;}};
  badges.select={textContent:'V'};let rail=null;
  const r=rig();
  const registry=Registry.create({storage:r.storage,onOverrideChanged:a=>rail.sync(a),onReset:()=>rail.syncAll()});
  rail=Rail.createBadges({doc,registry});
  const pane=Pane.create({doc:r.doc,win:r.win,registry,translate:tr,notify(){}});const list=el('div');pane.render(list);
  let btn=null;walk(list,n=>{if(n.tag==='div'&&n.children.length===2&&n.children[0]._text==='Select')btn=n.children[1];});
  btn.click();r.win.emit('keydown',{key:'y',preventDefault(){}});assert.equal(badges.select.textContent,'Y');
  registry.reset();assert.equal(badges.select.textContent,'V');
});

test('the view model groups tools, commands and read-only keys by category and filters by label or key',()=>{
  const r=rig(),all=r.pane.model('');
  assert.deepEqual(all.map(g=>g.id),['tools','nav','frames','layers','view','edit']);
  const frames=all.find(g=>g.id==='frames').rows;assert.equal(frames.find(x=>x.action==='cmdInsertFrame').key,'F5');
  assert.equal(all.find(g=>g.id==='nav').rows.some(x=>x.rebindable===false),true,'read-only rows are listed');
  assert.deepEqual(r.pane.model('insert frame').flatMap(g=>g.rows.map(x=>x.action)),['cmdInsertFrame']);
  assert.deepEqual(r.pane.model('f6').flatMap(g=>g.rows.map(x=>x.action)),['cmdInsertKey']);
  assert.deepEqual(r.pane.model('zzzz'),[]);
});

test('the pane markup is the baseline markup: search box, category headers, label span, dashed read-only key, key button',()=>{
  const r=rig();let search=null,hdr=null,ro=null,btn=null;
  walk(r.list,n=>{if(n.type==='search')search=n;if(n.className==='phdr'&&!hdr)hdr=n;if(n.style.cssText.indexOf('dashed')>=0&&!ro)ro=n;if(n.className==='pbtn'&&!btn)btn=n;});
  assert.equal(search.className,'pi');assert.equal(search.placeholder,'Search');assert.equal(hdr._text,'Tools');
  assert.match(ro.style.cssText,/border:1px dashed var\(--border\)/);assert.equal(ro.title,'Fixed');
  assert.equal(btn.style.cssText,'min-width:60px;font-family:monospace;text-transform:uppercase;');assert.equal(btn.title,'Click then press a key');
});

test('displayKey names stored keys for menus and the status bar',()=>{
  assert.deepEqual(['f5','j','home',' ','','arrowleft'].map(Registry.displayKey),['F5','J','Home','Space','','←']);
});

test('timeline.js builds the pane through the presenter and shows the current binding in the frame menu and status bar',()=>{
  assert.match(timeline,/NemoShortcutPane\.create\(\{doc:document,win:window,registry:shortcutRegistry/);
  assert.equal(/function keyRow\(|keyBtn\.addEventListener/.test(timeline),false,'no pane DOM left in the monolith');
  for(const [label,action] of [['ctxInsertFrame','cmdInsertFrame'],['ctxInsertKeyframe','cmdInsertKey'],['ctxInsertBlankKeyframe','cmdInsertBlankKey']])
    assert.ok(timeline.includes("{label:SM.t('"+label+"'),shortcut:shortcutHint('"+action+"')"),label+' hint follows the registry');
  assert.match(timeline,/rb\('cmdInsertFrame','F5'\)/);
  assert.match(timeline,/onOverrideChanged:function\(action\)\{syncToolButtonShortcutBadge\(action\);refreshShortcutHints\(\);\}/);
  assert.match(timeline,/onReset:function\(\)\{syncAllToolButtonShortcutBadges\(\);refreshShortcutHints\(\);\}/);
});
