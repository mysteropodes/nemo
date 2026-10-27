'use strict';
// U04 (#2370): the real shortcut dispatch (timeline.js runToolShortcut/runCommandShortcut) against its declared desktop
// targets (shortcut-targets.js), with the real shortcut registry.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const Targets=require('../src/js/application/shortcut-targets.js');
const Registry=require('../src/js/application/shortcut-registry.js');
const timeline=fs.readFileSync(path.join(__dirname,'../src/js/timeline.js'),'utf8');
function source(name){const start=timeline.indexOf('function '+name+'(');assert.ok(start>=0,name);let at=timeline.indexOf('{',start),depth=1,end=at+1;
  while(depth){if(timeline[end]==='{')depth++;if(timeline[end]==='}')depth--;end++;}return timeline.slice(start,end);}

// The frame edits native-frame-edit-desktop.js shortcut() owns while a native document is open.
const FRAME_EDITS=['cmdInsertFrame','cmdInsertKey','cmdInsertBlankKey','cmdDuplicateKey','cmdExtendExposure'];
// Dispatches keys through the real functions; returns what each key reached.
function keyboard({desktop=true,targets=true,nativeFrames=true}={}){
  const calls=[],notices=[];
  const call=(name)=>(...args)=>{calls.push(args.length&&typeof args[0]!=='object'?name+':'+args[0]:name);};
  const win={showToast:(m)=>notices.push(m),
    SM:{setTool:call('setTool'),toggleOnion:call('toggleOnion'),generateTweens:call('generateTweens'),flipPreview:call('flipPreview'),
      resetView:call('resetView'),duplicateKeyframe:call('duplicateKeyframe'),extendExposure:call('extendExposure')},
    NemoNativeFrameEditDesktop:{install:()=>({shortcut:(action)=>{if(!nativeFrames||!FRAME_EDITS.includes(action))return false;calls.push('native:'+action);return true;}})},
    state:{playing:false,currentFrame:3,totalFrames:24,activeLayerIdx:0,layers:[{}]},
    goToFrame:call('goToFrame'),insertFrame:call('insertFrame'),insertKeyframe:call('insertKeyframe'),insertBlankKeyframe:call('insertBlankKeyframe'),
    startLayerRename:call('startLayerRename'),prevKeyframeFrame:()=>1,nextKeyframeFrame:()=>5,stopPlay:call('stopPlay'),
    shortcutRegistry:Registry.create({})};
  if(desktop)win.__TAURI__={core:{}};
  if(targets)win.NemoShortcutTargets=Targets;
  win.window=win;
  const ctx=vm.createContext(win);
  vm.runInContext(['shortcutAllowed','runCommandShortcutAction','runCommandShortcut','runToolShortcut'].map(source).join('\n'),ctx);
  const event={shiftKey:false,preventDefault(){}};
  return {calls,notices,registry:win.shortcutRegistry,press(k){return ctx.runToolShortcut(k)||ctx.runCommandShortcut(k,event);}};
}
const actions=()=>Registry.TOOL_SHORTCUTS.concat(Registry.COMMAND_SHORTCUTS).map((d)=>d.action);

test('every rebindable shortcut has a declared desktop target, and no declared target is stale',()=>{
  const ids=actions();
  assert.deepEqual(ids.filter((a)=>!Object.prototype.hasOwnProperty.call(Targets.targets,a)),[],'an unclassified shortcut must be declared');
  assert.deepEqual(Object.keys(Targets.targets).filter((a)=>!ids.includes(a)),[],'no declared target is stale');
  for(const a of ids){const t=Targets.target({},a);assert.ok(['native','presentation','unavailable'].includes(t.kind),a);
    if(t.kind==='unavailable')assert.match(t.reason,/unavailable in native Nemo\.$/,a);}
});

test('on desktop an unavailable shortcut shows its reason and reaches nothing; native and presentation shortcuts run',()=>{
  const k=keyboard();
  assert.equal(k.press('t'),true,'the Tween key is consumed');
  assert.equal(k.press('F2'),true,'the Rename key is consumed');
  assert.deepEqual(k.calls,[],'neither generateTweens nor the rename field runs');
  assert.deepEqual(k.notices,['Generating tweens is unavailable in native Nemo.','Renaming a layer from the keyboard is unavailable in native Nemo.']);
  for(const key of ['b','o','F5','F6','F7','+',',','/'])assert.equal(k.press(key),true,key);
  assert.deepEqual(k.calls,['setTool:draw','toggleOnion','native:cmdInsertFrame','native:cmdInsertKey','native:cmdInsertBlankKey',
    'native:cmdExtendExposure','goToFrame:2','resetView']);
  assert.equal(k.notices.length,2,'no further notice');
});

test('a rebound key keeps its action\'s target',()=>{
  const k=keyboard();
  assert.equal(k.registry.clashFor('cmdTween','F8'),null,'F8 is free');
  k.registry.setKey('cmdTween','F8');
  assert.equal(k.press('F8'),true);assert.equal(k.press('t'),false,'the old key no longer reaches Tween');
  assert.deepEqual(k.calls,[]);assert.deepEqual(k.notices,['Generating tweens is unavailable in native Nemo.']);
});

test('without the target table every desktop shortcut refuses; outside the desktop host every shortcut runs as before',()=>{
  const missing=keyboard({targets:false});
  assert.equal(missing.press('b'),true);assert.equal(missing.press('F5'),true);
  assert.deepEqual(missing.calls,[]);assert.deepEqual(missing.notices,['Keyboard shortcuts are unavailable.','Keyboard shortcuts are unavailable.']);
  const browser=keyboard({desktop:false,nativeFrames:false});
  for(const key of ['t','F2','F5','b'])browser.press(key);
  assert.deepEqual(browser.calls,['generateTweens','startLayerRename:0','insertFrame','setTool:draw']);
  assert.deepEqual(browser.notices,[]);
});

test('an action missing from the table is unavailable on desktop',()=>{
  const notices=[],root={__TAURI__:{core:{}},showToast:(m)=>notices.push(m)};
  assert.equal(Targets.allow(root,'cmdUnknown'),false);assert.deepEqual(notices,['This shortcut is unavailable in native Nemo.']);
  assert.equal(Targets.allow({},'cmdUnknown'),true,'off desktop it is not this table\'s decision');
});
