'use strict';
// Execute the frozen production editor, independently of the native implementation.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { execFileSync } = require('node:child_process');
const { createHash } = require('node:crypto');
const root = path.resolve(__dirname, '../../../..');
const baseline = '46442ea96c09e6bfaadf956eb8586e273a658085';
const read = (p) => execFileSync('git', ['show', `${baseline}:${p}`], { cwd: root, encoding: 'utf8', maxBuffer: 8e6 });
const sources = Object.fromEntries(['src/js/tools.js', 'src/js/subselect-bridge.js', 'src/js/timeline.js', 'src/js/app.js', 'src/paper-full.min.js'].map(p => [p, read(p)]));
if (fs.readFileSync(path.join(root, 'src/paper-full.min.js'), 'utf8') !== sources['src/paper-full.min.js']) throw Error('Paper baseline changed');
const paper = require(path.join(root, 'tests/helpers/native-import-paper.cjs')).load();
const copy = v => JSON.parse(JSON.stringify(v));
const noop = () => {};
let selected;
const c = vm.createContext({ Point: paper.Point, Path: paper.Path, Segment: paper.Segment, Math,
  nodeEditTargetPath: () => selected, _nodeSel: [], allowLegacySelectionEdit: () => true,
  pushUndo: noop, renderNodeHandles: noop, fillRegenerateLinked: noop, saveActiveLayerFrame: noop,
  updateUI: noop, rebuildVectorBrushOutline: noop, nodeSelCommitTail: noop,
  userLayers: [{}], state: { activeLayerIdx: 0, tool:'subselect' }, view: { zoom: 1 },
  _nmq:{active:false}, renderArcs:noop, toLocalPoint:p=>p, event:{preventDefault:noop},
  nodeEditSegmentsData:p=>p.data.isVectorBrush?p.data.centerSegments:result(false),
  SM:{t:s=>s}, showToast:noop });
c.window=c;
c.SMEngineBridge={screenToWorld:(x,y)=>[x,y],isEnabled:()=>true,renderNow:noop};
const tools = sources['src/js/tools.js'], bridge = sources['src/js/subselect-bridge.js'];
vm.runInContext(tools.slice(tools.indexOf('function _ptGet('), tools.indexOf('// ---- BOOLEAN PATH OPERATIONS')), c);
vm.runInContext(tools.slice(tools.indexOf('function insertVertexAt('), tools.indexOf('function renderNodeHandles(')), c);
vm.runInContext(bridge.slice(bridge.indexOf('  function toggleTangentAtSegments('), bridge.indexOf('  function onDown(')), c);
vm.runInContext(bridge.slice(bridge.indexOf('  function onMove('),bridge.indexOf('  function onUp(')),c);
const timeline=sources['src/js/timeline.js'];
const deletion=timeline.slice(timeline.indexOf("else if(state.tool==='subselect'&&_nodeSel.length>0)"),timeline.indexOf('    // Same rule as Cmd+D'));
if(!deletion.startsWith('else if') || !deletion.includes('idxsDesc'))throw Error('Baseline deletion slice missing');
vm.runInContext('function deleteBaseline(){'+deletion.replace(/^else /,'')+'}',c);
const anchor = (x,y,hi=[0,0],ho=[0,0]) => ({point:[x,y],handleIn:hi,handleOut:ho});
const paths = [
  {name:'open',closed:false,segments:[anchor(0,0),anchor(30,20,[-4,3],[16,2]),anchor(100,0)]},
  {name:'closed',closed:true,segments:[anchor(0,0),anchor(30,20),anchor(100,0),anchor(60,-35)]},
  {name:'coincident',closed:false,segments:[anchor(0,0),anchor(0,0),anchor(0,0)]},
  {name:'tiny-handles',closed:false,segments:[anchor(0,0),anchor(20,10,[.009,.009],[-.009,-.009]),anchor(70,25)]},
];
const result = pressure => pressure ? copy(selected.data.centerSegments) : selected.segments.map(s => ({
  point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]}));
const app=sources['src/js/app.js'];
vm.runInContext(app.slice(app.indexOf('function _r3('),app.indexOf('function desP(')),c);
const cases=[];
function run(p,pressure,intent,fn) {
  selected = new paper.Path({insert:false,closed:p.closed,segments:p.segments.map(s => new paper.Segment(s.point,s.handleIn,s.handleOut))});
  if(p.vectorExt)selected.data=copy(p.vectorExt);
  if (pressure) selected.data = {isVectorBrush:true,centerSegments:copy(p.segments)};
  c._nodeSel = intent.indices || [intent.index];
  fn();
  cases.push({name:p.name,pressure,closed:p.closed,segments:copy(p.segments),intent,expected:result(pressure)});
  if(p.vectorExt){const saved=c.serP(selected);Object.assign(cases[cases.length-1],{vectorExt:copy(p.vectorExt),expectedVectorExt:{paramShape:copy(saved.paramShape),boxAngle:saved.boxAngle}});}
  selected.remove();
}
for(const p of paths) for(const pressure of [false,true]) {
  for(const pointType of ['corner','smooth','symmetric']) for(const indices of [[0],[1],[p.segments.length-1],[0,1]])
    run(p,pressure,{kind:'pointType',indices,pointType},()=>c.setPointType(pointType));
  for(const index of [0,1,p.segments.length-1]) run(p,pressure,{kind:'toggle',index},()=>c.toggleTangentAt(selected,index));
  run(p,pressure,{kind:'move',indices:[0,1],delta:[-7,13]},()=>c.nodeSelApplyMove(-7,13));
  run(p,pressure,{kind:'scale',indices:[0,1],scale:[-2,.5],anchor:[12,17]},()=>c.nodeSelApplyScale(-2,.5,new paper.Point(12,17)));
  run(p,pressure,{kind:'rotate',indices:[0,1],degrees:37,center:[12,17]},()=>c.nodeSelApplyRotate(37,new paper.Point(12,17)));
  for(const handle of ['handleIn','handleOut'])for(const breakPair of [false,true]) {
    run(p,pressure,{kind:'handle',index:1,handle,point:[47,31],breakPair},()=>{
      c._nodeDrag={active:true,type:handle,path:selected,segIndex:1};c.lastPt=new paper.Point(0,0);
      c.onMove({clientX:47,clientY:31,altKey:breakPair,shiftKey:false,stopImmediatePropagation:noop,preventDefault:noop});
    });
  }
  run(p,pressure,{kind:'delete',indices:[1]},()=>c.deleteBaseline());
}
// The hit test is presentation-only; the native intent receives the exact curve index and parameter.
for(const p of paths.slice(0,2)) for(const pressure of [false,true]) for(const time of [.15,.5,.9]) {
  const intent={kind:'insert',curve:0,time};
  run(p,pressure,intent,()=>{
    const geometry=pressure ? new paper.Path({insert:false,segments:p.segments.map(s=>new paper.Segment(s.point,s.handleIn,s.handleOut))}) : selected;
    const location=geometry.curves[0].getLocationAtTime(time);
    // Path.divideAt converts the hit to arc length and solves back to a curve parameter.
    // Capture that actual split parameter, including Paper's original numerical tolerance.
    const hit=geometry.getNearestLocation(location.point);
    intent.curve=hit.index;intent.time=hit.curve.getTimeAt(hit.curveOffset);
    c.insertVertexAt(selected,location.point,1e-6);
    if(pressure)geometry.remove();
  });
}
// Parametric intent remains attached when the original editor reshapes the stored path.
for(const kind of ['rect','ellipse','star']){
  const shape=kind==='rect'?new paper.Path.Rectangle({from:[10,10],to:[70,50],insert:false}):
    kind==='ellipse'?new paper.Path.Ellipse({rectangle:[10,10,60,40],insert:false}):new paper.Path.Star({center:[40,30],points:5,radius1:10,radius2:25,insert:false});
  shape.rotate(17,new paper.Point(40,30));
  const paramShape=Object.assign({kind,box:{x1:10,y1:10,x2:70,y2:50}},kind==='rect'?{tl:0,tr:0,br:0,bl:0}:kind==='ellipse'?{startAngle:0,sweep:359.9,innerRadius:0}:{pointCount:5,innerRatio:.4,cornerRadius:0});
  const p={name:'parametric-'+kind,closed:true,vectorExt:{paramShape,boxAngle:17},segments:shape.segments.map(s=>({point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]}))};shape.remove();
  for(const pointType of ['corner','smooth','symmetric'])run(p,false,{kind:'pointType',indices:[1],pointType},()=>c.setPointType(pointType));
  run(p,false,{kind:'move',indices:[0,1],delta:[-7,13]},()=>c.nodeSelApplyMove(-7,13));
  run(p,false,{kind:'handle',index:1,handle:'handleOut',point:[47,31],breakPair:false},()=>{c._nodeDrag={active:true,type:'handleOut',path:selected,segIndex:1};c.lastPt=new paper.Point(0,0);c.onMove({clientX:47,clientY:31,altKey:false,shiftKey:false,stopImmediatePropagation:noop,preventDefault:noop});});
  run(p,false,{kind:'toggle',index:1},()=>c.toggleTangentAt(selected,1));
  run(p,false,{kind:'delete',indices:[1]},()=>c.deleteBaseline());
  const intent={kind:'insert',curve:0,time:.5};
  run(p,false,intent,()=>{const loc=selected.curves[0].getLocationAtTime(.5),hit=selected.getNearestLocation(loc.point);intent.curve=hit.index;intent.time=hit.curve.getTimeAt(hit.curveOffset);c.insertVertexAt(selected,loc.point,1e-6);});
}
// Execute the original marquee completion on real inserted Paper paths. The input
// matrices describe presentation geometry; the original receives those posed paths.
const up=bridge.indexOf('  function onUp('),end=bridge.indexOf('    } else if (_nodeDrag.active &&',up);
if(up<0||end<0)throw Error('Baseline marquee slice missing');
vm.runInContext(bridge.slice(up,end)+'\n    }\n  }',c);
const marquee=[];
const square={closed:true,segments:[anchor(20,20),anchor(80,20),anchor(80,80),anchor(20,80)]};
const inputs=[
  {name:'topmost',items:[square,square],matrices:[[1,0,0,1,0,0],[1,0,0,1,5,5]],from:[0,0],to:[50,50]},
  {name:'existing-target',items:[square,square],matrices:[[1,0,0,1,0,0],[1,0,0,1,5,5]],target:0,from:[0,0],to:[90,50]},
  {name:'shear',items:[square],matrices:[[1.5,.2,.5,1,0,0]],from:[10,10],to:[80,100]},
  {name:'reverse',items:[square],matrices:[[1.5,.2,.5,1,0,0]],from:[80,100],to:[10,10]},
  {name:'empty',items:[square],matrices:[[1,0,0,1,0,0]],from:[0,0],to:[10,10]},
  {name:'inside-without-anchors',items:[square],matrices:[[1,0,0,1,0,0]],from:[0,30],to:[40,40],target:0},
  {name:'bounds-touch',items:[square],matrices:[[1,0,0,1,0,0]],from:[0,0],to:[20,20]},
];
for(const input of inputs){
  const layer=new paper.Layer();
  const paths=input.items.map((p,i)=>{const path=new paper.Path({insert:false,closed:p.closed,segments:p.segments.map(s=>[s.point,s.handleIn,s.handleOut])});path.transform(new paper.Matrix(...input.matrices[i]));layer.addChild(path);return path;});
  if(paths.some(p=>p.parent!==layer))throw Error('Marquee fixture not inserted');
  c.userLayers=[layer];c.selectedPaths=input.target===undefined?[]:[paths[input.target]];c._nodeSel=[];
  c.nodeEditTargetPath=()=>c.selectedPaths[0]||null;c.nodeEditSegmentsData=p=>p.segments.map(s=>({point:[s.point.x,s.point.y]}));
  c.clearSel=()=>{c.selectedPaths=[];c._nodeSel=[];};c.isSelectablePathChild=()=>true;c.getSI=p=>paths.indexOf(p);
  c._nodeDrag={active:false};c._nmq={active:true,rect:{bounds:new paper.Rectangle(new paper.Point(input.from),new paper.Point(input.to)),remove:noop}};
  c.onUp({stopImmediatePropagation:noop,preventDefault:noop});
  marquee.push({...input,expected:{target:c.selectedPaths.length?paths.indexOf(c.selectedPaths[0]):null,indices:Array.from(c._nodeSel)}});
  layer.remove();
}
const hashes=Object.fromEntries(Object.entries(sources).map(([p,s])=>[p,createHash('sha256').update(s).digest('hex')]));
process.stdout.write(JSON.stringify({baseline,hashes,cases,marquee})+'\n');
