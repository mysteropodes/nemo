'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { checkApplicationPolicy, checkApplicationSize } = require('./lib/boundaries-application.cjs');

const ROOT = path.resolve(__dirname, '../..');
const read = (name) => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

test('adopted application policy composes fresh discovery, exact coverage and provenance pins', () => {
  const policy = read('app-js.coverage.json');
  const result = checkApplicationPolicy(read('app-js.profile.json'), policy, { root: ROOT });
  assert.equal(result.ok, true);
  // Counted against the policy's own sets, never pinned to `{173, 161, 12}`.
  // That triple was the merge hazard of #1316 twice over: it lived here AND in
  // the artifact's `snapshotCounts`, so two branches each adding one module
  // wrote the same bumped number in both places and git merged them silently.
  // Every discovered source is either retained or excluded — state that.
  assert.equal(result.retainedPathCount, policy.retainedSources.length);
  assert.equal(result.excludedPathCount, policy.exclusions.length);
  assert.equal(result.sourcePathCount, result.retainedPathCount + result.excludedPathCount);
  assert.ok(result.sourcePathCount > 0, 'discovery found application sources at all');
  // The breakdowns are computed too, so they cannot drift from the lists.
  assert.deepEqual(result.snapshotCounts.byExclusionCategory,
    policy.exclusions.reduce((out, record) => ({ ...out, [record.category]: (out[record.category] || 0) + 1 }), {}));
  assert.equal(Object.values(result.snapshotCounts.retainedExecutionClasses).reduce((a, b) => a + b, 0),
    policy.retainedSources.length);
});

test('a source accounted for by neither list fails on the identity, not on a stale number', () => {
  // Removing the equation from the checker must make this test red. Dropping an
  // exclusion leaves 173 discovered against 161 + 11 accounted for, so the
  // equation fires before anything else and names both sides. Without it the
  // call would return ok:false from coverage instead of throwing, which is why
  // this asserts the throw and its message rather than just a failure.
  const policy = read('app-js.coverage.json');
  assert.throws(() => checkApplicationPolicy(read('app-js.profile.json'),
    { ...policy, exclusions: policy.exclusions.slice(1) }, { root: ROOT }),
  /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('a stored snapshotCounts block is rejected rather than trusted', () => {
  // Storing the counts beside the lists is what made them a merge hazard. The
  // checker computes them, so a reintroduced block is refused outright instead
  // of quietly becoming a second source of truth again.
  const policy = read('app-js.coverage.json');
  assert.equal(policy.snapshotCounts, undefined, 'the committed policy stores no counts');
  assert.throws(() => checkApplicationPolicy(read('app-js.profile.json'),
    { ...policy, snapshotCounts: { selectedSources: 173, retainedSources: 161, exclusions: 12 } }, { root: ROOT }),
  /snapshotCounts is computed/);
});

test('provisional policy and profile membership drift fail before becoming a standard pass', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  assert.throws(() => checkApplicationPolicy(profile, { ...policy, status: 'provisional' }, { root: ROOT }), /status must be adopted/);
  const changed = structuredClone(policy);
  changed.retainedSources[0].moduleId = 'candidate-controlled-reassignment';
  assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /paths and module IDs/);
});

test('N13 native viewport adapter cannot be omitted from fresh application discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-viewport.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.viewport.adapter'
    && entry.executionClass === 'classic-without-load-site'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.viewport.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N20 activates the N15 native application adapter without weakening discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-application.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.application.adapter'
    && entry.executionClass === 'document-classic'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.application.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N20 activates the N17 native opacity editor adapter without weakening discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-opacity-editor.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.opacity.editor.adapter'
    && entry.executionClass === 'document-classic'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.opacity.editor.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N20 activates the N17 native opacity selection adapter without weakening discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-opacity-selection.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.opacity.selection.adapter'
    && entry.executionClass === 'document-classic'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.opacity.selection.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N20 activates the N18 native opacity preview adapter without weakening discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-opacity-preview.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.opacity.preview.adapter'
    && entry.executionClass === 'document-classic'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.opacity.preview.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N20 activates the N18 native opacity export adapter without weakening discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/adapters/native-opacity-export.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.opacity.export.adapter'
    && entry.executionClass === 'document-classic'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.opacity.export.adapter');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N20 activates the N19C native edit guard without weakening discovery', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  const required = 'src/js/application/native-edit-guard.js';
  assert.ok(policy.retainedSources.some((entry) => entry.path === required
    && entry.moduleId === 'app.native.edit.guard.application'
    && entry.executionClass === 'document-classic'));
  const dropped = structuredClone(profile);
  dropped.modules = dropped.modules.filter((module) => module.id !== 'app.native.edit.guard.application');
  const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((entry) => entry.path !== required) };
  assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
    /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
});

test('N19G canvas adapters cannot be omitted from fresh discovery or loaded after consumers', () => {
  const profile = read('app-js.profile.json'), policy = read('app-js.coverage.json');
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  for (const name of ['motion', 'select']) {
    const sourcePath = `src/js/adapters/${name}-canvas-intent.js`, id = `app.${name}.canvas-intent.adapter`;
    const entry = policy.retainedSources.find((record) => record.path === sourcePath);
    assert.ok(entry && entry.moduleId === id && entry.executionClass === 'document-classic');
    const tag = `<script src="js/adapters/${name}-canvas-intent.js"></script>`;
    const consumer = `<script src="js/${name === 'select' ? 'select-bridge' : 'motion'}.js"></script>`;
    assert.ok(html.includes(tag) && html.indexOf(tag) < html.indexOf(consumer));
    assert.equal(html.split(tag).length - 1, 1, 'one classic registration');
    const dropped = { ...profile, modules: profile.modules.filter((module) => module.id !== id) };
    const droppedPolicy = { ...policy, retainedSources: policy.retainedSources.filter((record) => record.path !== sourcePath) };
    assert.throws(() => checkApplicationPolicy(dropped, droppedPolicy, { root: ROOT }),
      /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
  }
});

test('N20 contract, authority, adapters and bootstrap load in their frozen order', () => {
  const policy = read('app-js.coverage.json');
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8').split('\n');
  let ordinal = 0;
  const actual = new Map();
  html.forEach((line, index) => {
    if (!line.includes('<script')) return;
    ordinal += 1;
    const match = line.match(/src=["']([^"']+)/);
    if (match) actual.set('src/' + match[1], { path: 'src/index.html', line: index + 1, scriptOrdinal: ordinal });
  });
  const paths = [
    'src/js/application/native-opacity-contract.js',
    'src/js/application/native-opacity-replacement.js',
    'src/js/application/native-opacity-export-workflow.js',
    'src/js/application/native-opacity-preview-workflow.js',
    'src/js/application/native-opacity-v1.js',
    'src/js/application/native-opacity-lifecycle.js',
    'src/js/application/native-opacity-viewport.js',
    'src/js/application/native-object-save-as-workflow.js',
    'src/js/application/native-object-saved-acknowledgement.js',
    'src/js/adapters/native-font-panel.js',
    'src/js/adapters/native-viewport-underlay.js',
    'src/js/adapters/native-object-project-metadata.js',
    'src/js/adapters/native-object-project-desktop.js',
    'src/js/adapters/native-object-project.js',
    'src/js/adapters/native-object-project-file.js',
    'src/js/adapters/native-object-ui.js',
    'src/js/adapters/native-drawing-add.js',
    'src/js/adapters/native-drawing-add-desktop.js',
    'src/js/adapters/native-project-settings-intent.js',
    'src/js/adapters/native-project-settings-desktop.js',
    'src/js/adapters/native-object-reference.js',
    'src/js/application/native-opacity-operations.js',
    'src/js/application/opacity-application.js',
    'src/js/adapters/native-opacity-legacy-surface.js',
    'src/js/adapters/native-opacity-motion-surface.js',
    'src/js/adapters/native-motion-property-view.js',
    'src/js/adapters/native-motion-property-panel.js',
    'src/js/adapters/native-motion-property-binding.js',
    'src/js/adapters/native-object-inactive-project-tabs.js',
    'src/js/adapters/native-property-contract.js',
    'src/js/adapters/native-application.js',
    'src/js/adapters/native-opacity-editor.js',
    'src/js/adapters/native-opacity-selection.js',
    'src/js/adapters/native-opacity-preview.js',
    'src/js/adapters/native-opacity-export.js',
    'src/js/application/native-edit-guard.js',
    'src/js/bootstrap/opacity-application.js',
    'src/js/bootstrap/native-opacity-application.js',
    'src/js/adapters/application-mcp.js',
  ];
  for (const sourcePath of paths) {
    const entry = policy.retainedSources.find((record) => record.path === sourcePath);
    assert.equal(entry.executionClass, 'document-classic', sourcePath);
    assert.deepEqual(entry.loadSites, [actual.get(sourcePath)], sourcePath);
  }
  assert.deepEqual(paths.map((sourcePath) => actual.get(sourcePath).scriptOrdinal),
    [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 103, 104, 105, 149, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205]);
  assert.ok(actual.get('src/js/adapters/native-object-inactive-project-tabs.js').scriptOrdinal
    < actual.get('src/js/project.js').scriptOrdinal, 'inactive-tab adapter loads before the project consumer');
  const projectEntry = 'src/js/adapters/native-opacity-project-entry.js';
  const entry = policy.retainedSources.find((record) => record.path === projectEntry);
  assert.equal(entry.moduleId, 'adapter.native.opacity.project.entry');
  assert.equal(entry.executionClass, 'document-classic');
  assert.deepEqual(entry.loadSites, [actual.get(projectEntry)]);
  assert.equal(actual.get('src/js/adapters/project-dirty-baseline.js').scriptOrdinal, 146);
  assert.equal(actual.get('src/js/adapters/project-native-version-restore.js').scriptOrdinal, 147);
  assert.equal(actual.get(projectEntry).scriptOrdinal, 148);
  assert.equal(actual.get('src/js/project.js').scriptOrdinal, 150);
  const order = sourcePath => actual.get('src/' + sourcePath).scriptOrdinal;
  assert.ok(order('js/adapters/native-object-project-metadata.js') < order('js/adapters/native-object-project-desktop.js'));
  assert.ok(order('js/adapters/native-object-project-desktop.js') < order('js/adapters/native-object-project.js'));
  assert.ok(order('js/adapters/native-motion-property-binding.js') < order('js/motion.js'));
  assert.equal(policy.retainedSources.find(record => record.path === 'src/js/adapters/native-object-project-desktop.js').moduleId, 'app.native.object.project.adapter');
  for (const name of ['view', 'panel', 'binding']) assert.equal(policy.retainedSources.find(record => record.path === 'src/js/adapters/native-motion-property-' + name + '.js').moduleId, 'app.native.motion.properties.adapter');
  const updaterPath = 'src/js/adapters/updater-platform.js';
  const updater = policy.retainedSources.find((record) => record.path === updaterPath);
  assert.equal(updater.moduleId, 'app.updater.platform.adapter');
  assert.equal(updater.executionClass, 'document-classic');
  assert.deepEqual(updater.loadSites, [actual.get(updaterPath)]);
  const updaterTags = html.filter((line) => line.includes('src="js/adapters/updater-platform.js"'));
  assert.deepEqual(updaterTags, ['<script src="js/adapters/updater-platform.js"></script>']);
  assert.equal(actual.get(updaterPath).scriptOrdinal + 1,
    actual.get('src/js/updater-bridge.js').scriptOrdinal);
});

test('bootstrap numerical observations match actual HTML and retained declarations', () => {
  const policy = read('app-js.coverage.json'), observed = policy.bootstrapObservations;
  const html = fs.readFileSync(path.join(ROOT, observed.path), 'utf8');
  const attr = (text, name) => {
    const match = text.match(new RegExp('\\b' + name + '\\s*=\\s*(?:"([^"]*)"|\'([^\']*)\'|([^\\s>]+))', 'i'));
    return match && (match[1] || match[2] || match[3]);
  };
  const scripts = [...html.matchAll(/<script\b([^>]*)>/gi)].map((match, index) => ({
    src: attr(match[1], 'src'), module: attr(match[1], 'type') === 'module',
    path: observed.path, line: html.slice(0, match.index).split('\n').length, scriptOrdinal: index + 1,
  }));
  const retained = new Map(policy.retainedSources.map((record) => [record.path, record]));
  assert.equal(observed.totalScriptTags, scripts.length);
  assert.equal(observed.externalClassicTags, scripts.filter((s) => s.src && !s.module).length);
  assert.equal(observed.externalModuleTags, scripts.filter((s) => s.src && s.module).length);
  assert.equal(observed.inlineClassicTags, scripts.filter((s) => !s.src && !s.module).length);
  assert.equal(observed.retainedClassicTags, scripts.filter((s) => s.src && !s.module && retained.get('src/' + s.src)?.executionClass === 'document-classic').length);
  assert.equal(observed.retainedModuleTags, scripts.filter((s) => s.src && s.module && retained.get('src/' + s.src)?.executionClass === 'document-module').length);
  assert.equal(observed.classicTagsUnderSrcJsIncludingVendors, scripts.filter((s) => s.src?.startsWith('js/') && !s.module).length);
  assert.deepEqual(observed.inlineScripts, scripts.filter((s) => !s.src).map(({ path, line, scriptOrdinal }) => ({ path, line, scriptOrdinal })));
  assert.equal(observed.noLoadSiteAudit.paths, policy.retainedSources.filter((s) => !s.loadSites?.length).length);
});

test('source, profile and exclusion provenance cannot drift behind unchanged policy', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  for (const mutate of [
    (copy) => { copy.provenance.sourceRootTree = '0'.repeat(40); },
    (copy) => { copy.provenance.bootstrap.sha256 = '0'.repeat(64); },
    (copy) => { copy.exclusions[0].provenance.gitBlob = '0'.repeat(40); },
    (copy) => { copy.provenance.inventoryDigest.value = '0'.repeat(64); },
  ]) {
    const changed = structuredClone(policy);
    mutate(changed);
    assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /pin changed|SHA-256 changed|Git blob changed|digest changed/);
  }
});

test('every exclusion retains explicit classification, evidence and nested content pins', () => {
  const profile = read('app-js.profile.json');
  const policy = read('app-js.coverage.json');
  for (const mutate of [
    (copy) => { copy.exclusions[0].category = 'handwritten'; },
    (copy) => { copy.exclusions[0].component = ''; },
    (copy) => { copy.exclusions[0].reason = ''; },
    (copy) => { copy.exclusions[0].evidence = []; },
    (copy) => { delete copy.exclusions[0].provenance.sha256; },
    (copy) => { delete copy.exclusions[0].provenance.gitBlob; },
  ]) {
    const changed = structuredClone(policy);
    mutate(changed);
    assert.throws(() => checkApplicationPolicy(profile, changed, { root: ROOT }), /category|component|reason|evidence|pin is missing/);
  }
});

test('application size enforcement ignores known graph debt but rejects growth and expired exceptions', (t) => {
  const current = checkApplicationSize(read('app-js.profile.json'), { root: ROOT });
  assert.equal(current.ok, true);
  assert.equal(current.exceptionsApplied.length, 36);

  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo app size '));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'src/js'), { recursive: true });
  fs.writeFileSync(path.join(root, 'src/js/app.js'), 'window.SMLegacy = true;\nconst grew = true;\n');
  const profile = {
    modules: [{ id: 'app.legacy', layer: 'app-legacy', dir: 'src/js', files: ['app.js'],
      publicApi: ['app.js'], sizeProfile: 'legacy' }],
    layerRules: { 'app-legacy': { allowedLayers: ['app-legacy'] } },
    sizeProfiles: { legacy: { warn: 1, hardMax: 1 } },
    exceptions: [],
  };
  const growth = checkApplicationSize(profile, { root });
  assert.equal(growth.ok, false);
  assert.deepEqual(growth.violations.map((violation) => violation.rule), ['size']);
  profile.exceptions.push({ path: 'src/js/app.js', rule: 'size', owner: 'owner', issue: '901',
    reason: 'fixture', expires: '2026-01-01', ceiling: 2 });
  const expired = checkApplicationSize(profile, { root, now: new Date('2026-09-06T00:00:00Z') });
  assert.equal(expired.ok, false);
  assert.deepEqual(expired.violations.map((violation) => violation.rule).sort(), ['expired-exception', 'size']);
});

test('U03 localization runtime is parser-blocking immediately before dictionaries and preserves startup extensions', () => {
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const scripts = [...html.matchAll(/<script\b([^>]*)>/gi)].map((match, index) => ({
    attrs: match[1], src: match[1].match(/\bsrc=["']([^"']+)/)?.[1], ordinal: index + 1,
  }));
  const runtimePath = 'js/adapters/i18n-runtime.js', dataPath = 'js/i18n.js';
  const runtime = scripts.filter((entry) => entry.src === runtimePath), data = scripts.filter((entry) => entry.src === dataPath);
  assert.equal(runtime.length, 1); assert.equal(data.length, 1);
  assert.equal(runtime[0].ordinal, 91); assert.equal(data[0].ordinal, 92);
  assert.equal(runtime[0].ordinal + 1, data[0].ordinal);
  assert.ok(scripts.find((entry) => entry.src === 'js/app.js').ordinal < runtime[0].ordinal);
  assert.ok(scripts.find((entry) => entry.src === 'js/timeline.js').ordinal < runtime[0].ordinal);
  for (const entry of [...runtime, ...data]) assert.doesNotMatch(entry.attrs, /\b(?:async|defer|type)\s*(?:=|\s|$)/i);
  const policy = read('app-js.coverage.json'), retained = policy.retainedSources.find((entry) => entry.path === 'src/' + runtimePath);
  assert.equal(retained.moduleId, 'app.i18n.runtime.adapter'); assert.equal(retained.executionClass, 'document-classic');
  assert.equal(retained.loadSites[0].scriptOrdinal, 91);
  const callbacks = [], storage = [], events = new Map(), scope = {state: {}, SM: {retained: true, afterI18n: callbacks}};
  scope.window = scope; scope.document = {readyState: 'loading', querySelectorAll: () => [], getElementById: () => null,
    addEventListener(name, callback) {events.set(name, callback);}};
  scope.localStorage = {getItem: () => 'es', setItem(key, value) {storage.push([key, value]);}};
  for (const name of ['NemoNativeObjectProject', 'NemoNativeObjectProjectFile', '__TAURI__'])
    Object.defineProperty(scope, name, {get() {throw Error('localization resolved native authority');}});
  const context = require('node:vm').createContext(scope);
  for (const name of [runtimePath, dataPath]) require('node:vm').runInContext(fs.readFileSync(path.join(ROOT, 'src', name), 'utf8'), context, {filename: name});
  assert.equal(scope.SM.retained, true); assert.strictEqual(scope.SM.afterI18n, callbacks); assert.equal(events.size, 1);
  events.get('DOMContentLoaded')(); assert.equal(scope.state.language, 'es'); assert.equal(scope.SM.t('settingsTitle'), 'Ajustes');
  for (const [locale, title] of [['en', 'Settings'], ['fr', 'Réglages'], ['ja', '設定'], ['es', 'Ajustes']]) {
    scope.SM.setLanguage(locale); assert.equal(scope.SM.t('settingsTitle'), title);
  }
  assert.deepEqual(storage.map((entry) => entry[1]), ['en', 'fr', 'ja', 'es']);
});


function assertF06Bootstrap(html) {
  const scripts = [...html.matchAll(/<script\b([^>]*)>\s*<\/script>/gi)].map((match) => ({
    tag: match[0], src: match[1].match(/\bsrc=["']([^"']+)/)?.[1],
  }));
  const sources = ['native-font-panel', 'native-viewport-underlay', 'native-object-project-metadata',
    'native-object-project-desktop', 'native-object-project', 'native-object-project-file', 'native-object-ui'];
  const positions = sources.map((name) => {
    const source = 'js/adapters/' + name + '.js';
    const selected = scripts.filter((entry) => entry.src === source);
    assert.equal(selected.length, 1, source + ' must have one registration');
    assert.equal(selected[0].tag, '<script src="' + source + '"></script>', source + ' must be synchronous classic');
    return scripts.indexOf(selected[0]);
  });
  assert.deepEqual(positions, positions.map((_, index) => positions[0] + index),
    'underlay → parser → desktop → Project → ProjectFile → UI must remain in that order');
  const updater = ['js/adapters/updater-platform.js', 'js/updater-bridge.js'].map((source) => {
    const selected = scripts.filter((entry) => entry.src === source);
    assert.equal(selected.length, 1, source + ' must remain unique');
    assert.equal(selected[0].tag, '<script src="' + source + '"></script>');
    return scripts.indexOf(selected[0]);
  });
  assert.equal(updater[0] + 1, updater[1], 'I40 platform remains immediately before its bridge');
}

test('F06 parser bootstrap is unique synchronous and precedes Project, ProjectFile and UI', () => {
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  assertF06Bootstrap(html);
  const fonts = '<script src="js/adapters/native-font-panel.js"></script>';
  const parser = '<script src="js/adapters/native-object-project-metadata.js"></script>';
  const desktop = '<script src="js/adapters/native-object-project-desktop.js"></script>';
  const project = '<script src="js/adapters/native-object-project.js"></script>';
  for (const broken of [
    html.replace(fonts, ''),
    html.replace(fonts, fonts.replace('<script ', '<script defer ')),
    html.replace(parser, ''),
    html.replace(parser, parser + '\n' + parser),
    html.replace(parser, parser.replace('<script ', '<script async ')),
    html.replace(parser, parser.replace('<script ', '<script defer ')),
    html.replace(parser, parser.replace('<script ', '<script type="module" ')),
    html.replace(desktop + '\n' + project, project + '\n' + desktop),
  ]) assert.throws(() => assertF06Bootstrap(broken), assert.AssertionError);
});

test('F06 real browser exports initialize in declared order and missing parser refuses before Project export', () => {
  const vm = require('node:vm'), html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const names = ['native-object-project-metadata', 'native-object-project-desktop', 'native-object-project', 'native-object-project-file', 'native-object-ui'];
  const load = (context, name) => vm.runInContext(fs.readFileSync(path.join(ROOT, 'src/js/adapters', name + '.js'), 'utf8'), context, {filename: name});
  const context = vm.createContext({});
  const actual = [...html.matchAll(/<script src="js\/adapters\/([^"/]+)\.js"><\/script>/g)]
    .map((match) => match[1]).filter((name) => names.includes(name));
  assert.deepEqual(actual, names);
  for (const name of actual) load(context, name);
  for (const [global, method] of [['NemoNativeObjectProjectMetadata', 'decode'], ['NemoNativeObjectProjectDesktop', 'createPorts'], ['NemoNativeObjectProject', 'create'],
    ['NemoNativeObjectProjectFile', 'projectionReadyForUI'], ['NemoNativeObjectUI', 'createPolicyPorts']]) {
    assert.equal(typeof context[global][method], 'function'); assert.equal(Object.isFrozen(context[global]), true);
  }
  const missing = vm.createContext({});
  assert.throws(() => load(missing, 'native-object-project'), /Native project metadata parser is unavailable/);
  assert.equal(missing.NemoNativeObjectProject, undefined);
});

test('F06 project metadata parser has ordinary membership with real missing and duplicate controls', () => {
  const profile = read('app-js.profile.json'), id = 'app.native.object.project-metadata.adapter';
  const members = profile.modules.filter((module) => module.id === id);
  assert.equal(members.length, 1);
  assert.deepEqual(members[0], {id, layer: 'domain', dir: 'src/js/adapters',
    files: ['native-object-project-metadata.js'], sizeProfile: 'Domain/application',
    publicApi: ['native-object-project-metadata.js']});
  const {checkSourceCoverage} = require('./lib/boundaries-coverage.cjs');
  const options = {root: ROOT, sourcePaths: ['src/js/adapters/native-object-project-metadata.js'], exclusions: []};
  assert.equal(checkSourceCoverage({modules: members}, options).ok, true);
  assert.deepEqual(checkSourceCoverage({modules: []}, options).violations.map(({rule, file}) => ({rule, file})),
    [{rule: 'coverage-unprofiled-source', file: options.sourcePaths[0]}]);
  const duplicate = checkSourceCoverage({modules: [...members, {...members[0], id: id + '.duplicate'}]}, options);
  assert.deepEqual(duplicate.violations.map(({rule, file}) => ({rule, file})),
    [{rule: 'coverage-duplicate-declaration', file: options.sourcePaths[0]}]);
});

function assertU30Bootstrap(html, profile) {
  const scripts=[...html.matchAll(/<script\b([^>]*)>\s*<\/script>/gi)].map(m=>({tag:m[0],src:m[1].match(/\bsrc=["']([^"']+)/)?.[1]}));
  for(const [module,caller,id] of [['labs-registry-model','labs-core','app.labs.registry.model.adapter'],['labs-settings-ui','labs-panel','app.labs.settings.ui.adapter'],['labs-floating-ui','labs-float-panel','app.labs.floating.ui.adapter']]){
    const source='js/adapters/'+module+'.js',consumer='js/labs/'+caller+'.js';
    const adapters=scripts.filter(s=>s.src===source),callers=scripts.filter(s=>s.src===consumer);
    assert.equal(adapters.length,1);assert.equal(callers.length,1);
    assert.equal(adapters[0].tag,'<script src="'+source+'"></script>');assert.equal(callers[0].tag,'<script src="'+consumer+'"></script>');
    assert.equal(scripts.indexOf(adapters[0])+1,scripts.indexOf(callers[0]));
    const expected={id,layer:'adapters',dir:'src/js/adapters',files:[module+'.js'],sizeProfile:'Platform/engine adapter',publicApi:[module+'.js']};
    assert.deepEqual(profile.modules.filter(m=>m.id===id),[expected]);
    assert.equal(profile.modules.flatMap(m=>m.files.map(file=>m.dir+'/'+file)).filter(p=>p==='src/'+source).length,1);
  }
}
test('U30 modules have unique ordinary memberships and synchronous model-before-caller loads',()=>{
  assertU30Bootstrap(fs.readFileSync(path.join(ROOT,'src/index.html'),'utf8'),read('app-js.profile.json'));
});
test('U30 bootstrap refuses actual missing, deferred, reordered, duplicate loads and missing/duplicate membership',()=>{
  const html=fs.readFileSync(path.join(ROOT,'src/index.html'),'utf8'),profile=read('app-js.profile.json');
  for(const [module,caller] of [['labs-registry-model','labs-core'],['labs-settings-ui','labs-panel'],['labs-floating-ui','labs-float-panel']]){
    const a='<script src="js/adapters/'+module+'.js"></script>',b='<script src="js/labs/'+caller+'.js"></script>';
    for(const changed of [html.replace(a,''),html.replace(a,a.replace('<script ','<script defer ')),html.replace(a+'\n'+b,b+'\n'+a),html.replace(a,a+'\n'+a)])assert.throws(()=>assertU30Bootstrap(changed,profile),{code:'ERR_ASSERTION'});
    const copy=structuredClone(profile),member=copy.modules.find(m=>m.files.includes(module+'.js'));copy.modules=copy.modules.filter(m=>m!==member);assert.throws(()=>assertU30Bootstrap(html,copy),{code:'ERR_ASSERTION'});
    copy.modules.push(member,{...member,id:member.id+'.duplicate'});assert.throws(()=>assertU30Bootstrap(html,copy),{code:'ERR_ASSERTION'});
  }
});
test('U30 ordinary joined caller reaches the same registry effect without a dangling module',()=>{
  const html=fs.readFileSync(path.join(ROOT,'src/index.html'),'utf8'),order=[...html.matchAll(/<script src="(js\/(?:adapters\/labs-(?:registry-model|settings-ui|floating-ui)|labs\/labs-(?:core|panel|float-panel))\.js)"><\/script>/g)].map(m=>m[1]);
  assert.equal(order.length,6);const events=[],values=new Map(),trace=[],nodes={};
  const node=()=>({style:{},dataset:{},children:[],classList:{toggle(){}},appendChild(n){this.children.push(n);},addEventListener(k,f){this[k]=f;}});
  nodes['labs-tools-list']=node();nodes['labs-actions-list']=node();
  const scope={state:{layers:[],activeLayerIdx:0},SM:{t:k=>k},SMLabs:{retained:true},document:{readyState:'loading',getElementById:id=>nodes[id]||null,createElement:node,createTextNode:text=>({textContent:text}),addEventListener:(k,f)=>events.push(f)},addEventListener(){},localStorage:{getItem:k=>values.get(k)||null,setItem:(k,v)=>values.set(k,v)},showToast(){},console};scope.window=scope;
  const context=require('node:vm').createContext(scope),original=scope.SMLabs;
  for(const source of order)require('node:vm').runInContext(fs.readFileSync(path.join(ROOT,'src',source),'utf8'),context,{filename:source});
  assert.strictEqual(scope.SMLabs,original);assert.equal(scope.SMLabs.retained,true);scope.SMLabs.isKnifeTrimActive=()=>false;
  const def={flag:'probe',onEnable(){assert.strictEqual(this,def);trace.push('enabled');}};scope.SMLabs.register('predictive-stroke',def);scope.renderLabsPanel();
  const checkbox=nodes['labs-tools-list'].children[0].children[0].children[0];checkbox.checked=true;checkbox.change();
  assert.equal(values.get('probe'),'1');assert.deepEqual(trace,['enabled']);
});

test('G41 Font bootstrap preserves explicit native load order and membership', () => {
  const html = fs.readFileSync(path.join(ROOT, 'src/index.html'), 'utf8');
  const names = ['font-assets.js', 'native-font-contract.js', 'native-font-resources.js'];
  const check = (source) => {
    const tags = [...source.matchAll(/<script\b[^>]*>[\s\S]*?<\/script>/g)].map((m) => m[0]);
    const indices = names.map((name) => {
      const exact = '<script src="js/adapters/' + name + '"></script>';
      assert.equal(tags.filter((tag) => tag.includes('js/adapters/' + name)).length, 1);
      assert.ok(tags.includes(exact)); return tags.indexOf(exact);
    });
    assert.ok(indices[0] < indices[1] && indices[1] < indices[2]);
    assert.ok(indices[2] < tags.indexOf('<script src="js/vector-text-bridge.js"></script>'));
    assert.ok(indices[2] < tags.indexOf('<script src="js/adapters/native-application.js"></script>'));
  };
  check(html);
  for (const name of names) {
    const tag = '<script src="js/adapters/' + name + '"></script>';
    for (const broken of [html.replace(tag, ''), html.replace(tag, tag + tag), html.replace(tag, tag.replace('<script ', '<script defer '))])
      assert.throws(() => check(broken), assert.AssertionError);
    assert.equal(read('app-js.profile.json').modules.filter((m) => m.dir === 'src/js/adapters' && m.files.includes(name)).length, 1);
  }
  const contract = '<script src="js/adapters/native-font-contract.js"></script>';
  const resources = '<script src="js/adapters/native-font-resources.js"></script>';
  assert.throws(() => check(html.replace(contract + '\n' + resources, resources + '\n' + contract)), assert.AssertionError);
});
