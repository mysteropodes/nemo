'use strict';
// U05 (#2370): keyboard focus and clipboard arbitration. The pure decision and the single keydown/keyup pair per document
// (keyboard-arbiter.js). The real onKeyDown running it is tests/keyboard-arbitration-u05.test.cjs.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Arbiter=require('../src/js/application/keyboard-arbiter.js');
const Registry=require('../src/js/application/shortcut-registry.js');
const FrameEdit=require('../src/js/adapters/native-frame-edit-desktop.js');
const timeline=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');

const ev=(key,over)=>Object.assign({key,metaKey:true,ctrlKey:false,shiftKey:false,target:{tagName:'DIV'}},over);
const FIELD={tagName:'INPUT'};
const kind=(e,ctx)=>{const d=Arbiter.decide(e,ctx);return d&&d.kind;};

test('a field suppresses app undo, redo, copy, cut, paste and convert, while Save still routes',()=>{
  for(const tagName of ['INPUT','SELECT','TEXTAREA']){
    const t={tagName};
    for(const key of ['z','c','x','v'])assert.equal(kind(ev(key,{target:t})),'field',tagName+key);
    assert.equal(kind(ev('z',{target:t,shiftKey:true})),'field');
    assert.equal(kind(ev('C',{target:t,shiftKey:true})),'field');
    assert.equal(kind(ev('s',{target:t})),'save');
    assert.equal(kind(ev('s',{target:t,shiftKey:true})),'saveAs');
  }
  assert.equal(kind(ev('z',{target:{tagName:'DIV',isContentEditable:true}})),'field','contenteditable');
  assert.equal(kind(ev('z',{ctrlKey:true,metaKey:false})),'undo','Ctrl works like Cmd');
  assert.equal(kind(ev('z',{shiftKey:true})),'redo');
  assert.equal(kind(ev('Enter',{metaKey:false,target:FIELD})),'blurField','Enter confirms and blurs an input');
  assert.equal(kind(ev('Enter',{metaKey:false,target:{tagName:'TEXTAREA'}})),null,'a textarea keeps Enter');
  assert.equal(kind(ev('z',{metaKey:false})),null,'a bare z is not arbitrated');
});

test('the selection snapshot is only read for copy, cut and paste outside a field',()=>{
  let reads=0;const ctx=()=>{reads++;return {shapeSelection:true};};
  for(const key of ['z','s'])Arbiter.decide(ev(key),ctx);
  for(const key of ['c','x','v'])Arbiter.decide(ev(key,{target:{tagName:'INPUT'}}),ctx);
  assert.equal(reads,0);
  assert.equal(Arbiter.decide(ev('c'),ctx).kind,'copyShapes');assert.equal(reads,1);
});

test('copy: Motion keys beat shapes beat frame cells; cut: shapes beat frame cells and Motion keys are copy-only',()=>{
  const all={motionKeySelection:true,shapeSelection:true};
  assert.equal(kind(ev('c'),all),'copyKeys');
  assert.equal(kind(ev('c'),{shapeSelection:true}),'copyShapes');
  assert.equal(kind(ev('c'),{}),'copyFrames');
  assert.equal(kind(ev('x'),all),'cutShapes');
  assert.equal(kind(ev('x'),{motionKeySelection:true}),'cutFrames');
  assert.equal(kind(ev('C',{shiftKey:true}),all),'convertToComponent','Cmd+Shift+C is not a copy');
});

test('paste follows the clipboard filled last; Motion keys win in Motion; an empty clipboard lets SVG paste through',()=>{
  const both={canvasClipboard:true,frameClipboard:true};
  assert.equal(kind(ev('v'),Object.assign({lastClipKind:'canvas'},both)),'pasteShapes');
  assert.equal(kind(ev('v'),Object.assign({lastClipKind:'frames'},both)),'pasteFrames');
  assert.equal(kind(ev('v'),Object.assign({lastClipKind:'frames'},{canvasClipboard:true})),'pasteShapes','falls back to the filled one');
  assert.equal(kind(ev('v'),Object.assign({lastClipKind:'canvas'},{frameClipboard:true})),'pasteFrames');
  assert.equal(kind(ev('v'),Object.assign({motionKeyClipboard:true,lastClipKind:'canvas'},both)),'pasteKeys');
  const empty=Arbiter.decide(ev('v'),{svgImport:true});
  assert.equal(empty.kind,'pasteSvgFallback');assert.equal(empty.preventDefault,false,'the browser paste event must fire');
  assert.equal(kind(ev('v'),{svgImport:false}),'pasteFrames');
  assert.equal(kind(ev('v'),{svgImport:true,frameClipboard:true}),'pasteFrames','svg only when nothing internal');
});

test('mounting twice leaves exactly one listener pair, and dispose removes it',()=>{
  const added=[],doc={addEventListener:(t,f)=>added.push([t,f]),removeEventListener:(t,f)=>{const i=added.findIndex((a)=>a[0]===t&&a[1]===f);if(i>=0)added.splice(i,1);}};
  const d1=()=>{},u1=()=>{},d2=()=>{},u2=()=>{};
  const m1=Arbiter.mount(doc,{keydown:d1,keyup:u1});
  assert.equal(added.length,2);
  const m2=Arbiter.mount(doc,{keydown:d2,keyup:u2});
  assert.deepEqual(added.map((a)=>a[0]+':'+(a[1]===d2||a[1]===u2)),['keydown:true','keyup:true'],'only the latest pair');
  m1.dispose();assert.equal(added.length,2,'a stale handle cannot remove the current pair');
  m2.dispose();assert.equal(added.length,0);
  assert.equal(Arbiter.dispose(doc),false,'dispose is idempotent');
  const other={addEventListener(){},removeEventListener(){}};
  Arbiter.mount(doc,{keydown:d1});Arbiter.mount(other,{keydown:d1});assert.equal(added.length,1,'documents are independent');
  Arbiter.dispose(doc);Arbiter.dispose(other);
});

test('timeline.js mounts its keyboard handlers only through the arbiter',()=>{
  assert.match(timeline,/NemoKeyboardArbiter\.mount\(document,\{keydown:onKeyDown,keyup:onKeyUp\}\)/);
  assert.equal(/document\.addEventListener\('keydown',onKeyDown\)/.test(timeline),false);
});

test('F6 and F7 stay distinct native commands, and every frame insert also has a context-menu path for keyboards without F-keys',()=>{
  assert.equal(Registry.defFor('cmdInsertKey').key,'F6');assert.equal(Registry.defFor('cmdInsertBlankKey').key,'F7');
  assert.equal(Registry.defFor('cmdInsertFrame').key,'F5');
  // The frame-cell context menu (baseline 1065e9d8) runs the global writers, which the native adapter routes.
  const menu=timeline.slice(timeline.indexOf("getElementById('frame-grid').addEventListener('contextmenu'"));
  // U04a (#2370): the hint is the registry's current binding (F5/F6/F7 by default), so a remap shows in the menu.
  for(const [label,fn,action] of [['ctxInsertFrame','insertFrame','cmdInsertFrame'],['ctxInsertKeyframe','insertKeyframe','cmdInsertKey'],['ctxInsertBlankKeyframe','insertBlankKeyframe','cmdInsertBlankKey']])
    assert.ok(menu.includes("SM.t('"+label+"'),shortcut:shortcutHint('"+action+"'),action:function(){"+fn+"();}"),label);
  for(const fn of ['insertFrame','insertKeyframe','insertBlankKeyframe'])assert.ok(FrameEdit.writerNames('global').includes(fn),fn);
});
