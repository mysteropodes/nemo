'use strict';
// JS production join only: real File New, ProjectDesktop, metadata controllers,
// and images.js menu. Controlled native replies do not prove Rust or installed behavior.
const test = require('node:test');
const assert = require('node:assert/strict');
const { fixture } = require('./helpers/native-guide-production.cjs');
const Workflow = require('../src/js/application/native-object-save-as-workflow.js');
const Acknowledgement = require('../src/js/application/native-object-saved-acknowledgement.js');
const ProjectDocument = require('../src/js/project-document.js');

test('ordinary New metadata fanout settles before cancelled image import without repeated same-pin queries',
  { timeout: 5000 }, async t => {
    for (const refuse of [false, true]) await t.test(refuse ? 'held Palette refusal' : 'successful metadata',
      { timeout: 2000 }, async subtest => {
        const f = fixture({ composition: true, palette: true }), root = f.root, picks = [];
        const Files = root.NemoNativeObjectProjectFile;
        let alive = true, cleanup;
        function dispose() { if (!cleanup) { alive = false; cleanup = f.dispose(); } return cleanup; }
        subtest.after(dispose);
        try {
          const control = f.holdPaletteRead({ refuse });
          const budgetFailure = control.repeated.then(({ operation, selector }) => {
            assert.fail('Unsolicited second same-pin metadata query: ' + operation + '/' + selector.kind);
          });
          const bounded = promise => Promise.race([promise, budgetFailure]);
          root.SM.t = key => key;
          root.NemoNativeObjectSaveAsWorkflow = Workflow;
          root.NemoNativeObjectSavedAcknowledgement = Acknowledgement;
          root.SMProjectDocument = ProjectDocument;
          // RAF is a controlled layout boundary; production File.bind owns reveal and layout wait.
          root.requestAnimationFrame = callback => setImmediate(() => { if (alive) callback(); });
          root.__TAURI__.dialog = { open: async options => { picks.push(options); return null; },
            ask: () => assert.fail('Initial New must not ask to replace a project'),
            save: () => assert.fail('This control must not open a save picker') };
          const start = f.document.createElement('div'); start.id = 'start-screen'; f.document.body.appendChild(start);
          const meta = { tabId: null, path: null, name: 'Untitled' };
          const file = Files.bind(root, {
            read: () => ({ ...meta }), show: () => start.classList.remove('hid'), hide: () => start.classList.add('hid'),
            commit: (path, name) => { meta.path = path; meta.name = name; },
            opened: () => { meta.tabId = 'new-tab'; }, repaint() {}, notice: text => f.notices.push(text),
            touchRecent: () => assert.fail('New must not add a file recent'), renderRecents() {},
          });
          const menu = f.installImageMenu();
          assert.equal(await bounded(file.newProject({ w: 320, h: 180, fps: 24, name: 'New' })), true);
          await bounded(control.started);
          const bootstrap = f.calls.filter(call => call.command === 'nemo_native_object_bootstrap');
          assert.equal(bootstrap.length, 1);
          assert.deepEqual(bootstrap[0].request.newProject, { canvasW: 320, canvasH: 180, fps: 24 });
          assert.equal(Object.hasOwn(bootstrap[0].request, 'sourcePath'), false);
          assert.equal(start.classList.contains('hid'), true);
          assert.equal(meta.name, 'New'); assert.equal(meta.tabId, 'new-tab');
          assert.equal(f.session.readPresentedObjectSession(), null, 'The held real metadata command fences public presentation');
          assert.equal(typeof f.session.importBatch, 'undefined');
          assert.equal(Files.projectionReadyForUI(root, f.session.info()), true);
          assert.deepEqual(await bounded(menu()), { status: 'unavailable', reason: 'no_presented_native_session' });
          assert.equal(picks.length, 0, 'Busy admission refuses before the ordinary picker');
          assert.equal(f.edits().length, 0);
          control.release();
          await bounded(f.settle());
          assert.ok(f.session.readPresentedObjectSession(), 'Actual completion fanout must reach genuine idle presentation');
          assert.equal(typeof f.session.importBatch, 'function');
          const guide = root.NemoNativeGuideController.read(); assert.ok(guide);
          assert.deepEqual(await bounded(menu()), { status: 'cancelled' });
          assert.deepEqual(picks, [{ title: 'Import Image', multiple: false, filters: [{ name: 'PNG Image', extensions: ['png'] }] }]);
          await bounded(f.settle());
          assert.ok(control.counts().every(count => count === 1), 'Every same-pin metadata selector stays inside its one-query budget');
          const paletteQueries = f.calls.filter(call => call.request && call.request.operation === 'query.document.object.project.palette.read');
          assert.deepEqual(paletteQueries.map(call => call.request.payload.selector.kind), refuse ? ['settings'] : ['settings', 'palettePage']);
          const compositionQueries = f.calls.filter(call => call.request && call.request.operation === 'query.document.object.project.composition.read');
          assert.deepEqual(compositionQueries.map(call => call.request.payload.selector.kind), ['settings', 'markerPage']);
          assert.strictEqual(root.NemoNativeGuideController.read(), guide, 'Cancelled picker preserves the sealed Guide cache');
          assert.deepEqual(f.notices, ['New project created', 'Native import is unavailable.']);
          assert.equal(f.edits().length, 0);
          assert.equal(f.session.info().contentRevision, 0); assert.equal(f.session.info().undoDepth, 0);
          assert.equal(f.saved(), f.originalDocument);
        } finally { await dispose(); }
      });
  });
