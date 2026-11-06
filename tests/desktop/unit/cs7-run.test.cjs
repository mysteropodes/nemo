'use strict';
// Disposable declarations, scenarios and a stub session only: no app, helper or installed run.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { main, cli, refusalReason } = require('../cs7-run.cjs');
const { DECLARATION_SCHEMA, selectRows, readyRows } = require('../cs7-runner.cjs');
const identity = { sourceSha: 'a'.repeat(40), tree: 'b'.repeat(40), driverSha: 'c'.repeat(40),
  executableSha256: 'd'.repeat(64), sessionId: 'one-lifetime', pid: 123, dirty: false };
const evidence = 'Source complete: https://github.com/mysteropodes/nemo/pull/100 at ' + 'a'.repeat(40);
const row = (id, extra = {}) => ({ id, issue: 2000, status: 'Ready', tasks: ['F03'], surfaces: ['opacity'], dependsOn: [],
  scenario: id, evidence, checks: [{ id: 'workflow', clause: 'Original installed workflow', covers: ['positive'] }], ...extra });

// Rows: a, b (needs a), c are Ready; d is Blocked. Scenario c throws in prepare() to prove it is skipped.
function fixture(t) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cs7-run-')));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const parity = path.join(root, 'engineering/inventory/parity');
  fs.mkdirSync(path.join(parity, 'installed'), { recursive: true });
  fs.writeFileSync(path.join(parity, 'matrix.json'), JSON.stringify({ tasks: [{ task: 'F03' }], surfaces: [{ id: 'opacity' }] }));
  fs.writeFileSync(path.join(parity, 'installed/one.json'), JSON.stringify({ schema: DECLARATION_SCHEMA, rows: [
    row('a'), row('b', { dependsOn: ['a'] }), row('c'),
    row('d', { status: 'Blocked', reason: 'Source unavailable', evidence: undefined })] }));
  const scenarioDirectory = path.join(root, 'scenarios'), report = path.join(root, 'report'), events = [];
  fs.mkdirSync(scenarioDirectory); fs.mkdirSync(report);
  globalThis.__cs7RunEvents = events; t.after(() => { delete globalThis.__cs7RunEvents; });
  for (const id of ['a', 'b', 'c']) fs.writeFileSync(path.join(scenarioDirectory, id + '.cjs'), `'use strict';
module.exports = { async prepare() { globalThis.__cs7RunEvents.push('prepare-${id}');
  if ('${id}' === 'c' && process.env.CS7_TEST_C_THROWS) throw new Error('private packet missing');
  return { run: async () => { globalThis.__cs7RunEvents.push('run-${id}');
    return { checks: [{ id: 'workflow', status: 'pass' }], safeToContinue: true }; } }; } };\n`);
  const session = { start: async () => { events.push('start'); return identity; }, identity: async () => ({ ...identity }),
    cleanup: async () => { events.push('cleanup'); } };
  const out = [];
  const deps = { root, scenarioDirectory, stdout: { write: s => out.push(s) },
    createInstalledSession: () => { events.push('session-created'); return session; } };
  return { root, report, events, deps, out, config: extra => ({ reportDirectory: report, ...extra }) };
}

test('without rows every Ready row is prepared and run and the receipt carries no selection', async t => {
  const f = fixture(t);
  const result = await main(f.config(), f.deps);
  assert.deepEqual(f.events, ['prepare-a', 'prepare-b', 'prepare-c', 'session-created', 'start', 'run-a', 'run-b', 'run-c', 'cleanup']);
  assert.equal(result.verdict, 'installed-pass');
  assert.ok(!('selection' in result));
  const receipt = JSON.parse(fs.readFileSync(path.join(f.report, 'installed-results.json'), 'utf8'));
  assert.ok(!('selection' in receipt));
  assert.deepEqual(Object.keys(JSON.parse(f.out[0])), ['verdict', 'sourceSha', 'rows', 'operatorInputFailures', 'cleanup']);
  for (const name of fs.readdirSync(f.report)) if (name.startsWith('progress-')) assert.ok(!('selection' in JSON.parse(fs.readFileSync(path.join(f.report, name), 'utf8'))));
});

test('rows runs only the selected subset, never prepares the rest, and records the selection beside the verdict', async t => {
  const f = fixture(t); process.env.CS7_TEST_C_THROWS = '1'; t.after(() => { delete process.env.CS7_TEST_C_THROWS; });
  const result = await main(f.config({ rows: ['b', 'a'] }), f.deps);
  assert.deepEqual(f.events, ['prepare-a', 'prepare-b', 'session-created', 'start', 'run-a', 'run-b', 'cleanup']);
  assert.equal(result.verdict, 'installed-pass'); // every SELECTED row passed
  const selection = { scope: 'subset', selected: ['a', 'b'], omittedReady: ['c'] };
  assert.deepEqual(result.selection, selection);
  const receipt = JSON.parse(fs.readFileSync(path.join(f.report, 'installed-results.json'), 'utf8'));
  assert.deepEqual(receipt.selection, selection);
  assert.deepEqual(receipt.installed.rows.map(r => r.id), ['a', 'b']);
  const keys = Object.keys(receipt); assert.equal(keys[keys.indexOf('verdict') + 1], 'selection');
  const line = JSON.parse(f.out[0]); assert.deepEqual(line.selection, selection);
  assert.equal(Object.keys(line)[1], 'selection');
  const progress = fs.readdirSync(f.report).filter(n => n.startsWith('progress-'));
  assert.ok(progress.length > 0);
  for (const name of progress) assert.deepEqual(JSON.parse(fs.readFileSync(path.join(f.report, name), 'utf8')).selection, selection);
});

test('selecting every Ready row is labelled all-ready, with nothing omitted', async t => {
  const f = fixture(t);
  const result = await main(f.config({ rows: ['a', 'b', 'c'] }), f.deps);
  assert.deepEqual(result.selection, { scope: 'all-ready', selected: ['a', 'b', 'c'], omittedReady: [] });
});

test('an invalid selection refuses before any scenario is prepared, any session is created or any receipt is written', async t => {
  const cases = [
    [['nope'], /unknown: nope/], [['d'], /not Ready: d/], [['b'], /row b depends on a row outside the selection: a/],
    [[], /non-empty list/], ['a', /non-empty list/], [null, /non-empty list/], [{ a: 1 }, /non-empty list/],
    [['a', 'a'], /unique/], [['A'], /unique row ids/], [[1], /unique row ids/], [['a', 'c', 'x'], /unknown: x/],
  ];
  for (const [rows, pattern] of cases) {
    const f = fixture(t);
    await assert.rejects(() => main(f.config({ rows }), f.deps), pattern);
    assert.deepEqual(f.events, [], JSON.stringify(rows));
    assert.deepEqual(fs.readdirSync(f.report), []);
    assert.deepEqual(f.out, []);
  }
});

test('selection is also checked by readyRows itself when a dependency is outside it', () => {
  const declarations = [{ schema: DECLARATION_SCHEMA, rows: [row('a'), row('b', { dependsOn: ['a'] })] }];
  const known = { taskIds: new Set(['F03']), surfaceIds: new Set(['opacity']),
    scenarios: { a: { run() {} }, b: { run() {} } } };
  assert.deepEqual(readyRows(declarations, { ...known, only: new Set(['a']) }).map(r => r.id), ['a']);
  assert.throws(() => readyRows(declarations, { ...known, only: new Set(['b']) }), /outside the selection/);
  assert.throws(() => readyRows(declarations, { ...known, only: new Set(['zzz']) }), /missing or not Ready/);
  assert.deepEqual(selectRows(declarations, ['a', 'b']).omittedReady, []);
});

test('the shipped declarations accept a single-row selection that skips the other Ready rows', () => {
  const root = path.resolve(__dirname, '../../..');
  const { readDeclarations } = require('../cs7-runner.cjs');
  const declarations = readDeclarations(path.join(root, 'engineering/inventory/parity/installed'));
  const ready = declarations.flatMap(d => d.rows).filter(r => r.status === 'Ready');
  assert.ok(ready.length > 1);
  const picked = ready.find(r => r.dependsOn.length === 0);
  const selection = selectRows(declarations, [picked.id]);
  assert.deepEqual(selection.omittedReady, ready.filter(r => r.id !== picked.id).map(r => r.id));
  assert.throws(() => selectRows(declarations, [declarations.flatMap(d => d.rows).find(r => r.status === 'Blocked').id]), /not Ready/);
});

test('a refusal prints its check message and the no-acceptance sentence without evidence content', async t => {
  const f = fixture(t), err = [];
  const config = path.join(f.root, 'config.json');
  fs.writeFileSync(config, JSON.stringify(f.config({ rows: ['b'] })));
  const code = await cli(['node', 'cs7-run.cjs', config], { ...f.deps, stderr: { write: s => err.push(s) } });
  assert.equal(code, 1);
  assert.equal(err.length, 1);
  assert.match(err[0], /^CS7 batch refused or incomplete \(selected installed row b depends on a row outside the selection: a\); /);
  assert.match(err[0], /No acceptance claimed\.\n$/);
  assert.deepEqual(f.events, []);
  assert.deepEqual(f.out, []);
});

test('refusal reasons from non-check errors are reduced to class and code', async t => {
  const f = fixture(t), secret = 'SECRET-EVIDENCE-TOKEN';
  assert.equal(refusalReason(new Error('clean frozen source required')), 'clean frozen source required');
  const fsError = Object.assign(new Error("ENOENT: no such file or directory, open '/Users/example/" + secret + "'"), { code: 'ENOENT' });
  assert.equal(refusalReason(fsError), 'Error ENOENT');
  assert.equal(refusalReason(new SyntaxError('Unexpected token ' + secret)), 'SyntaxError');
  assert.equal(refusalReason(new Error('path /private/' + secret)), 'Error');
  assert.equal(refusalReason(new Error(secret.repeat(40))), 'Error');
  assert.equal(refusalReason(undefined), 'Error');
  const config = path.join(f.root, 'config.json'), err = [];
  fs.writeFileSync(config, '{"token": "' + secret + '", ');
  await cli(['node', 'cs7-run.cjs', config], { ...f.deps, stderr: { write: s => err.push(s) } });
  assert.match(err[0], /refused or incomplete \(SyntaxError\); /);
  assert.ok(!err.join('').includes(secret));
  err.length = 0; await cli(['node', 'cs7-run.cjs'], { ...f.deps, stderr: { write: s => err.push(s) } });
  assert.match(err[0], /\(one private config file required\)/);
  assert.deepEqual(f.events, []);
});
