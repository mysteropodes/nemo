'use strict';
// U05 (#2370) characterization of the real timeline.js onKeyDown arbitration, before any extraction:
// a focused field keeps undo and clipboard keys while Save stays global; a Motion key selection beats a canvas
// selection, which beats frame cells; paste follows the clipboard filled last. F6/F7 as distinct native commands are
// covered by tests/native-frame-edit-desktop.test.cjs.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const timeline=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');
const start=timeline.indexOf('function onKeyDown(event){'),end=timeline.indexOf('function onKeyUp(event)',start);
assert.ok(start>=0&&end>start);

function harness({motion=false,keySelection=false,keyClipboard=false,paths=0,canvasClip=0,frameClip=0,lastKind}={}){
  const calls=[],call=(name)=>()=>calls.push(name);
  const ctx={state:{appMode:motion?'motion':'anim2d'},selectedPaths:new Array(paths).fill({}),_sel:{frames:[],clipboard:new Array(frameClip).fill({})},
    _canvasClip:{snaps:new Array(canvasClip).fill({})},undo:call('undo'),redo:call('redo'),
    copySelection:call('copySelection'),cutSelection:call('cutSelection'),pasteSelection:call('pasteSelection'),
    SMProject:{save:call('save'),saveAs:call('saveAs')},
    SM:{copyFrames:call('copyFrames'),cutFrames:call('cutFrames'),pasteFrames:call('pasteFrames'),convertActiveLayerToComponent:call('toComponent')},
    SMMotion:{hasKeySelection:()=>keySelection,copySelectedKeys:call('copyKeys'),hasKeyClipboard:()=>keyClipboard,pasteKeys:call('pasteKeys')},
    _lastClipKind:lastKind};
  ctx.window=ctx;
  vm.runInNewContext(timeline.slice(start,end),ctx);
  const press=(key,mods={},target={tagName:'DIV'})=>{let prevented=0;
    ctx.onKeyDown(Object.assign({key,target,preventDefault(){prevented++;}},mods));return prevented;};
  return {calls,press};
}
const cmd={metaKey:true};

test('a focused field keeps undo and clipboard keys; Save and Save As stay global',()=>{
  for(const target of [{tagName:'INPUT'},{tagName:'TEXTAREA'},{tagName:'SELECT'},{tagName:'DIV',isContentEditable:true}]){
    const h=harness({paths:1,canvasClip:1,frameClip:1,lastKind:'canvas'});
    for(const [key,mods] of [['z',cmd],['z',{...cmd,shiftKey:true}],['c',cmd],['x',cmd],['v',cmd],['C',{...cmd,shiftKey:true}]])
      assert.equal(h.press(key,mods,target),0,target.tagName+' '+key+': the browser keeps the key');
    assert.deepEqual(h.calls,[],'no app undo, clipboard or convert from a field');
    assert.equal(h.press('s',cmd,target),1);assert.equal(h.press('s',{...cmd,shiftKey:true},target),1);
    assert.deepEqual(h.calls,['save','saveAs'],'Save routes from a field');
  }
  let blurred=0;const h=harness();h.press('Enter',{},{tagName:'INPUT',blur(){blurred++;}});
  assert.equal(blurred,1,'Enter confirms and releases the field, so the next undo is the app\'s');
  for(const [key,mods,name] of [['z',cmd,'undo'],['z',{...cmd,shiftKey:true},'redo'],['z',{ctrlKey:true},'undo']]){
    const o=harness();assert.equal(o.press(key,mods),1);assert.deepEqual(o.calls,[name]);
  }
});

test('copy: a Motion key selection beats a canvas selection, which beats frame cells',()=>{
  const cases=[
    [{motion:true,keySelection:true,paths:2},'copyKeys'],
    [{motion:false,keySelection:true,paths:2},'copySelection'],
    [{motion:true,keySelection:false,paths:2},'copySelection'],
    [{paths:0},'copyFrames'],
  ];
  for(const [state,name] of cases){const h=harness(state);assert.equal(h.press('c',cmd),1);assert.deepEqual(h.calls,[name],JSON.stringify(state));}
  const cut=harness({paths:1});cut.press('x',cmd);assert.deepEqual(cut.calls,['cutSelection']);
  const cutFrames=harness();cutFrames.press('x',cmd);assert.deepEqual(cutFrames.calls,['cutFrames']);
  const convert=harness({paths:1});convert.press('C',{...cmd,shiftKey:true});assert.deepEqual(convert.calls,['toComponent'],'Shift+Cmd+C is convert, never copy');
});

test('paste follows the clipboard filled last; a Motion key clipboard wins in Motion',()=>{
  const cases=[
    [{canvasClip:1,frameClip:1,lastKind:'frames'},'pasteFrames'],
    [{canvasClip:1,frameClip:1,lastKind:'canvas'},'pasteSelection'],
    [{canvasClip:1,frameClip:0,lastKind:'frames'},'pasteSelection'],
    [{canvasClip:0,frameClip:1,lastKind:'canvas'},'pasteFrames'],
    [{motion:true,keyClipboard:true,canvasClip:1,frameClip:1,lastKind:'canvas'},'pasteKeys'],
  ];
  for(const [state,name] of cases){const h=harness(state);assert.equal(h.press('v',cmd),1);assert.deepEqual(h.calls,[name],JSON.stringify(state));}
});
