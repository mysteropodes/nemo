'use strict';
// Real-parser bootstrap: NEMO_PARSE5_MODULE=/scratch/node_modules/parse5/dist/index.js
// node --test scripts/nemo/boundaries-script-order.test.cjs
// If unset, resolve the integration owner's installed parse5. Missing/wrong parser fails.
const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { spawnSync } = require('node:child_process');
const { inventoryHtmlScripts } = require('./lib/boundaries-script-order.cjs');

let parseHTML;
test.before(async () => {
  try {
    const explicit = process.env.NEMO_PARSE5_MODULE;
    if (explicit && !path.isAbsolute(explicit)) throw new Error('NEMO_PARSE5_MODULE must be an absolute module file path');
    const moduleFile = explicit || require.resolve('parse5');
    const metadata = JSON.parse(fs.readFileSync(path.resolve(path.dirname(moduleFile), '../package.json'), 'utf8'));
    assert.equal(metadata.name, 'parse5');
    assert.equal(metadata.version, '8.0.1', 'tests require verified parse5 8.0.1');
    ({ parse: parseHTML } = await import(pathToFileURL(moduleFile).href));
    assert.equal(typeof parseHTML, 'function');
  } catch (cause) {
    throw new Error('Real parse5 8.0.1 is required. Install it or set NEMO_PARSE5_MODULE to its dist/index.js; tests cannot skip the parser.', { cause });
  }
});

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-script-order-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'src/js'), { recursive: true });
  for (const name of ['a.js', 'b.js', 'c.js', 'space name.js', 'a&b.js', 'a#b.js', 'a?b.js', 'é.js', 'UPPER.JS']) {
    fs.writeFileSync(path.join(root, 'src/js', name), 'throw new Error("inventory must not evaluate sources");');
  }
  const options = { parseHTML, root, htmlFile: 'src/index.html', sourceRoot: 'src' };
  return { root, options, scan: (html, overrides = {}) => inventoryHtmlScripts(html, { ...options, ...overrides }) };
}

const tag = (attrs = '', text = '') => `<script ${attrs}>${text}</script>`;
const sources = (report) => report.scripts.map((s) => s.src?.value ?? '[inline]');
const ordered = (report) => report.eagerOrder.map((id) => report.scripts.find((s) => s.id === id));
const hasCode = (report, code) => [...report.diagnostics, ...report.findings].some((d) => d.code === code);

test('real parser version and a source-backed classic record', (t) => {
  const { scan } = fixture(t), report = scan('<!doctype html>\n' + tag('src="js/a.js"'));
  assert.equal(report.scripts.length, 1);
  assert.equal(report.scripts[0].kind, 'classic');
  assert.equal(report.scripts[0].src.path, 'src/js/a.js');
  assert.equal(report.scripts[0].startTag.startLine, 2);
  assert.deepEqual(report.diagnostics, []);
});

test('missing parser injection is an explicit error', (t) => {
  const { options } = fixture(t);
  assert.throws(() => inventoryHtmlScripts('', { ...options, parseHTML: undefined }), /parseHTML is required/);
  assert.throws(() => inventoryHtmlScripts('', { ...options, parseHTML: async () => ({}) }), /must be synchronous/);
  assert.throws(() => inventoryHtmlScripts('', { ...options, parseHTML: () => ({}) }), /default-adapter HTML document/);
  assert.throws(() => inventoryHtmlScripts(tag('', 'x'), { ...options, parseHTML: (html) => parseHTML(html) }), /sourceCodeLocationInfo/);
});

test('missing real parser bootstrap fails the process without silently skipping', (t) => {
  const { root } = fixture(t);
  const env = { ...process.env, NEMO_PARSE5_MODULE: path.join(root, 'missing/dist/index.js') };
  delete env.NODE_TEST_CONTEXT; // Child is a standalone CLI, not another runner worker.
  const run = spawnSync(process.execPath, ['--test', '--test-name-pattern=^real parser version', __filename], {
    env, encoding: 'utf8',
  });
  assert.equal(run.error, undefined);
  assert.notEqual(run.status, 0);
  assert.match(run.stdout + run.stderr, /Real parse5 8\.0\.1 is required/);
});

test('comments, attributes, RCDATA, RAWTEXT and noscript do not invent script nodes', (t) => {
  const { scan } = fixture(t);
  const decoy = tag('src="js/decoy.js"');
  const html = `<!doctype html><!-- ${decoy} --><title>${decoy}</title><style>${decoy}</style>
    <textarea>${decoy}</textarea><noscript>${decoy}</noscript><xmp>${decoy}</xmp>
    <iframe>${decoy}</iframe><div data-note='${decoy}'></div>${tag('src="js/a.js"')}`;
  assert.deepEqual(sources(scan(html)), ['js/a.js']);
});

test('raw script contents and quoted > attributes retain exact UTF-16 anchors', (t) => {
  const { scan } = fixture(t);
  const body = 'const fake = "<script src=ghost.js>";\r\nwindow.ready = true;';
  const html = '<!doctype html>\r\n<!-- 😀 -->\r\n  <SCRIPT data-note="a > b" SRC="js/a.js"></SCRIPT>\r\n' + tag('', body);
  const report = scan(html), [external, inline] = report.scripts;
  assert.deepEqual(sources(report), ['js/a.js', '[inline]']);
  assert.equal(external.startTag.startLine, 3);
  assert.equal(external.startTag.startCol, 3);
  assert.equal(html.slice(external.src.anchor.startOffset, external.src.anchor.endOffset), 'SRC="js/a.js"');
  assert.equal(html.slice(inline.content.startOffset, inline.content.endOffset), body);
  assert.equal(html.slice(external.anchor.startOffset, external.anchor.endOffset), '<SCRIPT data-note="a > b" SRC="js/a.js"></SCRIPT>');
});

test('entities are decoded by HTML parsing and query/fragment do not become filename bytes', (t) => {
  const { scan } = fixture(t), report = scan(tag('src="js/a&amp;b.js?x=1&amp;y=2#part"'));
  const src = report.scripts[0].src;
  assert.equal(src.value, 'js/a&b.js?x=1&y=2#part');
  assert.equal(src.path, 'src/js/a&b.js');
  assert.equal(src.query, '?x=1&y=2');
  assert.equal(src.fragment, '#part');
});

test('unquoted and duplicate attributes follow the real parser with diagnostics', (t) => {
  const { scan } = fixture(t), report = scan('<script src=js/a.js SRC="js/b.js"></script>');
  assert.deepEqual(sources(report), ['js/a.js']);
  assert.ok(report.diagnostics.some((d) => d.parserCode === 'duplicate-attribute' && d.anchor.startOffset >= 0));
});

test('nested template scripts remain inert records and do not resolve or order', (t) => {
  const { scan } = fixture(t);
  const report = scan(`<template>${tag('src="js/missing.js"')}<template>${tag('', 'x')}</template></template>${tag('src="js/a.js"')}`);
  assert.deepEqual(report.scripts.map((s) => s.execution), ['inert', 'inert', 'eager-classic']);
  assert.equal(report.scripts[0].src.status, 'not-fetched');
  assert.equal(report.eagerOrder.length, 1);
  assert.deepEqual(report.edges, []);
});

test('declarative shadow templates explicitly report unsupported activation', (t) => {
  const { scan } = fixture(t), report = scan(`<div><template shadowrootmode="open">${tag('', 'x')}</template></div>`);
  assert.equal(report.scripts[0].execution, 'unknown');
  assert.ok(hasCode(report, 'unsupported-shadow-template'));
  assert.equal(report.eagerOrder.length, 0);
});

test('inert and hidden HTML attributes do not disable scripts', (t) => {
  const { scan } = fixture(t), report = scan(`<div inert hidden>${tag('', 'x')}</div>`);
  assert.equal(report.scripts[0].execution, 'eager-classic');
});

test('foreign scripts are retained, while foreignObject reenters the HTML namespace', (t) => {
  const { scan } = fixture(t);
  const report = scan(`<svg><script href="js/a.js" xlink:href="js/b.js"></script><foreignObject>${tag('src="js/c.js"')}</foreignObject></svg><math><script>m</script></math>`);
  assert.deepEqual(report.scripts.map((s) => s.kind), ['foreign', 'classic', 'foreign']);
  assert.deepEqual(report.scripts[0].foreignReferences.map((a) => a.name), ['href', 'xlink:href']);
  assert.deepEqual(ordered(report).map((s) => s.src.value), ['js/c.js']);
  assert.ok(hasCode(report, 'unsupported-foreign-script'));
});

test('script type and language use exact HTML classification, including whitespace traps', (t) => {
  const { scan } = fixture(t);
  const controls = [
    ['', 'classic'], ['type=""', 'classic'], ['type="   "', 'data'],
    ['type=" Text/JavaScript "', 'classic'], ['type="text/javascript; charset=utf-8"', 'data'],
    ['type=" MODULE "', 'module'], ['type="APPLICATION/JSON"', 'data'],
    ['type="importmap"', 'importmap'], ['type="speculationrules"', 'speculationrules'],
    ['language="JavaScript1.5"', 'classic'], ['language=" javascript "', 'data'],
    ['language=""', 'classic'], ['language="vbscript" type=""', 'classic'],
    ['type="\u00a0text/javascript\u00a0"', 'data'],
  ];
  const report = scan(controls.map(([attrs]) => tag(attrs, '{}')).join(''));
  assert.deepEqual(report.scripts.map((s) => s.kind), controls.map(([, kind]) => kind));
  assert.ok(hasCode(report, 'unsupported-browser-data'));
});

test('data and browser-data src attributes never create fetched source dependencies', (t) => {
  const { scan } = fixture(t);
  const report = scan(['application/json', 'importmap', 'speculationrules'].map((type) => tag(`type="${type}" src="js/missing.js"`, '{}')).join(''));
  assert.ok(report.scripts.every((s) => s.src.status === 'not-fetched'));
  assert.deepEqual(report.edges, []);
});

test('async, defer, nomodule and modules stay outside the eager classic subsequence', (t) => {
  const { scan } = fixture(t);
  const attrs = ['src="js/a.js"', 'src="js/b.js" async="false" defer', 'src="js/c.js" defer',
    'type="module" src="js/a.js"', 'type="module" async', 'nomodule src="js/a.js"',
    'async defer', 'src="js/b.js"'];
  const report = scan(attrs.map((a) => tag(a, 'x')).join(''));
  assert.deepEqual(report.scripts.map((s) => s.execution), ['eager-classic', 'async-classic', 'defer-classic', 'module', 'async-module', 'conditional-classic', 'eager-classic', 'eager-classic']);
  assert.equal(report.scripts[1].async, true);
  assert.deepEqual(ordered(report).map((s) => s.index), [0, 6, 7]);
  assert.deepEqual(report.edges, [
    { from: report.scripts[0].id, to: report.scripts[6].id, relation: 'classic-evaluation-order' },
    { from: report.scripts[6].id, to: report.scripts[7].id, relation: 'classic-evaluation-order' },
  ]);
  assert.ok(hasCode(report, 'readiness-not-inferred'));
  assert.ok(hasCode(report, 'conditional-nomodule'));
});

test('duplicate loads keep separate occurrence IDs and adjacent ordering edges', (t) => {
  const { scan } = fixture(t), report = scan(tag('src="js/a.js"') + tag('src="js/a.js?again#x"'));
  assert.equal(report.scripts[0].src.path, report.scripts[1].src.path);
  assert.notEqual(report.scripts[0].id, report.scripts[1].id);
  assert.equal(report.edges.length, 1);
});

test('legacy event scripts, empty inline and unclosed elements cannot imply eager execution', (t) => {
  const { scan } = fixture(t);
  const report = scan(tag('for="window" event="onload"', 'x') + tag() + '<script src="js/a.js">');
  assert.deepEqual(report.scripts.map((s) => s.execution), ['unknown', 'empty', 'unknown']);
  assert.deepEqual(report.eagerOrder, []);
  assert.ok(hasCode(report, 'unsupported-legacy-event-script'));
  assert.ok(hasCode(report, 'unsupported-unclosed-script'));
  assert.ok(report.diagnostics.some((d) => d.parserCode === 'eof-in-element-that-can-contain-only-text'));
});

test('empty src is external and invalid; whitespace inline code is still classic', (t) => {
  const { scan } = fixture(t), report = scan(tag('src=""', 'fallback()') + tag('', ' '));
  assert.equal(report.scripts[0].mode, 'external');
  assert.equal(report.scripts[0].execution, 'invalid-source');
  assert.equal(report.scripts[1].execution, 'eager-classic');
  assert.ok(hasCode(report, 'invalid-script-src'));
});

test('source order is anchored independently of repaired DOM traversal order', (t) => {
  const { scan } = fixture(t);
  const report = scan(`<table>${tag('src="js/a.js"')}<div>${tag('src="js/b.js"')}</div></table>${tag('src="js/c.js"')}`);
  assert.deepEqual(sources(report), ['js/a.js', 'js/b.js', 'js/c.js']);
  assert.deepEqual(ordered(report).map((s) => s.index), [0, 1, 2]);
});

test('explicit sourceRoot resolves root URLs; absent mapping reports unknown', (t) => {
  const { scan } = fixture(t), html = tag('src="/js/a.js"');
  assert.equal(scan(html).scripts[0].src.path, 'src/js/a.js');
  const report = scan(html, { sourceRoot: undefined });
  assert.equal(report.scripts[0].src.status, 'unknown');
  assert.equal(report.scripts[0].src.reason, 'root-relative-src-needs-sourceRoot');
  assert.equal(scan(tag('src="js/a.js"'), { sourceRoot: undefined }).scripts[0].src.path, 'src/js/a.js');
});

test('base href affects only following records and never guesses a local mapping', (t) => {
  const { scan } = fixture(t);
  const report = scan(tag('src="js/a.js"') + '<base href="https://example.test/">' + tag('src="js/b.js"') + tag('src="/js/c.js"'));
  assert.deepEqual(report.scripts.map((s) => s.src.status), ['local', 'unknown', 'unknown']);
  assert.ok(hasCode(report, 'unsupported-base-href'));
  const inertBase = scan(`<template><base href="https://example.test/"></template>${tag('src="js/a.js"')}`);
  assert.equal(inertBase.scripts[0].src.status, 'local');
});

test('HTTP and network-path references are nonlocal; custom, file and data protocols are unknown', (t) => {
  const { scan } = fixture(t);
  const values = ['https://example.test/a.js', '//example.test/a.js', 'http://example.test/a.js',
    'data:text/javascript,x', 'javascript:x', 'file:///js/a.js', 'asset://localhost/js/a.js', 'tauri:js/a.js'];
  const report = scan(values.map((v) => tag(`src="${v}"`)).join(''));
  assert.deepEqual(report.scripts.map((s) => s.src.status), ['nonlocal', 'nonlocal', 'nonlocal', 'unknown', 'unknown', 'unknown', 'unknown', 'unknown']);
  assert.ok(report.scripts.every((s) => !s.src.path));
  assert.equal(scan(tag('src="http://["')).scripts[0].src.status, 'invalid');
});

test('URL decoding preserves safe encoded names, dot segments, query and fragment boundaries', (t) => {
  const { scan } = fixture(t);
  const pairs = [['js/space%20name.js', 'space name.js'], ['js/a%23b.js?x#y', 'a#b.js'],
    ['js/a%3Fb.js', 'a?b.js'], ['js/%C3%A9.js', 'é.js'], ['js/%2E/a.js', 'a.js'],
    ['js/sub/%2e%2e/a.js', 'a.js'], ['js/UPPER.JS', 'UPPER.JS']];
  const report = scan(pairs.map(([v]) => tag(`src="${v}"`)).join(''));
  assert.deepEqual(report.scripts.map((s) => s.src.path), pairs.map(([, v]) => `src/js/${v}`));
});

test('encoded separators, NUL, malformed percent escapes, controls and backslashes report unknown', (t) => {
  const { scan } = fixture(t);
  const values = ['js%2fa.js', 'js%5ca.js', 'js/%00a.js', 'js/%FF.js', 'js/a%.js', 'js\\a.js', 'js/\ta.js'];
  const report = scan(values.map((v) => tag(`src="${v}"`)).join(''));
  assert.ok(report.scripts.every((s) => s.src.status === 'unknown' && !s.src.path));
});

test('missing files and directories produce actionable source diagnostics', (t) => {
  const { scan } = fixture(t), report = scan(tag('src="js/nope.js"') + tag('src="js/"'));
  assert.deepEqual(report.scripts.map((s) => s.src.reason), ['missing-file', 'not-a-file']);
  assert.equal(report.diagnostics.filter((d) => d.code === 'unresolved-script-src').length, 2);
});

test('escaping roots and symlinks cannot produce falsely safe local paths', (t) => {
  const { scan, root } = fixture(t);
  fs.writeFileSync(path.join(root, 'outside.js'), 'outside');
  fs.symlinkSync(path.join(root, 'outside.js'), path.join(root, 'src/js/link.js'));
  const report = scan(tag('src="../outside.js"') + tag('src="js/link.js"'));
  assert.deepEqual(report.scripts.map((s) => s.src.reason), ['path-outside-source-root', 'symlink-outside-source-root']);
  fs.symlinkSync(path.join(root, 'src/js/a.js'), path.join(root, 'src/js/alias.js'));
  const src = scan(tag('src="js/alias.js"')).scripts[0].src;
  assert.equal(src.path, 'src/js/a.js');
  assert.equal(src.logicalPath, 'src/js/alias.js');
});

test('symlinked HTML origin and invalid configuration fail or report unknown', (t) => {
  const { scan, options, root } = fixture(t);
  fs.writeFileSync(path.join(root, 'actual.html'), '');
  fs.symlinkSync(path.join(root, 'actual.html'), path.join(root, 'src/index.html'));
  assert.equal(scan(tag('src="js/a.js"')).scripts[0].src.reason, 'symlinked-html-url-base');
  assert.throws(() => scan('', { root: '.' }), /root must be absolute/);
  assert.throws(() => scan('', { htmlFile: '../index.html' }), /repository-relative/);
  assert.throws(() => scan('', { sourceRoot: 'other' }), /within sourceRoot/);
  assert.throws(() => inventoryHtmlScripts(null, options), /html must be a string/);
});

test('real Nemo startup records establish classic ordering without callback or tool claims', () => {
  const root = path.resolve(__dirname, '../..'), html = fs.readFileSync(path.join(root, 'src/index.html'), 'utf8');
  const report = inventoryHtmlScripts(html, { parseHTML, root, htmlFile: 'src/index.html', sourceRoot: 'src' });
  const byPath = (p) => report.scripts.find((s) => s.src?.path === p);
  const app = byPath('src/js/app.js'), timeline = byPath('src/js/timeline.js');
  const vendor = byPath('src/js/delaunator.vendor.js'), mesh = byPath('src/js/image-mesh.js');
  assert.ok(report.scripts.length > 100);
  assert.ok(app.index < timeline.index);
  assert.ok(report.eagerOrder.indexOf(app.id) < report.eagerOrder.indexOf(timeline.id));
  assert.match(fs.readFileSync(path.join(root, 'src/js/app.js'), 'utf8'), /var state\s*=/);
  assert.ok(report.edges.some((e) => e.from === vendor.id && e.to === mesh.id));
  assert.equal(vendor.kind, 'classic');
  const modules = report.scripts.filter((s) => s.kind === 'module');
  assert.deepEqual(modules.map((s) => s.src.path), ['src/js/geometry-wasm-loader.js', 'src/js/vectorize-wasm-loader.js']);
  assert.ok(modules.every((s) => !report.eagerOrder.includes(s.id)));
  const beforeModules = byPath('src/js/assets-panel.js'), afterModules = byPath('src/js/engine-bridge.js');
  assert.ok(report.edges.some((e) => e.from === beforeModules.id && e.to === afterModules.id));
  assert.ok(byPath('src/js/tools.js'));
  assert.ok(report.edges.every((e) => e.relation === 'classic-evaluation-order'
    && report.eagerOrder.includes(e.from) && report.eagerOrder.includes(e.to)));
  assert.match(report.limits.ordering, /callback, tool-handler, module, async or defer readiness/);
  assert.equal(report.diagnostics.filter((d) => d.code.endsWith('-script-src')).length, 0);
});

test('callback names and dynamic loader strings cannot create HTML order records', (t) => {
  const { scan } = fixture(t);
  const report = scan(tag('', 'setTimeout(() => useTools(), 0); document.createElement("script");') + tag('src="js/a.js"'));
  assert.equal(report.scripts.length, 2);
  assert.equal(report.edges.length, 1);
  assert.deepEqual(Object.keys(report.edges[0]), ['from', 'to', 'relation']);
  assert.match(report.limits.runtime, /document.write/);
});
