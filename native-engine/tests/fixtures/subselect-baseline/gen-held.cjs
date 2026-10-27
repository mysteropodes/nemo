'use strict';
// Promotion is characterized by executing the original ensureKeyframe and
// getEffectiveStrokes. No native implementation supplies expected timing/copies.
const path=require('node:path'),vm=require('node:vm'),cp=require('node:child_process'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../../..'),baseline='46442ea96c09e6bfaadf956eb8586e273a658085';
const read=p=>cp.execFileSync('git',['show',`${baseline}:${p}`],{cwd:root,encoding:'utf8',maxBuffer:8e6});
const sources=Object.fromEntries(['src/js/app.js','src/js/tools.js'].map(p=>[p,read(p)]));
const app=sources['src/js/app.js'],tools=sources['src/js/tools.js'];
const at=app.indexOf('function getEffectiveStrokes('),tail=app.slice(at+1).search(/^function /m);
const ensure=tools.match(/^function ensureKeyframe\(blank\).*$/m)?.[0];
if(at<0||tail<0||!ensure)throw Error('Frozen promotion functions absent');
const c=vm.createContext({JSON,canEditActiveLayer:()=>true,layerHasTimeRange:()=>false,loadFrame:()=>{},syncLinkedKeyframeFolder:()=>{}});c.window=c;
vm.runInContext(app.slice(at,at+1+tail),c);vm.runInContext(ensure,c);
const stroke=id=>({strokeId:id,segments:[{point:[10,10],handleIn:[0,0],handleOut:[0,0]},{point:[60,10],handleIn:[0,0],handleOut:[0,0]}],closed:false,metadata:{name:id,nested:[1,'retained']}});
const copy=v=>JSON.parse(JSON.stringify(v)),cases=[];
for(const frame of [0,3,5]){
  const frames=Array.from({length:12},(_,i)=>({isKeyframe:i===0||i===10,isInterpolated:i===3,strokes:i===0?[stroke('s-a'),stroke('s-b')]:i===3?[stroke('sample')]:i===10?[stroke('later')]:[]}));
  const input=copy(frames);c.state={activeLayerIdx:0,currentFrame:frame,layers:[{frames}]};c.ensureKeyframe();
  cases.push({frame,input,expected:copy(c.state.layers[0].frames)});
  if(frame===5){c.state.layers[0].frames[5].strokes[0].metadata.nested[0]=99;if(c.state.layers[0].frames[0].strokes[0].metadata.nested[0]!==1)throw Error('Promotion aliases the original key');}
}
const hashes=Object.fromEntries(Object.entries(sources).map(([p,s])=>[p,crypto.createHash('sha256').update(s).digest('hex')]));
process.stdout.write(JSON.stringify({baseline,hashes,cases})+'\n');
