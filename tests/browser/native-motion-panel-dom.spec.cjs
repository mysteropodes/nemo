'use strict';
// Motion sidebar DOM parity (#2377, MS-7). Real index.html markup for #motion-props-sec, real css/style.css and the real native
// Motion adapters run in Chromium with a fake ports fixture (the property-metadata oracle), then the rendered sidebar is checked
// against the baseline contract (commit 1065e9d8, motion.js renderMotionPropsPanel / scrubField / stopwatch). Controlled DOM only,
// not installed-app acceptance. A screenshot is attached for the validation owner (and written under the test output dir).
const {test,expect}=require('@playwright/test');
const fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'../..');
const read=(file)=>fs.readFileSync(path.join(root,file),'utf8');
const adapters=['adapters/native-object-ui','application/presenter-seam','adapters/native-layer-list-presenter','adapters/native-motion-property-view','adapters/native-motion-property-keys','adapters/native-motion-property-value',
  'adapters/native-motion-property-panel','adapters/native-motion-property-binding'].map((name)=>'src/js/'+name+'.js');
const fixture=JSON.parse(read('engineering/application/examples/native-property-metadata-v1/cases.json'));
const motion=read('src/js/motion.js'),html=read('src/index.html'),css=read('src/css/style.css');
// Baseline propsFor() order of the base transform family, plus the dropped-in names used by the fixture rows.
const BASELINE_ORDER=['position','anchor','rotation','scale','opacity','order'];
const OLD_TEXT=/^(Add key|Remove key|Enable animation|Disable animation|Keys|Bake expression|All|Cancel|animated|modified|expressions)$/;

function section(){const a=html.indexOf('<div class="psec" id="motion-props-sec">'),b=html.indexOf('<div class="psec" id="assets-sec">',a);if(a<0||b<a)throw Error('motion-props-sec markup not found');return html.slice(a,b);}
function literal(name){const m=new RegExp('\\n  var '+name+' = (\\{[^\\n]*\\});').exec(motion);if(!m)throw Error(name+' not found');return new Function('return '+m[1])();}
function installer(){const a=motion.indexOf('  (function installNativeMotionMetadata(){'),b=motion.indexOf('  // Workspace continuity, restore half',a);if(a<0||b<a)throw Error('installer not found');return motion.slice(a,b);}
// A stopwatch exists where the native owner describes an animatable property whose authored facts are readable. 'order' is
// reported authored-unavailable (separate property owner), so it renders a labelled row without an editor.
const hasStopwatch=(row)=>row.animatable===true&&!!row.authored&&row.authored.availability==='available';
function fixtureData(){
  const first=fixture.cases.find((c)=>c.expectedResult);
  const targets=fixture.cases.filter((c)=>['targets-small-first-page','targets-next-page-deduplicates-authored-frames-excludes-reference-only'].includes(c.name)).flatMap((c)=>c.expectedResult.items);
  const authored=fixture.cases.filter((c)=>c.documentVariant==='authored-facts').flatMap((c)=>c.expectedResult.items);
  const rows=fixture.propertyPageOracle.map((row)=>authored.find((v)=>v.property===row.property)||row);
  return {request:first.request,categories:first.expectedResult.unavailableCategories,targets,rows};
}

async function mount(page,tab){
  await page.goto('about:blank');
  await page.setContent('<!doctype html><html><head><meta charset="utf-8"><style>'+css+'</style></head><body class="mode-motion">'+
    '<div id="props-panel">'+section()+'</div><div id="layer-list"></div><div id="frame-hdr"></div><div id="frame-grid"></div><canvas id="drawing-canvas"></canvas></body></html>');
  for(const file of adapters)await page.addScriptTag({content:read(file)});
  return page.evaluate(async({data,source,labels,dims})=>{
    const sleep=(ms)=>new Promise((r)=>setTimeout(r,ms));
    const q=data.request;
    let info={instanceId:q.instanceId,documentId:q.documentId,lifecycleGeneration:1,contentRevision:q.atRevision,documentSnapshotId:q.documentSnapshotId,totalFrames:21,canvasW:320,canvasH:180,fps:24,
      layers:[{layerUid:'L0',name:'Blue',visible:true},{layerUid:'L1',name:'Red',visible:true}]};
    Object.assign(window,{state:{appMode:'motion',activeLayerIdx:0,activeSymbolId:null,currentFrame:0},_layerSel:[0],showContextMenu(){},showToast(){}});
    const receipt=()=>({...info,available:true,kind:'native-object',presentation:{...info,owner:'native',status:'presented',frame:window.state.currentFrame}});
    const session={isActive:()=>true,blocksLegacy:()=>true,info:()=>info,readPresentedObjectSession:receipt};
    const base=(selector,kind)=>({instanceId:selector.instanceId,documentId:selector.documentId,atRevision:selector.atRevision,documentSnapshotId:selector.documentSnapshotId,kind});
    const ports={currentSession:()=>session,projectionReady:(value)=>value===info,readTransportGeneration:()=>0,
      async readPropertyMetadata(selector){const view=selector.view,list=view.kind==='targets'?data.targets:data.rows;
        const items=JSON.parse(JSON.stringify(list.slice(view.offset,view.offset+Math.min(view.limit,5))));
        return {...base(selector,view.kind),target:view.kind==='targets'?null:view.target,items,nextOffset:view.offset+items.length<list.length?view.offset+items.length:null,unavailableCategories:data.categories};},
      async readProperty(selector){const view=selector.view,row=data.rows.find((r)=>r.property===view.property);
        if(view.kind==='expressionState'){const on=row.authored.expressionEnabled===true;return {...base(selector,'expressionState'),target:view.target,property:view.property,frame:view.frame,expression:{present:on,enabled:on},evaluation:{state:'evaluated',diagnostic:null}};}
        if(view.kind==='authoredKeys')return {...base(selector,'authoredKeys'),target:view.target,property:view.property,items:[{frame:view.frame===undefined?0:view.frame}],nextOffset:null};
        const dim=row.dimension;return {...base(selector,'rawSample'),target:view.target,property:view.property,dimension:dim,frame:view.frame,value:Array.from({length:dim},(_,i)=>10*(i+1))};},
      async editProperty(){return {applied:true,historyEntriesAdded:1};},
      selectLayer:()=>true,repaint:()=>NemoNativeObjectUI.render(window,info,true,ports)};
    window.PROP_LABEL=labels;
    // The real installer, run against the real module objects (window.NemoNativeObjectUI/NemoNativeMotionBinding).
    new Function('PROP_LABEL','PROP_DIM_LABELS','_motionFilterMode','localStorage',source).call(window,labels,dims,'all',{setItem(){}});
    const ok=NemoNativeObjectUI.render(window,info,true,ports);
    const until=Date.now()+3000;
    while(Date.now()<until){const body=document.getElementById('motion-props-body');if(body.querySelector('.motion-prop-row')&&!/Loading/.test(body.textContent))break;await sleep(10);}
    await sleep(100);
    return ok;
  },{data:fixtureData(),source:installer(),labels:literal('PROP_LABEL'),dims:literal('PROP_DIM_LABELS')});
}

// One DOM read: structure plus the computed styles the baseline contract pins.
function snapshot(){
  const body=document.getElementById('motion-props-body'),cs=(el)=>getComputedStyle(el);
  const rows=Array.from(body.querySelectorAll('.lrow.motion-prop-row')),ordered=Array.from(body.querySelectorAll('.motion-props-layername,.motion-group-row,.lrow.motion-prop-row'));
  const visible=(el)=>{for(let n=el;n&&n!==document.documentElement;n=n.parentElement)if(cs(n).display==='none'||n.hidden)return false;return true;};
  return {
    sectionVisible:visible(document.getElementById('motion-props-sec')),
    first:ordered[0]?{cls:ordered[0].className,text:ordered[0].textContent}:null,
    second:ordered[1]?{cls:ordered[1].className,text:ordered[1].textContent,funnel:!!ordered[1].querySelector('.motion-filter-btn')}:null,
    rows:rows.map((row)=>({
      property:row.dataset.property,height:cs(row).height,fontSize:cs(row).fontSize,
      name:(row.querySelector('.motion-prop-name')||{}).textContent||null,
      dims:Array.from(row.querySelectorAll('.motion-dim-label'),(n)=>n.textContent),
      units:Array.from(row.querySelectorAll('.motion-unit'),(n)=>n.textContent),
      stopwatches:row.querySelectorAll('.motion-stopwatch').length,
      stopwatchDisabled:Array.from(row.querySelectorAll('.motion-stopwatch'),(n)=>n.classList.contains('disabled')),
      inputs:Array.from(row.querySelectorAll('input'),(n)=>({classes:Array.from(n.classList),type:n.type,step:n.step,width:cs(n).width,fontSize:cs(n).fontSize,value:n.value})),
      buttons:row.querySelectorAll('button').length,
    })),
    buttons:Array.from(body.querySelectorAll('button')).filter((n)=>visible(n)).map((n)=>n.textContent),
    searchInputs:body.querySelectorAll('input[type="search"]').length,
    text:body.innerText,
    ids:Array.from(body.querySelectorAll('[id]'),(n)=>n.id),
    headers:body.querySelectorAll('.phdr').length,
    chips:body.querySelectorAll('.native-motion-target-chip,[class*="target-chip"]').length,
  };
}

test('Motion sidebar markup and computed styles match the baseline contract',async({page},testInfo)=>{
  expect(await mount(page),'native Motion UI renders').toBe(true);
  const s=await page.evaluate(snapshot),animatable=Object.fromEntries(fixtureData().rows.map((r)=>[r.property,hasStopwatch(r)]));
  expect(s.sectionVisible,'section visible under body.mode-motion').toBe(true);
  // Baseline renderMotionPropsPanel: layer-name row first, then the Transform group header with the funnel.
  expect(s.first.cls).toContain('motion-props-layername');expect(s.first.text).toBe('Blue');
  expect(s.second.cls).toContain('motion-group-row');expect(s.second.text.toLowerCase()).toContain('transform');
  expect(s.second.funnel,'hide-unanimated funnel in the group header').toBe(true);
  expect(s.headers,'no second section header such as "Context <id>"').toBe(0);
  // Rows: baseline order, 24px height, 10.5px font, one name span.
  expect(s.rows.length).toBeGreaterThanOrEqual(5);
  const order=s.rows.map((r)=>r.property).filter((p)=>BASELINE_ORDER.includes(p));
  expect(order,'rows follow baseline propsFor order').toEqual(BASELINE_ORDER.filter((p)=>order.includes(p)));
  for(const row of s.rows){
    expect(row.height,row.property+' row height').toBe('24px');expect(row.fontSize,row.property+' row font').toBe('10.5px');
    expect(row.name,row.property+' has a label and no fused detail word').toBeTruthy();expect(row.name).not.toMatch(/Default|Animated|Modified|read-only/i);
    expect(row.buttons,row.property+' has no <button>').toBe(0);
    expect(row.stopwatches,row.property+' stopwatch count follows animatable').toBe(animatable[row.property]?1:0);
    for(const input of row.inputs){
      expect(input.classes,row.property+' input classes').toEqual(expect.arrayContaining(['pi','scrub','motion-val']));
      expect(input.type).toBe('number');expect(input.step).toBe('1');
      expect(input.width,row.property+' input width').toBe('34px');expect(input.fontSize,row.property+' input font').toBe('10.5px');
    }
  }
  const byName=Object.fromEntries(s.rows.map((r)=>[r.property,r]));
  for(const two of ['position','anchor','scale'])if(byName[two]){
    expect(byName[two].inputs.length,two+' has two fields').toBe(2);expect(byName[two].dims,two+' X/Y labels').toEqual(['X','Y']);
  }
  if(byName.order)expect(byName.order.inputs.length,'order has no editor in this fixture').toBe(0);
  for(const one of ['rotation','opacity'])if(byName[one]){expect(byName[one].inputs.length).toBe(1);expect(byName[one].dims,one+' has no X/Y labels').toEqual([]);}
  expect(byName.position.units).toEqual(['px']);
  if(byName.rotation)expect(byName.rotation.units).toEqual(['°']);
  if(byName.scale)expect(byName.scale.units).toEqual(['%']);
  if(byName.opacity)expect(byName.opacity.units).toEqual(['%']);
  // Order is non-editable here: a plain labelled row, never a sentence.
  expect(s.text).not.toMatch(/read-only/i);
  // None of the non-baseline chrome.
  expect(s.buttons.filter((t)=>OLD_TEXT.test(t.trim())),'no old button texts').toEqual([]);
  expect(s.buttons,'no buttons anywhere in the sidebar').toEqual([]);
  expect(s.searchInputs).toBe(0);expect(s.chips).toBe(0);
  expect(s.text,'no raw native ids or loading/status lines').not.toMatch(/native-(stroke|text|raster|layer)|\bim_\d|Context\b|Loading|Expression diagnostics|Select Keys|No matching properties/);
  const png=await page.locator('#motion-props-sec').screenshot();
  const file=testInfo.outputPath('motion-sidebar-native.png');fs.writeFileSync(file,png);
  await testInfo.attach('motion-sidebar-native',{body:png,contentType:'image/png'});
});

test('rows with readable animatable properties carry a live stopwatch; unavailable ones none',async({page})=>{
  await mount(page);
  const s=await page.evaluate(snapshot),animatable=Object.fromEntries(fixtureData().rows.map((r)=>[r.property,hasStopwatch(r)]));
  const byName=Object.fromEntries(s.rows.map((r)=>[r.property,r]));
  expect(byName.position.stopwatchDisabled,'animatable position stopwatch is enabled').toEqual([false]);
  expect(s.rows.filter((r)=>r.stopwatches===1).map((r)=>r.property).sort(),'one .motion-stopwatch per animatable row')
    .toEqual(s.rows.map((r)=>r.property).filter((p)=>animatable[p]).sort());
  expect(byName.order.stopwatches,'order: authored facts unavailable, so no editor and no stopwatch').toBe(0);
});
