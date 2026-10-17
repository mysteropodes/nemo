'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const FontAssets = require('../src/js/adapters/font-assets.js');
const adapterSource = fs.readFileSync(path.join(__dirname, '../src/js/adapters/font-assets.js'), 'utf8');
const bridgeSource = fs.readFileSync(path.join(__dirname, '../src/js/vector-text-bridge.js'), 'utf8');
const bytes = () => Uint8Array.from([1, 2, 3]).buffer;
const settle = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function fixture(changes = {}) {
  const reads = [], parses = [], invocations = [];
  const fonts = { 'Roboto-Regular': { url: 'fonts/Roboto-Regular.ttf', label: 'Roboto' } };
  const parsed = { family: 'fixed parsed font' };
  const options = {
    fonts,
    fetch(url) { reads.push(url); return Promise.resolve({ ok: true, arrayBuffer: () => Promise.resolve(bytes()) }); },
    parse(value) { parses.push(value); return parsed; },
    isDesktop: () => true,
    invoke(command, args) { invocations.push({ command, args }); return Promise.resolve('AQID'); },
    ...changes,
  };
  return { assets: FontAssets.create(options), fonts, parsed, reads, parses, invocations };
}

test('bundled bytes use the existing path/parser and concurrent loads share the accepted font', async () => {
  const f = fixture();
  const first = f.assets.load('Roboto-Regular');
  assert.equal(first, f.assets.load('Roboto-Regular'));
  assert.equal(await first, f.parsed);
  assert.equal(await f.assets.load('Roboto-Regular'), f.parsed);
  assert.deepEqual(f.reads, ['fonts/Roboto-Regular.ttf']);
  assert.equal(f.parses.length, 1);
  assert.deepEqual([...new Uint8Array(f.parses[0])], [1, 2, 3]);
  await assert.rejects(f.assets.load('Unknown'), /unknown vector font Unknown/);
  await assert.rejects(f.assets.load('__proto__'), /unknown vector font/);
  assert.equal(f.reads.length, 1);
});

test('live desktop bytes use the original native command and publish only after parsing', async () => {
  const read = deferred(), f = fixture({ invoke: () => read.promise });
  const pending = f.assets.googleWeight('Family X', 400, false);
  assert.equal(Object.hasOwn(f.fonts, 'Google:Family X-Regular'), false);
  read.resolve('AQID');
  assert.equal(await pending, f.parsed);
  assert.deepEqual(f.fonts['Google:Family X-Regular'], { url: null, label: 'Family X' });
  assert.deepEqual([...new Uint8Array(f.parses[0])], [1, 2, 3]);
  assert.equal(await f.assets.load('Google:Family X-Regular'), f.parsed);
  assert.deepEqual(f.reads, []);
  const actual = fixture();
  await actual.assets.googleWeight('Family X', 700, true);
  assert.deepEqual(actual.invocations, [{ command: 'fetch_google_font',
    args: { family: 'Family X', weight: 700, italic: true } }]);
  assert.equal(actual.fonts['Google:Family X-Bold'].label, 'Family X Bold');
});

test('web transport preserves the encoded Worker request and rejects failed HTTP responses', async () => {
  const f = fixture({ isDesktop: () => false });
  await f.assets.googleWeight('A & B', 700, true);
  assert.deepEqual(f.reads, ['/api/google-font?family=A%20%26%20B&weight=700&italic=1']);
  assert.equal(f.invocations.length, 0);
  let bodyReads = 0;
  const failed = fixture({ isDesktop: () => false, fetch: () => Promise.resolve({ ok: false,
    arrayBuffer() { bodyReads++; return Promise.resolve(bytes()); } }) });
  await assert.rejects(failed.assets.googleWeight('Missing', 400, false), /family not found or fetch failed/);
  assert.equal(bodyReads, 0);
  assert.equal(Object.hasOwn(failed.fonts, 'Google:Missing-Regular'), false);
});

test('desktop failure preserves the error and accepted fonts without a web fallback or cached failure', async () => {
  const error = Error('native transport failed');
  let calls = 0;
  const f = fixture({ invoke: () => { calls++; return calls === 1 ? Promise.resolve('AQID') : Promise.reject(error); } });
  await f.assets.googleWeight('Accepted', 400, false);
  const before = structuredClone(f.fonts);
  const first = f.assets.googleWeight('Failed', 400, false);
  await assert.rejects(first, value => value === error);
  assert.deepEqual(f.fonts, before);
  const next = f.assets.googleWeight('Failed', 400, false);
  assert.notEqual(next, first);
  await assert.rejects(next, value => value === error);
  assert.equal(calls, 3); // Only the caller's explicit next request tries again.
  assert.equal(f.reads.length, 0);
});

test('parser failure cannot advertise a font or damage another accepted font', async () => {
  const error = Error('invalid font bytes');
  let count = 0;
  const f = fixture({ parse: () => { if (++count === 1) return { accepted: true }; throw error; } });
  const accepted = await f.assets.googleWeight('Good', 400, false);
  const before = structuredClone(f.fonts);
  await assert.rejects(f.assets.googleWeight('Invalid', 400, false), value => value === error);
  assert.deepEqual(f.fonts, before);
  assert.equal(await f.assets.load('Google:Good-Regular'), accepted);
});

test('concurrent live requests share bytes and parsing, with Regular required and Bold best effort', async () => {
  const error = Error('no bold face'), calls = [];
  const f = fixture({ invoke: (command, args) => { calls.push(args);
    return args.weight === 700 ? Promise.reject(error) : Promise.resolve('AQID'); } });
  const a = f.assets.googleWeight('Shared', 400, false);
  assert.equal(a, f.assets.googleWeight('Shared', 400, false));
  await a;
  assert.equal(await f.assets.addGoogle('  Shared  '), 'Google:Shared-Regular');
  await settle();
  assert.deepEqual(calls.map(x => x.weight), [400, 700]);
  assert.equal(f.parses.length, 1);
  assert.equal(Object.hasOwn(f.fonts, 'Google:Shared-Bold'), false);
  await assert.rejects(f.assets.addGoogle('   '), /empty family name/);
});

test('Regular failure rejects the add and never starts Bold', async () => {
  const error = Error('no regular face'), weights = [];
  const f = fixture({ invoke: (command, args) => { weights.push(args.weight); return Promise.reject(error); } });
  await assert.rejects(f.assets.addGoogle('Unavailable'), value => value === error);
  await settle();
  assert.deepEqual(weights, [400]);
  assert.equal(Object.hasOwn(f.fonts, 'Google:Unavailable-Regular'), false);
});

test('disposal during transport rejects late results before parsing or catalog publication', async () => {
  const read = deferred(), f = fixture({ invoke: () => read.promise });
  const pending = f.assets.googleWeight('Delayed', 400, false);
  await settle();
  f.assets.dispose();
  read.resolve('AQID');
  await assert.rejects(pending, /font assets disposed/);
  assert.equal(f.parses.length, 0);
  assert.equal(Object.hasOwn(f.fonts, 'Google:Delayed-Regular'), false);
  await assert.rejects(f.assets.load('Roboto-Regular'), /font assets disposed/);
  await assert.rejects(f.assets.googleWeight('Later', 400, false), /font assets disposed/);
});

test('disposal during parsing also blocks publication; disposal before work sends no request', async () => {
  const parsed = deferred(), f = fixture({ parse: () => parsed.promise });
  const pending = f.assets.googleWeight('Parsing', 400, false);
  await settle();
  f.assets.dispose();
  parsed.resolve({ late: true });
  await assert.rejects(pending, /font assets disposed/);
  assert.equal(Object.hasOwn(f.fonts, 'Google:Parsing-Regular'), false);
  const early = fixture(), stopped = early.assets.googleWeight('Queued', 400, false);
  early.assets.dispose();
  await assert.rejects(stopped, /font assets disposed/);
  assert.equal(early.invocations.length, 0);
});

test('disposal after a cached Regular result prevents add completion and its later Bold request', async () => {
  const f = fixture();
  await f.assets.googleWeight('Cached', 400, false);
  const pending = f.assets.addGoogle('Cached');
  f.assets.dispose();
  await assert.rejects(pending, /font assets disposed/);
  assert.deepEqual(f.invocations.map(x => x.args.weight), [400]);
});

function bridge(change = () => {}) {
  const reads = [], invocations = [], nativeImports = [];
  const sandbox = { Map, Uint8Array, atob, Promise,
    fetch(url) { reads.push(url); return Promise.resolve({ ok: true, arrayBuffer: () => Promise.resolve(bytes()) }); },
    opentype: { parse: value => ({ bytes: [...new Uint8Array(value)] }) },
    __TAURI__: { core: { invoke(command, args) { invocations.push({ command, args }); return Promise.resolve('AQID'); } } } };
  sandbox.window = sandbox;
  sandbox.nativeFontRead = () => Promise.resolve(bytes());
  sandbox.NemoNativeFontResources = {desktop(root,parse) {
    assert.equal(root.NemoNativeFontResources,sandbox.NemoNativeFontResources);const parsed=new Map();
    return {ownerKey:()=> 'current-native-owner',discover:async()=>[],dispose(){},loadAsset:async id=>parsed.get(id),
      importSource(source){nativeImports.push({...source});return sandbox.nativeFontRead(source).then(value=>{const font=parse(value),assetId='asset-'+nativeImports.length;parsed.set(assetId,font);return {assetId,font};});}};
  }};
  change(sandbox);
  vm.runInNewContext(adapterSource, sandbox, { filename: 'font-assets.js' });
  vm.runInNewContext(bridgeSource, sandbox, { filename: 'vector-text-bridge.js' });
  return { sandbox, api: sandbox.SMVectorText, reads, invocations, nativeImports };
}

test('actual native bridge retains eight choices and public entry points without eager imports or legacy IO', async () => {
  const b = bridge();
  await settle();
  assert.deepEqual(Object.keys(b.api).sort(), ['VECTOR_FONTS', 'addGoogleFont', 'buildVectorTextGroup',
    'importNativeFontSource', 'listNativeFontAssets', 'loadNativeFontAsset', 'loadVectorFont', 'vectorTextGroupMembers'].sort());
  assert.deepEqual(Object.keys(b.api.VECTOR_FONTS), ['Roboto-Regular', 'Roboto-Bold', 'DejaVuSans-Regular',
    'DejaVuSans-Bold', 'DejaVuSerif-Regular', 'DejaVuSerif-Bold', 'Manrope-Regular', 'Inter-Regular']);
  assert.deepEqual(b.nativeImports,[]);
  assert.deepEqual(b.reads,[]);assert.deepEqual(b.invocations,[]);
  assert.deepEqual(await b.api.loadVectorFont('Inter-Regular'),{bytes:[1,2,3]});
  assert.equal(await b.api.addGoogleFont('New Family'),'Google:New Family-Regular');await settle();
  assert.deepEqual(b.nativeImports.filter(x=>x.kind==='google'),[{kind:'google',family:'New Family',weight:400,italic:false}]);
  assert.deepEqual(b.reads,[]);assert.deepEqual(b.invocations,[]);
});

test('actual glyph caller still selects real Bold and falls back to the original key for families without Bold', async () => {
  const b = bridge(), selected = [], stop = Error('stop before glyph construction');
  await settle();
  b.sandbox.loadVectorFont = key => { selected.push(key); return Promise.reject(stop); };
  for (const key of ['Roboto-Regular', 'Manrope-Regular', 'Inter-Regular']) {
    await assert.rejects(b.api.buildVectorTextGroup('text', key, 12, '#fff', 'left', null,
      { x: 0, y: 0 }, null, { bold: true }), value => value === stop);
  }
  assert.deepEqual(selected, ['Roboto-Bold', 'Manrope-Regular', 'Inter-Regular']);
});

test('retained non-native Bold caller awaits its pending weight without publishing an unparsed catalog entry', async () => {
  const bold = deferred(), stop = Error('stop before glyph construction'), selected = [];
  const b = bridge(s => { delete s.__TAURI__;s.fetch=url=> /weight=700/.test(url)?bold.promise:Promise.resolve({ok:true,arrayBuffer:async()=>bytes()}); });
  assert.equal(await b.api.addGoogleFont('Pending'), 'Google:Pending-Regular');
  assert.equal(Object.hasOwn(b.api.VECTOR_FONTS, 'Google:Pending-Bold'), false);
  b.sandbox.loadVectorFont = key => { selected.push(key); return Promise.reject(stop); };
  await assert.rejects(b.api.buildVectorTextGroup('text', 'Google:Pending-Regular', 12, '#fff', 'left', null,
    { x: 0, y: 0 }, null, { bold: true }), value => value === stop);
  assert.deepEqual(selected, ['Google:Pending-Bold']);
  bold.reject(Error('this family has no Bold'));
  await settle();
  await assert.rejects(b.api.buildVectorTextGroup('text', 'Google:Pending-Regular', 12, '#fff', 'left', null,
    { x: 0, y: 0 }, null, { bold: true }), value => value === stop);
  assert.deepEqual(selected, ['Google:Pending-Bold', 'Google:Pending-Regular']);
});

test('actual native Bold caller imports only on explicit request and awaits parsed native bytes before glyphs',async()=>{
  const bold=deferred(),stop=Error('stop before native glyph construction');let glyphs=0,settled=false;
  const b=bridge(s=>{s.nativeFontRead=source=>source.kind==='google'&&source.weight===700?bold.promise:Promise.resolve(bytes());s.opentype.parse=()=>({unitsPerEm:1000,charToGlyph(){glyphs++;throw stop;}});});
  await b.api.addGoogleFont('Native Pending');assert.equal(b.nativeImports.some(x=>x.kind==='google'&&x.weight===700),false);
  const pending=b.api.buildVectorTextGroup('text','Google:Native Pending-Regular',12,'#fff','left',null,{x:0,y:0},null,{bold:true});pending.then(()=>{settled=true;},()=>{settled=true;});
  await settle();assert.equal(glyphs,0);assert.equal(settled,false);assert.equal(Object.hasOwn(b.api.VECTOR_FONTS,'Google:Native Pending-Bold'),false);
  assert.deepEqual(b.nativeImports.filter(x=>x.kind==='google').map(x=>x.weight),[400,700]);bold.resolve(bytes());await assert.rejects(pending,error=>error===stop);assert.equal(glyphs,1);
  assert.equal(Object.hasOwn(b.api.VECTOR_FONTS,'Google:Native Pending-Bold'),true);assert.deepEqual(b.reads,[]);assert.deepEqual(b.invocations,[]);
});

test('retained non-native bridge keeps its four original warmups',async()=>{const b=bridge(s=>{delete s.__TAURI__;});await settle();assert.deepEqual(b.reads,['fonts/Roboto-Regular.ttf','fonts/Roboto-Bold.ttf','fonts/DejaVuSans.ttf','fonts/DejaVuSerif.ttf']);assert.deepEqual(b.nativeImports,[]);});
