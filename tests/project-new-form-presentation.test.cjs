'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');
const test=require('node:test');
const entrySource=fs.readFileSync('src/js/project-entry.js','utf8');
const fixtureSource=fs.readFileSync('tests/project-open.test.cjs','utf8');
const start=fixtureSource.indexOf('function harness('),end=fixtureSource.indexOf('\nfunction select(',start);
assert.ok(start>=0&&end>start,'reuse only the test DOM fixture; shipped sources remain intact');
const plain=value=>JSON.parse(JSON.stringify(value));
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};}
function shipped(){
  const context=vm.createContext({assert,vm,require,setTimeout,console,
    source:fs.readFileSync('src/js/project.js','utf8'),entrySource,
    documentSource:fs.readFileSync('src/js/project-document.js','utf8')});
  vm.runInContext(fixtureSource.slice(start,end)+'\nthis.makeHarness=harness;',context);
  return context.makeHarness();
}
function view(createProject){
  const root={},nodes=new Map(),notices=[];
  const make=()=>{const classes=new Set();return {style:{},value:'',children:[],listeners:{},attributes:{},
    classList:{contains:x=>classes.has(x),add:x=>classes.add(x),remove:x=>classes.delete(x)},
    setAttribute(key,value){this.attributes[key]=value;},getAttribute(key){return this.attributes[key];},
    appendChild(child){this.children.push(child);},addEventListener(type,callback){this.listeners[type]=callback;}};};
  const document={getElementById(id){if(!nodes.has(id))nodes.set(id,make());return nodes.get(id);},createElement:make};
  vm.runInNewContext(entrySource,{window:root});
  root.SMProjectEntry.bindNewProjectForm(document,{createProject,
    showStartScreen(){document.getElementById('start-screen').classList.remove('hid');},notice:message=>notices.push(message)});
  const element=id=>document.getElementById(id);
  element('np-preset').value='1920x1080';element('np-fps').value='24';
  const click=id=>element(id).listeners.click();
  const alert=element('start-newpanel').children.find(n=>n.getAttribute('role')==='alert');
  return {element,click,alert,notices};
}
function sourceFields(app){
  app.element('start-new').listeners.click();app.element('np-preset').value='1280x720';
  app.element('np-name').value='Preserve fields';app.element('np-fps').value='30';
}
test('binding delegates no authority and preserves preset/custom parsing and defaults',()=>{
  const calls=[],f=view(config=>{calls.push(plain(config));return false;});
  assert.deepEqual(calls,[]);f.click('start-new');
  f.element('np-name').value='  Project A  ';f.element('np-preset').value='1280x720';f.element('np-fps').value='30';
  f.click('np-create');assert.deepEqual(calls[0],{w:1280,h:720,fps:30,name:'Project A'});
  f.element('np-preset').value='custom';f.element('np-preset').listeners.change.call(f.element('np-preset'));
  assert.equal(f.element('np-custom-row').style.display,'flex');
  f.element('np-w').value='0';f.element('np-h').value='bad';f.element('np-fps').value='0';f.element('np-name').value=' ';
  f.click('np-create');assert.deepEqual(calls[1],{w:1920,h:1080,fps:24,name:'Untitled'});
  f.element('np-preset').value='320x180';f.element('np-preset').listeners.change.call(f.element('np-preset'));
  assert.equal(f.element('np-custom-row').style.display,'none');
});
test('sync denial/throw and async rejection preserve fields and show only fixed unavailable text',async()=>{
  for(const create of [()=>false,()=>{throw Error('private failure');},()=>Promise.reject(Error('private rejection'))]){
    const f=view(create);f.click('start-new');f.element('np-name').value='Keep';
    assert.equal(await f.click('np-create'),false);
    assert.equal(f.element('start-newpanel').style.display,'block');assert.equal(f.element('np-name').value,'Keep');
    assert.equal(f.alert.style.display,'block');assert.equal(f.notices.length,1);
    assert.equal(f.alert.textContent,'New project unavailable — native admission was denied.');
    assert.equal(f.alert.textContent.includes('private'),false);
  }
});
test('duplicate create is refused and cancelled/reopened generation ignores late completion',async()=>{
  const pending=deferred(),calls=[],f=view((config,current)=>{calls.push({config,current});return calls.length===1?pending.promise:true;});
  f.click('start-new');const first=f.click('np-create');
  assert.equal(f.click('np-create'),false);assert.equal(calls.length,1);assert.equal(calls[0].current(),true);
  f.click('np-cancel');assert.equal(calls[0].current(),false);f.click('start-new');
  assert.equal(f.click('np-create'),true);assert.equal(f.element('start-newpanel').style.display,'none');
  pending.reject(Error('old failure'));assert.equal(await first,false);
  assert.deepEqual(f.notices,[]);assert.equal(f.alert.style.display,'none');
});
test('current guard rejects a hidden start screen and hidden panel without false failure UI',async()=>{
  for(const hidden of ['screen','panel']){
    const pending=deferred();let current;const f=view((config,check)=>{current=check;return pending.promise;});
    f.click('start-new');const creation=f.click('np-create');
    if(hidden==='screen')f.element('start-screen').classList.add('hid');else f.element('start-newpanel').style.display='none';
    assert.equal(current(),false);pending.resolve(false);assert.equal(await creation,false);
    assert.deepEqual(f.notices,[]);assert.equal(f.alert.style.display,'none');
  }
});
test('partial creation failure reopens the start screen with the existing distinct warning',()=>{
  let f;f=view((config,current,failed)=>{f.element('start-screen').classList.add('hid');failed();throw Error('partial');});
  f.click('start-new');f.element('np-name').value='Keep partial fields';assert.equal(f.click('np-create'),false);
  assert.equal(f.element('start-screen').classList.contains('hid'),false);
  assert.equal(f.element('start-newpanel').style.display,'block');assert.equal(f.element('np-name').value,'Keep partial fields');
  assert.equal(f.alert.textContent,'New project creation failed after admission; the document may have changed. Reopen a project before editing.');
});
test('sync undefined admission keeps the original legacy success convention',()=>{
  const f=view(()=>undefined);f.click('start-new');assert.equal(f.click('np-create'),undefined);
  assert.equal(f.element('start-newpanel').style.display,'none');assert.deepEqual(f.notices,[]);
});
test('shipped controller callback resolves the live SMProject receiver after binding',()=>{
  const app=shipped(),calls=[];sourceFields(app);
  const creator=function(config,current,failed){assert.equal(this,app.window.SMProject);calls.push(plain(config));return false;};
  app.window.SMProject={...app.window.SMProject,newProject:creator};
  const before={json:app.json,writes:plain(app.writes),mutations:app.mutations};
  assert.equal(app.element('np-create').listeners.click(),false);
  assert.deepEqual(calls,[{w:1280,h:720,fps:30,name:'Preserve fields'}]);
  assert.equal(app.json,before.json);assert.deepEqual(plain(app.writes),before.writes);assert.equal(app.mutations,before.mutations);
});
test('shipped native denial never reaches release, history, persistence or document mutation',async()=>{
  const app=shipped();sourceFields(app);let releases=0;
  app.window.NemoNativeOpacityCutover={blocksLegacy:()=>true};
  app.window.NemoNativeOpacityProject={release(){releases++;throw Error('must not release native');}};
  const before={json:app.json,writes:plain(app.writes),mutations:app.mutations,label:app.project.getCurrentLabel()};
  assert.equal(await app.element('np-create').listeners.click(),false);assert.equal(releases,0);
  assert.equal(app.json,before.json);assert.deepEqual(plain(app.writes),before.writes);
  assert.equal(app.mutations,before.mutations);assert.equal(app.project.getCurrentLabel(),before.label);
  assert.equal(app.element('np-name').value,'Preserve fields');assert.equal(app.element('start-newpanel').style.display,'block');
});
test('actual cancel while admission is pending prevents creation after release resolves',async()=>{
  const app=shipped(),gate=deferred();sourceFields(app);
  app.window.NemoNativeOpacityLegacySurface={allowProjectTransition:()=>true,releaseProjectTransition:()=>gate.promise};
  app.window.n20AllowLegacyWrite=()=>true;
  const before={json:app.json,writes:plain(app.writes),mutations:app.mutations};
  const creating=app.element('np-create').listeners.click();app.element('np-cancel').listeners.click();
  gate.resolve(true);assert.equal(await creating,false);
  assert.equal(app.json,before.json);assert.deepEqual(plain(app.writes),before.writes);assert.equal(app.mutations,before.mutations);
  assert.equal(app.element('start-newpanel').style.display,'none');
});
