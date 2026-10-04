'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');
const { advance } = require('../src/js/domain/animation/playback-step.js');
const source = fs.readFileSync(path.join(root, 'src/js/timeline.js'), 'utf8');

// Fixed observations from the pre-extraction advancePlayFrame at the protected
// base. Keep the exact odd single-frame ping-pong flip: it alternates direction
// without leaving that frame.
test('frame-step decisions retain the existing edge, wrap and reversal table', () => {
  const cases = [
    [[2, 1, 0, 4, false, false], { frame: 3, direction: 1, audioLoop: false }],
    [[4, 1, 0, 4, false, false], { frame: null, direction: 1, audioLoop: false }],
    [[4, 1, 0, 4, true, false], { frame: 0, direction: 1, audioLoop: true }],
    [[4, 1, 0, 4, true, true], { frame: 3, direction: -1, audioLoop: false }],
    [[0, -1, 0, 4, true, true], { frame: 1, direction: 1, audioLoop: false }],
    [[0, -1, 0, 4, true, false], { frame: null, direction: -1, audioLoop: false }],
    [[3, 1, 3, 3, true, true], { frame: 3, direction: -1, audioLoop: false }],
    [[3, -1, 3, 3, true, true], { frame: 3, direction: 1, audioLoop: false }],
    [[3, 1, 3, 3, true, false], { frame: 3, direction: 1, audioLoop: true }],
  ];
  for (const [input, expected] of cases) {
    assert.deepEqual(advance(...input), expected, JSON.stringify(input));
  }
});

test('domain step is pure and never depends on browser, state or audio globals', () => {
  const input = [4, 1, 0, 4, true, true];
  const first = advance(...input);
  assert.deepEqual(input, [4, 1, 0, 4, true, true]);
  assert.deepEqual(advance(...input), first);
  const code = fs.readFileSync(path.join(root, 'src/js/domain/animation/playback-step.js'), 'utf8')
    .split('\n').map(line => line.replace(/\/\/.*$/, '')).join('\n');
  assert.doesNotMatch(code, /\b(?:window|document|state|SMAudio|Paper)\b/);
});

function loadPlayback(options = {}) {
  const events = [];
  const raf = [];
  const button = { innerHTML: '', classList: { add(name) { events.push('class+' + name); }, remove(name) { events.push('class-' + name); } } };
  const state = { currentFrame: options.frame ?? 0, playDir: options.direction ?? 1,
    waIn: options.workIn ?? 0, waOut: options.workOut ?? 4,
    loopPlayback: options.loop ?? false, pingPongPlayback: options.pingPong ?? false,
    playing: false, fps: options.fps ?? 10 };
  const context = { state, NemoPlaybackStep: { advance },
    document: { getElementById(id) { assert.equal(id, 'btn-play'); return button; } },
    performance: { now() { return 0; } },
    requestAnimationFrame(callback) { raf.push(callback); return raf.length; },
    cancelAnimationFrame() {}, clearInterval() {},
    saveAllLayerFrames() { events.push('save'); },
    loadFrame(frame) { events.push('load:' + frame); },
    updatePlayhead() { events.push('playhead'); },
    renderOS() {}, renderArcs() {}, updateUI() {},
    SMAudio: { onLoop(frame) { events.push('loop:' + frame); },
      onPlayStart(frame) { events.push('start:' + frame); }, onPlayStop() { events.push('stop'); } }
  };
  context.window = context;
  vm.createContext(context);
  const start = source.indexOf('var playInt=');
  const end = source.indexOf('// Keyframe diamonds', start);
  assert.ok(start >= 0 && end > start, 'playback functions found in production source');
  vm.runInContext(source.slice(start, end), context, { filename: 'src/js/timeline.js:playback' });
  return { context, state, events, tick(now) { const callback = raf.shift(); assert.ok(callback, 'scheduled real playStep'); callback(now); } };
}

test('production wrapper mutates direction and calls audio only on normal loop', () => {
  const wrapped = loadPlayback({ frame: 4, loop: true, pingPong: false });
  assert.equal(wrapped.context.advancePlayFrame(4), 0);
  assert.deepEqual(wrapped.events, ['loop:0']);
  assert.equal(wrapped.state.playDir, 1);

  const bounced = loadPlayback({ frame: 4, loop: true, pingPong: true });
  assert.equal(bounced.context.advancePlayFrame(4), 3);
  assert.equal(bounced.state.playDir, -1);
  assert.deepEqual(bounced.events, []);

  const stopped = loadPlayback({ frame: 4 });
  assert.equal(stopped.context.advancePlayFrame(4), null);
  assert.deepEqual(stopped.events, []);
});

test('real startPlay uses fixed clock, drops presentation frames and stops at non-loop edge', () => {
  const playback = loadPlayback({ frame: 0, fps: 10, workOut: 3 });
  playback.context.startPlay();
  assert.equal(playback.state.playing, true);
  playback.tick(300); // three source frames, one rendered destination frame
  assert.equal(playback.state.currentFrame, 3);
  assert.deepEqual(playback.events.filter(e => e.startsWith('load:')), ['load:3']);
  playback.tick(400); // next logical frame is past work-area edge
  assert.equal(playback.state.playing, false);
  assert.equal(playback.state.currentFrame, 3);
  assert.equal(playback.events.filter(e => e === 'stop').length, 1);
});

test('real startPlay crosses a wrap with exactly one audio-loop call', () => {
  const playback = loadPlayback({ frame: 3, fps: 10, workOut: 4, loop: true });
  playback.context.startPlay();
  playback.tick(200); // 3 -> 4 -> 0, rendered once at 0
  assert.equal(playback.state.currentFrame, 0);
  assert.deepEqual(playback.events.filter(e => e.startsWith('loop:')), ['loop:0']);
  assert.deepEqual(playback.events.filter(e => e.startsWith('load:')), ['load:0']);
});
