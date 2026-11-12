'use strict';
// U01 (#2369): the real app.js prelude, run before paper.setup with inert storage and DOM.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ROOT=path.resolve(__dirname,'..'),read=rel=>fs.readFileSync(path.join(ROOT,'src',rel),'utf8');
const APP=read('js/app.js'),HTML=read('index.html');
function prelude(end){const at=APP.indexOf(end);assert.ok(at>0,end);return APP.slice(0,at);}
function storage(seed,fail){const map=new Map(Object.entries(seed));const s={get length(){if(fail)throw Error('blocked');return map.size;},
  key:i=>[...map.keys()][i]??null,getItem:k=>map.has(k)?map.get(k):null,setItem:(k,v)=>{map.set(k,String(v));}};return {s,map};}
function runMigration(seed,fail){const {s,map}=storage(seed,fail);const context=vm.createContext({localStorage:s});
  const run=()=>vm.runInContext(prelude('paper.install(window);'),context,{filename:'app.js'});return {map,run};}

test('storage migration copies sm keys forward once, keeps old keys and never overwrites nemo keys',()=>{
  const {map,run}=runMigration({'sm-profile':'old','sm_recents':'r','nemo-profile':'kept','smother':'x','other':'y'});
  run();const first=Object.fromEntries(map);
  assert.deepEqual(first,{'sm-profile':'old','sm_recents':'r','nemo-profile':'kept','smother':'x','other':'y','nemo_recents':'r'});
  run();assert.deepEqual(Object.fromEntries(map),first);
  assert.doesNotThrow(()=>runMigration({'sm-a':'1'},true).run());
});

test('paper.install clobbers Event and MouseEvent, and the prelude restores the real DOM constructors only',()=>{
  class DomEvent{}class DomMouseEvent{}class PaperEvent{}class PaperMouseEvent{}class PaperPoint{}
  const attached=[];const iframe={style:{},contentWindow:{Event:DomEvent,MouseEvent:DomMouseEvent}};
  const context=vm.createContext({localStorage:storage({}).s,
    paper:{install(w){w.Event=PaperEvent;w.MouseEvent=PaperMouseEvent;w.Point=PaperPoint;}},
    document:{createElement:tag=>{assert.equal(tag,'iframe');return iframe;},
      documentElement:{appendChild:n=>attached.push(n),removeChild:n=>attached.splice(attached.indexOf(n),1)}}});
  context.window=context;
  vm.runInContext(prelude("paper.setup(document.getElementById('drawing-canvas'));"),context,{filename:'app.js'});
  assert.equal(context.Event,DomEvent);assert.equal(context.MouseEvent,DomMouseEvent);
  assert.equal(context.Point,PaperPoint,'Paper geometry classes stay installed');
  assert.equal(iframe.style.display,'none');assert.deepEqual(attached,[]);
});

function classicScripts(){return [...HTML.matchAll(/<script\b[^>]*\bsrc=["']([^"']+)["'][^>]*>/g)].map(m=>m[1]);}
const WRITE=/\bwindow\.SM(\.[A-Za-z_$][\w$]*)?\s*=(?!=)/;
function depthAt(source,index){let depth=0;for(let i=0;i<index;i++){if(source[i]==='{')depth++;else if(source[i]==='}')depth--;}return depth;}

test('startup resolves one SM namespace: a single literal owner, earlier writers deferred and non-clobbering',()=>{
  const srcs=classicScripts().filter(s=>s.startsWith('js/'));
  const owners=srcs.filter(s=>/\bwindow\.SM\s*=\s*\{/.test(read(s).split('\n').filter(l=>!/^\s*\/\//.test(l)).join('\n')));
  assert.deepEqual(owners,['js/timeline.js']);
  const ownerAt=srcs.indexOf('js/timeline.js');assert.equal(srcs.lastIndexOf('js/timeline.js'),ownerAt);
  for(const s of srcs.slice(0,ownerAt)){const source=read(s);let at=0;
    for(const line of source.split('\n')){const here=at;at+=line.length+1;
      if(/^\s*\/\//.test(line)||!WRITE.test(line))continue;
      if(/\bwindow\.SM\s*=(?!=)/.test(line))assert.match(line,/window\.SM\s*=\s*window\.SM\s*\|\|\s*\{\}/,`${s}: only the defaulting form may precede the owner`);
      assert.ok(depthAt(source,here)>=2,`${s}: window.SM writes before the owner must run from a deferred callback`);
      assert.match(source,/addEventListener\(\s*['"](load|DOMContentLoaded)['"]/,`${s}: writer must be deferred to load/DOMContentLoaded`);}}
});

test('frozen-feature flags keep their declared values and have one declaration',()=>{
  const decls=[...HTML.matchAll(/<script>\s*(window\.SM_FROZEN_IN_DEV\s*=[^<]*)<\/script>/g)];
  assert.equal(decls.length,1);
  const context=vm.createContext({});context.window=context;vm.runInContext(decls[0][1],context);
  assert.deepEqual({...context.SM_FROZEN_IN_DEV},{rig:false,storyboard:true,import3d:true});
  for(const s of classicScripts().filter(s=>s.startsWith('js/')))assert.doesNotMatch(read(s),/SM_FROZEN_IN_DEV\s*=(?!=)/,s);
});
