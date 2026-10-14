'use strict';
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),os=require('node:os'),zlib=require('node:zlib');
const {spawnSync}=require('node:child_process');
const P=require('./desktop/png-ui-installed-policy.cjs'),D=require('./desktop/png-ui-installed-driver.cjs');
const crc=require('./desktop/reference-installed-policy.cjs').crc32;
const complete={complete:true,truncated:false,depthClipped:false,childrenUnreadable:false,foreignChildren:false};
function fixtureBytes(){
  const objects=[],scopes=[];
  for(const [frame,fill] of [[0,{kind:'solid',r:0,g:0,b:1,a:1}],[1,{kind:'solid',r:0,g:1,b:0,a:1}]]){
    const target={contextId:'scene-root',frameScope:{kind:'authored',frame},layerUid:'L0',strokeId:'a'};
    objects.push({schemaVersion:1,family:'closed-solid-fill-cubic-path',target,geometry:{kind:'cubic-path',closed:true,coordinateSpace:'document-world',handleSpace:'relative',
      segments:[[16,16],[120,16],[120,120],[16,120]].map(([x,y])=>({point:{x,y},handleIn:{x:0,y:0},handleOut:{x:0,y:0}}))},fill});
    scopes.push({frameScope:target.frameScope,elements:[{layerUid:'L0',strokeId:'a',naturalPosition:0,order:{}}]});
  }
  const drawing={format:'nemo.native-object-document',formatVersion:2,totalFrames:2,layers:[{layerUid:'L0'}],objects,
    compositionOrder:{format:'nemo.native-object-order-plan',formatVersion:1,documentFormatVersion:2,
      contexts:[{contextId:'scene-root',layers:[{layerUid:'L0',naturalPosition:0,order:{}}],scopes}],migration:'explicit-plan-required',referencePolicy:'unresolved'}};
  const project={format:'nemo.native-project-document',formatVersion:1,project:{canvasW:320,canvasH:180,fps:24},drawing,viewportReference:null,assets:[]};
  return Buffer.from(JSON.stringify(project).replace('"fps":24}','"fps":24.0}')+'\n');
}
function setup(t){
  const root=fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()),'png-ui-test-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const fixture=path.join(root,'fixed.json');fs.writeFileSync(fixture,fixtureBytes());
  fs.mkdirSync(path.join(root,'cache'));
  const c={workflow:'PNG_UI',grant:{issue:1959,confirmed:true,id:'png-test-'+path.basename(root)},taskId:'png-owned',sourceHead:P.SOURCE,driverHead:'a'.repeat(40),
    executableSha256:'b'.repeat(64),helperSha256:'c'.repeat(64),timeoutMs:10000,appRoot:root,app:path.join(root,'Nemo.app'),buildProof:path.join(root,'proof.json'),
    windowHelper:path.join(root,'helper'),helperProof:path.join(root,'helper-proof.json'),fixture,reportRoot:path.join(root,'workspace')};
  return {root,c,prepare:()=>P.prepareWorkspace(c)};
}
function png(color){
  function chunk(type,data){const name=Buffer.from(type),out=Buffer.alloc(data.length+12);out.writeUInt32BE(data.length);name.copy(out,4);data.copy(out,8);out.writeUInt32BE(crc(Buffer.concat([name,data])),out.length-4);return out;}
  const header=Buffer.alloc(13);header.writeUInt32BE(320);header.writeUInt32BE(180,4);header[8]=8;header[9]=6;
  const raw=Buffer.alloc(180*(320*4+1));
  for(let y=0;y<180;y++)for(let x=0;x<320;x++){
    const rgba=x>=16&&x<120&&y>=16&&y<120?color:[255,255,255,255];Buffer.from(rgba).copy(raw,y*1281+1+x*4);
  }
  return Buffer.concat([Buffer.from('89504e470d0a1a0a','hex'),chunk('IHDR',header),chunk('IDAT',zlib.deflateSync(raw)),chunk('IEND',Buffer.alloc(0))]);
}
function outputs(w){
  const folder=path.join(w.output,'nemo-native-opacity-1791970000000-1');fs.mkdirSync(folder);
  fs.writeFileSync(path.join(folder,'frame_0001.png'),png([0,0,255,255]));fs.writeFileSync(path.join(folder,'frame_0002.png'),png([0,255,0,255]));return folder;
}
function witness(c,label,target='main',value){
  const a={type:'press',target,element:'0'},last={nodes:[{path:'0',control:label}]};
  if(value!==undefined)P.witnessAction(c,{type:'set',target:'sheet',value},null);
  P.witnessAction(c,a,last);
  if(target==='sheet')P.witnessAction(c,{type:'sheet-dismissed'},null);
}
function beforeUI(c,w){witness(c,'Open Project…');witness(c,'Open','sheet',w.fixture);witness(c,'Save As…');witness(c,'Save','sheet',w.saved);}
function cancelUI(c){witness(c,'Export');witness(c,'Cancel');}
function exportUI(c,w){P.witnessAction(c,{type:'press',element:'0'},{nodes:[{path:'0',control:'Export'},...['PNG','Whole timeline','1x'].map(control=>({role:'AXPopUpButton',control}))]});witness(c,'Choose','sheet',w.output);}
test('fixed independently authored two-frame project exactly matches admitted byte hash',()=>{
  const bytes=fixtureBytes();assert.equal(bytes.length,2103);assert.equal(P.hash(bytes),P.FIXTURE);
  const data=JSON.parse(bytes);assert.equal(data.drawing.totalFrames,2);assert.deepEqual(data.drawing.objects.map(x=>x.fill),[{kind:'solid',r:0,g:0,b:1,a:1},{kind:'solid',r:0,g:1,b:0,a:1}]);
});
test('PNG authorization refuses stale/cross-workflow/unconfirmed and malformed source grants before effects',t=>{
  const {c}=setup(t);assert.doesNotThrow(()=>P.authorize(c));
  for(const patch of [{workflow:'P38'},{workflow:'P37_REFERENCE'},{grant:{issue:1645,confirmed:true,id:'old'}},
    {grant:{issue:1864,confirmed:true,id:'reference'}},{grant:{issue:1959,confirmed:false,id:'png'}},{grant:null},
    {sourceHead:'6e4edc21bb92095cc8306183ab049a8f0288dbeb'},{sourceHead:'d'.repeat(40)},
    {timeoutMs:600001},{driverHead:'bad'},{fixture:'relative'}])assert.throws(()=>P.authorize({...c,...patch}));
});
test('fixed fixture/root/inode and path controls refuse changes without admitting a different document',t=>{
  const f=setup(t),w=f.prepare();assert.throws(f.prepare,/workspace-reused/);
  const sheet={token:'sheet'},last={...complete,target:'sheet',nodes:[{path:'0',role:'AXTextField',enabled:true}]};
  P.verifyAction(f.c,{type:'set',target:'sheet',sheetToken:'sheet',element:'0',value:w.fixture},sheet,last);
  for(const value of [f.root,path.join(w.output,'other'),'../A.json','/tmp/foreign.json'])
    assert.throws(()=>P.verifyAction(f.c,{type:'set',target:'sheet',sheetToken:'sheet',element:'0',value},sheet,last));
  fs.writeFileSync(w.fixture,'changed');assert.throws(()=>P.verifyWorkspace(f.c),/fixture-refused/);
});
test('symlink fixture and replaced workspace are refused',t=>{
  const f=setup(t),link=path.join(f.root,'link.json');fs.symlinkSync(f.c.fixture,link);
  assert.throws(()=>P.verifyInputs({...f.c,fixture:link}));
  const w=f.prepare();fs.renameSync(f.c.reportRoot,f.c.reportRoot+'-old');fs.mkdirSync(f.c.reportRoot);
  assert.throws(()=>P.verifyWorkspace(f.c),/root-refused/);assert.equal(fs.existsSync(w.output),false);
});
test('only fresh visible enabled allowed controls and current sheet/location tokens can act',t=>{
  const f=setup(t);f.prepare();const a={type:'press',element:'0'};
  const good={...complete,target:'main',nodes:[{path:'0',control:'Export',role:'AXButton',press:true,enabled:true}]};
  P.verifyAction(f.c,a,{},good);
  for(const node of [{...good.nodes[0],control:'Delete'},{...good.nodes[0],blocked:'child-owner'},
    {...good.nodes[0],enabled:false},{...good.nodes[0],press:false}])assert.throws(()=>P.verifyAction(f.c,a,{}, {...good,nodes:[node]}));
  assert.throws(()=>P.verifyAction(f.c,a,{},null));
  for(const type of ['key','eval','invoke','pointer','launch'])assert.throws(()=>P.verifyAction(f.c,{type},{},null));
  assert.throws(()=>P.verifyAction(f.c,{type:'inspect',target:'sheet',sheetToken:'old'},{token:'new'},null));
  assert.throws(()=>P.verifyAction(f.c,{type:'inspect',target:'sheet',sheetToken:'new',locationToken:'old'},{token:'new',locationToken:'live'},null));
  assert.throws(()=>P.verifyAction(f.c,{type:'go-to-folder',target:'sheet',sheetToken:'new',keys:'arbitrary'},{token:'new'},null));
  for(const a of [{type:'inspect',depth:6},{type:'inspect',maxNodes:129},{type:'inspect',maxNodes:0}])assert.throws(()=>P.verifyAction(f.c,a,{},null),/inspect-budget/);
});
test('real Save sheet button is admitted only through current owned projection and token',t=>{
  const f=setup(t);f.prepare();const sheet={token:'save-sheet'};
  const a={type:'press',target:'sheet',sheetToken:sheet.token,element:'1'};
  const node={path:'1',control:'Save',role:'AXButton',enabled:true,press:true},last={...complete,target:'sheet',nodes:[node]};
  P.verifyAction(f.c,a,sheet,last);assert.equal(P.sanitizeHelper({nodes:[node]},'sheet').nodes[0].control,'Save');
  for(const patch of [{control:'Save All'},{blocked:'child-owner'},{enabled:false},{role:'AXTextField'}])
    assert.throws(()=>P.verifyAction(f.c,a,sheet,{...last,nodes:[{...node,...patch}]}),/control-refused/);
  assert.throws(()=>P.verifyAction(f.c,{...a,sheetToken:'old'},sheet,last),/sheet-refused/);
  assert.throws(()=>P.verifyAction(f.c,a,sheet,null),/control-refused/);
});
test('frozen SVG/work-area popup defaults are navigable but cannot run or witness PNG export',t=>{
  const f=setup(t),w=f.prepare();fs.writeFileSync(w.saved,'saved');beforeUI(f.c,w);
  const node=(path,control,role='AXPopUpButton')=>({path,control,role,enabled:true,press:true});
  const run=node('0','Export','AXButton'),defaults={...complete,target:'main',nodes:[run,node('1','SVG Sequence'),node('2','Work area'),node('3','1x')]};
  for(const element of ['1','2'])P.verifyAction(f.c,{type:'press',element},{},defaults);
  for(const element of ['1','2'])for(const role of ['AXMenuItem','AXButton'])
    assert.throws(()=>P.verifyAction(f.c,{type:'press',element},{},{...defaults,nodes:defaults.nodes.map(n=>n.path===element?{...n,role}:n)}),/navigation-refused/);
  assert.deepEqual(P.sanitizeHelper(defaults).nodes.slice(1,3).map(n=>n.control),['SVG Sequence','Work area']);
  const good={...complete,target:'main',nodes:[run,node('1','PNG Sequence'),node('2','Whole timeline'),node('3','1x')]};
  P.verifyAction(f.c,{type:'press',element:'0'},{},good);
  const menu=['PNG Sequence','Whole timeline','1x'].map((control,i)=>({...node('4.'+i,control,'AXMenuItem'),selected:false}));
  for(const last of [defaults,{...defaults,nodes:[...defaults.nodes,...menu]},
    {...defaults,nodes:[run,...menu.map(n=>({...n,role:'AXStaticText'}))]},
    {...good,nodes:[...good.nodes,node('4','SVG Sequence')]},
    {...good,nodes:good.nodes.map(n=>n.control==='PNG Sequence'?{...n,blocked:'child-owner'}:n)}])
    assert.throws(()=>P.verifyAction(f.c,{type:'press',element:'0'},{},last),/export-options-refused/);
  const app={dirs:{appCache:path.join(f.root,'cache')}};P.artifacts(f.c,'baseline',app);cancelUI(f.c);P.artifacts(f.c,'cancelled',app);
  P.witnessAction(f.c,{type:'press',element:'0'},{...defaults,nodes:[...defaults.nodes,...menu]});
  witness(f.c,'Choose','sheet',w.output);outputs(w);
  assert.equal(P.witnessSummary(f.c).exportPickerObserved,false);
  assert.throws(()=>P.artifacts(f.c,'exported',app),/ui-unwitnessed/);
});
test('default export run is refused before a helper press even with desired menu labels present',async t=>{
  const f=setup(t),s=fakeServices(f,{actions:[{type:'inspect'},{type:'press',element:'0'}]});
  const nodes=[{path:'0',role:'AXButton',control:'Export',enabled:true,press:true},
    ...['SVG Sequence','Work area','1x'].map((control,i)=>({path:String(i+1),role:'AXPopUpButton',control,enabled:true,press:true})),
    ...['PNG Sequence','Whole timeline'].map((control,i)=>({path:'4.'+i,role:'AXMenuItem',control,selected:false,press:true}))];
  s.services.helper=()=>({async pin(){return {status:'pinned',pid:42,windowId:3,matchedAX:true};},async close(){s.events.push('helper-close');},
    async action(_w,a){s.events.push('helper:'+a.type);return {status:'ok',type:a.type,pid:42,windowId:3,matchedAX:true,...(a.type==='inspect'?{...complete,nodes}:{})};}});
  const r=await D.runOperator(f.c,s.services);assert.equal(r.reason,'png-export-options-refused');
  assert.equal(s.events.includes('helper:press'),false);assert.equal(r.installedAcceptance,'not-established');
});
test('PID/window mismatches cannot become ownership proof',()=>{
  for(const patch of [{pid:71768},{windowId:99},{matchedAX:false},{}]){
    const r={pid:42,windowId:3,matchedAX:true,...patch};if(Object.keys(patch).length)assert.throws(()=>P.identity(r,42,3));else P.identity(r,42,3);
  }
});
test('independent CRC/pixel/manifest and separate saved-before/after files prove bounded artifacts',t=>{
  const f=setup(t),w=f.prepare(),app={dirs:{appCache:path.join(f.root,'cache')}};
  fs.writeFileSync(w.saved,'native saved bytes');beforeUI(f.c,w);P.artifacts(f.c,'baseline',app);cancelUI(f.c);P.artifacts(f.c,'cancelled',app);exportUI(f.c,w);
  const folder=outputs(w);const receipt=P.artifacts(f.c,'exported',app);assert.equal(receipt.files.length,2);
  assert.deepEqual(receipt.files[1].pixels[0].rgba,[0,255,0,255]);
  assert.throws(()=>P.artifacts(f.c,'after-save',app));witness(f.c,'Save As…');witness(f.c,'Save','sheet',w.after);
  fs.writeFileSync(w.after,'changed');assert.throws(()=>P.artifacts(f.c,'after-save',app),/save-changed/);
  fs.writeFileSync(path.join(folder,'extra.png'),'unwanted');assert.throws(()=>P.artifacts(f.c,'after-save',app),/manifest-refused/);
  fs.unlinkSync(path.join(folder,'extra.png'));fs.writeFileSync(w.after,'native saved bytes');P.artifacts(f.c,'after-save',app);
});
test('changed color/CRC/partial output/staging never passes PNG artifact proof',t=>{
  for(const fault of ['color','crc','partial','staging','staging-symlink','broken-staging-symlink']){
    const f=setup(t),w=f.prepare(),app={dirs:{appCache:path.join(f.root,'cache')}};fs.writeFileSync(w.saved,'saved');beforeUI(f.c,w);P.artifacts(f.c,'baseline',app);cancelUI(f.c);P.artifacts(f.c,'cancelled',app);exportUI(f.c,w);
    const folder=outputs(w),second=path.join(folder,'frame_0002.png');
    if(fault==='color')fs.writeFileSync(second,png([255,0,0,255]));
    if(fault==='crc'){const b=fs.readFileSync(second);b[20]^=1;fs.writeFileSync(second,b);}
    if(fault==='partial')fs.unlinkSync(second);
    if(fault==='staging'){fs.mkdirSync(path.join(app.dirs.appCache,'native-export-staging'),{recursive:true});fs.writeFileSync(path.join(app.dirs.appCache,'native-export-staging','debt'),'pending');}
    if(fault.includes('symlink'))fs.symlinkSync(fault.startsWith('broken')?path.join(f.root,'missing'):f.root,path.join(app.dirs.appCache,'native-export-staging'));
    assert.throws(()=>P.artifacts(f.c,'exported',app));
  }
});
test('live owner cache pins allow initial absence then reject changed directory, inode and symlink',t=>{
  const f=setup(t);f.prepare();const file=path.join(f.root,'cache'),manifest={dirs:{appCache:file}};
  fs.rmdirSync(file);P.bindCache(f.c,manifest);assert.equal(fs.existsSync(file),false);
  fs.mkdirSync(file);P.bindCache(f.c,manifest);P.verifyWorkspace(f.c);
  const other=path.join(f.root,'other-cache');fs.mkdirSync(other);
  assert.throws(()=>P.bindCache(f.c,{dirs:{appCache:other}}),/cache-changed/);
  fs.renameSync(file,file+'-old');fs.mkdirSync(file);assert.throws(()=>P.verifyWorkspace(f.c),/root-refused/);
  fs.rmdirSync(file);fs.symlinkSync(file+'-old',file);assert.throws(()=>P.verifyWorkspace(f.c),/root-refused/);
});
test('validated snapshot cache changes stop before the next helper input; package carries no directories',async t=>{
  const f=setup(t),s=fakeServices(f),other=path.join(f.root,'other-cache');fs.mkdirSync(other);let calls=0;
  assert.equal(s.services.package().dirs,undefined);
  s.services.controller=()=>({async start(){return {};},async status(){return {value:{app:{pid:42,dirs:{appCache:++calls===1?path.join(f.root,'cache'):other}},runtime:{childPid:42}}};},
    async cleanup(){s.events.push('app-cleanup');}});
  const r=await D.runOperator(f.c,s.services);assert.equal(r.status,'partial');assert.equal(r.reason,'png-cache-changed');
  assert.equal(s.events.some(x=>x.startsWith('helper:')),false);assert.ok(s.events.includes('helper-close'));assert.ok(s.events.includes('app-cleanup'));
});
function fakeServices(f,options={}){
  const events=[],saved=[],actions=options.actions||[{type:'inspect'},{type:'stop'}];let index=0,timerFn;
  const helper={async pin(){return {status:'pinned',pid:options.foreignPin?71768:42,windowId:3,matchedAX:true};},
    async action(_w,a){events.push('helper:'+a.type);if(options.helperEOF)throw {code:'png-helper-eof'};
      return {status:options.refused?'refused':'ok',type:a.type,pid:options.changedPid?71768:42,windowId:3,matchedAX:true,nodes:[]};},async close(){events.push('helper-close');if(options.cleanupFault)throw Error('cleanup');}};
  const controller={async start(){events.push('start');return {};},async status(){return {value:{app:{pid:42,dirs:{appCache:path.join(f.root,'cache')}},runtime:{childPid:42}}};},
    async cleanup(){events.push('app-cleanup');if(options.cleanupFault)throw Error('cleanup');}};
  return {events,saved,services:{async transport(){if(options.transportFault)throw {code:'png-transport-refused'};},package(){return {bundle:path.join(f.root,'Nemo.app'),executableSha256:f.c.executableSha256};},
    journal(){events.push('journal');},workspace(){return f.prepare();},controller(){return controller;},validate(){},helper(){events.push('helper-start');return helper;},
    async next(){if(options.timeout){timerFn();return new Promise(()=>{});}if(index>=actions.length)throw {code:'png-input-eof'};return actions[index++];},
    feedback(){},save(r){saved.push(JSON.parse(JSON.stringify(r)));},setTimer(fn){timerFn=fn;return 1;},clearTimer(){},async close(){events.push('input-close');}}};
}
test('transport refusal performs no journal/start and wrong grant never invokes services',async t=>{
  const f=setup(t),s=fakeServices(f,{transportFault:true});const r=await D.runOperator(f.c,s.services);
  assert.equal(r.startAttempts,0);assert.equal(s.events.includes('journal'),false);
  await assert.rejects(D.runOperator({...f.c,workflow:'P38'},s.services));assert.equal(s.events.includes('start'),false);
});
test('non-string grant/task IDs refuse before transport, package, journal, workspace or start',async t=>{
  const spy=(s,calls)=>new Proxy(s.services,{get(target,key){const value=target[key];return typeof value==='function'?(...args)=>{calls.push(key);return value.apply(target,args);}:value;}});
  for(const value of [null,undefined,123,true,false,[],{},new String('looks-valid')]){
    for(const field of ['grant','taskId']){
      const f=setup(t),s=fakeServices(f),c=field==='grant'?{...f.c,grant:{...f.c.grant,id:value}}:{...f.c,taskId:value};
      const calls=[];await assert.rejects(D.runOperator(c,spy(s,calls)));assert.deepEqual(calls,[]);assert.deepEqual(s.events,[]);assert.deepEqual(s.saved,[]);
    }
  }
  const f=setup(t),s=fakeServices(f);
  const calls=[];await assert.rejects(D.runOperator({...f.c,taskId:'x'.repeat(121)},spy(s,calls)));assert.deepEqual(calls,[]);assert.deepEqual(s.events,[]);
  assert.doesNotThrow(()=>P.authorize({...f.c,taskId:'x'.repeat(120)}));
});
for(const fault of ['foreignPin','changedPid','helperEOF','refused','timeout','cleanupFault']){
  test('operator '+fault+' stops, cleans owned children and never claims installed completion',async t=>{
    const f=setup(t),s=fakeServices(f,{[fault]:true});const r=await D.runOperator(f.c,s.services);
    assert.equal(r.status,'partial');assert.equal(r.installedAcceptance,'not-established');assert.equal(r.startAttempts,1);
    assert.ok(s.events.includes('helper-close'));assert.ok(s.events.includes('app-cleanup'));
    if(fault==='foreignPin')assert.equal(s.events.some(x=>x.startsWith('helper:')),false);
    if(fault==='changedPid')assert.equal(s.events.includes('helper:inspect'),false);
  });
}
test('stop without all actual artifact phases stays incomplete; EOF still cleans up',async t=>{
  for(const actions of [[{type:'inspect'},{type:'stop'}],[]]){
    const f=setup(t),s=fakeServices(f,{actions}),r=await D.runOperator(f.c,s.services);
    assert.equal(r.status,'partial');assert.equal(r.installedAcceptance,'not-established');assert.ok(s.events.includes('app-cleanup'));
  }
});
test('prepopulated valid output plus forged artifact phase labels never establishes installed acceptance',async t=>{
  const f=setup(t),s=fakeServices(f,{actions:['baseline','cancelled','exported','after-save'].map(phase=>({type:'artifacts',phase}))});
  s.services.workspace=()=>{const w=f.prepare();fs.writeFileSync(w.saved,'same');fs.writeFileSync(w.after,'same');outputs(w);return w;};
  const r=await D.runOperator(f.c,s.services);assert.equal(r.status,'partial');assert.equal(r.reason,'png-ui-unwitnessed');
  assert.equal(r.installedAcceptance,'not-established');assert.equal(r.artifacts.length,0);assert.ok(s.events.includes('app-cleanup'));
});
test('wrong UI order and wrong real picker value cannot witness an export',t=>{
  const f=setup(t),w=f.prepare();assert.throws(()=>witness(f.c,'Save As…'),/ui-order/);
  witness(f.c,'Open Project…');witness(f.c,'Open','sheet',w.output);fs.writeFileSync(w.saved,'bytes');
  assert.throws(()=>P.artifacts(f.c,'baseline',{dirs:{appCache:f.root}}),/ui-unwitnessed/);
});
test('sanitized feedback drops private titles, values, filenames, paths and generic A/B controls',()=>{
  const r={status:'ok',pid:42,windowId:3,matchedAX:true,type:'inspect',privatePath:'/private/secret',value:'private',
    nodes:[{path:'0',role:'AXButton',control:'Export',press:true,title:'private-title',value:'/private/file'},
      {path:'1',role:'AXButton',control:'A'},{path:'2',role:'AXRow',control:'A.json'},{path:'3',role:'unknown',control:'Delete'}],capture:'/private/capture.png'};
  const clean=P.sanitizeHelper(r),text=JSON.stringify(clean);assert.equal(clean.nodes[0].control,'Export');
  assert.equal(clean.nodes[1].control,undefined);assert.equal(clean.nodes[2].control,undefined);
  assert.equal(clean.nodes[3].control,undefined);assert.equal(/private|title|value|Delete/.test(text),false);
  assert.equal(P.sanitizeHelper(r,'sheet').nodes[2].control,'A.json');
});
test('complete witnessed controller protocol still leaves installed acceptance for independent owner review',async t=>{
  for(const terminalSaveFault of [false,true]){
  const f=setup(t),w={fixture:path.join(f.c.reportRoot,'A.json'),saved:path.join(f.c.reportRoot,'Saved.json'),
    after:path.join(f.c.reportRoot,'SavedAfter.json'),output:path.join(f.c.reportRoot,'output')};
  const token='11111111-1111-4111-8111-111111111111';let main='open',dialog=null,value=null,cancelled=false;
  const actions=[];
  const inspect=target=>actions.push({type:'inspect',...(target?{target,sheetToken:token}:{})});
  const press=(element,target)=>actions.push({type:'press',element:String(element),...(target?{target,sheetToken:token}:{})});
  function picker(pathname){actions.push({type:'pin-sheet'});inspect('sheet');actions.push({type:'set',target:'sheet',sheetToken:token,element:'0',value:pathname});inspect('sheet');press(1,'sheet');actions.push({type:'sheet-dismissed',sheetToken:token});}
  inspect();press(0);picker(w.fixture);
  inspect();press(0);picker(w.saved);actions.push({type:'artifacts',phase:'baseline'});
  inspect();press(0);inspect();press(0);actions.push({type:'artifacts',phase:'cancelled'});
  inspect();press(0);picker(w.output);actions.push({type:'artifacts',phase:'exported'});
  inspect();press(0);picker(w.after);actions.push({type:'artifacts',phase:'after-save'},{type:'stop'});
  const s=fakeServices(f,{actions});
  s.services.helper=()=>({async pin(){return {status:'pinned',pid:42,windowId:3,matchedAX:true};},async close(){s.events.push('helper-close');},
    async action(_window,a){
      const r={status:'ok',type:a.type,pid:42,windowId:3,matchedAX:true};
      const node=(path,control,role='AXButton')=>({path,control,role,press:true,enabled:true});
      if(a.type==='inspect'){
        Object.assign(r,complete);
        if(a.target==='sheet')r.nodes=[node('0',undefined,'AXTextField'),node('1',dialog==='open'?'Open':dialog==='save'?'Save':'Choose')];
        else r.nodes=main==='open'?[node('0','Open Project…')]:main==='save'?[node('0','Save As…')]:main==='cancel'?[node('0','Cancel')]:
          [node('0','Export'),...(cancelled?[node('1','PNG','AXPopUpButton'),node('2','Whole timeline','AXPopUpButton'),node('3','1x','AXPopUpButton')]:[])];
      }
      if(a.type==='pin-sheet'){r.target='sheet';r.sheetToken=token;}
      if(a.target==='sheet'){r.target='sheet';r.sheetToken=token;}
      if(a.type==='set')value=a.value;
      if(a.type==='press'&&a.target!=='sheet'){
        assert.equal(a.expectedRole,'AXButton');assert.ok(P.controls.has(a.expectedControl));
        if(main==='open')dialog='open';else if(main==='save')dialog='save';else if(main==='cancel'){cancelled=true;main='export';}else if(!cancelled)main='cancel';else dialog='export';
      }
      if(a.type==='press'&&a.target==='sheet'){
        if(dialog==='save')fs.writeFileSync(value,'unchanged native saved bytes');
        if(dialog==='export')outputs(w);
      }
      if(a.type==='sheet-dismissed'){r.dismissed=true;main=dialog==='open'?'save':dialog==='save'?'export':'save';dialog=null;}
      return r;
    }});
  const originalSave=s.services.save;
  s.services.save=r=>{if(terminalSaveFault&&s.events.includes('app-cleanup')){s.events.push('terminal-save-failed');throw Error('/private/evidence-write-failed');}originalSave(r);};
  const r=await D.runOperator(f.c,s.services);assert.equal(r.status,terminalSaveFault?'partial':'operator-complete',r.reason);
  assert.equal(r.inputCleanup,'complete');assert.ok(s.events.includes('input-close'));
  if(terminalSaveFault){assert.equal(r.reason,'png-terminal-save-refused');assert.equal(r.evidenceSave,'incomplete');
    assert.ok(s.events.indexOf('input-close')<s.events.indexOf('terminal-save-failed'));}
  assert.equal(r.installedAcceptance,'not-established');assert.equal(r.ownerReview,'required');
  assert.equal(r.orderedUiWitnesses.step,5);assert.ok(r.actions.some(x=>x.control==='Open Project…'));
  assert.ok(r.actions.some(x=>x.control==='Choose'&&x.target==='sheet'));
  assert.equal(JSON.stringify(r).includes(f.root),false);assert.ok(s.events.includes('app-cleanup'));
  }
});
test('terminal save and input close failures retain the first refusal and truthful owned cleanup',async t=>{
  for(const fault of ['save','close']){
    const f=setup(t),s=fakeServices(f,{foreignPin:true}),originalSave=s.services.save;
    s.services.save=r=>{if(fault==='save'&&s.events.includes('app-cleanup'))throw Error('private-save-failed');originalSave(r);};
    s.services.close=async()=>{s.events.push('input-close');if(fault==='close')throw Error('private-close-failed');};
    const r=await D.runOperator(f.c,s.services);assert.equal(r.status,'partial');assert.equal(r.reason,'png-window-refused');
    assert.equal(r.cleanup,'complete');assert.equal(r.helperCleanup,'complete');assert.ok(s.events.includes('input-close'));
    assert.equal(r.inputCleanup,fault==='close'?'incomplete':'complete');assert.equal(JSON.stringify(r).includes('private-'),false);
    if(fault==='save')assert.equal(r.evidenceSave,'incomplete');else assert.equal(s.saved.at(-1).inputCleanup,'incomplete');
  }
});
test('actual Swift scope mapping preserves legacy gates and requires a distinct PNG prefix',()=>{
  if(process.platform!=='darwin')return;
  const source=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-window.swift'),'utf8');
  const begin=source.indexOf('func operatorScope('),end=source.indexOf('\nfunc sheetProof(',begin);
  assert.ok(begin>=0&&end>begin);
  const code='import Foundation\nstruct Refusal: Error { let code:String };func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}\n'+source.slice(begin,end)+`
for (mode,prefix,extended) in [("--persistent","NEMO_P38_",false),("--reference-persistent","NEMO_REFERENCE_",true),("--png-persistent","NEMO_PNG_UI_",true)] {
  let result=try operatorScope(mode,[prefix+"GRANT":"new-grant",prefix+"REPORT_ROOT":"/owned"])
  precondition(result.reference == extended && result.report == "/owned")
  for wrong in ["NEMO_P38_","NEMO_REFERENCE_","NEMO_PNG_UI_"] where wrong != prefix {
    var denied=false;do{_=try operatorScope(mode,[wrong+"GRANT":"old",wrong+"REPORT_ROOT":"/owned"])}catch{denied=true};precondition(denied)
  }
  for fields in [[:],[prefix+"REPORT_ROOT":"/owned"],[prefix+"GRANT":"new"]] {
    var denied=false;do{_=try operatorScope(mode,fields)}catch{denied=true};precondition(denied)
  }
}
print("three actual source modes, distinct grants; runtimeAX=false")`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:8192});assert.equal(r.status,0,r.stderr);assert.match(r.stdout,/distinct grants/);
});
test('compiled actual Swift inspect gates service reads on retained tokens, process, ancestry and owner proofs',()=>{
  if(process.platform!=='darwin')return;
  const source=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-window.swift'),'utf8');
  function slice(begin,end){const a=source.indexOf(begin),b=source.indexOf(end,a);assert.ok(a>=0&&b>a);return source.slice(a,b);}
  const gates=slice('    let target = request["target"]','    switch action {');
  const inspect=slice('    case "inspect":','    case "capture":').replace('    case "inspect":','');
  const definitions=slice('struct ProcessStart:','func processStart(')+slice('func localFacts(','// A path is diagnostic')+
    slice('func boundedOwnedTree<','func guardedElementAction(')+
    fs.readFileSync(path.join(__dirname,'desktop/p38-owned-projection.swift'),'utf8').replace(/^import .*$/gm,'');
  const verify=slice('  func verifyService()','  try emit(["status":"pinned"');
  const node=slice('  func node(_ value:','  func supports(');
  const code=`import Foundation
import CoreFoundation
typealias AXUIElement = Int
enum AXError: Int32 { case success = 0, attributeUnsupported = -1, noValue = -2, cannotComplete = -3 }
struct DarwinBoolean { var boolValue: Bool; init(_ value:Bool){boolValue=value} }
struct Refusal: Error { let code:String }
struct ActionRefusal: Error { let diagnostics:[String:Any] }
func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}
func locationCheck(_ ok:Bool,_ stage:String,_ reason:String?=nil)throws{try check(ok)}
func locationPredicate(_ ok:Bool,_ reason:String,_ facts:[String:Any]=[:])throws{try check(ok)}
func projectionLimit(_ depth:Int,_ nodes:Int)throws{try check(depth>0 && nodes>0)}
func requireResolvedSheetMutation(_ target:String,_ action:String,_ service:Bool,_ token:Bool)throws{}
func setInspectionContext(_ stage:String,_ reason:String){}
func CFEqual(_ a:Int,_ b:Int)->Bool{a==b}
func CFHash(_ value:Int)->UInt{UInt(value)}
func route(_ raw:String)throws->[Int]{raw.isEmpty ? [] : try raw.split(separator:".").map{guard let i=Int($0)else{throw Refusal(code:"route")};return i}}
let kAXRoleAttribute="role",kAXTitleAttribute="title",kAXValueAttribute="value",kAXChildrenAttribute="children",kAXDescriptionAttribute="description"
let kAXSelectedAttribute="selected",kAXEnabledAttribute="enabled",kAXSubroleAttribute="subrole",kAXFocusedAttribute="focused"
let kAXWindowRole="AXWindow",kAXSheetRole="AXSheet",kAXGroupRole="AXGroup",kAXDialogSubrole="AXDialog"
let kAXTextFieldRole="AXTextField",kAXPopUpButtonRole="AXPopUpButton",kAXPressAction="press",kAXConfirmAction="confirm"
${definitions}
func exercise(_ fault:String,_ requestTarget:String="sheet",_ token:String?="location")throws->([String:Any],Int){
  let pid:pid_t=10,main=1,action="inspect",scope=(reference:true,report:"owned")
  let sheetToken:String?="sheet",locationToken:String?=requestTarget == "main" || ["ordinary-sheet","foreign-sheet-child"].contains(fault) || fault.hasPrefix("fact-owner-") ? nil:"location"
  let serviceRoot:Int?=3,serviceParent:Int?=2,servicePID:pid_t?=fault == "missing-service" ? nil:20
  let serviceStart:ProcessStart?=ProcessStart(pid:20,seconds:1,microseconds:0),serviceSheetToken:String?="sheet"
  let location:Int?=4,locationField:Int?=5
  var activeInspectionContext:Int?=nil,result=[String:Any](),reads=0,foreignReads=0,locationChecks=0
  var projectionCursor:OwnedProjectionCursor<Int>?=nil
  var ownerCalls=[Int:Int]()
  var request:[String:Any]=["target":requestTarget,"sheetToken":"sheet","depth":2,"maxNodes":32]
  if let token,locationToken != nil{request["locationToken"]=token}
  if fault == "wrong-sheet-token"{request["sheetToken"]="wrong"}
  let owners:[Int:pid_t]=[1:10,2:10,3:20,4:20,5:fault == "foreign-field" ? 99:20,6:fault == "foreign-child" ? 99:20,
    7:fault == "foreign-main-child" ? 20:10,8:fault == "foreign-sheet-child" ? 20:10]
  func AXUIElementGetPid(_ n:Int,_ out:inout pid_t)->AXError{
    ownerCalls[n,default:0]+=1
    if n==8 && ownerCalls[n]==2 && fault.hasPrefix("fact-owner-") {
      if fault=="fact-owner-failed"{return .cannotComplete};out=99;return .success
    };out=owners[n] ?? 0;return .success
  }
  func attr(_ n:Int,_ key:String)->AnyObject?{
    if owners[n] != (n<=2 || n>=7 ? pid:20){foreignReads+=1;throwFatal()};reads+=1
    if key==kAXChildrenAttribute{return (n==1 ? [7]:n==2 ? [8]:n==3 ? [4]:n==4 ? ([5]+(["ambiguous-modal","foreign-child"].contains(fault) ? [6]:[])):[]) as NSArray}
    if key==kAXRoleAttribute{return (n==1 ? "AXWindow":n==2 || n==4 || n==6 ? "AXSheet":"AXTextField") as NSString}
    if key==kAXFocusedAttribute || key==kAXEnabledAttribute{return NSNumber(value:n==5 || n>=7)}
    if key==kAXTitleAttribute || key==kAXValueAttribute{return "private-secret-path" as NSString}
    if key==kAXDescriptionAttribute{
      if fault == "nontext-description"{return ["private-secret-description"] as NSArray}
      return (fault == "public-description" && n>=7 ? "Open Project…":"private-secret-description") as NSString
    };return nil
  }
  func throwFatal(){preconditionFailure("foreign attribute read")}
  func children(_ n:Int)->[Int]{attr(n,kAXChildrenAttribute) as? [Int] ?? []}
  func owns(_ n:Int,_ expected:pid_t)->Bool{owners[n]==expected}
  func supports(_ n:Int,_ action:String)->Bool{true}
  func textFieldKind(_ error:AXError,_ role:String?)->String{"plain"}
  func AXUIElementCopyAttributeValue(_ n:Int,_ key:CFString,_ out:inout CFTypeRef?)->AXError{
    if key as String==kAXChildrenAttribute {out=attr(n,kAXChildrenAttribute);return .success};out=nil;return .attributeUnsupported
  }
  func AXUIElementIsAttributeSettable(_ n:Int,_ key:CFString,_ out:inout DarwinBoolean)->AXError{out=DarwinBoolean(true);return .success}
  func verify()throws{try check(fault != "stale-nemo")}
  func verifySheet()throws->Int{try check(fault != "detached-sheet");return 2}
  func noSheet(inspectMain:Bool)throws{}
  func processStart(_ n:pid_t)->ProcessStart?{ProcessStart(pid:n,seconds:fault == "stale-service" || fault == "stale-after-project" && locationChecks>=3 ? 2:1,microseconds:0)}
  func authenticatedPanelService(_ n:pid_t)->Bool{fault != "unauthenticated-service"}
  func discoverServiceRoot(_ outer:Int)throws->(Int,Int,pid_t,ProcessStart){locationChecks+=1;return (3,fault == "detached-parent" ? 9:2,20,ProcessStart(pid:20,seconds:1,microseconds:0))}
  func serviceOwnerTree(_ root:Int,_ expected:pid_t)throws->[Int]{
    try boundedOwnedTree(root,owner:Int(expected),deadline:10,now:{1},equal:==,
      owner:{(0,owners[$0]==expected,Int(owners[$0] ?? 0))},children:{(0,children($0))},parent:{(0,$0==4 ? 3:4)})
  }
  ${node}
  ${verify}
  ${gates}
  ${inspect}
  try check(foreignReads==0);return (result,reads)
}
let good=try exercise("none");let nodes=good.0["nodes"] as! [[String:Any]]
precondition(nodes.contains{$0["role"] as? String=="AXTextField"} && !String(describing:good.0).contains("private-secret"))
for fault in ["wrong-sheet-token","missing-service","stale-nemo","stale-service","unauthenticated-service","detached-sheet","detached-parent","ambiguous-modal","foreign-field","foreign-child","stale-after-project"]{
  var refused=false;do{_=try exercise(fault)}catch{refused=true};precondition(refused,fault)
}
for token in [nil,"wrong"] as [String?]{var refused=false;do{_=try exercise("none","sheet",token)}catch{refused=true};precondition(refused)}
for (fault,target) in [("none","main"),("ordinary-sheet","sheet"),("foreign-main-child","main"),("foreign-sheet-child","sheet")]{
  let observation=try exercise(fault,target);let rows=observation.0["nodes"] as! [[String:Any]]
  precondition(rows.contains{fault.hasPrefix("foreign") ? $0["blocked"] as? String=="child-owner":$0["role"] as? String=="AXTextField"},fault)
}
for fault in ["public-description","private-description","nontext-description"]{
  let observation=try exercise(fault,"main"),rows=observation.0["nodes"] as! [[String:Any]]
  precondition(rows.contains{$0["control"] as? String=="Open Project…"} == (fault == "public-description"))
  precondition(!String(describing:observation.0).contains("private-secret"))
}
for fault in ["fact-owner-failed","fact-owner-mismatch"]{
  let result=try exercise(fault,"sheet").0,row=(result["nodes"] as! [[String:Any]]).last!
  precondition(result["foreignChildren"] as? Bool==true && result["complete"] as? Bool==false)
  precondition(row["ownerQuery"] as? String==(fault=="fact-owner-failed" ? "query-failed":"mismatch"))
  precondition(row["blocked"] as? String=="child-owner" && row["control"]==nil && row["childrenQuery"]==nil)
}
print("actual token dispatch, retained verification and inspect projection exercised; runtimeAX=false")`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:16384});
  const failedLine=Number(r.stderr.match(/-e:(\d+):/)?.[1]);
  assert.equal(r.status,0,(failedLine?code.split('\n').slice(failedLine-2,failedLine+1).join('\n')+'\n':'')+r.stderr);
  assert.match(r.stdout,/actual token dispatch/);
  const base=spawnSync('git',['show','d8b7a4fbd2ca056fc739aa9ae2cde4796e03b645:tests/desktop/p38-owned-window.swift'],
    {cwd:path.join(__dirname,'..'),encoding:'utf8',timeout:10000,maxBuffer:256000});assert.equal(base.status,0,base.stderr);
  const begin=base.stdout.indexOf('    case "inspect":'),end=base.stdout.indexOf('    case "capture":',begin);
  assert.ok(begin>=0&&end>begin);const oldInspect=base.stdout.slice(begin,end).replace('    case "inspect":','');
  const legacy=code.slice(0,code.indexOf('let good=try exercise')).replace(inspect,oldInspect)+`
let old=try exercise("none");let oldRows=old.0["nodes"] as! [[String:Any]]
precondition(oldRows.count==1 && oldRows[0]["blocked"] as? String=="child-owner")
print("baseline refuses authenticated service root before exposing its field; runtimeAX=false")`;
  const red=spawnSync('/usr/bin/swift',['-e',legacy],{encoding:'utf8',timeout:30000,maxBuffer:16384});
  assert.equal(red.status,0,red.stderr);assert.match(red.stdout,/baseline refuses/);
});

test('PNG input refuses incomplete or ambiguous discovery until a complete selected-root inspection',t=>{
  const f=setup(t),w=f.prepare();
  const node={path:'0.9',role:'AXButton',control:'Open Project…',press:true,enabled:true};
  const good={...complete,target:'main',nodes:[node]},press={type:'press',element:node.path};
  P.verifyAction(f.c,press,{},good);
  for(const patch of [{complete:false},{complete:undefined},{truncated:true},{depthClipped:true},
    {childrenUnreadable:true},{foreignChildren:true},{truncated:undefined},{depthClipped:undefined},
    {childrenUnreadable:undefined},{foreignChildren:undefined},{nodes:[node,node]}])
    assert.throws(()=>P.verifyAction(f.c,press,{}, {...good,...patch}),/control-refused/);
  const sheet={token:'sheet'},field={path:'0',role:'AXTextField',enabled:true};
  const select={path:'1',role:'AXRow',control:'A.json',enabled:true};
  for(const a of [{type:'set',element:'0',value:w.fixture},{type:'select',element:'1'}]){
    const request={...a,target:'sheet',sheetToken:'sheet'},last={...complete,target:'sheet',nodes:[field,select]};
    P.verifyAction(f.c,request,sheet,last);
    for(const key of ['truncated','depthClipped','childrenUnreadable','foreignChildren'])
      assert.throws(()=>P.verifyAction(f.c,request,sheet,{...last,[key]:true}),/control-refused/);
    assert.throws(()=>P.verifyAction(f.c,request,sheet,{...last,complete:false}),/control-refused/);
  }
});
test('incomplete discovery stops the real operator before any press and cleans up',async t=>{
  for(const patch of [{complete:false},{truncated:true},{depthClipped:true},{childrenUnreadable:true},{foreignChildren:true}]){
    const f=setup(t),s=fakeServices(f,{actions:[{type:'inspect'},{type:'press',element:'0'}]});
    s.services.helper=()=>({async pin(){return {status:'pinned',pid:42,windowId:3,matchedAX:true};},
      async close(){s.events.push('helper-close');},async action(_w,a){s.events.push('helper:'+a.type);
        return {status:'ok',type:a.type,pid:42,windowId:3,matchedAX:true,...complete,...patch,
          nodes:[{path:'0',role:'AXButton',control:'Open Project…',press:true,enabled:true}]};}});
    const r=await D.runOperator(f.c,s.services);assert.equal(r.reason,'png-control-refused');
    assert.equal(s.events.includes('helper:press'),false);assert.equal(r.cleanup,'complete');assert.equal(r.helperCleanup,'complete');
  }
});
test('oversized or malformed helper inspections never become complete input routes',async t=>{
  const node={path:'0',role:'AXButton',control:'Open Project…',press:true,enabled:true};
  for(const nodes of [Array.from({length:129},(_,i)=>({...node,path:String(i)})),undefined,null,{},[null],[[]],[{...node,path:'private'}]]){
    const reply={status:'ok',type:'inspect',pid:42,windowId:3,matchedAX:true,...complete,nodes};
    assert.throws(()=>P.sanitizeHelper(reply),/projection-refused/);
    const f=setup(t),s=fakeServices(f,{actions:[{type:'inspect'},{type:'press',element:'0'}]});
    s.services.helper=()=>({async pin(){return {status:'pinned',pid:42,windowId:3,matchedAX:true};},
      async close(){s.events.push('helper-close');},async action(_w,a){s.events.push('helper:'+a.type);return {...reply,type:a.type};}});
    const r=await D.runOperator(f.c,s.services);assert.equal(r.reason,'png-projection-refused');
    assert.equal(s.events.includes('helper:press'),false);assert.equal(r.cleanup,'complete');assert.equal(r.helperCleanup,'complete');
  }
});
test('query diagnostics stay categorical, reject contradictory metadata and never authorize blocked input',async t=>{
  const observation=node=>({status:'ok',type:'inspect',pid:42,windowId:3,matchedAX:true,...complete,complete:false,
    foreignChildren:node.blocked==='child-owner',childrenUnreadable:node.childrenQuery!==undefined,nodes:[node]});
  const foreign=query=>({path:'0',role:'other',blocked:'child-owner',ownerQuery:query});
  const unreadable=query=>({path:'0',role:'AXButton',control:'Open Project…',enabled:true,press:true,
    ownerQuery:'matched',childrenQuery:query,blocked:'children-unavailable'});
  const samples=[...['mismatch','query-failed'].map(foreign),...['unsupported','no-value','failed','wrong-type'].map(unreadable)];
  for(const node of samples){
    const clean=P.sanitizeHelper({...observation(node),privatePath:'/private/secret',nodes:[{...node,title:'private-secret',ownerPid:999,rawQuery:-99}]});
    assert.equal(clean.nodes[0].ownerQuery,node.ownerQuery);assert.equal(clean.nodes[0].childrenQuery,node.childrenQuery);
    assert.equal(JSON.stringify(clean).includes('private'),false);assert.equal(JSON.stringify(clean).includes('999'),false);
    const f=setup(t),s=fakeServices(f,{actions:[{type:'inspect'},{type:'press',element:'0'}]});
    s.services.helper=()=>({async pin(){return {status:'pinned',pid:42,windowId:3,matchedAX:true};},
      async close(){s.events.push('helper-close');},async action(_w,a){s.events.push('helper:'+a.type);return observation(node);}});
    const r=await D.runOperator(f.c,s.services);assert.equal(r.reason,'png-control-refused');
    assert.equal(s.events.includes('helper:press'),false);assert.equal(r.cleanup,'complete');assert.equal(r.helperCleanup,'complete');
  }
  for(const [query,count] of [['success-empty',0],['success',1]]){
    const row={path:'0',role:'AXButton',control:'Open Project…',press:true,ownerQuery:'matched',childrenQuery:query,childCount:count};
    assert.equal(P.sanitizeHelper({...complete,type:'inspect',nodes:[row]}).nodes[0].childrenQuery,query);
    for(const bad of [-1,4096,0.5,'0',null])assert.throws(()=>P.sanitizeHelper({...complete,type:'inspect',nodes:[{...row,childCount:bad}]}),/png-query-refused/);
    assert.throws(()=>P.sanitizeHelper({...complete,type:'inspect',nodes:[{...row,childCount:count===0?1:0}]}),/png-query-refused/);
  }
  for(const key of ['ownerQuery','childrenQuery'])for(const value of ['private-secret',true,1,null,{},[]]){
    assert.throws(()=>P.sanitizeHelper(observation({...unreadable('failed'),[key]:value})),/png-query-refused/);
  }
  for(const node of [{...foreign('mismatch'),control:'Open'}, {...foreign('query-failed'),press:true},
    {...foreign('mismatch'),childrenQuery:'failed'}, {...foreign('mismatch'),ownerQuery:'matched'},
    {...unreadable('unsupported'),childCount:0}, {...unreadable('failed'),ownerQuery:undefined}]){
    assert.throws(()=>P.sanitizeHelper(observation(node)),/png-query-refused/);
  }
  for(const node of samples)assert.throws(()=>P.sanitizeHelper({...observation(node),...complete}),/png-query-refused/);
});
test('PNG pages require observed roots and exact retained continuation; metadata stays categorical',t=>{
  const f=setup(t);f.prepare();const token='11111111-1111-4111-8111-111111111111',node={path:'0.0',role:'AXWebArea'};
  const first={target:'main',nodes:[node]};
  P.verifyAction(f.c,{type:'inspect',element:'0.0',depth:1,maxNodes:128,childOffset:0},{},first);
  assert.throws(()=>P.verifyAction(f.c,{type:'inspect',element:'0.9',depth:1,childOffset:0},{},first));
  const page={target:'main',pageToken:token,childOffset:0,nextChildOffset:127,childCount:300,childrenComplete:false,nodes:[node]};
  P.verifyAction(f.c,{type:'inspect',element:'0.0',depth:1,childOffset:127,pageToken:token},{},page);
  for(const edit of [{childOffset:true},{childOffset:-1},{childOffset:4096},{childOffset:1.5},{childOffset:'127'},
    {childOffset:128},{pageToken:'22222222-2222-4222-8222-222222222222'},{element:'0.8'},{depth:2},{target:'sheet'}])
    assert.throws(()=>P.verifyAction(f.c,{type:'inspect',element:'0.0',depth:1,childOffset:127,pageToken:token,...edit},{},page));
  assert.throws(()=>P.verifyAction(f.c,{type:'inspect',element:'0.0',depth:1,childOffset:127,pageToken:token},{},{...page,foreignChildren:true}));
  const clean=P.sanitizeHelper({status:'ok',type:'inspect',pid:42,windowId:3,matchedAX:true,...page,
    complete:false,depthClipped:true,truncated:true,childrenUnreadable:false,foreignChildren:false,private:'/private/secret'});
  assert.equal(clean.childCount,300);assert.equal(clean.depthClipped,true);assert.equal(clean.pageToken,token);
  assert.equal(JSON.stringify(clean).includes('private'),false);
  const candidate={path:'0.0.299',role:'AXButton',control:'Open Project…',press:true,enabled:true};
  assert.throws(()=>P.verifyAction(f.c,{type:'press',element:candidate.path},{},
    {...complete,...page,nodes:[node,candidate],complete:false,truncated:true}),/control-refused/);
  // Discovery can continue, but input waits for an unpaged, complete target root.
  P.verifyAction(f.c,{type:'inspect',element:candidate.path,depth:1},{},clean);
  P.verifyAction(f.c,{type:'press',element:candidate.path},{}, {...complete,target:'main',nodes:[candidate]});
  for(const bad of [{childCount:'/private'},{nextChildOffset:4096},{complete:'yes'},{depthClipped:{}},{pageToken:'private-token'}])
    assert.throws(()=>P.sanitizeHelper({status:'ok',type:'inspect',...bad}));
});
test('PNG no-value emptiness requires correlated matched-owner zero-count evidence',()=>{
  const identity={status:'ok',type:'inspect',pid:42,windowId:3,matchedAX:true,
    complete:true,truncated:false,depthClipped:false,childrenUnreadable:false,foreignChildren:false};
  const node={path:'0',role:'AXButton',ownerQuery:'matched',childrenQuery:'no-value',childCount:0,
    childrenCountQuery:'success-zero',childrenWitness:'empty-count-confirmed',press:true};
  const clean=P.sanitizeHelper({...identity,nodes:[node]});
  assert.equal(clean.nodes[0].childrenQuery,'no-value');
  assert.equal(clean.nodes[0].childrenWitness,'empty-count-confirmed');
  for(const change of [n=>delete n.childrenWitness,n=>delete n.childrenCountQuery,n=>n.childCount=1,
    n=>n.childrenCountQuery='success-positive',n=>n.childrenCountQuery='failed',n=>n.childrenQuery='unsupported',
    n=>n.ownerQuery='mismatch',n=>n.blocked='children-unavailable',n=>n.childrenWitness='private-secret']){
    const bad=structuredClone(node);change(bad);
    assert.throws(()=>P.sanitizeHelper({...identity,nodes:[bad]}),/png-query-refused/);
  }
  for(const outcome of ['success-positive','unsupported','no-value','failed','invalid']){
    const unreadable={...node,childrenCountQuery:outcome};delete unreadable.childCount;delete unreadable.childrenWitness;
    const negative=P.sanitizeHelper({...identity,complete:false,childrenUnreadable:true,nodes:[unreadable]});
    assert.equal(negative.complete,false);assert.equal(negative.nodes[0].childCount,undefined);
  }
});
