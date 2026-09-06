'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

test('native isolation refusal precedes Tauri context and builder initialization', () => {
  // Wiring check complements task_runtime's executable Rust behavior controls.
  // A setup-only check is too late: Tauri creates configured windows first.
  const source = fs.readFileSync(path.join(__dirname, '../src-tauri/src/lib.rs'), 'utf8');
  const run = source.slice(source.indexOf('pub fn run() {'));
  const resolve = run.indexOf('task_runtime::TaskRuntime::from_env()');
  const abort = run.indexOf('std::process::exit(2)');
  const context = run.indexOf('tauri::generate_context!()');
  const builder = run.indexOf('tauri::Builder::default()');
  assert.ok(resolve >= 0 && abort > resolve, 'invalid native startup must exit');
  assert.ok(context > abort, 'native startup must be validated before context creation');
  assert.ok(builder > context, 'native startup must be validated before building Tauri');
});
