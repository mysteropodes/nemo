'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');

function extractFunction(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `${name} is missing`);
  let depth = 0;
  let opened = false;
  for (let i = start; i < source.length; i++) {
    if (source[i] === '{') { depth++; opened = true; }
    else if (source[i] === '}' && opened && --depth === 0) return source.slice(start, i + 1);
  }
  throw new Error(`${name} is incomplete`);
}

test('project entry repaints after the first-frame native surface resize', () => {
  const source = fs.readFileSync(path.join(root, 'src/js/project.js'), 'utf8');
  const frames = [];
  let renderedScene = null;
  const sandbox = {
    window: { SMEngineBridge: { isEnabled: () => true, renderNow: () => { renderedScene = 'restored'; } } },
    requestAnimationFrame(callback) { frames.push(callback); },
  };
  sandbox.SMEngineBridge = sandbox.window.SMEngineBridge;
  vm.runInNewContext(`${extractFunction(source, 'repaintProjectEntry')}\nthis.repaint = repaintProjectEntry;`, sandbox);

  sandbox.repaint();
  assert.equal(frames.length, 1);

  frames.shift()();
  // The native canvas ResizeObserver may resize the WebGPU surface after
  // callbacks in this frame, clearing anything painted during it.
  renderedScene = null;
  assert.equal(frames.length, 1, 'a post-resize presentation frame must remain queued');

  frames.shift()();
  assert.equal(renderedScene, 'restored');
});
