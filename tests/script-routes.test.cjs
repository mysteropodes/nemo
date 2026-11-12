'use strict';
// CS8-SDK (#2354): every baseline sdk: surface row has exactly one declared route.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const ROOT=path.resolve(__dirname,'..'),read=rel=>JSON.parse(fs.readFileSync(path.join(ROOT,rel),'utf8'));
const {ROUTES}=require('../src/js/application/script-native-api.js'),routes={routes:ROUTES,kinds:{operation:1,presentation:1,host:1,unavailable:1}},matrix=read('engineering/inventory/parity/matrix.json');
const rows=(Array.isArray(matrix)?matrix:matrix.rows||matrix.surfaces).filter(r=>String(r.id).startsWith('sdk:')).map(r=>r.id);
const declared=new Set(fs.readdirSync(path.join(ROOT,'engineering/application/capabilities-v2'))
  .flatMap(f=>read('engineering/application/capabilities-v2/'+f).operations||[]));

// The validation the table must pass, as a function so the negative controls below can run it over deliberately broken copies.
function problems(table,expected,known){
  const out=[],ids=table.map(r=>r.row);
  new Set(ids.filter((id,i)=>ids.indexOf(id)!==i)).forEach(id=>out.push('duplicate '+id));
  expected.filter(id=>!ids.includes(id)).forEach(id=>out.push('absent '+id));
  ids.filter(id=>!expected.includes(id)).forEach(id=>out.push('stale '+id));
  for(const r of table){
    if(!Object.keys(routes.kinds).includes(r.kind)){out.push('kind '+r.row);continue;}
    if(r.kind==='operation'){
      if(!r.read&&!r.write)out.push('empty '+r.row);
      for(const op of [r.read,r.write].filter(Boolean))if(!known.has(op))out.push('undeclared-operation '+r.row+' '+op);
      if(r.write&&!/^command\./.test(r.write))out.push('write-name '+r.row);if(r.read&&!/^query\./.test(r.read))out.push('read-name '+r.row);
    }else if(r.kind==='unavailable'){if(!/^[a-z][a-z0-9-]+$/.test(String(r.reason)))out.push('reason '+r.row);if(r.bound!==false)out.push('bound-unavailable '+r.row);}
    else{if(typeof r.note!=='string')out.push('note '+r.row);if('write' in r||'read' in r)out.push('host-write '+r.row);}
  }
  return out;
}

test('the route table covers exactly the baseline sdk rows, once each, and nothing else can write the document',()=>{
  assert.deepEqual(problems(routes.routes,rows,declared),[]);assert.equal(routes.routes.length,73);assert.equal(new Set(rows).size,73);
});

test('every row that is not bound is a typed unavailable with its own reason: nothing is left "not yet bound" (I28/I33/I34)',()=>{
  const unbound=routes.routes.filter(r=>!r.bound);
  assert.equal(unbound.length,35);
  for(const r of unbound){assert.equal(r.kind,'unavailable',r.row);assert.match(r.reason,/^no-native-[a-z-]+$/,r.row);}
  assert.equal(routes.routes.filter(r=>r.bound).length,38);
  for(const row of ['sdk:nemo-script.undoGroup','sdk:nemo-plugin.closeAll','sdk:nemo-plugin.openCount','sdk:nemo-plugin.setTitle'])assert.equal(routes.routes.find(r=>r.row===row).bound,true,row);
});

test('negative controls: an absent, duplicate or stale declaration fails the normal validation (I29)',()=>{
  const table=routes.routes.map(r=>({...r})),one=table.find(r=>r.kind==='operation'&&r.write);
  assert.deepEqual(problems(table,rows,declared),[],'the control starts clean');
  assert.deepEqual(problems(table.filter(r=>r!==one),rows,declared),['absent '+one.row]);
  assert.deepEqual(problems(table.concat([{...one}]),rows,declared),['duplicate '+one.row]);
  assert.deepEqual(problems(table.concat([{row:'sdk:nemo-script.notABaselineRow',kind:'host',note:'x',bound:true}]),rows,declared),['stale sdk:nemo-script.notABaselineRow']);
  assert.deepEqual(problems(table.map(r=>r===one?{...r,write:'command.document.object.layer.nonexistent'}:r),rows,declared),['undeclared-operation '+one.row+' command.document.object.layer.nonexistent']);
  assert.deepEqual(problems(table.map(r=>r===one?{...r,write:'query.document.object'}:r),rows,declared),['write-name '+one.row]);
  const gone=table.find(r=>r.kind==='unavailable');
  assert.deepEqual(problems(table.map(r=>r===gone?{...r,bound:true}:r),rows,declared),['bound-unavailable '+gone.row]);
  assert.deepEqual(problems(table.map(r=>r===gone?{...r,reason:'Not Typed'}:r),rows,declared),['reason '+gone.row]);
  assert.deepEqual(problems(table.map(r=>r===one?{...r,kind:'write-anything'}:r),rows,declared),['kind '+one.row]);
  const host=table.find(r=>r.kind==='host');assert.deepEqual(problems(table.map(r=>r===host?{...r,write:'command.document.apply'}:r),rows,declared),['host-write '+host.row]);
});
