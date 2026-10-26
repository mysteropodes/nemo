'use strict';
// CS3c/#2383: src/index.html's body script region is generated from engineering/application/script-manifest.json.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const M = require('../scripts/nemo/lib/script-manifest.cjs');
const { main } = require('../scripts/nemo/script-manifest.cjs');

const ROOT = path.join(__dirname, '..');
const quiet = { log() {}, error() {} };
function sandbox() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-script-manifest-'));
  for (const name of ['native-new-contract.js', 'native-new-ui.js']) {
    fs.mkdirSync(path.join(root, 'src/js/adapters'), { recursive: true });
    fs.writeFileSync(path.join(root, 'src/js/adapters', name), '');
  }
  for (const file of ['src/index.html', M.MANIFEST]) {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.copyFileSync(path.join(ROOT, file), path.join(root, file));
  }
  return root;
}
const html = (root) => fs.readFileSync(path.join(root, 'src/index.html'), 'utf8');

test('the committed region is exactly the rendered manifest and every page tag follows manifest order', () => {
  assert.equal(main(['--check'], ROOT, quiet), 0);
  const page = html(ROOT), manifest = M.readManifest(ROOT);
  assert.equal(M.split(page).region, M.render(manifest));
  // The region holds every body tag; only the two head tags (gpu gate, Paper.js) precede it.
  const tags = M.observeScripts(page).map((s) => (s.src ? (s.module ? 'module:' : '') + s.src : 'inline'));
  const listed = manifest.entries.filter((e) => !e.comment).map((e) => (e.src ? (e.type ? 'module:' : '') + e.src : 'inline'));
  assert.deepEqual(tags.slice(0, 2), ['js/gpu-gate.js', 'paper-full.min.js']);
  assert.deepEqual(tags.slice(2), listed);
  // Reading the generated region back yields the manifest itself.
  assert.deepEqual(M.extract(M.split(page).region.split('\n').slice(1, -1)), manifest.entries);
});

test('a hand-edited or stale region fails --check and --write restores it byte for byte', () => {
  const root = sandbox(), original = html(root);
  for (const edit of [
    (t) => t.replace('<script src="js/adapters/native-application.js"></script>\n', ''),
    (t) => t.replace('<script src="js/adapters/native-application.js"></script>', '<script src="js/adapters/native-application.js" defer></script>'),
    (t) => t.replace(M.END, '<script src="js/extra.js"></script>\n' + M.END),
  ]) {
    fs.writeFileSync(path.join(root, 'src/index.html'), edit(original));
    assert.equal(main(['--check'], root, quiet), 1);
    assert.equal(main(['--write'], root, quiet), 0);
    assert.equal(html(root), original);
  }
  fs.writeFileSync(path.join(root, 'src/index.html'), original.replace(M.END, ''));
  assert.throws(() => main(['--check'], root, quiet), /exactly one script-manifest begin\/end marker pair/);
  fs.writeFileSync(path.join(root, 'src/index.html'), original.replace(/\n/g, '\r\n'));
  assert.throws(() => main(['--check'], root, quiet), /LF line endings/);
});

test('--add places a new adapter by name and the region follows; bad paths and duplicates are refused', () => {
  const root = sandbox();
  assert.equal(main(['--add', 'js/adapters/native-new-contract.js', '--before', 'js/adapters/native-application.js'], root, quiet), 0);
  assert.equal(main(['--add', 'js/adapters/native-new-ui.js'], root, quiet), 0);
  assert.equal(main(['--check'], root, quiet), 0);
  const order = M.observeScripts(html(root)).map((s) => s.src);
  assert.equal(order.indexOf('js/adapters/native-new-contract.js') + 1, order.indexOf('js/adapters/native-application.js'));
  assert.equal(order[order.length - 1], 'js/adapters/native-new-ui.js');
  assert.throws(() => main(['--add', 'js/adapters/native-new-ui.js'], root, quiet), /listed twice/);
  assert.throws(() => main(['--add', '../outside.js'], root, quiet), /invalid script path/);
  fs.writeFileSync(path.join(root, 'src/js/x.js'), '');
  assert.throws(() => main(['--add', 'js/x.js', '--after', 'js/not-listed.js'], root, quiet), /is not listed/);
  assert.throws(() => main(['--add', 'js/x.js', '--before'], root, quiet), /needs a value/);
  assert.throws(() => main(['--add', 'js/missing.js'], root, quiet), /does not exist/);
  assert.equal(main(['--check'], root, quiet), 0, 'a refused --add leaves page and manifest consistent');
  for (const entry of [{ src: 'js/x.js', defer: true }, { inline: 'a</script>' }, { comment: ['no comment'] }, { comment: ['<!-- script-manifest:end -->'] }]) {
    assert.equal(M.validEntry(entry), false, JSON.stringify(entry));
  }
});
