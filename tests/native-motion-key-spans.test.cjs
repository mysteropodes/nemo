'use strict';
// #2377: the duration blocks and connector bars behind the native Motion keys, drawn as the baseline motion.js trackRowHtml did.
const test = require('node:test'), assert = require('node:assert/strict');
const Spans = require('../src/js/adapters/native-motion-key-spans.js');
class Node {
  constructor() { this.children = []; this.style = {}; this.dataset = {}; this.className = ''; this.classes = new Set(); this.events = {};
    this.classList = { add: (k) => this.classes.add(k) }; }
  appendChild(n) { this.children.push(n); return n; }
  addEventListener(k, f) { this.events[k] = f; }
}
const doc = { createElement: () => new Node() };
const track = (items) => ({ items });

test('below 18px per frame there are no duration blocks; from 18px each key gets one, tinted with its own colour', () => {
  const row = new Node(), items = [{ frame: 2, v: [1], color: '#e8b64c' }, { frame: 5, v: [2] }];
  assert.equal(Spans.draw(doc, row, [{ track: track(items), top: 6 }], 17, null).filter((n) => /durblock/.test(n.className)).length, 0);
  const row2 = new Node(), nodes = Spans.draw(doc, row2, [{ track: track(items), top: 6 }], 20, null);
  const blocks = nodes.filter((n) => /durblock/.test(n.className));
  assert.deepEqual(blocks.map((b) => [b.style.left, b.style.top, b.style.width, b.style.height, b.classes.has('tinted'), b.style['--key-color']]),
    [['40px', '0px', '20px', '22px', true, '#e8b64c'], ['100px', '0px', '20px', '22px', false, undefined]]);
  assert.equal(blocks[0].style.pointerEvents, 'none', 'the block is pass-through');
});

test('a connector spans centre to centre, 5px tall, centred in a 22px lane; an unchanged value draws none; a second lane sits 22px lower', () => {
  const row = new Node();
  const nodes = Spans.draw(doc, row, [{ track: track([{ frame: 0, v: [5, 5] }, { frame: 3, v: [5, 5] }, { frame: 8, v: [5, 6] }]), top: 28 },
    { track: track([{ frame: 1, v: [0] }, { frame: 2, v: [1] }]), top: 50 }], 10, null);
  assert.deepEqual(nodes.map((n) => [n.dataset.keyFrom, n.dataset.keyTo, n.style.left, n.style.top, n.style.width, n.style.height]),
    [['3', '8', '35px', '31px', '50px', '5px'], ['1', '2', '15px', '53px', '10px', '5px']]);
});

test('a value-less key is drawn rather than hidden, and a selected pair reads selected', () => {
  assert.equal(Spans.changed({ frame: 0 }, { frame: 1 }), true);
  assert.equal(Spans.changed({ v: [1] }, { v: [1] }), false);
  const row = new Node();
  const [bar] = Spans.draw(doc, row, [{ track: track([{ frame: 0, selected: true, v: [1] }, { frame: 2, selected: true, v: [2] }]), top: 6 }], 10, null);
  assert.equal(bar.classes.has('sel'), true);
});

test('a press on a connector hands its track and both end keys to onPick', () => {
  const row = new Node(), picked = [], t = track([{ frame: 0, v: [1] }, { frame: 4, v: [2] }]);
  const [bar] = Spans.draw(doc, row, [{ track: t, top: 6 }], 10, (e, tr, a, b) => picked.push([tr === t, a.frame, b.frame]));
  bar.events.mousedown({});
  assert.deepEqual(picked, [[true, 0, 4]]);
});
