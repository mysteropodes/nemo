const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const root = path.resolve(__dirname, '..');

function scrubHarness() {
  const source = fs.readFileSync(path.join(root, 'src/js/ui.js'), 'utf8');
  const start = source.indexOf('var scrubState=null;');
  const end = source.indexOf('})();', start);
  assert.ok(start >= 0 && end > start, 'production numeric scrub is available');
  const documentListeners = new Map();
  const windowListeners = new Map();
  const events = [];
  const document = {
    activeElement: null,
    addEventListener(type, listener) { documentListeners.set(type, listener); },
  };
  const window = {
    pushUndo() { throw new Error('native authority denied legacy history checkpoint'); },
    addEventListener(type, listener) { windowListeners.set(type, listener); },
  };
  let value = '25';
  const input = {
    get value() { return value; }, set value(next) { value = String(next); },
    min: '0', max: '100', dataset: { step: '1', nativeOpacityScrub: 'static' },
    closest(selector) { return selector === 'input.scrub' ? this : null; },
    setPointerCapture() {}, focus() {}, select() {},
    classList: { add() {}, remove() {} },
    dispatchEvent(event) { events.push({ type: event.type, value: this.value }); },
  };
  vm.runInNewContext(source.slice(start, end), {
    document, window, Math,
    Event: class { constructor(type) { this.type = type; } },
    requestAnimationFrame() { throw new Error('native scrub must not dispatch an interim legacy change'); },
    cancelAnimationFrame() {},
  });
  function fire(type, clientX = 100) {
    documentListeners.get(type)({
      type, target: input, pointerId: 7, clientX, preventDefault() {},
    });
  }
  return { fire, input, events, windowListeners };
}

test('admitted native static-opacity drag submits one final value without legacy checkpoint or interim write', () => {
  const h = scrubHarness();
  h.fire('pointerdown');
  assert.doesNotThrow(() => h.fire('pointermove', 160));
  assert.equal(h.input.value, '40', '60px drag proposes a 15-point edit');
  assert.deepEqual(h.events, [], 'no document edit is dispatched before release');
  h.fire('pointerup', 160);
  assert.deepEqual(h.events, [{ type: 'change', value: '40' }], 'one release submits one native edit');
  assert.equal(h.input.value, '25', 'tentative field rolls back until native projection publishes');
  assert.equal(h.windowListeners.has('blur'), true);
});

test('cancelled native static-opacity drag restores the authoritative field without a command', () => {
  const h = scrubHarness();
  h.fire('pointerdown');
  h.fire('pointermove', 160);
  h.fire('pointercancel', 160);
  assert.deepEqual(h.events, []);
  assert.equal(h.input.value, '25');
});
