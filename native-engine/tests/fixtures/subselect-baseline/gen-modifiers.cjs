'use strict';
const path=require('node:path'),vm=require('node:vm'),{execFileSync}=require('node:child_process'),{createHash}=require('node:crypto');
// The affine map is injected; these cases characterize pointer handlers, not toLocalPoint.
const root=path.resolve(__dirname,'../../../..'),baseline='46442ea96c09e6bfaadf956eb8586e273a658085';
const read=p=>execFileSync('git',['show',baseline+':'+p],{cwd:root,encoding:'utf8',maxBuffer:8e6});
const sources=Object.fromEntries(['src/js/tools.js','src/js/subselect-bridge.js'].map(p=>[p,read(p)]));
const paper=require(path.join(root,'tests/helpers/native-import-paper.cjs')).load(),noop=()=>{};
const segments=[[20,20],[80,20],[80,80],[20,80]].map((p,i)=>({point:p,handleIn:i===0?[0,-15]:[0,0],handleOut:i===0?[20,0]:[0,0]}));
const cases=[];
const moves=[{point:[47,39],shiftKey:true},{point:[54,38],shiftKey:false},{point:[62,37],shiftKey:true}];
for(const matrix of [[1,0,0,1,0,0],[1.5,.2,.5,1,0,0],[0,2,-.5,0,100,0]])for(const type of ['point','point-shift','group','group-shift','shift-add','handleIn','handleOut'])for(const alts of type.startsWith('handle')?[[false,false,false],[true,true,true],[false,true,true],[true,false,true],[false,true,false]]:[[false,false,false]]){
 const selected=new paper.Path({insert:false,closed:true,segments:segments.map(a=>[a.point,a.handleIn,a.handleOut])});
 const indices=type.startsWith('group')?[0,3]:type==='shift-add'?[3]:[0],key=type.startsWith('handle')?type:'point',anchor=selected.segments[0];
 const start=(key==='point'?anchor.point:anchor.point.add(anchor[key])).clone(),M=new paper.Matrix(...matrix),inverse=M.inverted();
 const c=vm.createContext({Math,Point:paper.Point,Path:paper.Path,view:{zoom:1},state:{activeLayerIdx:0,currentFrame:0,appMode:'anim2d',layers:[{frames:{0:{isKeyframe:true}}}]},
  userLayers:[{}],selectedPaths:[selected],_nodeSel:indices.slice(),_nodeDrag:{active:false},_nmq:{active:false},
  nodeHandles:selected.segments.flatMap((s,i)=>[{segIndex:i,type:'point',pos:s.point.clone()},...(indices.includes(i)?['handleIn','handleOut'].filter(k=>s[k].length).map(k=>({segIndex:i,type:k,pos:s.point.add(s[k])})):[])]),
  shouldIntercept:()=>true,allowLegacySelectionEdit:()=>true,nodeEditTargetPath:()=>selected,toLocalPoint:p=>inverse.transform(p),
  nodeEditSegmentsData:p=>p.segments.map(s=>({point:[s.point.x,s.point.y]})),
  pushUndo:noop,ensureKeyframe:noop,renderNodeHandles:noop,fillRegenerateLinked:noop,
  SMEngineBridge:{screenToWorld:(x,y)=>[x,y],suspend:noop,isEnabled:()=>true,renderNow:noop}});
 c.window=c;
 const tools=sources['src/js/tools.js'],bridge=sources['src/js/subselect-bridge.js'];
 vm.runInContext(tools.slice(tools.indexOf('function constrainAngle45('),tools.indexOf('var _moveDragStarted')),c);
 vm.runInContext(bridge.slice(bridge.indexOf('  function onDown('),bridge.indexOf('  function onUp(')),c);
 const event=(point,extra={})=>{const p=M.transform(new paper.Point(point));return {clientX:p.x,clientY:p.y,stopImmediatePropagation:noop,preventDefault:noop,altKey:alts[0],...extra};};
 c.onDown(event([start.x,start.y],{shiftKey:type.includes('shift')}));
 const steps=[];let mirrored=false;
 for(const [i,move] of moves.entries()){
  const altKey=alts[i];c.onMove(event(move.point,{shiftKey:move.shiftKey,altKey}));
  const expected=selected.segments.map(s=>({point:[s.point.x,s.point.y],handleIn:[s.handleIn.x,s.handleIn.y],handleOut:[s.handleOut.x,s.handleOut.y]}));
  const point=expected[0].point,intent=key==='point'?{kind:'move',indices:type.startsWith('group')?[0,3]:[0],delta:[point[0]-segments[0].point[0],point[1]-segments[0].point[1]]}:
   {kind:'handle',index:0,handle:key,point:[point[0]+expected[0][key][0],point[1]+expected[0][key][1]],breakPair:altKey};
  if(key!=='point'&&altKey&&mirrored){const other=expected[0][key==='handleIn'?'handleOut':'handleIn'];intent.pairedPoint=[point[0]-other[0],point[1]-other[1]];}
  if(!altKey)mirrored=true;
  steps.push({...move,altKey,intent,expected});
 }
 cases.push({name:type+'-'+alts.map(Number).join(''),matrix,type:key,indices,downShift:type.includes('shift'),altKey:alts[0],start:[start.x,start.y],selection:Array.from(c._nodeSel),segments,steps});selected.remove();
}
const hashes=Object.fromEntries(Object.entries(sources).map(([p,s])=>[p,createHash('sha256').update(s).digest('hex')]));
process.stdout.write(JSON.stringify({baseline,hashes,cases})+'\n');
