'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const os=require('node:os');
const crypto=require('node:crypto');
const {createRequire}=require('node:module');
const Project=require('../../src/js/adapters/native-object-project.js');
const Files=require('../../src/js/adapters/native-object-project-file.js');
const Workflow=require('../../src/js/application/native-object-save-as-workflow.js');
const Ack=require('../../src/js/application/native-object-saved-acknowledgement.js');
const Observer=require('./p38-fixture-open-owner.cjs');
const fromTests=createRequire(path.resolve('tests/native-object-project.test.cjs'));
const fixture=fs.readFileSync('tests/native-object-project.test.cjs','utf8');
const hostCode=fixture.slice(fixture.indexOf('function host(){'),fixture.indexOf('\nfunction session('));
const ordinaryCode=fixture.slice(fixture.indexOf('function ordinary('),fixture.indexOf("\ntest('complete production",fixture.indexOf('function ordinary(')))
  .replace('let id=0;','root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>diagnostics.push({event,code});let id=0;');
function harness(){
  const diagnostics=[];
  const context=vm.createContext({fs,vm,assert,require:fromTests,Project,Files,Workflow,Ack,setTimeout,console,diagnostics});
  vm.runInContext(hostCode+'\n'+ordinaryCode+'\nthis.makeHost=host;this.makeOrdinary=ordinary;',context);
  const host=context.makeHost();
  return {host,diagnostics,ordinary:()=>context.makeOrdinary(host)};
}
function failure(h){return h.diagnostics.find(e=>e.event==='post-bootstrap-failed')?.code;}
const code=detail=>'post-bootstrap-first-preview-'+detail+':local-refusal:unknown:other';
test('real desktop mapping distinguishes missing canvas, thrown mapping and invalid viewport without preview input',async()=>{
  for(const detail of ['mapping','viewport']){
    const h=harness(),app=h.ordinary();
    if(detail==='mapping'){
      const original=app.window.document.getElementById;
      app.window.document.getElementById=id=>id==='drawing-canvas'?null:original(id);
    }else app.element('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:0,height:180});
    assert.equal(await app.project.openPath('/ordinary.json'),false);
    assert.equal(failure(h),code(detail));
    assert.equal(h.host.calls.filter(c=>c.command==='nemo_native_object_preview').length,0);
    assert.equal(Project.desktop(app.window).isIndeterminate(),true);
  }
  const h=harness(),app=h.ordinary(),original=new Error('PRIVATE /path/document');
  app.element('drawing-canvas').getBoundingClientRect=()=>{throw original;};
  await assert.rejects(Project.desktop(app.window).openPath('/ordinary.json'),e=>e===original);
  assert.equal(failure(h),code('mapping'));assert.doesNotMatch(JSON.stringify(h.diagnostics),/PRIVATE|\/path/);
});
test('real ordinary Open distinguishes returned status, work and generation guards after exactly one preview',async()=>{
  for(const [patch,detail]of [[null,'receipt'],[{status:'deferred-occluded'},'status-deferred-occluded'],
    [{status:'PRIVATE /path'},'status'],[{workId:''},'work-id'],[{viewGeneration:0},'view-generation']]){
    const h=harness(),invoke=h.host.invoke;
    h.host.invoke=async(command,args)=>{
      const value=await invoke(command,args);return command==='nemo_native_object_preview'?(patch===null?null:{...value,...patch}):value;
    };
    const app=h.ordinary();assert.equal(await app.project.openPath('/ordinary.json'),false);
    assert.equal(failure(h),code(detail));assert.equal(h.host.calls.filter(c=>c.command==='nemo_native_object_preview').length,1);
    assert.equal(Project.desktop(app.window).readPresentedObjectSession(),null);
    assert.equal(app.element('project-tabs-list').children.length,0);assert.doesNotMatch(JSON.stringify(h.diagnostics),/PRIVATE|\/path/);
  }
});
test('receipt accessor and invoke rejection preserve the exact original error object and finite privacy',async()=>{
  for(const getter of [false,true]){
    const h=harness(),original={message:'PRIVATE document bytes'};
    Object.defineProperty(original,'code',{get(){throw Error('PRIVATE code getter');}});
    const invoke=h.host.invoke;
    h.host.invoke=async(command,args)=>{
      if(command!=='nemo_native_object_preview')return invoke(command,args);
      if(!getter)throw original;
      return Object.defineProperty({},'status',{get(){throw original;}});
    };
    const app=h.ordinary();await assert.rejects(Project.desktop(app.window).openPath('/ordinary.json'),e=>e===original);
    assert.equal(failure(h),getter?code('receipt-access'):'post-bootstrap-first-preview:invoke-rejected:unknown:other');
    assert.doesNotMatch(JSON.stringify(h.diagnostics),/PRIVATE|bytes/);
  }
});
test('positive ordinary Open keeps both original requests and order; range refusal outside Open stays unchanged',async()=>{
  const h=harness(),control=harness(),app=h.ordinary(),other=control.ordinary();
  assert.equal(await app.project.openPath('/ordinary.json'),true);assert.equal(await other.project.openPath('/ordinary.json'),true);
  assert.deepEqual(JSON.parse(JSON.stringify(h.host.calls)),JSON.parse(JSON.stringify(control.host.calls)));
  assert.deepEqual(Array.from(h.host.calls,c=>c.command),['nemo_native_object_client_status','nemo_native_object_bootstrap',
    'nemo_native_object_client_status','nemo_native_object_reference','nemo_native_object_preview',
    'nemo_native_object_client_status','nemo_native_object_reference','nemo_native_object_preview',
    'nemo_native_object_client_status','nemo_native_object_reference']);
  assert.equal(h.host.calls.filter(c=>c.command==='nemo_native_object_preview').length,2);
  assert.equal(failure(h),undefined);assert.ok(Project.desktop(app.window).readPresentedObjectSession());
  const count=h.host.calls.length;await assert.rejects(Project.desktop(app.window).presentPreview(-1),/Frame is outside the native project/);
  assert.equal(h.host.calls.length,count);assert.equal(failure(h),undefined);
});
test('second preview identifies its own actual guard and keeps the first presentation and reveal order',async()=>{
  for(const detail of ['viewport','work-id']){
    const h=harness(),invoke=h.host.invoke;let previews=0,mappings=0;
    h.host.invoke=async(command,args)=>{
      const value=await invoke(command,args);
      return command==='nemo_native_object_preview'&&++previews===2&&detail==='work-id'?{...value,workId:''}:value;
    };
    const app=h.ordinary();
    app.element('drawing-canvas').getBoundingClientRect=()=>({left:0,top:0,width:++mappings===2&&detail==='viewport'?0:320,height:180});
    assert.equal(await app.project.openPath('/ordinary.json'),false);
    assert.equal(failure(h),'post-bootstrap-second-preview-'+detail+':local-refusal:unknown:other');
    assert.equal(h.host.calls.filter(c=>c.command==='nemo_native_object_preview').length,detail==='viewport'?1:2);
    assert.equal(Project.desktop(app.window).isIndeterminate(),true);
  }
});
const statuses=['stale-discarded','deferred-timeout','deferred-occluded','failed-validation','failed-device-lost',
  'failed-retry-lost','failed-retry-outdated','failed-recreate','failed-reconfigure'];
test('finite status vocabulary matches the actual native receipt producer',()=>{
  const source=fs.readFileSync('src-tauri/src/native_application_viewport.rs','utf8');
  const start=source.indexOf('pub(crate) fn status_label('),end=source.indexOf('\n}',start);
  assert.ok(start>=0&&end>start);
  const actual=Array.from(source.slice(start,end).matchAll(/=> "([a-z-]+)"/g),m=>m[1]);
  assert.deepEqual(actual,['presented',...statuses]);
});
test('only exact native status labels become witnesses; status getters are read once and later guards short circuit',async()=>{
  for(const status of [...statuses,'deferred-PRIVATE','deferred-occluded:PRIVATE',{},null]){
    const h=harness(),invoke=h.host.invoke;let reads=0;
    h.host.invoke=async(command,args)=>{
      if(command!=='nemo_native_object_preview')return invoke(command,args);
      return Object.defineProperties({},{status:{get(){reads++;return status;}},workId:{get(){assert.fail('status refusal read work identity');}}});
    };
    const app=h.ordinary();assert.equal(await app.project.openPath('/ordinary.json'),false);
    assert.equal(failure(h),code(statuses.includes(status)?'status-'+status:'status'));assert.equal(reads,1);
    assert.doesNotMatch(JSON.stringify(h.diagnostics),/PRIVATE/);
  }
});
test('actual observer retains exact new finite witnesses after ready and rejects unknown or extra components',async()=>{
  const text=fs.readFileSync('tests/desktop/p38-fixture-open-owner.test.cjs','utf8');
  const definitions=text.slice(text.indexOf('const config ='),text.indexOf('\nasync function probeFailure('));
  const checkpoint=text.slice(text.indexOf('async function checkpointObservation('),text.indexOf("\ntest(",text.indexOf('async function checkpointObservation(')));
  const context=vm.createContext({assert,fs,os,path,crypto,...Observer});
  vm.runInContext(definitions+'\n'+checkpoint+'\nthis.observe=checkpointObservation;',context);
  const details=['frame-range','mapping','viewport','owner','receipt','status','work-id','view-generation','receipt-access',...statuses.map(s=>'status-'+s)];
  for(const prefix of ['first-preview','second-preview'])for(const detail of details){
    const native='a-post-bootstrap-'+prefix+'-'+detail+':local-refusal:unknown:other';
    assert.ok(Observer.PROBE_FAILURE_CODES.a.includes(native));
    const result=await context.observe({checkpoint:1,failureCode:native});
    assert.equal(result.firstFailure.code,'probe-manifest-failed-'+native);assert.equal(result.saved.code,result.firstFailure.code);
    assert.equal(result.observations.length,1);assert.equal(result.observations[0].decision,'accepted');assert.equal(result.cleanup,'complete');
  }
  for(const stage of ['first-preview-PRIVATE','first-preview-mapping:PRIVATE','first-preview-mapping\\u0000','third-preview-mapping']){
    const native='a-post-bootstrap-'+stage+':local-refusal:unknown:other';
    assert.equal(Observer.PROBE_FAILURE_CODES.a.includes(native),false);
    const result=await context.observe({checkpoint:1,failureCode:native});
    assert.equal(result.firstFailure.code,'probe-manifest-failure-unknown');assert.doesNotMatch(JSON.stringify(result),/PRIVATE/);
  }
});
