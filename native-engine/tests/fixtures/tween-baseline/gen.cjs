'use strict';
// #2358: execute the independently pinned production baseline, never the native port.
// Run from any directory: node gen.cjs > oracle.json
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const cp = require('node:child_process');
const crypto = require('node:crypto');
const root = path.resolve(__dirname, '../../../..');
const baseline = '46442ea96c09e6bfaadf956eb8586e273a658085';
const read = (file) => cp.execFileSync('git', ['show', `${baseline}:${file}`], { cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
const source = read('src/js/tweens.js');
if (fs.readFileSync(path.join(root,'src/paper-full.min.js'),'utf8') !== read('src/paper-full.min.js')) throw new Error('Paper source must match the pinned baseline');
const paper = require(path.join(root, 'tests/helpers/native-import-paper.cjs')).load();
const context = vm.createContext({
  Path: paper.Path, Point: paper.Point, Segment: paper.Segment, Math, console,
  window: {}, state: { layers: [{ layerUid: 'L0' }], activeLayerIdx: 0, motionArcs: {} },
});
context.window = context;
vm.runInContext(read('src/js/domain/tween/assignment.js'), context);
// All declarations and original flags, ending before the document-mutating entry point.
vm.runInContext(source.slice(0, source.indexOf('function generateTweens(')), context);
const seg = (x, y) => ({ point: [x, y], handleIn: [0, 0], handleOut: [0, 0] });
const stroke = (points, extra = {}) => ({ segments: points.map(([x, y]) => seg(x, y)),
  closed: false, strokeColor: '#112233', strokeWidth: 4, strokeCap: 'round', strokeJoin: 'round', fillColor: null, opacity: 1, ...extra });
const copy = (x) => JSON.parse(JSON.stringify(x));
const cases = [];
const call = (operation, input, fn) => cases.push({ operation, input: copy(input), expected: copy(fn()) });
const a = stroke([[0, 0], [40, 0]]);
const b = stroke([[20, 30], [60, 30]], { strokeWidth: 8, opacity: 0.5 });
for (const t of [0, 0.25, 0.5, 1]) {
  call('interpolate', { a, b, t }, () => context.interpStroke(copy(a), copy(b), t, (v) => v, 0, 4, 0));
}
const turnA = stroke([[-40, -10], [40, -10], [40, 10], [-40, 10]], { closed: true, fillColor: '#cc5533' });
const turnB = stroke([[10, -40], [10, 40], [-10, 40], [-10, -40]], { closed: true, fillColor: '#3366cc' });
call('interpolate', { a: turnA, b: turnB, t: 0.5 }, () => context.interpStroke(copy(turnA), copy(turnB), 0.5, (v) => v, 0, 4, 0));
for (const [name, s] of [['line', a], ['closed', turnA], ['curved', { ...a, segments: [seg(0, 0), { ...seg(40, 20), handleIn: [-30, 0] }] }]]) {
  call('resample', { name, stroke: s, count: 6 }, () => context.resamplePJS(copy(s), 6));
}
const flipped = { ...b, segments: copy(b.segments).reverse() };
call('align', { a, b: flipped }, () => context.alignResampledPairJS(copy(a), copy(flipped)));
const matchA = [a, stroke([[0, 50], [40, 50]], { strokeColor: '#ff0000' })];
const matchB = [stroke([[20, 80], [60, 80]], { strokeColor: '#ff0000' }), b];
call('match', { a: matchA, b: matchB }, () => context.autoMatchJS(copy(matchA), copy(matchB)));
const partition = [a, { ...b, isBrushTextureCopy: true, brushGroupId: 'g1' }, { ...b, isLinkedFillCompanion: true, linkedFillId: 's1' },
  { ...b, isRevisionGhost: true }, { isRaster: true }, { ...b, tweenOn: true }];
for (const manual of [false, true]) call('partition', { strokes: partition, manual }, () => context.splitTweenables(copy(partition), manual));
// Relational matching, including nonuniform movement, unequal counts and repeated shapes.
for (let variant = 0; variant < 6; variant++) {
  const left = Array.from({ length: 8 }, (_, i) => stroke([[i % 4 * 35, Math.floor(i / 4) * 55],
    [i % 4 * 35 + 10 + i * 2, Math.floor(i / 4) * 55 + 4],
    [i % 4 * 35 + 22 + i * 3, Math.floor(i / 4) * 55]], { strokeColor: i % 3 ? '#112233' : '#bb1122' }));
  const right = left.map((s, i) => ({ ...copy(s), segments: s.segments.map((p) => ({ ...copy(p),
    point: [p.point[0] + 18 + (i < 4 ? variant * 4 : -variant * 2), p.point[1] + 12 + (i % 3) * variant * 3] })) }));
  right.reverse();
  if (variant === 4) right.splice(2, 1);
  if (variant === 5) right.push(stroke([[240, 90], [280, 90]], { strokeColor: '#00ff00' }));
  call('match', { name: `relational-${variant}`, a: left, b: right }, () => context.autoMatchJS(copy(left), copy(right)));
}
const hairpin = stroke([[0, 0], [35, 0], [70, 0], [100, 0], [100, 10], [70, 10], [35, 10], [0, 10]]);
const hairpinB = { ...copy(hairpin), segments: copy(hairpin.segments).reverse() };
call('align', { a: hairpin, b: hairpinB }, () => context.alignResampledPairJS(copy(hairpin), copy(hairpinB)));
const toolsSource = read('src/js/tools.js');
const rngStart = toolsSource.indexOf('function seededRng(');
vm.runInContext(toolsSource.slice(rngStart, toolsSource.indexOf('// Tip-shape geometry', rngStart)), context);
for (const seed of [0, 1, 0xffffffff, 123456789]) {
  call('rng', { seed, count: 16 }, () => { const rng = context.seededRng(seed); return Array.from({ length: 16 }, () => rng()); });
}
// Longer deforming strokes exercise the original intrinsic/MLS/fold/handle strategy probes.
for (let variant = 0; variant < 5; variant++) {
  const a = stroke(Array.from({ length: 18 }, (_, i) => [i * 8, Math.sin(i * 0.35) * 22]));
  const b = stroke(Array.from({ length: 18 }, (_, i) => {
    if (variant === 0) return [i * 8 + 12, Math.sin(i * 0.35) * 22 + 9];
    if (variant === 1) return [-Math.sin(i * 0.35) * 22, i * 8];
    if (variant === 2) return [i * 8, Math.sin(i * 0.25 + 0.2) * 48];
    if (variant === 3) return [Math.sin(i * 0.15) * 92, i * 5];
    return [i * 6, Math.sin(i * 0.6) * 18];
  }));
  for (const t of [0.15, 0.5, 0.85]) call('interpolate', { name: `deform-${variant}`, a, b, t },
    () => context.interpStroke(copy(a), copy(b), t, (v) => v, 0, 4, 0));
}
// Production feature-aware sampling: moving corners, reversal, closed curves, pressure.
vm.runInContext(toolsSource.slice(toolsSource.indexOf('function widthAtFrac('), toolsSource.indexOf('// Paper.js', toolsSource.indexOf('function widthAtFrac('))), context);
const curveA = stroke([[0, 0], [60, 30], [40, 80]], { closed: true });
curveA.segments[0].handleIn = [-20, 0];
curveA.segments[2].handleOut = [-30, 0];
call('resample', { name: 'closing-handles', stroke: curveA, count: 6 }, () => context.resamplePJS(copy(curveA), 6));
for (const n of [6, 24, 90]) {
  const cornerA = stroke([[0, 0], [50, 0], [50, 60], [100, 60]]);
  const cornerB = stroke([[10, 20], [100, 20], [100, 60], [130, 100]]);
  const vbA = { ...copy(cornerA), isVectorBrush: true, centerSegments: cornerA.segments.map((s, i) => ({ ...s, width: i + 3 })), widthProfile: [{t:0,width:2},{t:0.3,width:8},{t:1,width:1}] };
  const vbB = { ...copy(cornerB), isVectorBrush: true, centerSegments: cornerB.segments.map((s, i) => ({ ...s, width: 7 - i })).reverse(), widthProfile: [{t:0,width:3},{t:0.7,width:9},{t:1,width:2}] };
  for (const [name, a, b, vb] of [['corners', cornerA, cornerB, false], ['reversed', cornerA, {...cornerB, segments: copy(cornerB.segments).reverse()}, false], ['closed-curves', curveA, {...curveA, segments: [curveA.segments[1],curveA.segments[2],curveA.segments[0]]},false], ['pressure',vbA,vbB,true]]) {
    call('pair', {name, a,b,n,vb}, () => context.resamplePairFeatureAware(copy(a),copy(b),n,vb));
  }
}
// Execute the original document-mutating generation entry point in an isolated baseline store.
vm.runInContext(source.slice(source.indexOf('function generateTweens('),source.indexOf('// #122 ("harmoniser')),context);
Object.assign(context,{n20AllowLegacyWrite:()=>true,saveAllLayerFrames:()=>{},pushUndoLayers:()=>{},
  loadFrame:()=>{},renderOS:()=>{},renderArcs:()=>{},updateUI:()=>{},showToast:()=>{},SM:{t:()=>''}});
const idStroke=(s,id)=>({...copy(s),strokeId:id,hasRealStroke:true});
for (const mode of ['move','reorder','step','manual','force','style','deform','fade','gradient-alpha','arc','force-arc']) {
  const aa=[idStroke(a,'a')],bb=[idStroke(b,'b')];
  if(mode==='reorder'){aa.push(idStroke(stroke([[0,60],[40,60]]),'a2'));bb.unshift(idStroke(stroke([[20,90],[60,90]]),'b2'));}
  if(mode==='force'){bb[0]=idStroke(stroke([[200,300],[350,350]]),'b');}
  if(mode==='fade'){bb[0]=idStroke(stroke([[800,900],[900,900],[900,1000],[800,1000]],{closed:true,fillColor:'#ff0000'}),'b');}
  if(mode==='style'){aa[0].dashArray=[4,3];bb[0].dashArray=[8,2];aa[0].dashOffset=3;bb[0].dashOffset=8;aa[0].miterLimit=10;bb[0].miterLimit=5;}
  if(mode==='gradient-alpha'){
    aa[0].fillColor='#ff0000';bb[0].fillColor='#0000ff';
    aa[0].fillGradient={kind:'linear',from:[0,0],to:[40,0],stops:[{offset:0,color:'#ff000020'},{offset:1,color:'#00ff0080'}]};
    bb[0].fillGradient={kind:'linear',from:[20,30],to:[60,30],stops:[{offset:0,color:'#0000ff80'},{offset:1,color:'#ff8800'}]};
  }
  if(mode==='deform'){aa[0]=idStroke(stroke([[0,0],[60,0],[60,60],[100,60]]),'a');bb[0]=idStroke(stroke([[10,10],[30,30],[90,90],[150,90]]),'b');}
  const frames=Array.from({length:6},()=>({strokes:[],isKeyframe:false,isInterpolated:false}));
  frames[0]={strokes:aa,isKeyframe:true,isInterpolated:false};frames[5]={strokes:bb,isKeyframe:true,isInterpolated:false};
  if(mode==='manual'){frames[2]={strokes:[idStroke(stroke([[200,200],[220,230]]),'manual')],isKeyframe:false,isInterpolated:true,isManualEdit:true};}
  const options={resamplePoints:12,step:mode==='step'?2:1,skipManual:true,easing:[]};
  const span={from:0,to:5,overrides:mode.startsWith('force')?[{aId:'a',bId:'b'}]:[]};
  context._sel={frames:[]};context.state={layers:[{layerUid:'L0',frames:copy(frames)}],activeLayerIdx:0,currentFrame:0,totalFrames:6,
    resamplePts:options.resamplePoints,tweenStep:options.step,tweenSkipManual:true,motionArcs:mode.includes('arc')?{['L0:0-5-'+(mode.startsWith('force')?-1:0)]:{out:[0,80],in:[0,-60]}}:{},tweenOverrides:{'L0:0-5':span.overrides}};
  call('generate',{name:mode,frames,options,span,motionArcs:copy(context.state.motionArcs)},()=>{context.generateTweens();return context.state.layers[0].frames;});
}
// The original vector dab producer (all built-in nib families and one pressure/custom case).
vm.runInContext(toolsSource.slice(toolsSource.indexOf('var BRUSH_PRESETS='),toolsSource.indexOf('// Builds the real, layer-inserted texture')),context);
for(const [key,preset] of Object.entries(context.BRUSH_PRESETS)){
  const rec={...stroke([[0,0],[24,7]]),brushGroupId:'group'};
  call('dabs',{name:key,rec,preset:copy(preset),width:5,seed:7919,mul:0.7},()=>context.dabRecordsForTween(copy(rec),key,'#223344',5,7919,0.7));
}
const custom={tipShape:'custom',customStamp:{closed:true,segments:[seg(-.5,-.5),seg(.5,-.5),seg(0,.5)]},roughness:.7,scatterDistribution:'center',noise:.3,sharpness:.5,spacing:.5};
context.state.customBrushPresets={'custom-case':custom};
const pressureRec={...stroke([[0,0],[24,7]]),brushGroupId:'group',isVectorBrush:true,centerSegments:[{...seg(0,0),width:2},{...seg(24,7),width:8}]};
call('dabs',{name:'custom-pressure',rec:pressureRec,preset:custom,width:5,seed:0xffffffff,mul:.8},()=>context.dabRecordsForTween(copy(pressureRec),'custom-case','#223344',5,0xffffffff,.8));
// Pressure geometry expectations use the original cleanup and closed-ring implementation.
context.CompoundPath=paper.CompoundPath;
vm.runInContext(toolsSource.slice(toolsSource.indexOf('function _polylineCrossCandidates('),toolsSource.indexOf('// Reverses buildClosedRingOutline')),context);
vm.runInContext(toolsSource.slice(toolsSource.indexOf('function _polyClosestPair('),toolsSource.indexOf('function ',toolsSource.indexOf('function _polyClosestPair(')+10)),context);
for(const [name,pts,widths] of [
 ['pressure-line',[[0,0],[40,0]],[3,8]],
 ['pressure-curve',[[0,0],[20,15],[50,10],[80,25]],[2,9,5,3]],
 ['pressure-cross',[[0,0],[40,40],[0,40],[40,0]],[5,8,3,7]],
 ['pressure-hairpin',[[0,0],[70,0],[70,3],[0,3]],[5,5,5,5]],
 ['pressure-vertical',[[12,0],[12,30],[12,70]],[5,5,5]],
 ['pressure-tiny',[[0,0],[.1,0]],[0,0]],
 ['pressure-ring',Array.from({length:13},(_,i)=>[50*Math.cos(i*Math.PI/6),35*Math.sin(i*Math.PI/6)]),Array.from({length:13},(_,i)=>5+i/2)],
]){
 const center=pts.map((p,i)=>({...seg(...p),width:widths[i]}));
 call('outline',{name,center},()=>context.outlineFromCenterSegs(copy(center)));
}
// End-to-end textured spans exercise partitioning (including >256 source dabs), ordering and fades.
for (const mode of ['texture-both','texture-a','texture-b','texture-fade','texture-custom','texture-alpha']) {
  const textured=(s,id,key)=>({...idStroke(s,id),strokeColor:null,hasRealStroke:false,opacity:0,
    brushTexturePreset:key,brushGroupId:'group-'+id,preTextureStroke:'#223344',preTextureOpacity:1});
  const left = mode==='texture-b' ? idStroke(a,'a') : textured(a,'a','pencil-thick');
  const right = mode==='texture-a' ? idStroke(b,'b') : textured(b,'b','pencil-thick');
  if(mode==='texture-custom'){left.brushTexturePreset=right.brushTexturePreset='custom-case';}
  if(mode==='texture-alpha'){delete left.preTextureStroke;delete right.preTextureStroke;left.strokeColor=right.strokeColor='#22334460';left.hasRealStroke=right.hasRealStroke=true;}
  context.state.customBrushPresets={'custom-case':custom};
  const bundle=(r)=>[r,...(r.brushTexturePreset?context.dabRecordsForTween(copy(r),r.brushTexturePreset,'#223344',4,17,1).map((d,i)=>({...d,strokeId:r.strokeId+'_source_'+i})):[])];
  const leftStrokes=bundle(left), rightStrokes=mode==='texture-fade'?[idStroke(stroke([[200,200],[220,210]],{closed:true,fillColor:'#ff0000'}),'other')]:bundle(right);
  if(mode==='texture-both') for(let i=leftStrokes.length;i<300;i++)leftStrokes.push({...copy(leftStrokes[1]),strokeId:'extra-dab-'+i});
  const frames=Array.from({length:4},()=>({strokes:[],isKeyframe:false,isInterpolated:false}));
  frames[0]={strokes:leftStrokes,isKeyframe:true,isInterpolated:false};frames[3]={strokes:rightStrokes,isKeyframe:true,isInterpolated:false};
  const options={resamplePoints:12,step:1,skipManual:true,easing:[]};
  context._sel={frames:[]};context.state={layers:[{layerUid:'L0',frames:copy(frames)}],activeLayerIdx:0,currentFrame:0,totalFrames:4,
    resamplePts:12,tweenStep:1,tweenSkipManual:true,motionArcs:{},customBrushPresets:mode==='texture-custom'?{'custom-case':custom}:{}};
  call('generate',{name:mode,frames,options,customBrushPresets:copy(context.state.customBrushPresets)},()=>{context.generateTweens();return context.state.layers[0].frames;});
}
// One-to-many and many-to-one assignment uses ordered arc-fraction pieces.
for(const mode of ['auto-split-b','auto-split-a','force-multiple']){
 const parts=[idStroke(stroke([[0,0],[35,0]]),'part-a'),idStroke(stroke([[36,0],[80,0]]),'part-b')];
 const merged=[idStroke(stroke([[2,5],[82,5]]),'merged')];
 const frames=Array.from({length:4},()=>({strokes:[],isKeyframe:false,isInterpolated:false}));
 frames[0]={strokes:mode==='auto-split-a'?merged:parts,isKeyframe:true,isInterpolated:false};
 frames[3]={strokes:mode==='auto-split-a'?parts:merged,isKeyframe:true,isInterpolated:false};
 const options={resamplePoints:12,step:1,skipManual:true,easing:[]};
 const span={from:0,to:3,overrides:mode==='force-multiple'?[{aIds:['part-b','part-a'],bId:'merged'}]:[]};
 context._sel={frames:[]};context.state={layers:[{layerUid:'L0',frames:copy(frames)}],activeLayerIdx:0,currentFrame:0,totalFrames:4,resamplePts:12,tweenStep:1,tweenSkipManual:true,motionArcs:{},tweenOverrides:{'L0:0-3':span.overrides}};
 call('generate',{name:mode,frames,options,span,motionArcs:copy(context.state.motionArcs)},()=>{context.generateTweens();return context.state.layers[0].frames;});
}
// Regeneration reuses persisted overrides; splitting promotes the corrected cell then regenerates its two neighbors.
const uiSource=read('src/js/ui.js');
vm.runInContext(uiSource.slice(uiSource.indexOf('function cubicAt('),uiSource.indexOf('function evalCurve(')),context);
for(const mode of ['regenerate-force','split-generated','span-easing','selected-span']) {
 const origin=cases.find(c=>c.operation==='generate'&&c.input.name===(mode==='regenerate-force'?'force':'move'));
 const frames=copy(mode==='regenerate-force'||mode==='split-generated'?origin.expected:origin.input.frames);
 const options=copy(origin.input.options),span=copy(origin.input.span);
 if(mode==='span-easing')span.easing=[{x:0,y:0},{x:.3,y:.1},{x:.7,y:.85},{x:1,y:1}];
 if(mode==='selected-span'){
   const extra=copy(frames[5]);extra.strokes[0].segments.forEach(s=>{s.point[0]+=60;s.point[1]+=50;});
   frames.push({strokes:[],isKeyframe:false,isInterpolated:false},extra);
 }
 const command=mode==='split-generated'?'split':mode==='regenerate-force'?'regenerate':'generate';
 const selection=mode==='selected-span'?[2]:undefined;
 context._sel={frames:selection?selection.map(frame=>({layer:0,frame})):[]};
 context._curveEditor={evalCurve:t=>t,evalPointsCurve:context.evalPointsCurve};
 context.state={layers:[{layerUid:'L0',frames:copy(frames)}],activeLayerIdx:0,currentFrame:0,totalFrames:frames.length,resamplePts:options.resamplePoints,tweenStep:options.step,tweenSkipManual:true,motionArcs:{},
 tweenOverrides:{'L0:0-5':span.overrides},tweenEasing:span.easing?{'L0:0-5':{points:span.easing}}:{}};
 const tweens=mode==='regenerate-force'||mode==='span-easing'?{layers:[{layerUid:'L0',options,spans:[span]}]}:undefined;
 call('generate',{name:mode,frames,options,span,command,tweens,selection,frame:command==='split'?2:undefined},()=>{
   if(command==='split'){context.state.layers[0].frames[2].isKeyframe=true;context.state.layers[0].frames[2].isInterpolated=false;context.generateTweens({0:true,2:true});}
   else context.generateTweens();return context.state.layers[0].frames;
 });
}
delete context._curveEditor;
// D21: editing an endpoint key after generation. The real baseline saveActiveLayerFrame (src/js/app.js) runs on the
// edited key; the in-betweens must stay untouched and unflagged, and only an explicit regenerate recomputes them.
const appSource=read('src/js/app.js');
{
 const origin=cases.find(c=>c.operation==='generate'&&c.input.name==='move');
 const generated=copy(origin.expected),options=copy(origin.input.options),span=copy(origin.input.span);
 const edited=copy(generated[5].strokes);edited[0].segments[0].point[0]+=10;edited[0].segments[0].point[1]+=10;
 const save=vm.createContext({window:{_tweenFrameDirty:true,_sceneVersion:0},JSON,state:{activeLayerIdx:0,currentFrame:5,layers:[{layerUid:'L0',frames:copy(generated)}]},
  n20RequireLegacyWrite(){},layerIsEffectivelyVisible:()=>true,layerHasTimeRange:()=>false,_writeBackGhostProxies(){},
  _collectLayerStrokes:()=>copy(edited),strokesEqual:(x,y)=>JSON.stringify(x)===JSON.stringify(y)});
 vm.runInContext(appSource.slice(appSource.indexOf('function _maybePromoteInterpolated('),appSource.indexOf('function saveAllLayerFrames(')),save);
 vm.runInContext('saveActiveLayerFrame()',save);
 const stay=copy(save.state.layers[0].frames);
 call('endpoint-edit',{name:'endpoint-edit-move',frames:generated,edited:{frame:5,strokes:edited},options,span},()=>{
  context._sel={frames:[]};
  context.state={layers:[{layerUid:'L0',frames:copy(stay)}],activeLayerIdx:0,currentFrame:0,totalFrames:6,resamplePts:options.resamplePoints,tweenStep:options.step,tweenSkipManual:true,motionArcs:{},tweenOverrides:{'L0:0-5':span.overrides}};
  context.generateTweens();
  return {stay,regenerated:context.state.layers[0].frames};
 });
}

// Pressure generation reuses original centreline interpolation, outline cleanup and per-pair seeded dabs.
for(const mode of ['pressure-pair','pressure-texture','pressure-mixed','pressure-unmatched','pressure-backdrop-both','pressure-backdrop-a','pressure-backdrop-vanish']) {
 const pressure=(pts,widths,id)=>{
   const cs=pts.map((p,i)=>({...seg(...p),width:widths[i]}));
   return {...stroke([]),strokeId:id,segments:context.outlineFromCenterSegs(copy(cs)),centerSegments:cs,
     closed:true,strokeColor:null,fillColor:'#223344',strokeWidth:4,hasRealStroke:false,isVectorBrush:true};
 };
 const left=pressure([[0,0],[30,8],[60,0]],[2,8,4],'a');
 let right=pressure([[12,10],[45,15],[75,8]],[5,6,3],'b');
 if(mode==='pressure-mixed')right=idStroke(b,'b');
 if(mode==='pressure-unmatched'||mode==='pressure-backdrop-vanish')right=idStroke(stroke([[500,500],[530,500]],{strokeColor:'#ff0000'}),'b');
 if(mode==='pressure-texture')for(const r of [left,right])Object.assign(r,{brushTexturePreset:'pencil-thick',brushGroupId:'group-'+r.strokeId,preTextureStroke:'#223344',preTextureOpacity:1,opacity:0});
 const backdrop = (r,color) => {
   const link='fill-'+r.strokeId;r.linkedFillId=link;
   return {...context._backdropFromCenterSegs(r.centerSegments,color,.8),strokeId:link,linkedFillId:link,isLinkedFillCompanion:true};
 };
 const bdA=mode.startsWith('pressure-backdrop')?backdrop(left,'#884433'):null;
 const bdB=mode==='pressure-backdrop-both'?backdrop(right,'#228855'):null;
 const bundle=r=>[...(r===left&&bdA?[bdA]:r===right&&bdB?[bdB]:[]),r,...(r.brushTexturePreset?context.dabRecordsForTween(copy(r),r.brushTexturePreset,'#223344',4,17,1).map((d,i)=>({...d,strokeId:r.strokeId+'_source_'+i})):[])];
 const frames=[{strokes:bundle(left),isKeyframe:true,isInterpolated:false},{strokes:[],isKeyframe:false,isInterpolated:false},{strokes:bundle(right),isKeyframe:true,isInterpolated:false}];
 const options={resamplePoints:12,step:1,skipManual:true,easing:[]};
 context._sel={frames:[]};context.state={layers:[{layerUid:'L0',frames:copy(frames)}],activeLayerIdx:0,currentFrame:0,totalFrames:3,resamplePts:12,tweenStep:1,tweenSkipManual:true,motionArcs:{}};
 call('generate',{name:mode,frames,options},()=>{context.generateTweens();return context.state.layers[0].frames;});
}
// Rasters are held from A; an old generated raster is replaced along with the whole cell.
for(const mode of ['held-raster','raster-only-a','raster-only-b','held-manual']) {
 const raster=(id,src,x)=>({isRaster:true,strokeId:id,src,x,y:20,width:16,height:12,opacity:.6});
 let aa=[idStroke(a,'a'),raster('image-a','img-1',10)],bb=[idStroke(b,'b'),raster('image-b','img-2',90)];
 if(mode==='raster-only-a')aa=aa.slice(1);
 if(mode==='raster-only-b')bb=bb.slice(1);
 if(mode==='held-manual'){aa.push(idStroke(stroke([[30,70],[60,70]]),'held'));aa[0].tweenOn=true;}
 const frames=[{strokes:aa,isKeyframe:true,isInterpolated:false},{strokes:[raster('old','img-2',0)],isKeyframe:false,isInterpolated:true},{strokes:bb,isKeyframe:true,isInterpolated:false}];
 if(mode==='held-manual')frames[0].tweenManualMode=true;
 const options={resamplePoints:12,step:1,skipManual:true,easing:[]};
 const span={from:0,to:2,overrides:[],manualStrokes:['a']};
 const tweens=mode==='held-manual'?{layers:[{layerUid:'L0',options,spans:[span]}]}:undefined;
 context._sel={frames:[]};context.state={layers:[{layerUid:'L0',frames:copy(frames)}],activeLayerIdx:0,currentFrame:0,totalFrames:3,resamplePts:12,tweenStep:1,tweenSkipManual:true,motionArcs:{}};
 call('generate',{name:mode,frames,options,tweens,requiresCompositionOrder:mode==='held-manual'},()=>{context.generateTweens();return context.state.layers[0].frames;});
}
// Original bitmap orchestration and bake bounds. Canvas is a recording sink, not a pixel oracle.
// Pixel/resource admission is exercised separately through the existing native baker and PNG decoder.
const canvas=()=>{
 const c={width:0,height:0,toDataURL:()=>`baseline-bitmap:${c.width}x${c.height}`};
 const ctx=new Proxy({}, {get:(o,k)=>k==='createRadialGradient'?()=>({addColorStop(){}}):k==='canvas'?c:()=>{},set:(o,k,v)=>(o[k]=v,true)});
 c.getContext=()=>ctx;return c;
};
const bitmapSource=read('src/js/bitmap-brush.js');
context.document={readyState:'loading',addEventListener(){},createElement:()=>canvas()};
vm.runInContext(bitmapSource,context);
for(const mode of ['bitmap-both','bitmap-a','bitmap-b','bitmap-fade']) {
 const spec={tip:'soft',size:8,spacing:50,scatter:20,opacity:.75,color:'#334455',pressure:true,seed:12};
 const textured=(s,id)=>({...idStroke(s,id),strokeColor:null,hasRealStroke:false,opacity:0,preTextureOpacity:1,bitmapBrushSpec:copy(spec),brushGroupId:'bitmap-'+id});
 const left=mode==='bitmap-b'?idStroke(a,'a'):textured(a,'a');
 const right=mode==='bitmap-a'?idStroke(b,'b'):mode==='bitmap-fade'?idStroke(stroke([[600,600],[680,600]],{strokeColor:'#ff0000'}),'b'):textured(b,'b');
 const bundle=(r,src)=>[r,...(r.bitmapBrushSpec?[{isRaster:true,strokeId:r.strokeId+'-source',src,x:20,y:20,width:70,height:30,opacity:.75,isBrushTextureCopy:true,isBitmapBrush:true,brushGroupId:r.brushGroupId}]:[])];
 const frames=[{strokes:bundle(left,'img-1'),isKeyframe:true,isInterpolated:false},{strokes:[],isKeyframe:false,isInterpolated:false},{strokes:bundle(right,'img-2'),isKeyframe:true,isInterpolated:false}];
 const options={resamplePoints:12,step:1,skipManual:true,easing:[]};
 context._sel={frames:[]};context.state={layers:[{layerUid:'L0',frames:copy(frames)}],activeLayerIdx:0,currentFrame:0,totalFrames:3,resamplePts:12,tweenStep:1,tweenSkipManual:true,motionArcs:{}};
 call('generate',{name:mode,frames,options},()=>{context.generateTweens();return context.state.layers[0].frames;});
}
// Run the original overlay orchestration. Rendering sinks only record its final native-consumable policy.
vm.runInContext(source.slice(source.indexOf('function renderGhostAll('),source.indexOf('// ---- UNDO ----')),context);
const overlayOrigin=copy(cases.find(c=>c.operation==='generate'&&c.input.name==='move').expected);
for(const mode of ['default','tinted','outline']) {
 const frames=copy(overlayOrigin);
 for(const f of frames){
   f.strokes.push({...copy(f.strokes[0]),strokeId:'invisible',strokeColor:null,fillColor:null,hasRealStroke:false,opacity:0,brushTexturePreset:'pencil-thick'});
   f.strokes.push({...copy(f.strokes[0]),strokeId:'dab',strokeColor:null,fillColor:'#445566',hasRealStroke:false,opacity:.35,isBrushTextureCopy:true});
 }
 const prefs={'nemo-onion-tint-prev':'#aabbcc','nemo-onion-tint-next':'#112244','nemo-onion-shift':JSON.stringify({x:4,y:-3,next:true})};
 const layer=()=>({children:[],removeChildren(){this.children=[];}});
 const prev=layer(),next=layer(),ghost=layer();
 let sourceFrame;
 const sink=(sd,layer,op)=>{
   const p={id:sd.strokeId,sourceFrame,data:{},opacity:op,clearFill:false,translation:[0,0],stroke:null,strokeWidth:null,
     translate(v){this.translation[0]+=v.x;this.translation[1]+=v.y;}};
   Object.defineProperty(p,'strokeColor',{set(v){this.stroke=[v.red,v.green,v.blue,v.alpha];}});
   Object.defineProperty(p,'fillColor',{set(v){this.clearFill=v===null;}});
   layer.children.push(p);return p;
 };
 Object.assign(context,{Color:paper.Color,localStorage:{getItem:k=>prefs[k]||null},onionPrevLayer:prev,onionNextLayer:next,ghostAllLayer:ghost,
   desP:sink,desR:(sd,l)=>sink(sd,l,sd.opacity),userLayers:[{activate(){}}],getEffectiveStrokesRendered:(li,fi)=>{sourceFrame=fi;return copy(frames[fi].strokes);}});
 context.state={layers:[{layerUid:'L0',frames}],activeLayerIdx:0,currentFrame:2,totalFrames:6,ghostAllFrames:true,onionSkin:true,onionIn:0,onionOut:5,onionPrevOpacity:30,onionNextOpacity:40,onionMode:mode};
 const query={contextId:'scene-root',layerUid:'L0',frame:2,onion:true,ghostAll:true,from:0,to:5,previousOpacity:30,nextOpacity:40,mode,previousTint:'#aabbcc',nextTint:'#112244',shift:{x:4,y:-3,next:true}};
 call('ghosts',{frames,query},()=>{
   context.renderOS();
   return [[ghost,'allKeys'],[prev,'previous'],[next,'next']].flatMap(([layer,kind])=>layer.children.map(p=>({kind,frame:kind==='allKeys'?p.data.ghostFrame:p.sourceFrame,id:p.id,visual:{opacity:p.opacity,stroke:p.stroke,strokeWidth:p.strokeWidth,clearFill:p.clearFill,translation:p.translation,omit:false}})));
 });
}
// Actual baseline dict combines used by Ghost All; the native join retains the existing boolean provider.
const groupSource=read('src/js/group-bridge.js');
vm.runInContext(groupSource,context);
vm.runInContext(toolsSource.slice(toolsSource.indexOf('function _flattenSegsToPoints('),toolsSource.indexOf('// ---- DYNAMIC SHAPES, phase 1')),context);
const wasmSource=read('src/wasm/geometry_wasm.js');
const wasmBytes=cp.execFileSync('git',['show',`${baseline}:src/wasm/geometry_wasm_bg.wasm`],{cwd:root,maxBuffer:32*1024*1024});
if(fs.readFileSync(path.join(root,'src/wasm/geometry_wasm.js'),'utf8')!==wasmSource ||
 !fs.readFileSync(path.join(root,'src/wasm/geometry_wasm_bg.wasm')).equals(wasmBytes))throw new Error('WASM sources must match pinned baseline');
const wasm=require(path.join(root,'src/wasm/geometry_wasm.js'));wasm.initSync({module:wasmBytes});
vm.runInContext(toolsSource.slice(toolsSource.indexOf('function _findRingRevisit('),toolsSource.indexOf('// Places a freshly-committed Fill Brush')),context);
for(const provider of ['paper','wasm']) for(const mode of ['unite','subtract','intersect','exclude']) {
  context.GeometryWasm=provider==='wasm'?{ready:true,boolean_op_multi:wasm.boolean_op_multi}:null;
  context._combineMemo=Object.create(null);context._combineMemoKeys=[];
  const shapes=[idStroke(stroke([[0,0],[40,0],[40,30],[0,30]],{closed:true,fillColor:'#cc5533'}),'a'),
    idStroke(stroke([[20,10],[60,10],[60,40],[20,40]],{closed:true,fillColor:'#3366cc'}),'b')];
  const groups={g:{combineMode:mode,order:['a','b']}};
  const output=context.SMGroup.applyCombinesToStrokes(copy(shapes),{groups},0,0).filter(s=>s.fillColor||s.strokeColor);
  if(!output.length||!output.every(s=>s.isGroupCombineResult))throw new Error('baseline ghost combine failed '+mode);
  call(provider==='wasm'?'ghost-combine':'ghost-combine-paper',{shapes,groups,provider},()=>output);
}

// D20 topology fixtures: nested outer/inner contours, reverse winding, and zero-length inputs.
const outer=stroke([[0,0],[100,0],[100,100],[0,100]],{closed:true,fillColor:'#cc5533'});
const inner=stroke([[25,25],[25,75],[75,75],[75,25]],{closed:true,fillColor:'#cc5533'});
const shifted=s=>({...copy(s),segments:s.segments.map(q=>({...copy(q),point:[q.point[0]+17,q.point[1]-9]}))});
for(const [name,left,right] of [
 ['nested-outer-inner-contours',[outer,inner],[shifted(inner),shifted(outer)]],
 ['reversed-nested-contours',[outer,inner],[shifted({...inner,segments:copy(inner.segments).reverse()}),shifted({...outer,segments:copy(outer.segments).reverse()})]],
 ['collapsed-tie',[stroke([[4,8],[4,8]]),stroke([[4,8],[4,8]])],[stroke([[4,8],[4,8]]),stroke([[4,8],[4,8]])]],
 ['empty-target',[outer,inner],[]],['empty-source',[],[outer,inner]],
]) call('match',{name,a:left,b:right},()=>context.autoMatchJS(copy(left),copy(right)));
for(const [name,s] of [
 ['reversed-outer-contour',{...outer,segments:copy(outer.segments).reverse()}],
 ['inner-counterwound-contour',inner],['collapsed',stroke([[4,8],[4,8],[4,8]])],
 ['single-anchor',stroke([[4,8]])],['empty-path',stroke([])],
]) call('resample',{name,stroke:s,count:8},()=>context.resamplePJS(copy(s),8));

const hash=s=>crypto.createHash('sha256').update(s).digest('hex');
const provenance={baseline,sourceSha256:hash(source),toolsSha256:hash(toolsSource),uiSha256:hash(uiSource),bitmapSha256:hash(bitmapSource),appSha256:hash(appSource),groupSha256:hash(groupSource),wasmJsSha256:hash(wasmSource),wasmBinarySha256:hash(wasmBytes),paperSha256:hash(read('src/paper-full.min.js'))};
process.stdout.write(JSON.stringify(provenance).slice(0,-1)+',"cases":[\n'+cases.map(c=>JSON.stringify(c)).join(',\n')+'\n]}\n');
