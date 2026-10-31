'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const test = require('node:test');
const vm = require('node:vm');
const source = fs.readFileSync('src/js/project.js', 'utf8');
const A = '{"project":"A","paint":[20,80]}', B = '{"project":"B","paint":[40,60]}';
const EDITED_A = '{"project":"A","paint":[25,85],"edited":true}';

// Runs the complete shipped project entry and its real tab DOM listeners.
// Native modes are introduced only after disposable browser A/B setup;
// this is a source fixture, never a production native-tab setup mechanism.
function fixture({ populated = true, nativePorts = null, nativeFileAdapter = null } = {}) {
  const effects = [], notices = [], elements = new Map();
  let bytes = A;
  function element() {
    const classes = new Set();
    const el = { children: [], style: {}, dataset: {}, listeners: {}, value: '', className: '', attributes: {},
      addEventListener(type, callback) { this.listeners[type] = callback; },
      appendChild(child) { this.children.push(child); child.parent = this; return child; },
      setAttribute(name, value) { this.attributes[name] = String(value); },
      replaceWith(child) { child.parent = this.parent; this.parent.children[this.parent.children.indexOf(this)] = child; },
      focus() {}, select() {}, blur() { this.listeners.blur?.(); },
      getAttribute(name) { return this.attributes[name]; }, removeChild() {}, click() { this.listeners.click?.(); } };
    el.classList = { add: name => classes.add(name), remove: name => classes.delete(name),
      contains: name => classes.has(name) || el.className.split(' ').includes(name),
      toggle(name, force) { if (force === false || force === undefined && classes.has(name)) classes.delete(name); else classes.add(name); } };
    Object.defineProperty(el, 'innerHTML', { get: () => '', set() { el.children = []; } });
    return el;
  }
  const document = { readyState: 'loading', body: element(),
    addEventListener(type, callback) { if (type === 'DOMContentLoaded') this.ready = callback; },
    getElementById(id) { if (!elements.has(id)) elements.set(id, element()); return elements.get(id); },
    createElement: element, querySelector(selector) {
      const match = selector.match(/data-tab="([^"]+)"/);
      const row = match ? elements.get('project-tabs-list')?.children.find(row => row.dataset.tab === match[1]) : null;
      return selector.endsWith(' .pt-name') ? row?.children.find(child => child.className === 'pt-name') : row;
    } };
  const root = { document, location: { protocol: 'http:', hostname: 'localhost', port: '1420' },
    addEventListener() {}, requestAnimationFrame(callback) { callback(); },
    NemoNativeObjectInactiveProjectTabs: require('../src/js/adapters/native-object-inactive-project-tabs.js'),
    NemoNativeObjectProjectTabsUI: require('../src/js/adapters/native-object-project-tabs-ui.js'),
    NemoNativeOpacityProjectEntry: require('../src/js/adapters/native-opacity-project-entry.js'),
    NemoNativeOpacityLegacySurface: require('../src/js/adapters/native-opacity-legacy-surface.js'),
    NemoProjectNativeSave: require('../src/js/adapters/project-native-save.js'),
    NemoProjectDirtyBaseline: require('../src/js/adapters/project-dirty-baseline.js'),
    NemoProjectNativeVersionRestore: require('../src/js/adapters/project-native-version-restore.js'),
    SM: { t: key => key, fitCanvas() {}, exportJSON() { effects.push('Paper export'); return bytes; },
      importJSON(json) { effects.push('browser import'); bytes = JSON.stringify({ ...JSON.parse(json), normalized: true }); return true; } } };
  if(nativeFileAdapter)root.NemoNativeObjectProjectFile=nativeFileAdapter;
  const noop = () => {};
  const sandbox = { window: root, document, SM: root.SM, state: {}, userLayers: [], _symbolPaperLayers: {},
    localStorage: { getItem: () => null, setItem() { effects.push('storage'); }, removeItem() {} },
    showToast(message) { notices.push(message); }, saveAllLayerFrames() { effects.push('Paper frame save'); },
    createUserLayer() { effects.push('create layer'); }, activateUL: noop, drawStage: noop, loadFrame: noop,
    renderOS: noop, renderArcs: noop, updateUI: noop, renderSymbolTabs: noop, syncDocFields: noop, exitToScene: noop,
    view: { update: noop }, setTimeout, console,
    FileReader: class { readAsText(file) { this.onload({ target: { result: file.text } }); } } };
  if(nativePorts)nativePorts(root,sandbox,effects);
  vm.runInNewContext(fs.readFileSync('src/js/project-document.js', 'utf8'), sandbox);
  vm.runInNewContext(fs.readFileSync('src/js/project-entry.js', 'utf8'), sandbox);
  sandbox.SMProjectEntry = root.SMProjectEntry;
  vm.runInNewContext(source, sandbox, { filename: 'project.js' });
  const beforeReady = { bytes, state: JSON.stringify(sandbox.state), effects: effects.slice() };
  document.ready();
  const afterReady = { bytes, state: JSON.stringify(sandbox.state), effects: effects.slice() };
  const list = () => document.getElementById('project-tabs-list').children;
  let a,b;
  if(!nativePorts){
    root.SMProject.newProject({ name: 'A', w: 320, h: 180, fps: 12 });
    a=list()[0].dataset.tab;
    document.getElementById('project-tab-add').listeners.click();
    b=list()[1].dataset.tab;
  }
  if (populated&&!nativePorts) {
    bytes = B; root.SMProject.markSaved();
    row(a).listeners.click();
    bytes = EDITED_A;
    root.SMProject.refreshActiveTabDirtyDot(EDITED_A);
  }
  function row(id) { return list().find(node => node.dataset.tab === id); }
  function state() {
    return { bytes, label: root.SMProject.getCurrentLabel(), open: root.SMProject.getOpenTabs(),
      rows: list().map(node => ({ id: node.dataset.tab, active: node.classList.contains('act'),
        name: node.children[2].textContent, dirty: node.classList.contains('dirty') })) };
  }
  effects.length = 0; notices.length = 0;
  return { root, effects, notices, document, sandbox, a, b, row, state, beforeReady, afterReady,
    close(id) { row(id).children[3].listeners.click({ stopPropagation() {} }); },
    switch(id) { row(id).listeners.click(); }, setBytes(value) { bytes = value; } };
}
function native(f) {
  const owner = { instanceId: 'native-i', documentId: 'native-A', generation: 3, revision: 11,
    snapshot: 'snapshot-A', frame: 7, undo: ['saved-A'], redo: [] };
  f.root.__TAURI__ = { core: { invoke() { f.effects.push('native invoke'); } } };
  f.root.NemoNativeOpacityCutover = { blocksLegacy: () => true, isActive: () => true,
    identity: () => ({ instanceId: owner.instanceId, documentId: owner.documentId, contentRevision: owner.revision }),
    persistenceJSON() { f.effects.push('native serialization'); return EDITED_A; } };
  f.root.NemoNativeOpacityProject = {
    async importJSON() { f.effects.push('native import'); owner.documentId = 'native-B'; owner.revision = 0; return { owner: 'native', status: 'presented' }; },
    finishOpenAfterReveal() { f.effects.push('native finalizer'); },
    release() { f.effects.push('native release'); return Promise.resolve({ owner: 'none', status: 'closed' }); } };
  return owner;
}
const modes = [
  ['active native', () => {}],
  ...['closed', 'replacing', 'release-requested', 'releasing', 'indeterminate'].map(name =>
    [name, f => { f.root.NemoNativeOpacityCutover.isActive = () => false; }]),
  ['missing cutover', f => { delete f.root.NemoNativeOpacityCutover; }],
  ['broken cutover', f => { f.root.NemoNativeOpacityCutover = {}; }],
  ['null cutover', f => { f.root.NemoNativeOpacityCutover = null; }],
  ['throwing cutover', f => { Object.defineProperty(f.root, 'NemoNativeOpacityCutover', { get() { throw Error('cutover inaccessible'); } }); }],
  ['desktop marker without native globals', f => { delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject; }],
  ['malformed desktop API', f => { f.root.__TAURI__ = {}; delete f.root.NemoNativeOpacityProject; }],
  ['explicit undefined desktop marker', f => { f.root.__TAURI__ = undefined; delete f.root.NemoNativeOpacityProject; }],
  ['packaged origin without API/globals', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject;
    f.root.location = { protocol: 'tauri:', hostname: 'localhost', port: '' };
  }],
  ['browser surface with published native owner', f => { delete f.root.__TAURI__; }],
  ['browser surface with only native cutover', f => { delete f.root.__TAURI__; delete f.root.NemoNativeOpacityProject; }],
  ['browser surface with broken cutover', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityProject; f.root.NemoNativeOpacityCutover = {};
  }],
  ['browser surface with throwing cutover phase', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityProject;
    f.root.NemoNativeOpacityCutover.blocksLegacy = () => { throw Error('phase unavailable'); };
  }],
  ['malformed browser location', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject;
    f.root.location.hostname = 123;
  }],
  ['unknown location', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject; delete f.root.location;
  }],
  ['inaccessible location', f => {
    delete f.root.__TAURI__; delete f.root.NemoNativeOpacityCutover; delete f.root.NemoNativeOpacityProject;
    Object.defineProperty(f.root, 'location', { get() { throw Error('location inaccessible'); } });
  }],
];
for (const [name, change] of modes) {
  for (const action of ['switch', 'active close']) {
    test(`real ${action} refuses ${name} before every document/tab effect`, async () => {
      const f = fixture(), owner = native(f); change(f);
      const before = JSON.stringify({ owner, state: f.state() });
      if (action === 'switch') f.switch(f.b); else f.close(f.a);
      const alert = f.document.getElementById('project-tabs-bar').children.find(node => node.getAttribute('role') === 'alert');
      assert.ok(alert, 'real tab bar exposes the unavailable result');
      assert.match(alert.textContent, /unavailable/i);
      assert.equal(alert.style.display, 'block');
      assert.deepEqual(f.notices, [], 'availability alert does not add a success or duplicate toast');
      assert.deepEqual(f.effects, []);
      assert.equal(JSON.stringify({ owner, state: f.state() }), before);
      await Promise.resolve(); await new Promise(resolve => setImmediate(resolve));
      assert.deepEqual(f.effects, []);
      assert.equal(JSON.stringify({ owner, state: f.state() }), before);
    });
  }
}
for (const released of [false, true]) {
  test(`ordinary browser ${released ? 'legacy-phase' : 'without native marker'} preserves normalized switch/active close`, () => {
    const f = fixture();
    if (released) f.root.NemoNativeOpacityCutover = { blocksLegacy: () => false };
    f.switch(f.b);
    assert.equal(f.effects.filter(x => x === 'browser import').length, 1);
    assert.equal(f.state().rows.find(x => x.active).id, f.b);
    assert.equal(JSON.parse(f.state().bytes).project, 'B');
    assert.equal(JSON.parse(f.state().bytes).normalized, true);
    assert.equal(f.notices.length, 0);
    f.effects.length = 0;
    f.close(f.b);
    assert.equal(f.effects.filter(x => x === 'browser import').length, 1);
    assert.equal(f.state().rows.length, 1);
    assert.equal(f.state().rows[0].id, f.a);
    assert.equal(JSON.parse(f.state().bytes).project, 'A');
    assert.equal(f.root.SMProject.isDirty(), true, 'edited outgoing A remains dirty after normalized browser import');
  });
}
test('native same-tab click and inactive close retain their existing metadata-only behavior', () => {
  const f = fixture(), owner = native(f), before = JSON.stringify(owner);
  f.switch(f.a);
  assert.deepEqual(f.effects, []); assert.deepEqual(f.notices, []);
  f.close(f.b);
  assert.equal(f.state().rows.length, 1); assert.equal(f.state().rows[0].id, f.a);
  assert.equal(JSON.stringify(owner), before);
  assert.equal(f.effects.some(x => /import|release|frame save/.test(x)), false);
  f.effects.length = 0; f.close(f.a);
  assert.deepEqual(f.effects, []);
  assert.equal(f.notices.at(-1), 'hsCannotCloseLastTab');
});
test('ordinary browser new-tab callback retains existing creation behavior', () => {
  const f = fixture({ populated: false });
  f.document.getElementById('project-tab-add').listeners.click();
  assert.equal(f.state().rows.length, 3);
  assert.equal(f.state().rows.find(x => x.active).name, 'Untitled 3');
  assert.equal(f.effects.filter(x => x === 'create layer').length, 1);
  assert.equal(f.effects.filter(x => x === 'browser import').length, 0);
});

// Source UI fixture: A is admitted through ordinary Open, never browser-seeded tabs.
async function admittedDesktop(options={}) {
  const doc=require('../src/js/project-document.js');
  const original=JSON.parse(fs.readFileSync('tests/animation/fixtures/curve-workflow.json','utf8'));
  original.layers[0].motionStatic={opacity:[25]};
  const other=structuredClone(original);other.layers[0].motionStatic.opacity=[60];
  const files={'/A.json':JSON.stringify(original),'/B.json':JSON.stringify(other),'/bad.json':'invalid'};
  const token='60753e70-372f-47ad-913e-cfb7faf3ac59';
  const owner={active:false,instanceId:'native-instance-A',documentId:'native-document-A',revision:4,local:3,host:17,
    bytes:files['/A.json'],frame:0,undo:['A-saved'],redo:[]};
  const calls=[];let sequence=0;
  const f=fixture({nativePorts(root,sandbox,effects){
    sandbox.state={canvasW:320,canvasH:180,fps:24,totalFrames:21,currentFrame:0};root.crypto={randomUUID:()=>`ui-request-${++sequence}`};
    root.NemoNativeOpacityCutover={blocksLegacy:()=>owner.active,isActive:()=>owner.active,
      identity:()=>({instanceId:owner.instanceId,documentId:owner.documentId,contentRevision:owner.revision}),
      getNativeIdentity:()=>({documentId:owner.documentId,generation:owner.local}),persistenceJSON:()=>owner.bytes};
    root.NemoNativeOpacityProject={async importJSON(raw){effects.push('native import');doc.prepareNativeOpacity(raw);owner.bytes=raw;owner.active=true;
      return {owner:'native',status:'presented',frame:0,instanceId:owner.instanceId,documentId:owner.documentId,
        contentRevision:owner.revision,lifecycleGeneration:owner.local,documentSnapshotId:'snapshot-A'};},
      finishOpenAfterReveal(){effects.push('initial reveal');},release(){effects.push('native release');throw Error('must not release A');}};
    root.__TAURI__={fs:{async readTextFile(path){calls.push(['read',path]);if(options.read)return options.read(path,files);if(!(path in files))throw Error('missing');return files[path];}},
      dialog:{async open(){calls.push(['picker']);return options.picker?options.picker():'/B.json';}},
      window:{getCurrentWindow:()=>({async setFocus(){effects.push('focus');}})},
      core:{async invoke(name,args){calls.push([name,structuredClone(args)]);
        if(options.invoke){const changed=await options.invoke(name,args,owner);if(changed!==undefined)return changed;}
        if(name==='nemo_native_revision_sync')return {instanceId:owner.instanceId,documentId:owner.documentId,contentRevision:owner.revision,
          lifecycleGeneration:owner.host,subscriptionId:'existing-binding-observation'};
        const r=args.request;
        if(name==='nemo_native_prepare_inactive_project')return {apiVersion:2,requestId:r.requestId,preparationToken:token,state:'inactive-prepared',
          format:'nemo.native-opacity-document',formatVersion:1,layerCount:1,totalFrames:21,geometryCount:21};
        if(name==='nemo_native_cancel_inactive_project')return {apiVersion:2,requestId:r.requestId,preparationToken:r.preparationToken,state:'inactive-cancelled'};
        throw Error('unexpected command '+name);
      }}};
  }});
  await f.root.SMProject.openPath('/A.json');
  assert.equal(f.state().rows.length,1,'ordinary first Open must admit A and create its real row');
  assert.match(f.root.SMProject.getCurrentLabel(),/^A — \/A.json$/);
  assert.equal(f.root.SMProject.isDirty(),false);
  f.effects.length=0;calls.length=0;
  return Object.assign(f,{owner,calls,token,files,
    snapshot(){return JSON.stringify({owner:{...owner,bytes:require('node:crypto').createHash('sha256').update(owner.bytes).digest('hex')},label:f.root.SMProject.getCurrentLabel(),dirty:f.root.SMProject.isDirty(),
      active:f.state().rows.find(r=>r.active),open:f.root.SMProject.getOpenTabs(),frame:f.sandbox.state.currentFrame});},
    requests(name){return calls.filter(c=>c[0]===name);},
    editA(){owner.revision++;const edited=JSON.parse(owner.bytes);edited.layers[0].motionStatic.opacity=[40];owner.bytes=JSON.stringify(edited);owner.undo.push('edit-A');f.root.SMProject.refreshActiveTabDirtyDot(owner.bytes);}});
}
async function settleUntil(predicate) {
  for(let turn=0;turn<20&&!predicate();turn++)await new Promise(r=>setImmediate(r));
  assert.ok(predicate(),'source UI effect must settle within20 event-loop turns');
}
test('ordinary desktop toolbar Open appends opaque inactive B and preserves dirty native A',async()=>{
  const f=await admittedDesktop();f.editA();const before=f.snapshot();
  const result=await f.document.getElementById('btn-open').listeners.click();
  assert.equal(result.status,'prepared');assert.equal(f.snapshot(),before);
  const rows=f.state().rows;assert.equal(rows.length,2);const b=rows.find(r=>!r.active);assert.equal(b.name,'B');assert.equal(b.dirty,false);
  assert.equal(f.root.SMProject.getOpenTabs().length,0,'no native B JSON reaches transplant/component consumers');
  const request=f.requests('nemo_native_prepare_inactive_project')[0][1].request;
  assert.equal(request.lifecycleGeneration,17);assert.equal(request.expectedRevision,5);assert.equal(request.resources.length,21);
  assert.deepEqual(request.projection,{format:'nemo.native-opacity-document',formatVersion:1,totalFrames:21,
    layers:[{layerUid:'r08_curve_layer',motionStatic:{opacity:[60]}}]});
  assert.equal(f.effects.length,0,'no active import, release, Paper writer, focus or reveal');
  f.switch(b.id);assert.equal(f.snapshot(),before);assert.equal(f.requests('nemo_native_cancel_inactive_project').length,0);
  const alert=f.document.getElementById('project-tabs-bar').children.find(e=>e.getAttribute('role')==='alert');assert.match(alert.textContent,/^Switching project tabs is unavailable/i);
  f.close(rows.find(r=>r.active).id);assert.equal(f.state().rows.length,2);assert.equal(f.snapshot(),before);
  f.close(b.id);await settleUntil(()=>f.state().rows.length===1);assert.equal(f.snapshot(),before);
  assert.equal(f.requests('nemo_native_cancel_inactive_project')[0][1].request.preparationToken,f.token);
});
test('direct desktop OpenPath uses same inactive controller and duplicate request does not append',async()=>{
  const f=await admittedDesktop(),before=f.snapshot();assert.equal((await f.root.SMProject.openPath('/B.json')).status,'prepared');
  assert.equal((await f.root.SMProject.openPath('/B.json')).status,'unavailable');assert.equal(f.state().rows.length,2);
  assert.equal(f.snapshot(),before);assert.equal(f.requests('nemo_native_prepare_inactive_project').length,1);
});
for(const mode of ['cancel','picker-error','unsupported','read-error','native-refused','malformed','release'])test('ordinary native B Open refuses '+mode+' without changing A or tab membership',async()=>{
  const opts={};if(mode==='cancel')opts.picker=()=>null;if(mode==='picker-error')opts.picker=()=>{throw Error('picker failed');};
  if(mode==='unsupported')opts.picker=()=>'/bad.json';if(mode==='read-error')opts.picker=()=>'/missing.json';
  if(mode==='native-refused')opts.invoke=(name)=>{if(name==='nemo_native_prepare_inactive_project')throw {code:'preparation_busy'};};
  if(mode==='malformed')opts.invoke=(name,args)=>name==='nemo_native_prepare_inactive_project'?{apiVersion:2,requestId:args.request.requestId,preparationToken:'unknown'}:undefined;
  const f=await admittedDesktop(opts);if(mode==='release')f.owner.active=false;
  const before=JSON.stringify({owner:f.owner,label:f.root.SMProject.getCurrentLabel(),rows:f.state().rows});
  if(mode==='release')f.root.NemoNativeOpacityCutover.blocksLegacy=()=>true;
  const result=await f.root.SMProject.open();assert.notEqual(result.status,'prepared');
  assert.equal(JSON.stringify({owner:f.owner,label:f.root.SMProject.getCurrentLabel(),rows:f.state().rows}),before);
  assert.equal(f.effects.length,0);assert.ok(f.document.getElementById('project-tabs-bar').children.some(e=>e.getAttribute('role')==='alert'));
});
for(const mode of ['revision','local','frame','path'])test('held ordinary picker rejects stale A/UI '+mode,async()=>{
  let release;const gate=new Promise(r=>release=r);const f=await admittedDesktop({picker:async()=>{await gate;return '/B.json';}});
  const pending=f.root.SMProject.open();if(mode==='revision')f.editA();if(mode==='local')f.owner.local++;
  if(mode==='frame')f.sandbox.state.currentFrame=10;
  if(mode==='path'){f.root.__TAURI__.dialog.save=async()=>'/changed.json';f.root.__TAURI__.fs.writeTextFile=async()=>{};f.root.__TAURI__.fs.rename=async()=>{};f.root.__TAURI__.fs.remove=async()=>{};
    await f.root.SMProject.saveAs();}
  const changed=f.snapshot();release();assert.notEqual((await pending).status,'prepared');assert.equal(f.snapshot(),changed);
  assert.equal(f.state().rows.length,1);assert.equal(f.requests('nemo_native_prepare_inactive_project').length,0);
});
test('native B close waits for cancellation acknowledgement and stale lifetime leaves both rows untouched',async()=>{
  let release;const gate=new Promise(r=>release=r);const f=await admittedDesktop({invoke:async name=>{if(name==='nemo_native_cancel_inactive_project')await gate;}});
  await f.root.SMProject.openPath('/B.json');const b=f.state().rows.find(r=>!r.active),before=f.snapshot();
  f.close(b.id);await settleUntil(()=>f.requests('nemo_native_cancel_inactive_project').length===1);
  assert.equal(f.state().rows.length,2);f.owner.local++;const changed=f.snapshot();release();
  await settleUntil(()=>f.document.getElementById('project-tabs-bar').children.some(e=>/indeterminate/.test(e.textContent||'')));
  assert.equal(f.state().rows.length,2);assert.equal(f.snapshot(),changed);assert.notEqual(before,changed);
});
test('retained native token row cannot fall into blank/legacy activation if native globals disappear',async()=>{
  const f=await admittedDesktop();await f.root.SMProject.openPath('/B.json');
  const b=f.state().rows.find(r=>!r.active),before=JSON.stringify({rows:f.state().rows,label:f.root.SMProject.getCurrentLabel()});
  delete f.root.__TAURI__;delete f.root.NemoNativeOpacityCutover;delete f.root.NemoNativeOpacityProject;
  f.switch(b.id);assert.equal(JSON.stringify({rows:f.state().rows,label:f.root.SMProject.getCurrentLabel()}),before);
  f.close(f.state().rows.find(r=>r.active).id);assert.equal(JSON.stringify({rows:f.state().rows,label:f.root.SMProject.getCurrentLabel()}),before);
  assert.equal(f.effects.length,0);assert.equal(f.root.SMProject.getOpenTabs().length,0);
});
test('scoped inactive close refusal keeps B visible and leaves A untouched',async()=>{
  const f=await admittedDesktop({invoke(name){if(name==='nemo_native_cancel_inactive_project')throw {code:'stale_owner'};}});
  await f.root.SMProject.openPath('/B.json');const before=f.snapshot(),b=f.state().rows.find(r=>!r.active);
  f.close(b.id);await settleUntil(()=>f.document.getElementById('project-tabs-bar').children.some(e=>/indeterminate/.test(e.textContent||'')));
  assert.equal(f.state().rows.length,2);assert.equal(f.snapshot(),before);assert.equal(f.effects.length,0);
});

async function nativeInactiveTabDesktop(options={}) {
  // Model a native A that has already been admitted and edited; these tests
  // exercise the later Open/tab callbacks, not first-open desktop acceptance.
  const owner={admitted:true,active:false,apiVersion:2,instanceId:'native-i',documentId:'native-A',contentRevision:9,
    documentSnapshotId:'snapshot-A',lifecycleGeneration:23,savedDigest:'a'.repeat(64),documentDigest:'b'.repeat(64),
    snapshotBytes:'native-A-bytes-at-revision-9',undo:['open-A','edit-A'],redo:['redo-A']};
  const calls=[];let sequence=0,retained=null,projectUI=null;
  const projectFileAdapter={bind(root,ports){projectUI=ports;const session={isActive:()=>owner.active,isDirty:()=>owner.documentDigest!==owner.savedDigest,canOpen:()=>true,
    identity:()=>({apiVersion:2,instanceId:owner.instanceId,documentId:owner.documentId,contentRevision:owner.contentRevision,
      documentSnapshotId:owner.documentSnapshotId,lifecycleGeneration:owner.lifecycleGeneration}),
    info(){return this.identity();},openPath(){throw Error('same-document replacement must not run for inactive B');},saveAs(){throw Error('unexpected Save As');}};
    root.NemoNativeObjectProject={desktop:()=>session};return session;}};
  // Skip browser New entirely: A is declared already admitted by this source
  // model. Publish its identity through the same UI ports as native file Open.
  const f=fixture({populated:false,nativePorts:()=>{},nativeFileAdapter:projectFileAdapter});
  f.root.crypto={randomUUID:()=>`ui-${++sequence}`};
  f.root.__TAURI__={dialog:{async open(){return '/Projects/B.json';}},window:{getCurrentWindow:()=>({async setFocus(){}})},core:{async invoke(name,args){
    const req=JSON.parse(args.requestJson);calls.push({name,req});
    if(options.invoke)return options.invoke({name,req,owner,get retained(){return retained;},set retained(v){retained=v;}});
    assert.equal(name,'nemo_native_object_inactive_project');
    const base={apiVersion:2,requestId:req.requestId,instanceId:req.instanceId,documentId:req.documentId,
      contentRevision:req.expectedRevision,documentSnapshotId:req.documentSnapshotId,lifecycleGeneration:req.lifecycleGeneration,retryExecution:false};
    if(req.action.kind==='inspect')return {...base,state:retained?'inactive-retained':'inactive-absent',
      preparationToken:retained&&retained.preparationToken||null,project:retained};
    if(req.action.kind==='prepare'){
      retained={instanceId:req.instanceId,contentRevision:0,documentSnapshotId:'snapshot-B',
        documentDigest:'b'.repeat(64),byteCount:256,canvasW:640,canvasH:360,fps:24,totalFrames:12,layers:[{layerUid:'b-layer'}],reference:null,
        origin:{documentId:req.documentId,documentSnapshotId:req.documentSnapshotId,contentRevision:req.expectedRevision},
        preparationToken:'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',preparationRequestId:req.requestId};
      return {...base,state:'inactive-prepared',preparationToken:retained.preparationToken,project:retained};
    }
    if(req.action.kind==='close'){
      if(!retained||retained.preparationToken!==req.action.preparationToken)throw {code:'preparation_expired',message:'unknown token',details:{disposition:'notDispatched',retryExecution:false}};
      retained=null;return {...base,state:'inactive-closed',preparationToken:req.action.preparationToken,project:null};
    }
    throw Error('unexpected inactive action');
  }}};
  owner.active=true;
  assert.ok(projectUI,'native file UI ports were bound');
  projectUI.commit('/Projects/A.json','A');projectUI.hide();projectUI.opened();
  assert.equal(f.state().rows.length,1,'pre-admitted A has one real tab row');
  assert.equal(f.state().rows[0].active,true);assert.equal(f.state().rows[0].name,'A');
  assert.equal(f.effects.some(effect=>/import|release|create layer|frame save|Paper/.test(effect)),false);
  f.root.SMProject.refreshActiveTabDirtyDot();
  f.effects.length=0;f.notices.length=0;
  return Object.assign(f,{owner,calls,retained:()=>retained,
    activeSnapshot(){return JSON.stringify({label:f.root.SMProject.getCurrentLabel(),active:f.state().rows.find(r=>r.active),
      bytes:f.state().bytes,dirty:f.root.SMProject.isDirty(),owner:{admitted:owner.admitted,active:owner.active,apiVersion:owner.apiVersion,
        instanceId:owner.instanceId,documentId:owner.documentId,contentRevision:owner.contentRevision,documentSnapshotId:owner.documentSnapshotId,
        lifecycleGeneration:owner.lifecycleGeneration,documentDigest:owner.documentDigest,savedDigest:owner.savedDigest,snapshotBytes:owner.snapshotBytes,
        undo:owner.undo,redo:owner.redo},frame:f.sandbox.state.currentFrame,open:f.root.SMProject.getOpenTabs()});}});
}

function semanticRows(f) {
  const list=f.document.getElementById('project-tabs-list');
  assert.equal(list.getAttribute('role'),'tablist');
  assert.equal(list.getAttribute('aria-label'),'Project tabs');
  const rows=list.children.map(row=>{
    assert.equal(row.getAttribute('role'),'tab');
    assert.equal(row.getAttribute('aria-label'),row.children[2].textContent);
    assert.equal(row.getAttribute('aria-selected'),String(row.classList.contains('act')));
    return {name:row.getAttribute('aria-label'),selected:row.getAttribute('aria-selected')};
  });
  assert.equal(new Set(list.children.map(row=>row.dataset.tab)).size,rows.length);
  return rows;
}
test('actual empty startup publishes the named tablist without document or history effects',()=>{
  const f=fixture({nativePorts(){}});
  assert.deepEqual(semanticRows(f),[]);
  assert.deepEqual(f.afterReady,f.beforeReady);
  assert.deepEqual(f.effects,[]);
});
test('presentation adapter refuses missing DOM before resolving any rows or dirty state',()=>{
  const f=fixture({nativePorts(){}});let reads=0;
  f.root.SMProjectEntry.renderTabs({getElementById:()=>null},{readRows(){reads++;throw Error('dirty read');}});
  assert.equal(reads,0);
});
test('semantic initialization is idempotent and preserves already-rendered rows and document state',()=>{
  const f=fixture(),before=f.state(),list=f.document.getElementById('project-tabs-list'),rows=list.children.slice();
  f.root.SMProjectEntry.initializeTabs(f.document);f.root.SMProjectEntry.initializeTabs(f.document);
  assert.deepEqual(list.children,rows);
  assert.deepEqual(f.state(),before);
  assert.deepEqual(f.effects,[]);
});
test('shipped ordinary Open produces semantic A, A+B and A while native switch guard stays intact',async()=>{
  const f=await admittedDesktop();
  assert.deepEqual(semanticRows(f),[{name:'A',selected:'true'}]);
  const before=f.snapshot();await f.root.SMProject.openPath('/B.json');
  assert.deepEqual(semanticRows(f),[{name:'A',selected:'true'},{name:'B',selected:'false'}]);
  const b=f.state().rows[1].id;f.switch(b);
  assert.deepEqual(semanticRows(f),[{name:'A',selected:'true'},{name:'B',selected:'false'}]);
  assert.equal(f.snapshot(),before);
  f.close(b);await settleUntil(()=>f.state().rows.length===1);
  assert.deepEqual(semanticRows(f),[{name:'A',selected:'true'}]);
  assert.equal(f.snapshot(),before);
});
test('real browser switch, rename and close listeners keep displayed names and selection synchronized',()=>{
  const f=fixture();
  assert.equal(semanticRows(f)[0].selected,'true');
  f.switch(f.b);assert.equal(semanticRows(f)[1].selected,'true');
  let stopped=false;f.row(f.b).listeners.dblclick({stopPropagation(){stopped=true;}});
  const input=f.row(f.b).children.find(node=>node.className==='pt-rename-input');
  input.value='ordinary <name> & label';input.listeners.keydown({key:'Enter',stopPropagation(){}});
  assert.equal(stopped,true);
  assert.equal(semanticRows(f)[1].name,'ordinary <name> & label');
  f.close(f.b);assert.deepEqual(semanticRows(f),[{name:'A',selected:'true'}]);
});

test('real Open callback prepares metadata-only B while pre-admitted dirty native A is unchanged',async()=>{
  const f=await nativeInactiveTabDesktop();const before=f.activeSnapshot();
  assert.equal(f.owner.admitted,true);assert.equal(f.root.SMProject.isDirty(),true);
  assert.equal(f.state().rows.find(row=>row.active).dirty,true,'active native A begins dirty');
  await f.document.getElementById('start-open').listeners.click();
  assert.equal(f.calls.filter(call=>call.req.action.kind==='prepare').length,1);assert.equal(f.activeSnapshot(),before);
  assert.equal(f.state().rows.length,2,JSON.stringify(f.state()));const b=f.state().rows.find(row=>!row.active);
  assert.equal(b.name,'B');assert.equal(b.dirty,false);assert.equal(f.root.SMProject.getOpenTabs().length,0);
  assert.equal(f.calls.at(-1).name,'nemo_native_object_inactive_project');
  assert.deepEqual(f.calls.find(call=>call.req.action.kind==='prepare').req.action,{kind:'prepare',sourcePath:'/Projects/B.json'});
  assert.equal(f.effects.some(effect=>/import|release|frame save|Paper/.test(effect)),false);
});

test('real B tab click refuses activation and close removes it only after exact receipt beside dirty A',async()=>{
  const f=await nativeInactiveTabDesktop();await f.root.SMProject.openPath('/Projects/B.json');
  assert.equal(f.owner.admitted,true);assert.equal(f.root.SMProject.isDirty(),true);
  const before=f.activeSnapshot(),b=f.state().rows.find(row=>!row.active);
  f.switch(b.id);await settleUntil(()=>f.calls.filter(call=>call.req.action.kind==='inspect'&&call.req.action.preparationToken).length===1);
  assert.equal(f.activeSnapshot(),before);assert.equal(f.state().rows.length,2);
  const alert=f.document.getElementById('project-tabs-bar').children.find(node=>node.getAttribute('role')==='alert');
  assert.match(alert.textContent,/Switching project tabs is unavailable/);
  f.close(b.id);await settleUntil(()=>f.state().rows.length===1);
  assert.equal(f.activeSnapshot(),before);assert.equal(f.calls.at(-1).req.action.kind,'close');
  assert.equal(f.calls.at(-1).req.action.preparationToken,'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb');
  assert.equal(f.effects.some(effect=>/import|release|frame save|Paper/.test(effect)),false);
});

test('#2373 SMProject.closeActiveTab is the tab x button path for the active tab, and the last tab stays open', async () => {
  const viaX = fixture(), viaKey = fixture();
  assert.equal(typeof viaKey.root.SMProject.closeActiveTab, 'function');
  viaX.close(viaX.a); await new Promise(setImmediate);
  await viaKey.root.SMProject.closeActiveTab(); await new Promise(setImmediate);
  const shape = f => { const v = f.state(); return { ...v, rows: v.rows.map(({ id, ...row }) => row) }; };
  const same = (a, b) => assert.equal(JSON.stringify(shape(a)), JSON.stringify(shape(b)));
  same(viaKey, viaX);
  assert.equal(JSON.stringify(viaKey.effects), JSON.stringify(viaX.effects));
  assert.equal(JSON.stringify(viaKey.notices), JSON.stringify(viaX.notices));
  assert.equal(viaKey.state().rows.length, 1);
  viaKey.effects.length = 0; viaKey.notices.length = 0;
  await viaKey.root.SMProject.closeActiveTab();
  assert.equal(viaKey.effects.length, 0);
  assert.equal(viaKey.notices.at(-1), 'hsCannotCloseLastTab');
  assert.equal(viaKey.state().rows.length, 1);
});
