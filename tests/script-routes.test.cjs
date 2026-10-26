'use strict';
// CS8-SDK (#2354): every baseline sdk: surface row has exactly one declared route.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const ROOT=path.resolve(__dirname,'..'),read=rel=>JSON.parse(fs.readFileSync(path.join(ROOT,rel),'utf8'));
const {ROUTES}=require('../src/js/application/script-native-api.js'),routes={routes:ROUTES,kinds:{operation:1,presentation:1,host:1,unavailable:1}},matrix=read('engineering/inventory/parity/matrix.json');
const rows=(Array.isArray(matrix)?matrix:matrix.rows||matrix.surfaces).filter(r=>String(r.id).startsWith('sdk:')).map(r=>r.id);
const declared=new Set(fs.readdirSync(path.join(ROOT,'engineering/application/capabilities-v2'))
  .flatMap(f=>read('engineering/application/capabilities-v2/'+f).operations||[]));

test('the route table covers exactly the baseline sdk rows, once each',()=>{
  const ids=routes.routes.map(r=>r.row);
  assert.equal(new Set(ids).size,ids.length,'no duplicate route');
  assert.deepEqual([...ids].sort(),[...rows].sort());assert.equal(ids.length,73);
});

test('operation routes name only declared v2 operations; nothing else can write the document',()=>{
  for(const r of routes.routes){
    assert.ok(Object.keys(routes.kinds).includes(r.kind),r.row);
    if(r.kind==='operation'){
      assert.ok(r.read||r.write,r.row);
      for(const op of [r.read,r.write].filter(Boolean))assert.ok(declared.has(op),`${r.row}: ${op} is not a declared operation`);
      if(r.write)assert.match(r.write,/^command\./,r.row);if(r.read)assert.match(r.read,/^query\./,r.row);
    }else if(r.kind==='unavailable')assert.match(r.reason,/^[a-z][a-z0-9-]+$/,r.row);
    else{assert.equal(typeof r.note,'string',r.row);assert.ok(!('write' in r)&&!('read' in r),r.row);}
    if(r.kind==='unavailable')assert.equal(r.bound,false,r.row);
  }
});
