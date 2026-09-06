'use strict';

// Opt-in R03 document-contract acceptance. This is deliberately not named
// *.test.cjs: Playwright is supplied externally and is not a repo dependency.
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { chromium } = require(process.env.NEMO_PLAYWRIGHT_MODULE || 'playwright');
const { startBrowserRuntime, IDENTITY_PATH } = require('../../scripts/nemo/lib/browser-runtime.cjs');

const root = path.resolve(__dirname, '../..');
const output = path.resolve(process.argv[2] || path.join(os.tmpdir(), `nemo-r03-browser-acceptance-${process.pid}`));
const negativeControls = process.argv[3] === '--negative-controls';
assert.ok(process.argv[3] === undefined || negativeControls, 'unknown option');
const contexts = new Set();
const manifestPath = path.join(__dirname, 'manifest.json');
const fixtures = {
  migration: {
    project: path.join(__dirname, 'migration/project.json'),
    expected: path.join(__dirname, 'migration/expected.json'),
  },
  text: {
    project: path.join(__dirname, 'text/project.json'),
    expected: path.join(__dirname, 'text/expected.json'),
  },
};
const sourceFiles = [
  'src/index.html',
  'src/paper-full.min.js',
  'src/js/app.js',
  'src/js/timeline.js',
  'src/js/motion.js',
  'src/js/project.js',
  'src/js/vector-text-bridge.js',
  'src/js/text-selector.js',
];
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const readJson = file => JSON.parse(fs.readFileSync(file, 'utf8'));
function chromeExecutable() {
  if (process.env.NEMO_CHROME_EXECUTABLE) return path.resolve(process.env.NEMO_CHROME_EXECUTABLE);
  const candidates = process.platform === 'darwin'
    ? ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome']
    : process.platform === 'win32'
      ? [path.join(process.env.PROGRAMFILES || '', 'Google', 'Chrome', 'Application', 'chrome.exe')]
      : ['/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser'];
  const executable = candidates.find(candidate => candidate && fs.existsSync(candidate));
  assert.ok(executable, 'Chrome/Chromium executable not found; set NEMO_CHROME_EXECUTABLE');
  return executable;
}
const report = {
  schema: 'nemo.r03.browser-acceptance.v1',
  status: 'running',
  source: {
    head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    branch: execFileSync('git', ['branch', '--show-current'], { cwd: root, encoding: 'utf8' }).trim(),
    files: {},
  },
  backend: { kind: 'document', rendererRequired: false },
  fixtures: {},
  expectationIds: [],
  executions: [],
  negativeControls: [],
  artifacts: [],
  pageErrors: [],
  cleanup: { contextsClosed: 0, browserClosed: false, runtimeClosed: false, runtimeRootsRemoved: false },
  limits: [
    'R03 document import/export contract only; no render pixels, GPU benchmark, native GUI, build, or package acceptance.',
    'Uses committed independent expectations and generated fixtures without changing or regenerating them.',
  ],
};

function artifact(name, text) {
  const bytes = Buffer.from(text);
  fs.writeFileSync(path.join(output, name), bytes);
  const item = { name, bytes: bytes.length, sha256: sha256(bytes) };
  report.artifacts.push(item);
  return item;
}

function expectationId(fixture, check) {
  if (check.op) return `${fixture}.migration.${check.op}`;
  if (check.path) return `${fixture}.document.${check.path}`;
  if (check.kind === 'bounds') return `${fixture}.bounds.${check.layerUid}.${check.strokeId}`;
  if (check.kind === 'text-units') return `${fixture}.text-units.${check.layerUid}`;
  if (check.kind === 'text-group') return `${fixture}.text-group.${check.rootStrokeId}`;
  throw new Error(`no stable expectation id for ${fixture}/${check.kind}`);
}

function atPath(value, expression) {
  const parts = expression.replace(/\[(\d+)\]/g, '.$1').split('.');
  return parts.reduce((current, part) => current == null ? undefined : current[part], value);
}

function layer(project, uid) {
  const found = project.layers.find(item => item.layerUid === uid);
  assert.ok(found, `layer ${uid} exists`);
  return found;
}

function stroke(project, uid, strokeId) {
  const found = layer(project, uid).frames.flatMap(frame => frame.strokes).find(item => item.strokeId === strokeId);
  assert.ok(found, `stroke ${strokeId} exists in ${uid}`);
  return found;
}

function normalizedHex(value) {
  const match = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(value || '');
  assert.ok(match, `expected a hex color, got ${value}`);
  let hex = match[1].toLowerCase();
  if (hex.length === 3) hex = hex.split('').map(char => char + char).join('');
  if (hex.length === 8 && hex.endsWith('ff')) hex = hex.slice(0, 6);
  return `#${hex}`;
}

function verifyMigration(project, checks, stage) {
  for (const check of checks) {
    const id = expectationId('migration', check);
    let actual;
    switch (check.op) {
      case 'matteSourceUid':
        actual = layer(project, check.layerUid).matteSourceLayerUid;
        assert.equal(actual, check.expect, id);
        break;
      case 'easingCurveDefault':
        actual = project.easingCurve.points;
        assert.deepEqual(actual, check.expect, id);
        break;
      case 'legacyStrokeFallback': {
        actual = { strokeColor: stroke(project, check.layerUid, check.strokeId).strokeColor };
        assert.equal(normalizedHex(actual.strokeColor), normalizedHex(check.expect.strokeColor), id);
        break;
      }
      case 'tweenSpanKey':
        actual = Object.keys(project.tweenOverrides || {});
        assert.ok(Object.hasOwn(project.tweenOverrides || {}, check.expect), id);
        assert.ok(!Object.hasOwn(project.tweenOverrides || {}, '0:0-6'), `${id}: legacy index key removed`);
        break;
      case 'framePadding': {
        const frames = layer(project, check.layerUid).frames;
        actual = { frames: frames.length, isInterpolated: frames.at(-1).isInterpolated };
        assert.deepEqual(actual, check.expect, id);
        break;
      }
      default:
        throw new Error(`unsupported migration expectation ${check.op}`);
    }
    report.executions.push({ id, stage, result: 'pass', actual });
  }
}

async function textBounds(page, checks) {
  // Read the actual loaded curves, including handles, rather than the anchors'
  // bounding box. Expected bounds still come only from text/expected.json.
  const boundsChecks = checks.filter(check => check.kind === 'bounds');
  return page.evaluate(checks => Object.fromEntries(checks.map(check => {
    const index = state.layers.findIndex(layer => layer.layerUid === check.layerUid);
    const owner = userLayers[index];
    const item = owner && owner.children.find(item => item.data.strokeId === check.strokeId);
    if (!item || item.parent !== owner) throw new Error(`missing inserted glyph ${check.strokeId}`);
    const { x, y, width, height } = item.bounds;
    return [check.strokeId, { x, y, width, height }];
  })), boundsChecks);
}

function verifyText(project, checks, stage, bounds) {
  for (const check of checks) {
    const id = expectationId('text', check);
    let actual;
    switch (check.kind) {
      case 'document':
        actual = atPath(project, check.path);
        assert.deepEqual(actual, check.expect, id);
        break;
      case 'bounds':
        stroke(project, check.layerUid, check.strokeId);
        actual = bounds[check.strokeId];
        assert.deepEqual(actual, check.expect, id);
        break;
      case 'text-units':
        actual = layer(project, check.layerUid).frames[0].strokes
          .filter(item => item.isVectorText).map(item => item.charIndex);
        assert.deepEqual(actual, check.expectCharIndices, id);
        break;
      case 'text-group': {
        const rootStroke = stroke(project, check.layerUid, check.rootStrokeId);
        assert.equal(rootStroke.isTextRoot, true, `${id}: root marker`);
        actual = layer(project, check.layerUid).frames[0].strokes
          .filter(item => item.groupId === rootStroke.groupId).map(item => item.strokeId);
        assert.deepEqual(actual, check.expectMembers, id);
        break;
      }
      default:
        throw new Error(`unsupported text round-trip expectation ${check.kind}`);
    }
    report.executions.push({ id, stage, result: 'pass', actual });
  }
}

async function until(page, predicate, message, timeout = 30000, argument) {
  const deadline = Date.now() + timeout;
  while (!await page.evaluate(predicate, argument)) {
    assert.ok(Date.now() < deadline, message);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
}

async function openProject(browser, origin, filename, firstLayerName, stage) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  contexts.add(context);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  page.on('pageerror', error => report.pageErrors.push({ stage, message: error.message }));
  await page.goto(origin, { waitUntil: 'networkidle' });
  await until(page, () => !!(window.SM && window.state && document.querySelector('#start-open')),
    `${stage}: application readiness timed out`);
  const chooser = page.waitForEvent('filechooser');
  await page.locator('#start-open').click({ force: true });
  await (await chooser).setFiles(filename);
  await until(page, name => state.layers.length > 0 && state.layers[0].name === name,
    `${stage}: importJSON did not load ${firstLayerName}`, 30000, firstLayerName);
  assert.equal(await page.evaluate(() => state.layers[0].name), firstLayerName);
  const json = await page.evaluate(() => window.SM.exportJSON());
  assert.equal(typeof json, 'string');
  return { context, page, json };
}

async function closeContext(context) {
  if (!context) return;
  await context.close();
  contexts.delete(context);
  report.cleanup.contextsClosed += 1;
}

async function verifyNegativeControls(browser, origin, checks) {
  const check = checks.find(check => check.kind === 'bounds' && check.strokeId === 's_text_2');
  assert.ok(check, 'negative controls require the committed glyph bounds expectation');
  const id = expectationId('text', check);
  for (const mode of ['curve', 'translate']) {
    let opened = await openProject(browser, origin, fixtures.text.project, 'Vector text', `control.${mode}.import`);
    const before = (await textBounds(opened.page, checks))[check.strokeId];
    assert.deepEqual(before, check.expect, `${mode}: unmodified glyph bounds`);
    await opened.page.evaluate(({ mode, check }) => {
      const index = state.layers.findIndex(layer => layer.layerUid === check.layerUid);
      const owner = userLayers[index];
      const target = owner.children.find(item => item.data.strokeId === check.strokeId);
      if (!target || target.parent !== owner) throw new Error('control requires an inserted glyph');
      if (mode === 'curve') target.segments[0].handleOut = new paper.Point(0, -100);
      else target.translate(new paper.Point(1, 0));
    }, { mode, check });
    const json = await opened.page.evaluate(() => window.SM.exportJSON());
    const saved = stroke(JSON.parse(json), check.layerUid, check.strokeId);
    if (mode === 'curve') assert.deepEqual(saved.segments[0].handleOut, [0, -100], 'handle persisted by exportJSON');
    else assert.deepEqual(saved.segments[0].point, [181, 100], 'translation persisted by exportJSON');
    const file = artifact(`control-${mode}-roundtrip.json`, json);
    const after = (await textBounds(opened.page, checks))[check.strokeId];
    await closeContext(opened.context);

    opened = await openProject(browser, origin, path.join(output, file.name), 'Vector text', `control.${mode}.reopen`);
    const reopened = stroke(JSON.parse(opened.json), check.layerUid, check.strokeId);
    assert.deepEqual(reopened.segments, saved.segments, `${mode}: all segments survived fresh reopen`);
    assert.equal(reopened.closed, saved.closed, `${mode}: topology survived fresh reopen`);
    artifact(`control-${mode}-reopened.json`, opened.json);
    const bounds = await textBounds(opened.page, checks);
    assert.deepEqual(bounds[check.strokeId], after, `${mode}: loaded curve bounds survived reopen`);
    // Exercise the same geometry assertion as baseline acceptance. An unrelated
    // exception or a missing glyph must not count as a successful rejection.
    assert.throws(() => verifyText(JSON.parse(opened.json), [check], `control.${mode}.reopen`, bounds),
      error => error.code === 'ERR_ASSERTION' && error.message.startsWith(id)
        && JSON.stringify(error.actual) === JSON.stringify(after)
        && JSON.stringify(error.expected) === JSON.stringify(check.expect),
      `${mode}: persisted corruption must fail the geometry assertion`);
    report.negativeControls.push({ mode, id, result: 'rejected', before, after, reopened: bounds[check.strokeId], artifact: file.name });
    await closeContext(opened.context);
  }
}

async function main() {
  fs.mkdirSync(output, { recursive: true });
  const manifest = readJson(manifestPath);
  report.fixtures.manifestSha256 = sha256(fs.readFileSync(manifestPath));
  for (const [id, files] of Object.entries(fixtures)) {
    const entry = manifest.fixtures.find(item => item.id === id);
    assert.ok(entry, `manifest entry ${id}`);
    const projectHash = sha256(fs.readFileSync(files.project));
    const expectedHash = sha256(fs.readFileSync(files.expected));
    assert.equal(projectHash, entry.sha256[`${id}/project.json`], `${id} project manifest hash`);
    assert.equal(expectedHash, entry.sha256[`${id}/expected.json`], `${id} expected manifest hash`);
    report.fixtures[id] = {
      backend: entry.backend,
      formatVersion: entry.generation.formatVersion,
      seed: entry.seed,
      projectSha256: projectHash,
      expectedSha256: expectedHash,
    };
  }
  const migrationChecks = readJson(fixtures.migration.expected).checks.filter(check => check.verify === 'gate' && check.kind === 'migration');
  const textChecks = readJson(fixtures.text.expected).checks.filter(check => check.verify === 'node' && ['document', 'bounds', 'text-units', 'text-group'].includes(check.kind));
  assert.equal(migrationChecks.length, 5, 'exactly five committed migration importJSON expectations');
  assert.equal(textChecks.length, 11, 'all committed text document round-trip expectations');
  report.expectationIds = [...migrationChecks.map(check => expectationId('migration', check)), ...textChecks.map(check => expectationId('text', check))];

  const playwrightPackage = path.join(path.dirname(require.resolve(process.env.NEMO_PLAYWRIGHT_MODULE || 'playwright')), 'package.json');
  const playwright = readJson(playwrightPackage);
  report.backend.playwright = { version: playwright.version, packageSha256: sha256(fs.readFileSync(playwrightPackage)) };
  report.source.harnessSha256 = sha256(fs.readFileSync(__filename));

  const runtime = await startBrowserRuntime({ taskId: `r03-fixtures-${process.pid}-${Date.now()}`, port: 0 });
  let browser;
  try {
    const identityBefore = await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json());
    assert.equal(identityBefore.healthy, true, 'runtime identity healthy before acceptance');
    assert.equal(identityBefore.source.current.head, report.source.head, 'runtime serves requested source HEAD');
    for (const file of sourceFiles) {
      const local = fs.readFileSync(path.join(root, file));
      const response = await fetch(`${runtime.origin}/${file.slice(4)}`);
      assert.equal(response.status, 200, `${file} served`);
      const served = Buffer.from(await response.arrayBuffer());
      assert.equal(sha256(served), sha256(local), `${file} served bytes equal checkout`);
      report.source.files[file] = sha256(local);
    }
    report.backend.documentSourceSetSha256 = sha256(Buffer.from(JSON.stringify(report.source.files)));

    const executable = chromeExecutable();
    browser = await chromium.launch({ executablePath: executable, headless: true });
    report.backend.browser = {
      name: 'chromium',
      version: browser.version(),
      executableSha256: sha256(fs.readFileSync(executable)),
    };

    let opened = await openProject(browser, runtime.origin, fixtures.migration.project, 'Legacy curve', 'migration.initial-import');
    let migrated = JSON.parse(opened.json);
    verifyMigration(migrated, migrationChecks, 'migration.initial-import');
    const migratedArtifact = artifact('migration-roundtrip.json', opened.json);
    await closeContext(opened.context);

    opened = await openProject(browser, runtime.origin, path.join(output, migratedArtifact.name), 'Legacy curve', 'migration.roundtrip-reopen');
    migrated = JSON.parse(opened.json);
    verifyMigration(migrated, migrationChecks, 'migration.roundtrip-reopen');
    await closeContext(opened.context);

    opened = await openProject(browser, runtime.origin, fixtures.text.project, 'Vector text', 'text.initial-import');
    let textProject = JSON.parse(opened.json);
    verifyText(textProject, textChecks, 'text.initial-import', await textBounds(opened.page, textChecks));
    const textArtifact = artifact('text-roundtrip.json', opened.json);
    await closeContext(opened.context);

    opened = await openProject(browser, runtime.origin, path.join(output, textArtifact.name), 'Vector text', 'text.roundtrip-reopen');
    textProject = JSON.parse(opened.json);
    verifyText(textProject, textChecks, 'text.roundtrip-reopen', await textBounds(opened.page, textChecks));
    await closeContext(opened.context);

    if (negativeControls) await verifyNegativeControls(browser, runtime.origin, textChecks);

    const identityAfter = await fetch(runtime.origin + IDENTITY_PATH).then(response => response.json());
    assert.equal(identityAfter.healthy, true, 'runtime identity healthy after acceptance');
    assert.equal(identityAfter.source.matches, true, 'source identity did not drift');
    assert.equal(identityAfter.build.matches, true, 'build identity did not drift');
    report.runtimeIdentity = {
      healthyBefore: identityBefore.healthy,
      healthyAfter: identityAfter.healthy,
      sourceMatches: identityAfter.source.matches,
      buildMatches: identityAfter.build.matches,
      isolatedOrigin: true,
      isolatedContexts: report.cleanup.contextsClosed,
    };
    assert.deepEqual(report.pageErrors, [], 'application page errors');
    report.status = 'pass';
  } finally {
    if (browser) {
      for (const context of contexts) await closeContext(context);
      await browser.close();
      report.cleanup.browserClosed = true;
    }
    const runtimeRoot = runtime.roots.root;
    await runtime.close();
    report.cleanup.runtimeClosed = true;
    // browser-runtime owns this unique task root but its generic release API
    // intentionally refuses the still-live launcher process. Once both the
    // browser and server are closed, remove only that exact owned root.
    fs.rmSync(runtimeRoot, { recursive: true, force: true });
    report.cleanup.runtimeRootsRemoved = !fs.existsSync(runtimeRoot);
  }
  fs.writeFileSync(path.join(output, 'receipt.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ status: report.status, expectations: report.expectationIds.length, executions: report.executions.length, output }));
}

main().catch(error => {
  report.status = 'fail';
  report.failure = error.stack;
  fs.mkdirSync(output, { recursive: true });
  fs.writeFileSync(path.join(output, 'receipt.json'), JSON.stringify(report, null, 2) + '\n');
  console.error(error);
  process.exitCode = 1;
});
