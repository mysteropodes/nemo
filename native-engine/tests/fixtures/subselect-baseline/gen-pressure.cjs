'use strict';
const path=require('node:path'),vm=require('node:vm'),{execFileSync}=require('node:child_process'),{createHash}=require('node:crypto');
// Freeze the original edit and ribbon-rebuild behavior independently. Publication/UI tails are no-ops;
// the production centerline edits, Paper geometry and pressure outline rebuild are executed unchanged.
const root=path.resolve(__dirname,'../../../..'),baseline='46442ea96c09e6bfaadf956eb8586e273a658085',read=p=>execFileSync('git',['show',baseline+':'+p],{cwd:root,encoding:'utf8',maxBuffer:8e6});
const sources=Object.fromEntries(['src/js/tools.js','src/js/subselect-bridge.js','src/js/app.js','src/paper-full.min.js'].map(p=>[p,read(p)]));
if(require('node:fs').readFileSync(path.join(root,'src/paper-full.min.js'),'utf8')!==sources['src/paper-full.min.js'])throw Error('Pressure oracle requires the frozen baseline Paper bytes');
const paper=require(path.join(root,'tests/helpers/native-import-paper.cjs')).load(),noop=()=>{},copy=v=>JSON.parse(JSON.stringify(v));let selected;
const c=vm.createContext({Math,Path:paper.Path,CompoundPath:paper.CompoundPath,Point:paper.Point,Segment:paper.Segment,console,
 nodeEditTargetPath:()=>selected,_nodeSel:[1],allowLegacySelectionEdit:()=>true,pushUndo:noop,nodeSelCommitTail:noop,renderNodeHandles:noop,fillRegenerateLinked:noop,saveActiveLayerFrame:noop,updateUI:noop,userLayers:[{}],state:{activeLayerIdx:0,tool:'subselect',smoothing:0},view:{zoom:1},_nmq:{active:false},toLocalPoint:p=>p,nodeEditSegmentsData:p=>p.data.centerSegments,SM:{t:s=>s},showToast:noop});
c.window=c;c.SMEngineBridge={screenToWorld:(x,y)=>[x,y],isEnabled:()=>true,renderNow:noop};
const tools=sources['src/js/tools.js'],bridge=sources['src/js/subselect-bridge.js'],app=sources['src/js/app.js'];
for(const [begin,end] of [['function _polylineCrossCandidates(','// Reverses buildClosedRingOutline'],['function widthAtFrac(','// Paper.js'],['function sampleVectorBrushCenterline(','window.sampleVectorBrushCenterline'],['function rebuildVectorBrushOutline(','// ---- BOOLEAN PATH OPERATIONS'],['function insertVertexAt(','function renderNodeHandles(']]){
 const at=tools.indexOf(begin),stop=tools.indexOf(end,at);if(at<0||stop<0)throw Error(begin);vm.runInContext(tools.slice(at,stop),c);
}
const at=tools.indexOf('function _polyClosestPair(');vm.runInContext(tools.slice(at,tools.indexOf('function ',at+10)),c);
vm.runInContext(app.slice(app.indexOf('function applyBrushKeyline('),app.indexOf('// Persisted-coordinate rounding')),c);
vm.runInContext(bridge.slice(bridge.indexOf('  function toggleTangentAtSegments('),bridge.indexOf('  function onDown(')),c);
vm.runInContext(bridge.slice(bridge.indexOf('  function onMove('),bridge.indexOf('  function onUp(')),c);
const segment=(point,width,handleIn=[0,0],handleOut=[0,0])=>({point,handleIn,handleOut,width});
const paths=[{name:'line',center:[segment([0,0],3),segment([40,0],8),segment([80,10],4)]},{name:'curve',center:[segment([0,0],2,[0,0],[20,-20]),segment([40,20],9,[-15,-10],[20,20]),segment([90,10],3,[-20,15],[0,0])]}];
paths.push({name:'repeated-anchor',center:[segment([0,0],0),segment([0,0],-3),segment([45,15],6)]},{name:'collapsed',center:[segment([10,20],0),segment([10,20],-2),segment([10,20],3)]},{name:'near-loop',center:[segment([0,0],4),segment([40,0],8),segment([40,40],2),segment([0,40],6),segment([1,1],4)]});
const intents=[{kind:'pointType',indices:[1],pointType:'corner'},{kind:'pointType',indices:[1],pointType:'smooth'},{kind:'pointType',indices:[1],pointType:'symmetric'},{kind:'move',indices:[0,1],delta:[-7,13]},{kind:'scale',indices:[0,1],scale:[1.2,.5],anchor:[20,10]},{kind:'rotate',indices:[0,1],degrees:37,center:[20,10]},{kind:'handle',index:1,handle:'handleOut',point:[57,31],breakPair:false},{kind:'toggle',index:1},{kind:'handle',index:1,handle:'handleIn',point:[25,4],breakPair:true},{kind:'move',indices:[1],delta:[0,0]},{kind:'toggle',index:0}];
const result=()=>({center:copy(selected.data.centerSegments),outline:selected.segments.map(s=>({point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]})),closed:selected.closed,stroke:selected.strokeColor&&selected.strokeColor.toCSS(true)});
const cases=[];
for(const p of paths)for(const profile of [null,[{t:0,width:2},{t:.3,width:12},{t:.5,width:3},{t:.8,width:8},{t:1,width:1}],[{t:0,width:0},{t:.3,width:-2},{t:.3,width:5},{t:1,width:0}]])for(const intent of [...intents,{kind:'toggle',index:p.center.length-1}]){
 selected=new paper.Path({insert:false,strokeColor:'#223344',fillColor:'#223344'});selected.data={isVectorBrush:true,centerSegments:copy(p.center)};if(profile)selected.data.widthProfile=copy(profile);c._nodeSel=intent.indices||[intent.index];c.rebuildVectorBrushOutline(selected);const before=result();
 if(intent.kind==='pointType')c.setPointType(intent.pointType);
 else if(intent.kind==='move')c.nodeSelApplyMove(...intent.delta);
 else if(intent.kind==='scale')c.nodeSelApplyScale(...intent.scale,new paper.Point(intent.anchor));
 else if(intent.kind==='rotate')c.nodeSelApplyRotate(intent.degrees,new paper.Point(intent.center));
 else if(intent.kind==='toggle'){c.toggleTangentAt(selected,intent.index);c.rebuildVectorBrushOutline(selected);}
 else {c._nodeDrag={active:true,type:intent.handle,path:selected,segIndex:intent.index};c.lastPt=new paper.Point(0,0);c.onMove({clientX:intent.point[0],clientY:intent.point[1],altKey:intent.breakPair,shiftKey:false,stopImmediatePropagation:noop,preventDefault:noop});}
 cases.push({name:p.name,profile,intent,before,expected:result()});selected.remove();
}
process.stdout.write(JSON.stringify({baseline,hashes:Object.fromEntries(Object.entries(sources).map(([p,s])=>[p,createHash('sha256').update(s).digest('hex')])),cases})+'\n');
