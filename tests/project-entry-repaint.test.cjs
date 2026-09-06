'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const source = fs.readFileSync(path.resolve(__dirname, '../src/js/project-entry.js'), 'utf8');

for (const native of [true, false]) {
  test(`project entry repaints after surface resize (${native ? 'native' : 'Paper'})`, () => {
    const frames = [];
    let renderedScene = null;
    const sandbox = {
      SMEngineBridge: { isEnabled: () => native, renderNow: () => { renderedScene = 'native'; } },
      view: { update: () => { renderedScene = 'Paper'; } },
      requestAnimationFrame(callback) { frames.push(callback); },
    };
    sandbox.window = sandbox;
    vm.runInNewContext(source, sandbox);
    sandbox.SMProjectEntry.repaint();
    assert.equal(frames.length, 1);
    frames.shift()();
    // ResizeObserver can clear the canvas after callbacks in the first frame.
    renderedScene = null;
    assert.equal(frames.length, 1, 'a post-resize presentation frame must remain queued');
    frames.shift()();
    assert.equal(renderedScene, native ? 'native' : 'Paper');
    assert.equal(frames.length, 0);
  });
}
