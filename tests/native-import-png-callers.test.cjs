'use strict';
// Actual caller + Core + PNG-stage modules, controlled filesystem/DOM encoder/native receipt boundaries.
// No native admission, installed UI, history implementation or resource-budget acceptance is inferred.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Psd = require('../src/js/adapters/native-import-psd.js');
const Core = require('../src/js/adapters/native-import-core.js');
const Stage = require('../src/js/adapters/native-import-png-stage.js');
const Svg = require('../src/js/adapters/native-import-svg.js');
const Paper = require('./helpers/native-import-paper.cjs');
const { makeSession, makePorts } = require('./helpers/native-import-session.cjs');
const project = path.join(__dirname, '..');
const oracle = fs.readFileSync(path.join(project, 'native-engine/tests/fixtures/object-raster-asset-attach/oracle.txt'), 'utf8')
  .split('\n').filter(line => line && !line.startsWith('#')).map(line => JSON.parse(line)).find(row => row.name === 'blue-2x1');
const PNG = Buffer.from(oracle.hex, 'hex');
const flows = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures/native-import-adapters/figma-walk.json'), 'utf8'));
const paper = Paper.load();

function files(options) {
  const dirs = new Set(), rows = new Map(), o = options || {};
  return { dirs, rows, api: {
    exists: async name => name === '/owned-temp' || dirs.has(name) || rows.has(name),
    mkdir: async (name, args) => { assert.deepEqual(args, { recursive: false }); assert.equal(dirs.has(name), false); dirs.add(name); },
    writeFile: async (name, bytes) => { assert.ok(bytes instanceof Uint8Array); rows.set(name, { kind: 'bytes', value: Uint8Array.from(bytes) }); },
    writeTextFile: async (name, value) => { assert.equal(typeof value, 'string'); rows.set(name, { kind: 'text', value }); },
    remove: async (name, args) => {
      assert.deepEqual(args, { recursive: false });
      assert.ok(name.startsWith('/owned-temp/'), 'cleanup cannot remove the shared temp root');
      if (o.failCleanup) throw new Error('controlled cleanup refusal');
      if (rows.has(name)) rows.delete(name);
      else {
        assert.ok(![...rows.keys()].some(key => key.startsWith(name + '/')), 'directory removal requires no remaining leaves');
        dirs.delete(name);
      }
    },
  } };
}
function canvas(encode) {
  return { width: oracle.width, height: oracle.height, toBlob(callback, type) {
    assert.equal(type, 'image/png');
    if (encode) encode(callback); else callback(new Blob([PNG], { type: 'image/png' }));
  } };
}
function boundary(options) {
  const o = options || {}, h = makeSession(), io = files(o), calls = [];
  Object.defineProperty(h.session, 'importBatch', { value: async (payload, before) => {
    calls.push(structuredClone(payload));
    const staged = payload.sources.filter(row => row.mediaType === 'image/png');
    for (const row of staged) {
      assert.deepEqual(Object.keys(row.source), ['kind', 'path']);
      assert.equal(row.source.kind, 'file');
      assert.deepEqual(Buffer.from(io.rows.get(row.source.path).value), PNG);
    }
    assert.ok([...io.rows.values()].some(row => row.kind === 'text'), 'durable obligation exists before invocation');
    if (o.transport) throw new Error('controlled transport loss');
    if (o.refuse) throw new Error('Native command refused: unavailable');
    const revision = before.contentRevision + 1;
    const layers = payload.layers.map((row, i) => ({ sourceId: row.sourceId, layerUid: 'native-layer:' + revision + ':' + i }));
    const objects = payload.records.concat(payload.raster || [], payload.text || []).map((row, i) => ({ sourceId: row.sourceId, strokeId: 'native-object:' + i }));
    const assets = payload.sources.map((row, i) => ({ sourceId: row.sourceId, assetId: 'native-asset:' + i }));
    h.state.contentRevision = revision; h.state.documentSnapshotId = 'snapshot-' + revision;
    payload.layers.forEach((row, i) => h.state.layers.push({ ...h.state.layers[0], layerUid: layers[i].layerUid, name: row.name }));
    const result = { applied: true, historyEntriesAdded: 1, layers, objects, assets, skipped: payload.skipped };
    if (o.tamper) o.tamper(result);
    return { apiVersion: 2, requestId: 'controlled-png-batch', instanceId: before.instanceId, documentId: before.documentId,
      contentRevision: revision, ok: true, result };
  } });
  const controls = makePorts(h), core = Core.create(controls.ports);
  let serial = 0;
  const stage = Stage.create({ core, fs: io.api, path: { tempDir: async () => '/owned-temp', join: async (...parts) => path.posix.join(...parts) },
    uuid: () => '00000000-0000-4000-8000-' + String(++serial).padStart(12, '0') });
  return { h, io, core, stage, calls, selection: controls.selection };
}
function psdInput(first) {
  return { width: 320, height: 180, children: [
    { name: 'Blue', left: -8, top: 4, opacity: 0, canvas: first || canvas() },
    { name: 'Hidden', hidden: true, canvas: canvas() },
    { name: '', left: 10, top: -3, canvas: canvas() },
  ] };
}
function psdCaller(f, input) {
  const resized = [], toasts = [];
  const api = Psd.create({ core: f.core, stage: f.stage, readPsd: async () => input, t: key => key,
    notify: value => toasts.push(value), resizeCanvas: async (w, h) => { resized.push([w, h]); return 'applied'; } });
  return { api, resized, toasts };
}
function miniSvg(text) {
  const group = new paper.Group({ insert: false, applyMatrix: false });
  const tag = /<path transform="matrix\(([^)]*)\)" d="([^"]*)" fill-rule="[a-z]+" fill="rgba\((\d+),(\d+),(\d+),([\d.]+)\)"\/>/g;
  let m;
  while ((m = tag.exec(text))) {
    const item = new paper.Path({ insert: false });
    for (const command of m[2].match(/[MLZ][^MLZ]*/g) || []) {
      const [x, y] = command.slice(1).trim().split(/\s+/).map(Number);
      if (command[0] === 'M' || command[0] === 'L') item.add(new paper.Point(x, y)); else item.closed = true;
    }
    item.transform(new paper.Matrix(...m[1].split(' ').map(Number)));
    item.fillColor = new paper.Color(Number(m[3]) / 255, Number(m[4]) / 255, Number(m[5]) / 255, Number(m[6]));
    group.addChild(item);
  }
  return group;
}
function figmaCaller(f, fetchImage) {
  const source = fs.readFileSync(path.join(project, 'src/js/vector-text-bridge.js'), 'utf8');
  const start = source.indexOf('var VECTOR_FONTS = {'), end = source.indexOf('var _vectorFontAssets =', start);
  assert.ok(start >= 0 && end > start);
  const host = { SMVectorText: { VECTOR_FONTS: new Function(source.slice(start, end) + ';return VECTOR_FONTS;')(),
    addGoogleFont() { assert.fail('no individual font publication'); } } };
  const module = { exports: {} };
  new Function('module', 'exports', 'require', 'globalThis', fs.readFileSync(path.join(project, 'src/js/adapters/native-import-figma.js'), 'utf8'))
    (module, module.exports, name => {
      if (name === './native-import-svg.js') return Svg;
      assert.equal(name, './native-import-figma-plan.js');
      return require('../src/js/adapters/native-import-figma-plan.js');
    }, host);
  const fetches = [], resized = [];
  const api = module.exports.create({ core: f.core, stage: f.stage, paper: Paper.port(paper, miniSvg), t: key => key,
    readCatalog: () => Object.entries(host.SMVectorText.VECTOR_FONTS).map(([key, row]) =>
      ({ key, label: row.label, source: { kind: 'bundled', fontKey: key } })),
    fetch: async url => {
      fetches.push(url);
      const response = fetchImage ? await fetchImage(url) : new Response(PNG, { headers: { 'Content-Length': String(PNG.length) } });
      // Preserve the earlier controlled response bodies while supplying the actual stream interface.
      if (response && !Object.hasOwn(response, 'body') && typeof response.blob === 'function' && !(response instanceof Response)) {
        return new Response(await response.blob(), { status: response.ok ? 200 : 503 });
      }
      return response;
    },
    resizeCanvas: async (w, h) => { resized.push([w, h]); return 'applied'; } });
  return { api, fetches, resized };
}
function mixed() {
  const flow = structuredClone(flows.flows.find(row => row.id === 'mixed-everything'));
  flow.json.document.children[0].children[0].children[1].style.fontFamily = 'Roboto';
  return flow;
}

test('PSD stages actual canvas PNGs and exact original placement/name/opacity/frame in one native batch', async () => {
  const f = boundary(), input = psdInput(), caller = psdCaller(f, input);
  const result = await caller.api.importFile({ name: 'ordinary.psd' });
  assert.equal(result.status, 'imported');
  assert.equal(result.historyEntriesAdded, 1);
  assert.equal(result.count, 2);
  assert.equal(f.calls.length, 1);
  const payload = f.calls[0];
  assert.deepEqual(payload.layers, [{ sourceId: 'psd:0', name: 'Blue' }, { sourceId: 'psd:2', name: 'Calque PSD 2' }]);
  assert.deepEqual(payload.raster.map(row => row.entry.item), [
    { assetId: 'psd:0:asset', x: -7, y: 4.5, width: 2, height: 1, opacity: 0 },
    { assetId: 'psd:2:asset', x: 11, y: -2.5, width: 2, height: 1, opacity: 1 },
  ]);
  assert.deepEqual(payload.raster.map(row => row.entry.target.frameScope), [{ kind: 'authored', frame: 0 }, { kind: 'authored', frame: 0 }]);
  assert.deepEqual(payload.skipped, [{ kind: 'layer', sourceId: 'psd:1', reason: 'hidden' }]);
  assert.deepEqual(payload.unavailable, []);
  assert.equal(new Set(payload.sources.map(row => row.source.path)).size, 2);
  assert.equal(payload.documentSnapshotId, 'snapshot-7');
  assert.equal(payload.lifecycleGeneration, 1);
  assert.equal(f.h.state.contentRevision, 8);
  assert.deepEqual(caller.resized, [[320, 180]]);
  assert.equal(f.io.rows.size, 0);
  assert.equal(f.io.dirs.size, 0);
});

test('Figma HTTPS PNG, Text/font and vector records reach one batch without attach-first or data URLs', async () => {
  const f = boundary(), caller = figmaCaller(f), flow = mixed();
  const result = await caller.api.importFile(flow.json, flow.opts);
  assert.equal(result.status, 'imported');
  assert.equal(result.historyEntriesAdded, 1);
  assert.equal(f.calls.length, 1);
  assert.deepEqual(caller.fetches, ['https://s3.example/p1.png']);
  assert.equal(f.calls[0].records.length, 1);
  assert.equal(f.calls[0].text.length, 1);
  assert.equal(f.calls[0].raster.length, 1);
  assert.deepEqual(f.calls[0].raster[0].entry.item, { assetId: 'figma:0:asset:0', x: 35, y: 35, width: 10, height: 10, opacity: 1 });
  assert.deepEqual(f.calls[0].sources.map(row => row.mediaType), ['font/ttf', 'image/png']);
  assert.deepEqual(f.calls[0].objectOrder, ['figma:0:svg:1:0', 'figma:0:text:0', 'figma:0:image:0']);
  assert.deepEqual(result.objects.map(row => row.sourceId), ['figma:0:svg:1:0', 'figma:0:image:0', 'figma:0:text:0']);
  assert.deepEqual(result.report, { framesImported: 1, shapesImported: 1, textsImported: 1, imagesImported: 1, skipped: ['ELLIPSE (noGeo)'] });
  assert.equal(JSON.stringify(f.calls[0]).includes('data:'), false);
  assert.equal(f.io.rows.size, 0);
});

test('later Figma fetch failure, PSD encoder failure and request overflow clean only before invocation', async () => {
  const f = boundary(), flow = mixed(), frame = flow.json.document.children[0].children[0];
  frame.children.push({ ...structuredClone(frame.children[2]), name: 'second', fills: [{ type: 'IMAGE', imageRef: 'p2' }] });
  flow.opts.imageMap.p2 = 'https://s3.example/p2.png';
  const caller = figmaCaller(f, url => {
    if (url.endsWith('p2.png')) throw new Error('controlled later fetch failure');
    return { ok: true, blob: async () => new Blob([PNG], { type: 'image/png' }) };
  });
  assert.equal((await caller.api.importFile(flow.json, flow.opts)).status, 'unavailable');
  assert.equal(f.calls.length, 0);
  assert.equal(f.io.rows.size, 0);
  const bad = boundary(), psd = psdCaller(bad, psdInput(canvas(callback => callback(null))));
  assert.notEqual((await psd.api.importFile({})).status, 'imported');
  assert.equal(bad.calls.length, 0);
  assert.equal(bad.io.rows.size, 0);
  const large = boundary(), text = mixed();
  text.json.document.children[0].children[0].children[1].characters = 'x'.repeat(4096);
  assert.equal((await figmaCaller(large).api.importFile(text.json, text.opts)).status, 'too_large');
  assert.equal(large.calls.length, 0);
  assert.equal(large.io.rows.size, 0);
});

test('owner drift or disposal during canvas encoding publishes nothing and releases its preinvoke attempt', async () => {
  for (const mutate of [f => f.core.dispose(), f => { f.h.state.documentId = 'replacement'; }, f => { f.h.frame.value = 1; }]) {
    let release, started;
    const entered = new Promise(resolve => { started = resolve; });
    const held = canvas(callback => { release = callback; started(); });
    const f = boundary(), caller = psdCaller(f, psdInput(held));
    const pending = caller.api.importFile({});
    await entered;
    mutate(f); release(new Blob([PNG], { type: 'image/png' }));
    assert.notEqual((await pending).status, 'imported');
    assert.equal(f.calls.length, 0);
    assert.equal(f.h.state.contentRevision, 7);
    assert.equal(f.io.rows.size, 0);
    assert.deepEqual(caller.resized, []);
  }
});

test('every postinvoke refusal, transport loss or uncorrelated receipt retains immutable PNGs and durable obligation', async () => {
  for (const options of [{ refuse: true }, { transport: true }, { tamper: result => result.assets.pop() }]) {
    const f = boundary(options), caller = psdCaller(f, psdInput());
    const result = await caller.api.importFile({});
    assert.notEqual(result.status, 'imported');
    assert.equal(f.calls.length, 1, 'no automatic replay');
    assert.equal(result.staging.status, 'retained');
    assert.ok([...f.io.rows.values()].some(row => row.kind === 'text'));
    const pngs = [...f.io.rows.values()].filter(row => row.kind === 'bytes');
    assert.equal(pngs.length, 2);
    pngs.forEach(row => assert.deepEqual(Buffer.from(row.value), PNG));
    assert.deepEqual(caller.resized, []);
  }
});

test('cleanup failure after a correlated native success does not erase the imported result or its obligation', async () => {
  const f = boundary({ failCleanup: true }), caller = psdCaller(f, psdInput());
  const result = await caller.api.importFile({});
  assert.equal(result.status, 'imported');
  assert.equal(result.historyEntriesAdded, 1);
  assert.equal(f.calls.length, 1);
  assert.equal(result.staging.status, 'retained');
  assert.ok([...f.io.rows.values()].some(row => row.kind === 'text'));
  assert.equal(f.h.state.contentRevision, 8);
});


test('Figma PNG response header and streamed body are bounded before staging or native invocation', async () => {
  const limit = 16 * 1024 * 1024;
  for (const declared of [String(limit + 1), null]) {
    let cancelled = 0, delivered = 0;
    const body = new ReadableStream({
      pull(controller) {
        delivered++;
        controller.enqueue(new Uint8Array(declared === null ? limit + 1 : 1));
      },
      cancel() { cancelled++; },
    }, { highWaterMark: 0 });
    const response = new Response(body, { headers: declared === null ? {} : { 'Content-Length': declared } });
    const f = boundary(), caller = figmaCaller(f, () => response), flow = mixed();
    const result = await caller.api.importFile(flow.json, flow.opts);
    assert.equal(result.status, 'unavailable');
    assert.equal(result.code, 'png_size');
    assert.equal(cancelled, 1);
    assert.equal(delivered, declared === null ? 1 : 0);
    assert.equal(f.calls.length, 0);
    assert.equal(f.io.rows.size, 0);
    assert.equal(f.io.dirs.size, 0);
    assert.equal(f.h.state.contentRevision, 7);
    assert.deepEqual(caller.resized, []);
  }
});

test('Figma consumes a bounded PNG stream once and never uses the unbounded response blob method', async () => {
  let reads = 0;
  const response = new Response(new ReadableStream({
    start(controller) {
      controller.enqueue(Uint8Array.from(PNG.subarray(0, 33)));
      controller.enqueue(Uint8Array.from(PNG.subarray(33)));
      controller.close();
    },
  }), { headers: { 'Content-Length': String(PNG.length) } });
  response.blob = () => { assert.fail('unbounded response.blob must not be used'); };
  const getReader = response.body.getReader.bind(response.body);
  response.body.getReader = () => {
    const reader = getReader(), read = reader.read.bind(reader);
    reader.read = () => { reads++; return read(); };
    return reader;
  };
  const f = boundary(), caller = figmaCaller(f, () => response), flow = mixed();
  const result = await caller.api.importFile(flow.json, flow.opts);
  assert.equal(result.status, 'imported');
  assert.equal(reads, 3);
  assert.equal(f.calls.length, 1);
  assert.equal(result.historyEntriesAdded, 1);
  assert.equal(f.io.rows.size, 0);
  assert.equal(f.io.dirs.size, 0);
});

test('Figma refuses missing streams and cancels a stream if the captured owner changes during read', async () => {
  for (const kind of ['missing', 'stale']) {
    const f = boundary();
    let cancelled = 0, fallback = 0;
    const response = kind === 'missing' ? { ok: true, body: null, blob() { fallback++; return new Blob([PNG]); } } :
      new Response(new ReadableStream({
        pull(controller) { f.h.state.documentId = 'replacement'; controller.enqueue(Uint8Array.from(PNG)); },
        cancel() { cancelled++; },
      }, { highWaterMark: 0 }));
    const caller = figmaCaller(f, () => response), flow = mixed();
    const result = await caller.api.importFile(flow.json, flow.opts);
    assert.equal(result.status, kind === 'missing' ? 'unavailable' : 'stale');
    assert.equal(cancelled, kind === 'missing' ? 0 : 1);
    assert.equal(fallback, 0);
    assert.equal(f.calls.length, 0);
    assert.equal(f.io.rows.size, 0);
    assert.equal(f.h.state.contentRevision, 7);
    assert.deepEqual(caller.resized, []);
  }
});
