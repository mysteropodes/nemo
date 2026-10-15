'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const assert=require('node:assert/strict'),test=require('node:test');
const root=path.resolve(__dirname,'..');
const fixture=JSON.parse(fs.readFileSync(path.join(root,'engineering/application/examples/native-layer-timing-v1/cases.json')));
const app=fs.readFileSync(path.join(root,'src/js/app.js'),'utf8');
const timeline=fs.readFileSync(path.join(root,'src/js/timeline.js'),'utf8');
function extract(source,name){
  const start=source.indexOf(`function ${name}(`),end=source.indexOf('\n}\n',start);
  assert.ok(start>=0&&end>start,name);return source.slice(start,end+2);
}
function element(){
  const value={children:[],dataset:{},style:{setProperty(){}},classes:new Set(),appendChild(child){this.children.push(child)}};
  value.classList={add(...names){names.forEach(name=>value.classes.add(name))}};return value;
}
const frame=(strokes=[],key=false,tween=false,manual=false)=>({strokes,isKeyframe:key,isInterpolated:tween,isManualEdit:manual});
function harness(layer,total){
  const state={layers:[layer],symbols:{},totalFrames:total,currentFrame:0,waIn:0,waOut:total-1};
  const context={state,window:{},document:{createElement:element},selHas:()=>false,layerHasTimeRange:()=>false};
  vm.createContext(context);vm.runInContext([extract(app,'getEffectiveStrokes'),extract(app,'resolveSymbolFrameIdx'),extract(timeline,'renderKeyframeCellsInto')].join('\n'),context);
  return context;
}
function drawing(){
  const input=fixture.drawing;
  const frames=Array.from({length:input.totalFrames},()=>frame());
  input.objects.forEach(object=>frames[object.frame].strokes.push(object.strokeId));
  input.timing.keys.forEach(key=>frames[key.frame].isKeyframe=true);
  input.timing.samples.forEach(sample=>Object.assign(frames[sample.frame],{isInterpolated:true,isManualEdit:sample.manualEdit||false}));
  return {frames};
}

test('frozen observed baseline separates author keys, sample frames, blank keys and absent keys',()=>{
  assert.equal(fixture.baselineRef,'66ece0641708122eb8447e85ad8dd7e3402aaf6c');
  assert.equal(fixture.acceptedProducerFamily,'drawing');
  for(const evidence of Object.values(fixture.characterizationEvidence)){
    assert.equal(evidence.observedExpectedMatrix,true);
    assert.deepEqual(Object.keys(evidence.functionSha256),['getEffectiveStrokes','resolveSymbolFrameIdx','renderKeyframeCellsInto']);
    Object.values(evidence.functionSha256).forEach(hash=>assert.match(hash,/^[a-f0-9]{64}$/));
  }
  const h=harness(drawing(),8);
  const effective=Array.from({length:8},(_,i)=>Array.from(h.getEffectiveStrokes(0,i)));
  assert.deepEqual(effective,fixture.drawing.effectiveStrokes);
  assert.deepEqual(effective[4],['A'],'held frame after C sample returns A key, not C sample');
  assert.deepEqual(effective[7],[],'manual D sample does not end blank-key hold');
  assert.deepEqual(effective[2],['A'],'non-key stored strokes do not invent a key');
});

test('actual shipped timeline classes preserve full/empty key, held spans and manual interpolation',()=>{
  const h=harness(drawing(),8),row=element();h.renderKeyframeCellsInto(row,0);
  assert.deepEqual(row.children.map(cell=>Array.from(cell.classes).filter(name=>!['cur','outside-wa','sel'].includes(name))),fixture.drawing.timelineClasses);
});

test('component outer timing remains a meaningful retained oracle and unsupported native obligation',()=>{
  assert.equal(fixture.componentOracle.producerAvailability,'unavailable');
  assert.deepEqual(fixture.pendingProducerFamilies,['component-outer-timing']);
  const layer={symbolId:'symbol',frames:[frame([],true),{...frame([],true),blankOverride:true},frame(),{...frame([],true),componentFrame:2},frame(),frame(),frame(),frame()]};
  const h=harness(layer,8);
  h.state.symbols={symbol:{totalFrames:5,layers:[{visible:true,frames:fixture.componentOracle.internalContent.map(content=>frame([content],true))}]}};
  const row=element();h.renderKeyframeCellsInto(row,0);
  assert.ok(row.children[0].classes.has('kf-full'),'component seed does not become blank just because outer strokes are empty');
  assert.ok(row.children[1].classes.has('kf-empty'));assert.ok(row.children[2].classes.has('span-empty'));
  assert.deepEqual(Array.from(h.getEffectiveStrokes(0,2,true)),[],'blank override holds into next cell over real nonempty internal content');
  layer.frames[1].blankOverride=false;
  assert.deepEqual(Array.from(h.getEffectiveStrokes(0,2,true)),['S2'],'negative control proves actual content was hidden');
  layer.frames[1].blankOverride=true;
  assert.deepEqual(Array.from(h.getEffectiveStrokes(0,4,true)),['S2']);
  assert.equal(h.resolveSymbolFrameIdx(h.state.symbols.symbol,layer,4),2);
  layer.frames[5]=frame([],true);assert.equal(h.resolveSymbolFrameIdx(h.state.symbols.symbol,layer,6),1,'next outer key without explicit pick restores default clock');
});

test('timing schema is a separately versioned sparse drawing field with explicit relational constraints',()=>{
  const schema=JSON.parse(fs.readFileSync(path.join(root,'engineering/application/native-layer-timing-v1.schema.json')));
  assert.equal(schema.additionalProperties,false);assert.equal(schema.properties.family.const,'drawing');
  assert.equal(schema.properties.formatVersion.const,1);assert.equal(schema.properties.contextId.const,'scene-root');
  assert.equal(schema.properties.keys.maxItems,4096);assert.equal(schema.properties.samples.maxItems,4096);
  assert.deepEqual(schema.$defs.Key.required,['frame']);assert.equal(schema.$defs.Key.additionalProperties,false);
  assert.equal(schema.$defs.Sample.properties.manualEdit.type,'boolean');
  assert.match(schema.description,/strictly increase/);assert.match(schema.description,/disjoint/);assert.match(schema.description,/Missing timing/);
});
