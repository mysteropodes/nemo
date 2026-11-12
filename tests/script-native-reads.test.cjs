'use strict';
// CS8-SDK (#2354): the declared SDK read rows (Layer.get/keys, isKeyframe, markers.list/next/prev, comp.beat/beats) read ONE pinned
// model - detached values in frame units - fetched in rounds before the real pass; nothing unfetched is guessed.
const test=require('node:test'),assert=require('node:assert/strict');
const Api=require('../src/js/application/script-native-api.js'),Reads=require('../src/js/application/script-native-reads.js');
const L=[{layerUid:'a',name:'A',visible:true},{layerUid:'b',name:'B',visible:true},{layerUid:'c',name:'C',visible:true}];
const KEYS=Array.from({length:11},(_,i)=>({frame:i*2,v:[i*10,i*5],...(i===3?{hold:true}:{}),curvePoints:null}));
function world(opts={}){
  const w={info:{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:3,documentSnapshotId:'native-object:d:3',fps:24,canvasW:320,canvasH:180,totalFrames:30,layers:L.map(l=>({...l}))},
    fetched:[],frames:[],toasts:[],frame:4,fail:null,composition:{markers:[{frame:6,name:'Beat'},{frame:12,name:'',color:'#f00'},{frame:20,name:'End',color:null}],effectiveBpm:120,effectiveBpmOffset:2,effectiveBpmShow:true},...opts};
  const cell=(uid,frame)=>uid==='a'&&frame===6?KEYS:null;void cell;
  w.reads={
    property:async view=>{
      w.fetched.push(view);
      if(w.fail&&w.fail(view))throw Object.assign(new Error('refused'),{code:'stale_revision'});
      if(w.mutate)w.mutate(view);
      if(view.kind==='rawSample'){const t=view.target.layerUid==='a'&&view.property==='position';return {kind:'rawSample',dimension:t?2:1,frame:view.frame,value:t?[view.frame*5,view.frame*2.5]:[100]};}
      const all=view.target.layerUid==='a'&&view.property==='position'?KEYS:[];
      const items=all.slice(view.offset,view.offset+view.limit);const next=view.offset+view.limit<all.length?view.offset+view.limit:null;
      return {kind:'authoredKeys',dimension:2,trackPresent:all.length>0,items:JSON.parse(JSON.stringify(items)),nextOffset:next};
    },
    cells:async uid=>{w.fetched.push({kind:'cells',uid});return uid==='a'?[[0,4,'key',0,true,false],[4,4,'held',0,true,false],[8,2,'interpolated',8,true,false],[10,20,'unexposed',null,false,false]]:[[0,30,'key',0,true,false]];},
    composition:()=>w.composition,
  };
  w.ports={info:()=>w.info,frame:()=>w.frame,goToFrame:f=>{w.frames.push(f);w.frame=f;},toast:m=>w.toasts.push(m),selection:()=>[0],reads:w.reads,ops:{}};
  return w;
}
const run=(w,src)=>Api.execute(w.ports,src);

test('every read row answers from the pin: values are detached arrays in frame units, keys carry hold, cells say key or not',async()=>{
  const w=world();
  const r=await run(w,`
    var a=nemo.layer('A');
    var keys=a.keys('position'), at=a.get('position',8), here=a.get('position'), other=nemo.layer('B').get('opacity',3);
    a.keys('position')[0].value[0]=999; a.get('position',8)[0]=-1;
    ({keys:keys.length,first:keys[0],held:keys[3],last:keys[10].frame,at:at,here:here,other:other,
      cells:[0,3,4,8,10,29].map(function(f){return nemo.isKeyframe('A',f);}),here0:nemo.isKeyframe('A'),b:nemo.isKeyframe(1,17),
      markers:nemo.markers.list(),beats:nemo.comp.beats().slice(0,4),beat:[nemo.comp.beat(9),nemo.comp.beat(),nemo.comp.beat(1)],
      again:a.get('position',8),strokes:(function(){try{a.strokes(0);}catch(e){return e.code+':'+e.reason;}})()})`);
  assert.equal(r.ok,true,r.error);
  const v=JSON.parse(JSON.stringify(r.value));
  assert.equal(v.keys,11);assert.deepEqual(v.first,{frame:0,value:[0,0],hold:false});assert.deepEqual(v.held,{frame:6,value:[30,15],hold:true});assert.equal(v.last,20);
  assert.deepEqual(v.at,[40,20]);assert.deepEqual(v.here,[20,10],'the playhead is the default frame');assert.deepEqual(v.other,[100]);
  assert.deepEqual(v.again,[40,20],'a returned value is detached from the pin');
  assert.deepEqual(v.cells,[true,true,false,false,false,false]);assert.equal(v.here0,false);assert.equal(v.b,true);
  assert.deepEqual(v.markers,[{frame:6,name:'Beat',color:null},{frame:12,name:'',color:'#f00'},{frame:20,name:'End',color:null}]);
  assert.deepEqual(v.beats,[2,14,26],'12 frames per beat at 120 bpm / 24 fps, offset 2');
  assert.deepEqual(v.beat,[14,2,2]);
  assert.equal(v.strokes,'unavailable:no-native-stroke-read');
  assert.deepEqual(w.frames,[],'reads never move the playhead');
});

test('the script runs in quiet rounds over one cache: each read is fetched once, presentation and writes happen only in the last pass',async()=>{
  const w=world(),writes=[];
  w.ports.ops={rename:(uid,name)=>{writes.push([uid,name]);w.info={...w.info,contentRevision:4,layers:w.info.layers.map(l=>l.layerUid===uid?{...l,name}:l)};return Promise.resolve({ok:true,applied:true});}};
  let passes=0;
  const r=await run(w,`__count();
    var a=nemo.layer('A'); nemo.toast('go'); nemo.goToFrame(9);
    var k=a.keys('position'), g=a.get('position'), again=a.keys('position'), s=a.get('position',9);
    a.name='Renamed'; [k.length,g,again.length,s,nemo.frame]`.replace('__count();',''));
  assert.equal(r.ok,true,r.error);assert.deepEqual(JSON.parse(JSON.stringify(r.value)),[11,[45,22.5],11,[45,22.5],9]);
  void passes;
  assert.deepEqual(w.toasts,['go'],'one toast: the quiet passes do not toast');assert.deepEqual(w.frames,[9],'one goToFrame');
  const samples=w.fetched.filter(f=>f.kind==='rawSample');
  assert.deepEqual(samples.map(f=>f.frame),[9],'the sample at the script frame is fetched once (get() and get(.,9) share it)');
  assert.equal(w.fetched.filter(f=>f.kind==='authoredKeys').length,2,'11 keys are two pages, fetched once for both keys() calls');
  assert.deepEqual(w.fetched.filter(f=>f.kind==='authoredKeys').map(f=>f.offset),[0,8]);
  assert.deepEqual(await r.committed,{ok:true,applied:1,pending:0});assert.deepEqual(writes,[['a','Renamed']]);
});

test('a script without a read runs once and answers synchronously; Object.keys is not a read',()=>{
  const w=world();
  const plain=run(w,'Object.keys({a:1}).length+nemo.layerCount');assert.equal(plain.ok,true);assert.equal(plain.value,4);
  assert.equal(typeof plain.then,'undefined');assert.equal(w.fetched.length,0);
  const reading=run(w,'nemo.layer(0).get("opacity")');assert.equal(typeof reading.then,'function');
  assert.equal(Reads.needs('x.get(1)'),true);assert.equal(Reads.needs('a.getTime()'),false);assert.equal(Reads.needs('nemo.isKeyframe(0,1)'),true);
});

test('markers.next and prev walk the pinned markers from the script playhead and present the frame; the end of the list stays put',async()=>{
  const w=world({frame:7});
  const r=await run(w,'var f=[]; nemo.markers.next(); f.push(nemo.frame); nemo.markers.next(); f.push(nemo.frame); nemo.markers.next(); f.push(nemo.frame); nemo.markers.prev(); f.push(nemo.frame); nemo.markers.prev(); f.push(nemo.frame); nemo.markers.prev(); f.push(nemo.frame); f');
  assert.equal(r.ok,true,r.error);assert.deepEqual(JSON.parse(JSON.stringify(r.value)),[12,20,20,12,6,6]);
  assert.deepEqual(w.frames,[12,20,12,6],'only moves that found a marker present a frame');
  const sync=run(world({frame:7}),'nemo.markers.next()===nemo.markers');assert.equal(sync.value,true);
});

test('a stale or removed handle fails the read; handles follow layerUid, so a reordered document cannot alias another layer',async()=>{
  const w=world();
  const gone=await run(w,'var b=nemo.layer("B"); b.remove(); b.get("opacity")');
  assert.equal(gone.ok,false);
  const w2=world();w2.ports.ops={remove:()=>Promise.resolve({ok:true,applied:true})};
  const stale=await run(w2,'var b=nemo.layer("B"); b.remove(); b.get("opacity",1)');
  assert.equal(stale.ok,false);assert.match(stale.error,/layer b is not in this document/);assert.equal(stale.committed,undefined);
  const stale2=await run(w2,'var b=nemo.layer("B"); b.remove(); nemo.isKeyframe(b,1)');assert.match(stale2.error,/layer b is not in this document/);
  const stale3=await run(w2,'var b=nemo.layer("B"); b.remove(); b.keys("opacity")');assert.match(stale3.error,/layer b is not in this document/);
  // The same script over a reordered document reads the same layer by uid, not the layer now at that index.
  const first=await run(world(),'nemo.layer("C").keys("position").length+"/"+nemo.layer("A").keys("position").length');
  const flipped=world();flipped.info={...flipped.info,layers:[...flipped.info.layers].reverse()};
  const second=await run(flipped,'nemo.layer("C").keys("position").length+"/"+nemo.layer("A").keys("position").length');
  assert.equal(first.value,'0/11');assert.equal(second.value,'0/11');
  const idx=await run(flipped,'var h=nemo.layer(0); [h.layerUid,h.keys("position").length].join(":")');assert.equal(idx.value,'c:0');
  assert.ok(flipped.fetched.filter(f=>f.kind==='authoredKeys').every(f=>f.target.layerUid==='a'||f.target.layerUid==='c'),'no read was issued for a layer the script did not name');
});

test('a read the pin cannot give throws a typed unavailable: unknown fetch, refused query, no composition, pending layer, absent port, bad arguments',async()=>{
  const typed=(r,reason,row)=>{assert.equal(r.ok,false,reason);assert.equal(r.code,'unavailable',reason);assert.match(r.error,new RegExp('\\('+reason+'\\)'));if(row)assert.match(r.error,new RegExp(row));};
  // Reached by name only after the scan, so the real pass finds nothing fetched.
  typed(await run(world(),'var a=nemo.layer(0); var m=["g","et"].join(""); a[m]("opacity",1)'),'not-prefetched','Layer.get');
  const refused=world({fail:v=>v.kind==='rawSample'});typed(await run(refused,'nemo.layer(0).get("opacity",2)'),'read-stale-revision','Layer.get');
  typed(await run(world({composition:null}),'nemo.markers.list()'),'composition-unavailable','markers.list');
  typed(await run(world({composition:null}),'nemo.comp.beats()'),'composition-unavailable','comp.beats');
  typed(await run(world(),'nemo.addLayer("x")'),'native-entry-absent','addLayer');
  const w=world();w.ports.ops={add:()=>Promise.resolve({ok:true,applied:true,layerUid:'n1'}),rename:()=>Promise.resolve({ok:true,applied:true})};
  typed(await run(w,'var n=nemo.addLayer("x"); n.keys("opacity")'),'pending-layer','Layer.keys');
  typed(await run(w,'var n=nemo.addLayer("x"); nemo.isKeyframe(n,0)'),'pending-layer','isKeyframe');
  const noPorts=world();delete noPorts.ports.reads;typed(await run(noPorts,'nemo.layer(0).keys("opacity")'),'native-entry-absent','Layer.keys');
  typed(await run(noPorts,'nemo.isKeyframe(0,1)'),'native-entry-absent','isKeyframe');
  assert.ok(!Api.createRun(noPorts.ports).nemo.help().some(r=>/^(Layer\.get|Layer\.keys|isKeyframe|markers\.(list|next|prev)|comp\.beats?)$/.test(r)),'help lists no read row whose port is absent');
  const full=Api.createRun(world().ports).nemo.help();for(const row of ['Layer.get','Layer.keys','isKeyframe','markers.list','markers.next','markers.prev','comp.beat','comp.beats'])assert.ok(full.includes(row),row);
  assert.ok(!full.includes('Layer.strokes'),'strokes stay unavailable');
  const bad=(r,pattern)=>{assert.equal(r.ok,false);assert.notEqual(r.code,'unavailable');assert.match(r.error,pattern);};
  bad(await run(world(),'nemo.layer(0).get("colour")'),/unknown property "colour"/);bad(await run(world(),'nemo.layer(0).get("opacity",NaN)'),/not a finite number/);
  bad(await run(world(),'nemo.layer(0).keys("nope")'),/unknown property/);bad(await run(world(),'nemo.isKeyframe("zzz",1)'),/no layer "zzz"/);
  typed(await run(world(),'nemo.layer(0).get("timeRemap")'),'no-native-time-remap','Layer.get');typed(await run(world(),'nemo.layer(0).keys("timeRemap")'),'no-native-time-remap','Layer.keys');
});

test('a document that moves between rounds is re-read from scratch; one that moves before the last pass refuses the run',async()=>{
  const w=world();let moved=false;
  w.mutate=()=>{if(!moved){moved=true;w.info={...w.info,contentRevision:4,documentSnapshotId:'native-object:d:4'};}};
  const r=await run(w,'nemo.layer(0).keys("position").length');
  assert.equal(r.ok,true,r.error);assert.equal(r.value,11);
  assert.ok(w.fetched.filter(f=>f.kind==='authoredKeys').length>=4,'the first round was discarded and fetched again at the new revision');
  const late=world();let calls=0;
  late.ports.info=()=>{calls++;return calls>=3?{...late.info,contentRevision:9,documentSnapshotId:'native-object:d:9'}:late.info;};
  const r2=await run(late,'nemo.layer(0).keys("position").length');
  assert.equal(r2.ok,false);assert.match(r2.error,/document changed while the script was reading/);assert.deepEqual(late.frames,[]);
});

test('a script that reads more than one run can hold, or never settles, is refused instead of looping',async()=>{
  const w=world();
  const many=await run(w,'var out=0; for(var f=0;f<2000;f++){ out+=nemo.layer(0).get("opacity",f)[0]; } out');
  assert.equal(many.ok,false);assert.equal(many.code,'refused');assert.match(many.error,/reads more than one run can hold/);
  const deep=world();
  // Each pass learns one more frame to ask for from the previous answer, so it can never settle inside the round budget.
  const chain=await run(deep,'var f=0; for(var i=0;i<40;i++){ f=nemo.layer(0).get("position",f)[0]+1; } f');
  assert.equal(chain.ok,false);assert.equal(chain.code,'refused');
});

test('the reads module: a cache binds to one pin, paged keys must advance, cells find the covering run',async()=>{
  const c=Reads.cache();assert.equal(Reads.bind(c,{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:1,documentSnapshotId:'s1'},false),true);
  assert.equal(Reads.bind(c,{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:2,documentSnapshotId:'s2'},false),false);
  assert.equal(Reads.bind(c,{instanceId:'i',documentId:'d',lifecycleGeneration:1,contentRevision:2,documentSnapshotId:'s2'},true),true);
  c.wanted['{"kind":"keys","uid":"a","prop":"position"}']={kind:'keys',uid:'a',prop:'position'};
  await Reads.fill(c,{property:async()=>({items:[{frame:1,v:[1,2]}],nextOffset:0})});
  assert.deepEqual(Object.values(c.entries).map(e=>[e.ok,e.reason]),[[false,'read-malformed']]);
  assert.equal(Reads.isKey([[0,2,'key'],[2,3,'held']],1),true);assert.equal(Reads.isKey([[0,2,'key'],[2,3,'held']],2),false);assert.equal(Reads.isKey([[0,2,'key']],9),false);
  assert.deepEqual(Reads.beats({effectiveBpm:6000,effectiveBpmOffset:0,effectiveBpmShow:true},24,10),[],'a beat shorter than half a frame draws nothing');
  assert.equal(Reads.snap({effectiveBpm:120,effectiveBpmOffset:0,effectiveBpmShow:false},24,30,7),7,'the grid off leaves the frame alone');
});
