'use strict';
// P24 installed check: with the observer package started under NEMO_TEST_PLAY_FIXED_CLOCK=1 the product
// scheduler advances exactly one frame period per tick, so the frames it asks the native owner to present
// are fully determined by the kernel. This offline verifier derives that sequence from the shipped kernel
// and compares it with the retained play-step requests. It drives nothing and never launches the app.
//
//   node tests/desktop/p24-fixed-clock-play.cjs <native-app.stderr.log> --start 0 --wa-in 0 --wa-out 20 [--loop] [--ping] [--steps N]
const fs = require('node:fs');
const surface = require('../../src/js/adapters/native-opacity-motion-surface.js');

// Frames requested after `start` at one frame per tick; ends where the kernel stops playback.
function expectedFrames({ start, waIn, waOut, loop = false, ping = false, steps = 4096 }) {
  const state = { playDir: 1, waIn, waOut, loopPlayback: loop, pingPongPlayback: ping };
  const audio = { onLoop() {} };
  const frames = [];
  for (let cur = start, guard = 0; frames.length < steps && guard++ < 100000;) {
    const next = surface.advancePlaybackFrame(state, audio, cur);
    if (next === null) break;
    if (next !== cur) frames.push(next);
    cur = next;
  }
  return frames;
}

// `events` are observer events (the `event` member of each retained record), in order.
function verifyFixedClockSteps(events, spec) {
  const requests = events.filter(e => e.event === 'play-step' && e.result === undefined);
  const returns = events.filter(e => e.event === 'play-step' && e.result !== undefined);
  const observed = requests.map(e => e.frame);
  const expected = expectedFrames({ ...spec, steps: spec.steps ?? (observed.length || 1) });
  const failures = [];
  if (!observed.length) failures.push('no play-step request was observed');
  if (observed.length > expected.length) failures.push('more requests than the kernel allows');
  const n = Math.min(observed.length, expected.length);
  for (let i = 0; i < n; i++) if (observed[i] !== expected[i]) { failures.push(`request ${i}: ${observed[i]} != ${expected[i]}`); break; }
  if (returns.length !== requests.length || returns.some(e => e.result !== true)) failures.push('a step did not return a presented frame');
  return { ok: failures.length === 0, failures, observed, expected: expected.slice(0, Math.max(n, 0)) };
}

function eventsFromLog(text) {
  const out = [];
  for (const line of text.split('\n')) {
    if (!line.startsWith('NEMO_PLAY_OBSERVER ')) continue;
    const record = JSON.parse(line.slice('NEMO_PLAY_OBSERVER '.length));
    if (record.kind === 'event') out.push(record.event);
  }
  return out;
}

function main(argv) {
  const number = name => { const i = argv.indexOf(name); return i < 0 ? undefined : Number(argv[i + 1]); };
  const file = argv.find(a => !a.startsWith('--') && !/^\d+$/.test(a));
  if (!file || number('--wa-out') === undefined) {
    process.stderr.write('usage: p24-fixed-clock-play.cjs <native-app.stderr.log> --start N --wa-in N --wa-out N [--loop] [--ping] [--steps N]\n');
    return 2;
  }
  const spec = { start: number('--start') ?? 0, waIn: number('--wa-in') ?? 0, waOut: number('--wa-out'),
    loop: argv.includes('--loop'), ping: argv.includes('--ping'), steps: number('--steps') };
  const result = verifyFixedClockSteps(eventsFromLog(fs.readFileSync(file, 'utf8')), spec);
  process.stdout.write(JSON.stringify(result) + '\n');
  return result.ok ? 0 : 1;
}

module.exports = { expectedFrames, verifyFixedClockSteps, eventsFromLog };
if (require.main === module) process.exitCode = main(process.argv.slice(2));
