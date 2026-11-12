'use strict';
// Frozen Motion functions and Paper cubic bounds, never the new native projection.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),{execFileSync}=require('node:child_process'),{createHash}=require('node:crypto');
const root=path.resolve(__dirname,'../../../..'),baseline='46442ea96c09e6bfaadf956eb8586e273a658085';
const motion=execFileSync('git',['show',baseline+':src/js/motion.js'],{cwd:root,encoding:'utf8',maxBuffer:8e6});
const paper=require(path.join(root,'tests/helpers/native-import-paper.cjs')).load(),ctx=vm.createContext({Math});
for(const name of ['rotPt','rotVec','transformSegments']){const at=motion.indexOf('  function '+name+'('),end=motion.indexOf('\n  function ',at+1);vm.runInContext(motion.slice(at,end),ctx);}
const seg=(p,hi=[0,0],ho=[0,0])=>({point:p,handleIn:hi,handleOut:ho});
const paths=[[seg([10,10],[0,-3],[7,2]),seg([35,15],[-3,4],[5,7]),seg([20,40],[-4,2],[2,-4])],[seg([55,18]),seg([70,18]),seg([70,33]),seg([55,33])]];
const makePath=s=>new paper.Path({insert:false,closed:true,segments:s.map(s=>new paper.Segment(s.point,s.handleIn,s.handleOut))});
const element={dx:3,dy:-2,rot:-17,sx:.7,sy:1.3,op:1,ax:2,ay:1},layer={dx:8,dy:4,rot:33,sx:1.4,sy:.6,op:1,ax:1,ay:-3},parent={dx:-4,dy:7,rot:61,sx:.8,sy:1.6,op:1,ax:3,ay:2};
const centre=(s,m)=>{const p=makePath(s),c=p.bounds.center;p.remove();return {x:c.x+m.ax,y:c.y+m.ay};};
const ep=centre(paths[0],element),rest=[ctx.transformSegments(paths[0],ep,element),paths[1]];
let bounds=null;for(const s of rest){const p=makePath(s);bounds=bounds?bounds.unite(p.bounds):p.bounds.clone();p.remove();}
const lp={x:bounds.center.x+layer.ax,y:bounds.center.y+layer.ay},pp={x:50+parent.ax,y:40+parent.ay};
function apply(s){const pivot=centre(s,element),one=ctx.transformSegments(s,pivot,element);let b=null;
  for(const geometry of [one,paths[1]]){const p=makePath(geometry);b=b?b.unite(p.bounds):p.bounds.clone();p.remove();}
  return ctx.transformSegments(ctx.transformSegments(one,{x:b.center.x+layer.ax,y:b.center.y+layer.ay},layer),pp,parent);
}
const tools=execFileSync('git',['show',baseline+':src/js/tools.js'],{cwd:root,encoding:'utf8',maxBuffer:8e6});let selected;
const noop=()=>{};const edit=vm.createContext({Point:paper.Point,nodeEditTargetPath:()=>selected,_nodeSel:[1],pushUndo:noop,renderNodeHandles:noop,fillRegenerateLinked:noop,saveActiveLayerFrame:noop,updateUI:noop,userLayers:[{}],state:{activeLayerIdx:0}});
vm.runInContext(tools.slice(tools.indexOf('function _ptGet('),tools.indexOf('// ---- BOOLEAN PATH OPERATIONS')),edit);
const edits=['corner','smooth','symmetric'].map(pointType=>{selected=makePath(paths[0]);edit.setPointType(pointType);const segments=selected.segments.map(s=>seg([s.point.x,s.point.y],[s.handleIn.x,s.handleIn.y],[s.handleOut.x,s.handleOut.y]));selected.remove();return {intent:{kind:'pointType',indices:[1],pointType},segments,expected:apply(segments)};});
const props=m=>[{property:'position',static:[m.dx,m.dy]},{property:'anchor',static:[m.ax,m.ay]},{property:'rotation',static:[m.rot]},{property:'scale',static:[m.sx*100,m.sy*100]}];
process.stdout.write(JSON.stringify({baseline,sourceHash:createHash('sha256').update(motion).digest('hex'),paths,element:props(element),layer:props(layer),parent:props(parent),expected:apply(paths[0]),edits})+'\n');
