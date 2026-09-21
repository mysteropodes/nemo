'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const ROOT = path.resolve(__dirname, '..');
const kitsuSource = fs.readFileSync(path.join(ROOT, 'src/js/kitsu.js'), 'utf8');
const tutorialSource = fs.readFileSync(path.join(ROOT, 'src/js/tutorial.js'), 'utf8');

function deferred() {
  let resolve;
  const promise = new Promise((res) => { resolve = res; });
  return { promise, resolve };
}

function nextTurn() {
  return new Promise((resolve) => setImmediate(resolve));
}

// Bracket-balanced extraction keeps the test on the production function while
// avoiding tutorial.js's large translated lesson catalog and DOM bootstrap.
function extractFunctionSource(source, functionName) {
  const asyncMarker = 'async function ' + functionName;
  const syncMarker = 'function ' + functionName;
  const asyncStart = source.indexOf(asyncMarker);
  const marker = asyncStart === -1 ? syncMarker : asyncMarker;
  const start = asyncStart === -1 ? source.indexOf(syncMarker) : asyncStart;
  assert.notEqual(start, -1, `${functionName}: function not found`);
  const braceStart = source.indexOf('{', start);
  assert.notEqual(braceStart, -1, `${functionName}: opening brace not found`);
  let depth = 0;
  for (let i = braceStart; i < source.length; i++) {
    if (source[i] === '{') depth++;
    else if (source[i] === '}') {
      depth--;
      if (depth === 0) return source.slice(start, i + 1);
    }
  }
  throw new Error(`${functionName}: unbalanced braces`);
}

function makeReplacement(mode, installFreshProject, calls) {
  const gate = deferred();
  const error = new Error('project replacement refused');
  return {
    gate,
    newProject(cfg) {
      calls.push(['newProject', cfg]);
      if (mode === 'legacy') {
        installFreshProject();
        return undefined;
      }
      if (mode === 'delayed') {
        return gate.promise.then(() => { installFreshProject(); });
      }
      return Promise.reject(error);
    },
  };
}

function kitsuHarness(mode) {
  const replacementCalls = [];
  const continuationCalls = [];
  const networkCalls = [];
  const state = {
    totalFrames: 9,
    waIn: 2,
    waOut: 8,
    layers: [{ name: 'Old', frames: [{ old: true }, { old: true }] }],
  };
  const installFreshProject = () => {
    state.layers = [{ name: 'Layer 1', frames: [
      { strokes: [], isKeyframe: false, isInterpolated: false },
      { strokes: [], isKeyframe: false, isInterpolated: false },
      { strokes: [], isKeyframe: false, isInterpolated: false },
      { strokes: [], isKeyframe: false, isInterpolated: false },
    ] }];
  };
  const replacement = makeReplacement(mode, installFreshProject, replacementCalls);
  const context = {
    console,
    state,
    URLSearchParams,
    sessionStorage: { getItem: () => null, setItem() {}, removeItem() {} },
    document: { readyState: 'loading', addEventListener() {} },
    createUserLayer(name) {
      continuationCalls.push(['createUserLayer', name]);
      state.layers.push({ name, frames: [] });
      return state.layers.length - 1;
    },
    activateUL(index) { continuationCalls.push(['activateUL', index]); },
    loadFrame(frame) { continuationCalls.push(['loadFrame', frame]); },
    renderOS() { continuationCalls.push(['renderOS']); },
    renderArcs() { continuationCalls.push(['renderArcs']); },
    updateUI() { continuationCalls.push(['updateUI']); },
  };
  context.window = context;
  context.SMProject = { newProject: replacement.newProject };
  context.renderSymbolTabs = () => continuationCalls.push(['renderSymbolTabs']);
  context.__TAURI__ = {
    http: {
      async fetch(url) {
        networkCalls.push(url);
        return { ok: true, json: async () => [], text: async () => '' };
      },
    },
  };
  vm.runInNewContext(kitsuSource, context, { filename: 'src/js/kitsu.js' });
  return { context, state, replacement, replacementCalls, continuationCalls, networkCalls };
}

function tutorialHarness(mode) {
  const replacementCalls = [];
  const continuationCalls = [];
  const replacement = makeReplacement(mode, () => {}, replacementCalls);
  const startScreen = { classList: { contains: () => false } };
  const context = {
    console,
    MODULES: [{ id: 'lesson', title: 'Lesson', steps: [] }],
    active: null,
    document: { getElementById: (id) => id === 'start-screen' ? startScreen : null },
    ensureDom() { continuationCalls.push(['ensureDom']); },
    closeLauncher() { continuationCalls.push(['closeLauncher']); },
    renderStep() { continuationCalls.push(['renderStep']); },
  };
  context.window = context;
  context.SMProject = { newProject: replacement.newProject };
  vm.createContext(context);
  const source = extractFunctionSource(tutorialSource, 'startModule');
  vm.runInContext(`${source}\nthis.startModuleUnderTest = startModule;`, context,
    { filename: 'src/js/tutorial.js:startModule' });
  return { context, replacement, replacementCalls, continuationCalls };
}

function kitsuUiHarness(openShot) {
  const continuationCalls = [];
  const list = { innerHTML: '' };
  const context = {
    console,
    el(id) {
      assert.equal(id, 'kitsu-list');
      return list;
    },
    hideModal() { continuationCalls.push(['hideModal']); },
    updateKitsuShotUI() { continuationCalls.push(['updateKitsuShotUI']); },
    showToast(message) { continuationCalls.push(['showToast', message]); },
  };
  context.window = context;
  context.SMKitsu = { getSession: () => session, openShot };
  context.SMProject = { enterEditor() { continuationCalls.push(['enterEditor']); } };
  vm.createContext(context);
  const source = extractFunctionSource(kitsuSource, 'pickShot');
  vm.runInContext(`${source}\nthis.pickShotUnderTest = pickShot;`, context,
    { filename: 'src/js/kitsu.js:pickShot' });
  return { context, list, continuationCalls };
}

const session = { serverUrl: 'https://kitsu.test', accessToken: 'token' };
const project = { id: 'project-1', name: 'Film' };
const sequence = { id: 'sequence-1', name: 'Seq' };
const shot = { id: 'shot-1', name: 'Shot', nb_frames: 3, data: { fps: 12, resolution: '320x180' } };

test('Kitsu openShot preserves immediate undefined legacy replacement success', async () => {
  const h = kitsuHarness('legacy');
  const result = await h.context.SMKitsu.openShot(session, project, sequence, shot);

  assert.equal(h.replacementCalls.length, 1);
  assert.equal(result.meta.frameCount, 3);
  assert.equal(h.state.layers[0].name, 'Rough');
  assert.deepEqual(h.state.layers.slice(1).map((layer) => layer.name), ['Clean', 'Color']);
  assert.ok(h.continuationCalls.length > 0, 'shot setup must continue after immediate legacy success');
  assert.equal(h.networkCalls.length, 2);
});

test('Kitsu openShot waits for delayed replacement before shot mutation or render', async () => {
  const h = kitsuHarness('delayed');
  const before = JSON.stringify(h.state);
  const pending = h.context.SMKitsu.openShot(session, project, sequence, shot);
  await nextTurn();

  assert.equal(JSON.stringify(h.state), before);
  assert.deepEqual(h.continuationCalls, []);
  assert.deepEqual(h.networkCalls, []);

  h.replacement.gate.resolve();
  await pending;
  assert.equal(h.state.layers[0].name, 'Rough');
  assert.ok(h.continuationCalls.length > 0);
});

test('Kitsu openShot rejection propagates with zero post-replacement effects', async () => {
  const h = kitsuHarness('reject');
  const before = JSON.stringify(h.state);

  await assert.rejects(
    () => h.context.SMKitsu.openShot(session, project, sequence, shot),
    /project replacement refused/,
  );
  assert.equal(JSON.stringify(h.state), before);
  assert.deepEqual(h.continuationCalls, []);
  assert.deepEqual(h.networkCalls, []);
  assert.equal(h.state.kitsuShot, null);
});

test('Kitsu UI suppresses editor entry and success toast when replacement rejects', async () => {
  const h = kitsuHarness('reject');
  const ui = kitsuUiHarness((...args) => h.context.SMKitsu.openShot(...args));

  await ui.context.pickShotUnderTest(project, sequence, shot);
  assert.deepEqual(ui.continuationCalls, []);
  assert.match(ui.list.innerHTML, /project replacement refused/);
});

test('tutorial startModule preserves immediate undefined legacy replacement success', async () => {
  const h = tutorialHarness('legacy');
  await h.context.startModuleUnderTest('lesson');

  assert.equal(h.replacementCalls.length, 1);
  assert.deepEqual(h.continuationCalls, [['ensureDom'], ['closeLauncher'], ['renderStep']]);
  assert.equal(h.context.active.module.id, 'lesson');
});

test('tutorial startModule waits for delayed replacement before lesson setup', async () => {
  const h = tutorialHarness('delayed');
  const pending = h.context.startModuleUnderTest('lesson');
  await nextTurn();

  assert.deepEqual(h.continuationCalls, []);
  assert.equal(h.context.active, null);

  h.replacement.gate.resolve();
  await pending;
  assert.deepEqual(h.continuationCalls, [['ensureDom'], ['closeLauncher'], ['renderStep']]);
  assert.equal(h.context.active.module.id, 'lesson');
});

test('tutorial startModule rejection propagates with zero lesson setup', async () => {
  const h = tutorialHarness('reject');

  await assert.rejects(
    () => h.context.startModuleUnderTest('lesson'),
    /project replacement refused/,
  );
  assert.deepEqual(h.continuationCalls, []);
  assert.equal(h.context.active, null);
});
