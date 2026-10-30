'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const C = require('./lib/boundaries-coverage.cjs');
const root = path.resolve(__dirname, '../..');
const read = f => fs.readFileSync(path.join(root, f), 'utf8');
const profile = JSON.parse(read('engineering/boundaries/profiles/rust.profile.json'));
const source = 'src-tauri/src/native_object_recovery.rs';
const store = 'src-tauri/src/native_object_recovery_store.rs';
const listing = 'src-tauri/src/native_object_recovery_list.rs';
const tests = ['recovery_tests', 'recovery_failure_tests', 'recovery_list_tests'].map(n => `src-tauri/src/native_object_${n}.rs`);

test('recovery store and its controls have an exact module census', () => {
  const commands = profile.modules.find(m => m.id === 'rust.desktop.native.application.commands');
  for (const file of [source, store, listing]) assert.equal(commands.files.filter(f => f === path.basename(file)).length, 1);
  const controls = profile.modules.find(m => m.id === 'rust.desktop.native.host.release.tests');
  for (const file of tests) assert.equal(controls.files.filter(f => f === path.basename(file)).length, 1);
  for (const file of [source, store, listing, ...tests]) {
    const omitted = structuredClone(profile);
    for (const m of omitted.modules) m.files = m.files.filter(f => f !== path.basename(file));
    assert.ok(C.checkSourceCoverage(omitted, { root, sourcePaths: [file] }).violations
      .some(v => v.rule === 'coverage-unprofiled-source' && v.file === file));
  }
  const tooling = JSON.parse(read('engineering/boundaries/profiles/scripts-nemo.profile.json'));
  const module = tooling.modules.find(m => m.id === 'nemo.test.boundariesRust');
  assert.equal(module.files.filter(f => f === 'boundaries-native-object-recovery.test.cjs').length, 1);
});

test('four private main-window async commands are registered and gated', () => {
  const producer = read(source), bootstrap = read('src-tauri/src/native_object_bootstrap.rs'), lib = read('src-tauri/src/lib.rs');
  assert.match(bootstrap, /#\[path = "native_object_recovery.rs"\]\s*mod recovery;/);
  assert.match(bootstrap, /#\[cfg\(test\)\]\s*#\[path = "native_object_recovery_tests.rs"\]\s*mod recovery_tests;/);
  for (const name of ['write', 'list', 'discard', 'quarantine']) {
    assert.match(lib, new RegExp(`native_object_bootstrap::nemo_native_object_recovery_${name},`));
    assert.match(producer, new RegExp(`pub\\(crate\\) async fn nemo_native_object_recovery_${name}\\(`));
  }
  assert.equal((producer.match(/require_main\(&window\)\?/g) || []).length, 4);
  assert.equal((producer.match(/spawn_blocking/g) || []).length, 1);
  assert.equal((producer.match(/blocking\(move/g) || []).length, 4);
});

test('publication order is temp, sync, unique rename, dir sync, manifest rename and never a path from JS', () => {
  const producer = read(store) + read(source) + read(listing);
  assert.match(producer, /create_new\(true\)/);
  assert.match(producer, /options\.mode\(0o600\)/);
  assert.match(producer, /builder\.mode\(0o700\)/);
  const order = ['write_temp(&slot_dir, "data"', 'fs::rename(&tmp, &final_path)', 'sync_dir(&slot_dir)',
    'write_manifest(&slot_dir, &manifest, effect)', 'prune_slot(&slot_dir'];
  let at = -1;
  for (const marker of order) {
    const next = producer.indexOf(marker, at + 1);
    assert.ok(next > at, marker);
    at = next;
  }
  assert.match(producer, /file\.sync_all\(\)/);
  assert.match(producer, /symlink_metadata/);
  assert.match(producer, /\.corrupt-/);
  assert.match(producer, /deny_unknown_fields/);
  assert.doesNotMatch(producer, /\bsuper::|native_engine::|remove_dir_all\(path|request\.[a-z_]*path\b[^;]*\.join/);
  assert.doesNotMatch(producer, /slot_path|destination_path/);
  assert.match(read('src-tauri/src/native_object_project_route.rs'), /fn native_project_recovery_snapshot/);
});
