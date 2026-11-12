'use strict';
// CS5 presenter seam in a real DOM (#2343 B1, #2494). The node fixtures stand in for Element; this renders the actual
// native-object-ui metadata path through the seam and the default layer-list presenter in Chromium, where a row's
// children are a live HTMLCollection, as in the installed WebView. Controlled DOM only, not installed acceptance.
const {test,expect}=require('@playwright/test');
const fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'../..');
const scripts=['src/js/adapters/native-object-project-file.js','src/js/adapters/native-object-ui.js','src/js/application/presenter-seam.js',
  'src/js/adapters/native-layer-list-presenter.js'];

async function page$(page){
 await page.setContent(['layer-list','frame-hdr','frame-grid','tl-cf','tl-tf','info-frame','info-badge','info-strokes','info-sel']
  .map((id)=>`<div id="${id}"></div>`).join(''));
 for(const file of scripts)await page.addScriptTag({content:fs.readFileSync(path.join(root,file),'utf8')});
}
// The same read model and ports as tests/native-layer-list-presenter.test.cjs, with window as the root.
function render({layers,selection}){
 const info={apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:3,documentSnapshotId:'document-snapshot-3',
  documentDigest:'4'.repeat(64),byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers};
 Object.assign(window,{state:{currentFrame:7,activeLayerIdx:selection[0]||0},userLayers:[],_layerSel:selection.slice()});
 NemoNativeObjectProjectFile.projectView(window,info);window._layerSel=selection.slice();
 const ports={projectionReady:(i)=>NemoNativeObjectProjectFile.projectionReadyForUI(window,i),currentSession:()=>null,selectLayer:()=>true,repaint:()=>true,
  readShyMode:()=>false};
 const ok=NemoNativeObjectUI.render(window,info,false,ports),list=document.getElementById('layer-list');
 return {ok,collection:Object.prototype.toString.call(list.children[0]&&list.children[0].children),
  rows:Array.from(list.children,(row)=>({children:row.children.length,text:row.textContent}))};
}

test('the default layer-list presenter mounts real DOM rows through the seam',async({page})=>{
 await page.goto('about:blank');await page$(page);
 const layers=[{layerUid:'opaque A:雪',name:'<b>A</b>',visible:true},{layerUid:'B',name:'B',visible:false},{layerUid:'C',visible:true}];
 const out=await page.evaluate(render,{layers,selection:[0,2]});
 expect(out.ok,'native metadata render completes').toBe(true);
 expect(out.collection,'a real row exposes its children as an HTMLCollection').toBe('[object HTMLCollection]');
 expect(out.rows.length,'one row per read-model layer').toBe(3);
 expect(out.rows.map((r)=>r.children),'pick, eye and two policy controls per row').toEqual([4,4,4]);
 expect(out.rows[0].text,'a layer name renders as text, never markup').toContain('<b>A</b>');
});

test('a replacement presenter registered first returns real DOM rows and is used verbatim',async({page})=>{
 await page.goto('about:blank');await page$(page);
 const used=await page.evaluate((layers)=>{
  NemoPresenterSeam.register(window,'layer-list',{render:(list,context)=>({rows:context.info.layers.map((layer)=>{
   const row=document.createElement('div');row.append(document.createElement('span'));row.firstChild.textContent='custom '+layer.layerUid;return row;})})});
  const info={apiVersion:2,instanceId:'instance',documentId:'document',lifecycleGeneration:1,contentRevision:3,documentSnapshotId:'document-snapshot-3',
   documentDigest:'4'.repeat(64),byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:24,reference:null,undoDepth:0,redoDepth:0,layers};
  Object.assign(window,{state:{currentFrame:0,activeLayerIdx:0},userLayers:[],_layerSel:[]});
  NemoNativeObjectProjectFile.projectView(window,info);
  const ports={projectionReady:(i)=>NemoNativeObjectProjectFile.projectionReadyForUI(window,i),currentSession:()=>null,selectLayer:()=>true,
   repaint:()=>true,readShyMode:()=>false};
  return {ok:NemoNativeObjectUI.render(window,info,false,ports),text:Array.from(document.getElementById('layer-list').children,(r)=>r.textContent)};
 },[{layerUid:'A',visible:true},{layerUid:'B',visible:true}]);
 expect(used).toEqual({ok:true,text:['custom A','custom B']});
});
