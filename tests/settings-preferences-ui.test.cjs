'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const path=require('node:path'),ROOT=path.resolve(__dirname,'..');
const UI=require('../src/js/adapters/settings-preferences-ui.js');
// Frozen ordinary controls at protected8fe320f8; behavior oracle, not a native-success fixture.
const BASELINE_FRAGMENT=String.raw`function initSettingsTabs(){
  var bar=document.getElementById('settings-tabs');if(!bar)return;
  bar.querySelectorAll('.settings-tab').forEach(function(tab){
    tab.addEventListener('click',function(){
      bar.querySelectorAll('.settings-tab').forEach(function(t){t.classList.remove('active');});
      tab.classList.add('active');
      document.querySelectorAll('#settings-modal .settings-pane').forEach(function(p){
        p.style.display=(p.dataset.pane===tab.dataset.tab)?'':'none';
      });
    });
  });
}
function initSettingsModal(){
  var btn=document.getElementById('btn-settings'),modal=document.getElementById('settings-modal');
  if(!btn||!modal)return;
  initSettingsTabs();
  btn.addEventListener('click',function(){renderShortcutsList();syncProfileFields();syncFolderFields();refreshFeedbackList();if(window.renderLabsPanel)renderLabsPanel();modal.style.display='flex';});
  var topBtn=document.getElementById('project-tabs-settings');
  if(topBtn)topBtn.addEventListener('click',function(){btn.click();});
  var closeBtn=document.getElementById('settings-close');
  if(closeBtn)closeBtn.addEventListener('click',function(){modal.style.display='none';});
  modal.addEventListener('click',function(e){if(e.target===modal)modal.style.display='none';});
  var resetBtn=document.getElementById('shortcuts-reset');
  if(resetBtn)resetBtn.addEventListener('click',function(){
    shortcutRegistry.reset();
    renderShortcutsList();showToast(SM.t('toastShortcutsReset'));
  });
}
var ROLE_HINTS={
  animator:'Édite ses propres traits. Éditer le trait d\'un autre profil crée une correction à Accepter/Rejeter.',
  supervisor:'Édite n\'importe quel calque directement, sans créer de correction — autorité de révision.',
  producer:'Lecture seule + commentaires. Les outils de dessin/édition sont désactivés.'
};
function syncProfileFields(){
  var nameEl=document.getElementById('profile-name'),colorEl=document.getElementById('profile-color'),wellEl=document.getElementById('profile-color-well'),roleEl=document.getElementById('profile-role'),hintEl=document.getElementById('profile-role-hint'),emailEl=document.getElementById('profile-email');
  if(!nameEl||!state.userProfile)return;
  nameEl.value=state.userProfile.name;
  if(emailEl)emailEl.value=state.userProfile.email||'';
  var c=state.userProfile.color;
  setHex8Input(colorEl,c);
  if(wellEl)wellEl.style.background=c;
  var role=state.userProfile.role||'animator';
  if(roleEl)roleEl.value=role;
  if(hintEl)hintEl.textContent=ROLE_HINTS[role]||'';
}
function initProfileFields(){
  var nameEl=document.getElementById('profile-name'),colorEl=document.getElementById('profile-color'),roleEl=document.getElementById('profile-role'),emailEl=document.getElementById('profile-email');
  if(!nameEl||!colorEl)return;
  syncProfileFields();
  nameEl.addEventListener('input',function(){
    state.userProfile.name=this.value||'Animateur';
    saveUserProfile();
  });
  if(emailEl)emailEl.addEventListener('input',function(){
    state.userProfile.email=this.value||'';
    saveUserProfile();
  });
  colorEl.addEventListener('input',function(){
    var v=this.dataset.hex8||this.value;
    state.userProfile.color=v;
    document.getElementById('profile-color-well').style.background=v;
    saveUserProfile();
  });
  if(roleEl)roleEl.addEventListener('change',function(){
    state.userProfile.role=this.value;
    saveUserProfile();
    var hintEl=document.getElementById('profile-role-hint');
    if(hintEl)hintEl.textContent=ROLE_HINTS[this.value]||'';
    // Switching TO producer while mid-edit with a non-view tool active would
    // leave a blocked tool selected — force back to a safe one immediately,
    // same as the boot-time role check.
    if(this.value==='producer'&&PRODUCER_ALLOWED_TOOLS.indexOf(state.tool)<0)window.SM.setTool('hand');
  });
  if(window.ColorPicker)window.ColorPicker.wireColorSwatches([{wrap:'profile-color-well',input:'profile-color'}]);
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initProfileFields);else initProfileFields();
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initSettingsModal);else initSettingsModal();

function syncRelTime(ts){
  var s=Math.max(0,Math.round((Date.now()-ts)/1000));
  if(s<60)return 'il y a '+s+'s';
  var m=Math.round(s/60);
  if(m<60)return 'il y a '+m+' min';
  var h=Math.round(m/60);
  return 'il y a '+h+'h'+(m%60?Math.round(m%60)+'min':'');
}
function syncFolderFields(){
  var pathEl=document.getElementById('sync-folder-path');
  if(!pathEl)return;
  pathEl.value=(window.SMProject&&window.SMProject.getSyncFolder())||'';
  var listEl=document.getElementById('sync-updates-list');
  if(listEl)listEl.innerHTML='';
}
function renderSyncUpdates(entries){
  var listEl=document.getElementById('sync-updates-list');if(!listEl)return;
  listEl.innerHTML='';
  if(!entries||!entries.length){
    listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Aucune mise à jour d\'un autre profil.</div>';
    return;
  }
  entries.forEach(function(entry){
    var row=document.createElement('div');
    row.style.cssText='display:flex;align-items:center;justify-content:space-between;gap:6px;padding:5px 6px;border-radius:4px;background:var(--panel3);font-size:10px';
    var label=document.createElement('span');
    var dot=document.createElement('span');dot.style.cssText='display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:5px;background:'+(entry.profileColor||'#888');
    label.appendChild(dot);
    label.appendChild(document.createTextNode(entry.profileName+' — '+syncRelTime(entry.ts)));
    var btn=document.createElement('button');
    btn.className='pbtn';btn.textContent=SM.t('syncMergeBtn');btn.style.cssText='font-size:10px;padding:3px 8px';
    btn.addEventListener('click',function(){
      window.SMProject.pullAndMerge(entry).then(function(){
        btn.disabled=true;btn.textContent=SM.t('syncMergedBtn');
      });
    });
    row.appendChild(label);row.appendChild(btn);
    listEl.appendChild(row);
  });
}
function initSyncUI(){
  var chooseBtn=document.getElementById('sync-choose-folder');
  var publishBtn=document.getElementById('sync-publish');
  var checkBtn=document.getElementById('sync-check');
  if(!chooseBtn||!publishBtn||!checkBtn)return;
  chooseBtn.addEventListener('click',function(){
    window.SMProject.chooseSyncFolder().then(function(){syncFolderFields();});
  });
  publishBtn.addEventListener('click',function(){window.SMProject.publishToShared();});
  checkBtn.addEventListener('click',function(){
    var listEl=document.getElementById('sync-updates-list');
    if(listEl)listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Recherche…</div>';
    window.SMProject.checkSharedUpdates().then(renderSyncUpdates);
  });
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initSyncUI);else initSyncUI();

`;
function actualCallerFragment(){
 const source=fs.readFileSync(path.join(ROOT,'src/js/timeline.js'),'utf8');
 const start=source.indexOf('var settingsPreferencesUi=null;'),middle=source.indexOf('// Burger menu',start);
 const sync=source.indexOf('function syncRelTime(ts){return settingsPreferencesUiInstance().syncRelTime(ts);}');
 const end=source.indexOf('// ---- Feedback inbox',sync);
 assert.ok(start>=0&&middle>start&&sync>middle&&end>sync,'ordinary caller boundaries must remain present');
 return source.slice(start,middle)+source.slice(sync,end);
}
class Element {
  constructor(id,doc){this.id=id;this.doc=doc;this.value='';this.dataset={};this.style={};this.children=[];this.listeners=new Map();this._html='';this.textContent='';this.disabled=false;const values=new Set();this.classList={add:x=>values.add(x),remove:x=>values.delete(x),contains:x=>values.has(x)};}
  set innerHTML(v){this._html=v;this.children=[];}get innerHTML(){return this._html;}
  addEventListener(name,callback){if(!this.listeners.has(name))this.listeners.set(name,[]);this.listeners.get(name).push(callback);}
  appendChild(node){this.children.push(node);return node;}
  click(){this.fire('click');}
  fire(name,event={}){for(const callback of this.listeners.get(name)||[])callback.call(this,{target:this,...event});}
  querySelectorAll(){return this.doc.tabs;}
}
function fixture(proposed,{ready='loading',missing=[]}={}){
 const trace=[],nodes=new Map(),callbacks=[],doc={readyState:ready,tabs:[],panes:[],getElementById:id=>nodes.get(id)||null,querySelectorAll:()=>doc.panes,addEventListener:(event,callback)=>{assert.equal(event,'DOMContentLoaded');callbacks.push(callback);},createElement:tag=>new Element(tag,doc),createTextNode:text=>({textContent:text})};
 const ids=['settings-tabs','btn-settings','settings-modal','project-tabs-settings','settings-close','shortcuts-reset','profile-name','profile-color','profile-color-well','profile-role','profile-role-hint','profile-email','sync-folder-path','sync-updates-list','sync-choose-folder','sync-publish','sync-check'];
 for(const id of ids)if(!missing.includes(id))nodes.set(id,new Element(id,doc));
 for(const name of ['profile','sync']){const tab=new Element(name,doc);tab.dataset.tab=name;doc.tabs.push(tab);const pane=new Element(name,doc);pane.dataset.pane=name;doc.panes.push(pane);}
 const state={userProfile:{name:'Original',email:'old@example.invalid',color:'#12345678',role:'animator'},tool:'brush'};let folder='/shared',mergeResolve;
 const copy=x=>JSON.parse(JSON.stringify(x));const project={getSyncFolder(){trace.push(['folder',this===project]);return folder;},chooseSyncFolder(){trace.push(['choose',this===project]);folder='/chosen';return Promise.resolve();},publishToShared(){trace.push(['publish',this===project]);},checkSharedUpdates(){trace.push(['check',this===project]);return Promise.resolve([{profileName:'<img src=x>',profileColor:'#234567',ts:3599900}]);},pullAndMerge(entry){trace.push(['merge',this===project,entry.profileName]);return new Promise(r=>{mergeResolve=r;});}};
 const context={document:doc,state,console,NemoSettingsPreferencesUI:UI,Date:class extends Date{static now(){return 3600000;}},PRODUCER_ALLOWED_TOOLS:['hand','zoom','rotate','comment'],saveUserProfile(){trace.push(['save',copy(state.userProfile)]);},setHex8Input(input,color){trace.push(['hex',input.id,color]);input.value=color.slice(0,7);input.dataset.hex8=color;},renderShortcutsList(){trace.push(['shortcuts']);},refreshFeedbackList(){trace.push(['feedback']);},renderLabsPanel(){trace.push(['labs']);},showToast:text=>trace.push(['toast',text]),shortcutRegistry:{reset(){trace.push(['reset']);}},SM:{t:key=>'translated:'+key,setTool(tool){trace.push(['tool',tool]);state.tool=tool;}},SMProject:project,ColorPicker:{wireColorSwatches(fields){trace.push(['swatches',copy(fields),this===context.ColorPicker]);}}};context.window=context;
 vm.createContext(context);vm.runInContext((proposed?actualCallerFragment():BASELINE_FRAGMENT),context);
 function snapshot(){return {trace:copy(trace),state:copy(state),nodes:[...nodes].map(([id,n])=>({id,value:n.value,style:copy(n.style),text:n.textContent,html:n.innerHTML,disabled:n.disabled,children:n.children.map(c=>({text:c.textContent,children:c.children?.map(k=>k.textContent)}))})),panes:doc.panes.map(n=>n.style.display),tabs:doc.tabs.map(n=>n.classList.contains('active'))};}
 return {context,state,trace,nodes,doc,snapshot,boot(){for(const callback of callbacks)callback();},merge(){mergeResolve();},callbackCount:callbacks.length,callbackNames:callbacks.map(callback=>callback.name)};
}
const flush=async()=>{await Promise.resolve();await Promise.resolve();};
async function parity(options,run){const old=fixture(false,options),now=fixture(true,options);await run(old);await run(now);assert.deepEqual(now.snapshot(),old.snapshot());return now;}
test('actual profile/modal/sync DOM-ready boot preserves order, values and zero service writes',async()=>{const f=await parity({},async f=>{assert.equal(f.callbackCount,3);assert.deepEqual(f.callbackNames,['initProfileFields','initSettingsModal','initSyncUI']);f.boot();});assert.equal(f.nodes.get('profile-name').value,'Original');assert.equal(f.trace.some(x=>['publish','merge','choose'].includes(x[0])),false);});
test('already-ready initialization preserves the same ordinary fields and listeners',async()=>{await parity({ready:'complete'},async f=>{assert.equal(f.callbackCount,0);f.nodes.get('btn-settings').click();});});
test('settings modal tabs, top shortcut, close and backdrop use the original controls',async()=>{const f=await parity({},async f=>{f.boot();f.nodes.get('project-tabs-settings').click();assert.equal(f.nodes.get('settings-modal').style.display,'flex');f.doc.tabs[1].click();f.nodes.get('settings-modal').fire('click',{target:f.nodes.get('settings-close')});assert.equal(f.nodes.get('settings-modal').style.display,'flex');f.nodes.get('settings-modal').click();assert.equal(f.nodes.get('settings-modal').style.display,'none');f.nodes.get('btn-settings').click();f.nodes.get('settings-close').click();});assert.deepEqual(f.doc.panes.map(x=>x.style.display),['none','']);});
test('profile edits preserve fallback name, email, alpha color and save-before-safe-tool order',async()=>{const f=await parity({},async f=>{f.boot();let e=f.nodes.get('profile-name');e.value='';e.fire('input');e=f.nodes.get('profile-email');e.value='next@example.invalid';e.fire('input');e=f.nodes.get('profile-color');e.dataset.hex8='#ABCDEF33';e.fire('input');e=f.nodes.get('profile-role');e.value='producer';e.fire('change');});assert.equal(f.state.userProfile.name,'Animateur');assert.equal(f.state.userProfile.color,'#ABCDEF33');assert.equal(f.state.tool,'hand');assert.equal(f.trace.at(-2)[0],'save');assert.deepEqual(f.trace.at(-1),['tool','hand']);});
test('producer role retains an already allowed tool and shortcut reset translation order',async()=>{await parity({},async f=>{f.boot();f.state.tool='zoom';const role=f.nodes.get('profile-role');role.value='producer';role.fire('change');assert.equal(f.state.tool,'zoom');f.nodes.get('shortcuts-reset').click();assert.deepEqual(f.trace.slice(-3).map(x=>x[0]),['reset','shortcuts','toast']);});});
test('shared-folder choice/publish/check/merge preserve owner receivers and async confirmation',async()=>{const f=await parity({},async f=>{f.boot();f.nodes.get('sync-choose-folder').click();await flush();assert.equal(f.nodes.get('sync-folder-path').value,'/chosen');f.nodes.get('sync-publish').click();f.nodes.get('sync-check').click();await flush();const row=f.nodes.get('sync-updates-list').children[0],button=row.children[1];assert.equal(row.children[0].children[1].textContent,'<img src=x> — il y a 0s');button.click();assert.equal(button.disabled,false);f.merge();await flush();assert.equal(button.disabled,true);assert.equal(button.textContent,'translated:syncMergedBtn');});assert.equal(f.trace.filter(x=>['choose','publish','check','merge'].includes(x[0])).every(x=>x[1]===true),true);});
test('empty update list and relative time rounding preserve observed baseline output',async()=>{await parity({},async f=>{f.boot();f.context.renderSyncUpdates([]);assert.match(f.nodes.get('sync-updates-list').innerHTML,/Aucune mise/);assert.equal(f.context.syncRelTime(3601000),'il y a 0s');assert.equal(f.context.syncRelTime(3540000),'il y a 1 min');assert.equal(f.context.syncRelTime(0),'il y a 1h');});});
test('missing optional settings/profile/sync roots preserve no-op boot behavior',async()=>{await parity({missing:['btn-settings','profile-name','sync-choose-folder']},async f=>{f.boot();assert.equal(f.trace.length,0);});});
test('plain-script presentation factory loads without provider access or startup effects',()=>{const context={};vm.createContext(context);vm.runInContext(fs.readFileSync(path.join(ROOT,'src/js/adapters/settings-preferences-ui.js'),'utf8'),context);assert.equal(typeof context.NemoSettingsPreferencesUI.create,'function');assert.equal(Object.isFrozen(context.NemoSettingsPreferencesUI),true);const ports=new Proxy({}, {get(target,key){if(key==='document')return null;throw Error('unexpected early provider access: '+String(key));}});const ui=context.NemoSettingsPreferencesUI.create(ports);assert.equal(Object.isFrozen(ui),true);});
