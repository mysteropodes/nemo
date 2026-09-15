'use strict';
// #1307 — area classification must not depend on where the repo is checked out.
//
// AREA_BY_FILE in lib/inventory-surfaces.cjs is an ordered list of unanchored
// substring regexes. Matching it against an ABSOLUTE path let a token in the
// checkout directory name decide the area for every file beneath it, and the
// earlier the hijacking rule sits, the more it swallowed.
//
// The reason this was worth a leaf rather than a note: the wrong output still
// passes `inventory.cjs --check` on the machine that produced it, because
// regenerating from the same path reproduces the same misclassification. The
// generator agrees with itself. It only surfaces as unexplained churn in
// someone else's diff, or as a test failure pointing somewhere unrelated.
//
// Found by Pollen while delivering P29 from a worktree named
// `nemo-p29-project-save`: the `project` rule outranks `storyboard`, so all 17
// storyboard rows silently became `project-lifecycle-integrations` and broke
// `fixtures manifest: areas exist in the surface inventory` — a failure with no
// visible connection to the change.
//
// These run the REAL generator over identical content from two roots that
// differ only in their directory name, so they fail for the actual reason
// rather than by inspecting the rule table.

const assert = require('node:assert/strict');
const test = require('node:test');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const inventory = require('./inventory.cjs');

// Minimal but real: a button the generator can find, bound from a file whose
// area is decided by the `storyboard` rule, which sits AFTER the `project` and
// `tools` rules — exactly the ordering that produced the original incident.
const HTML = [
  '<!doctype html><html><body>',
  '<div id="probe-box">',
  '<button id="btn-probe" title="Probe">Probe</button>',
  '</div>',
  '<script src="js/storyboard.js"></script>',
  '</body></html>',
].join('\n');

// A dom row AND a dynamic-menu row. The menu row is the one that matters:
// menu rows classify from the handler FILE (`areaFor(f.file, …)`), which is the
// absolute path, and they are exactly the rows the P29 incident moved. A dom
// row alone does NOT reproduce the defect — a fixture without this menu literal
// passes with the fix reverted, which is to say it proves nothing.
const APP_JS = [
  "document.getElementById('btn-probe').addEventListener('click', function () {",
  '  updateUI();',
  '});',
  'function wireSpace() {',
  '  return [',
  "    { label: 'New montage', action: function () { updateUI(); } },",
  '  ];',
  '}',
].join('\n');

function buildFromRootNamed(prefix) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  try {
    fs.mkdirSync(path.join(dir, 'src', 'js'), { recursive: true });
    fs.writeFileSync(path.join(dir, 'src', 'index.html'), HTML);
    fs.writeFileSync(path.join(dir, 'src', 'js', 'storyboard.js'), APP_JS);
    return inventory.build({ root: dir });
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

// `project` and `tools` both sit earlier in AREA_BY_FILE than the `storyboard`
// rule that legitimately owns this file, so a directory carrying them outranks it.
const TRAPPED = 'nemo-project-tools-';
const PLAIN = 'plain-';

test('#1307: the same repo-relative file classifies identically from a trapped and a plain checkout path', () => {
  const trapped = buildFromRootNamed(TRAPPED);
  const plain = buildFromRootNamed(PLAIN);

  assert.ok(plain.rows.length > 0, 'the fixture produced no rows, so the comparison would be vacuous');
  assert.equal(trapped.rows.length, plain.rows.length);

  const areasOf = (built) => built.rows.map((r) => r.id + ' ' + r.area).sort();
  assert.deepEqual(areasOf(trapped), areasOf(plain),
    'area classification changed with the checkout directory name');
});

test('#1307: the victim row lands on its real area, not the one its directory name suggests', () => {
  // The aggregate above would also pass if BOTH roots were wrong in the same
  // way, so pin the answer itself, not just the agreement.
  const areaOfProbe = (built) => {
    // The MENU row, not the dom row: menu rows are the ones classified from the
    // absolute handler path, and the ones the P29 incident actually moved.
    const row = built.rows.find((r) => r.kind === 'menu');
    assert.ok(row, 'no menu row; the fixture stopped exercising the path that carries the defect');
    return row.area;
  };
  const trapped = areaOfProbe(buildFromRootNamed(TRAPPED));
  const plain = areaOfProbe(buildFromRootNamed(PLAIN));
  assert.equal(trapped, plain);
  assert.equal(trapped, 'storyboard',
    'the row must land on the area its own path implies, not the one the checkout directory suggests');
});

test('#1307: no row carries an absolute path, so nothing downstream can reintroduce the dependency', () => {
  const built = buildFromRootNamed(TRAPPED);
  const leaked = built.rows.filter((r) => JSON.stringify(r).includes(os.tmpdir()));
  assert.deepEqual(leaked.map((r) => r.id), [],
    'a row leaked an absolute path; a later consumer matching on it would depend on the checkout again');
});
