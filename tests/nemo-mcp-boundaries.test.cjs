'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { checkSourceSizes } = require('../scripts/nemo/lib/boundaries-size.cjs');
const profile = require('../engineering/boundaries/profiles/mcp-rust.profile.json');
const root = path.join(__dirname, '..');

function rustFiles(dir) {
  return fs.readdirSync(path.join(root, dir), { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? rustFiles(`${dir}/${entry.name}`) : entry.name.endsWith('.rs') ? [`${dir}/${entry.name}`] : []);
}
test('all production MCP Rust modules have actual size enforcement without legacy waivers', () => {
  const declared = profile.modules.flatMap(module => module.files.map(file => `${module.dir}/${file}`)).sort();
  const discovered = [...rustFiles('nemo-mcp/src'), 'nemo-mcp/build.rs', 'src-tauri/src/application_mcp.rs'].sort();
  assert.deepEqual(declared, discovered);
  assert.deepEqual(profile.exceptions, []);
  const result = checkSourceSizes(profile, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
});

test('MCP transport crate cannot acquire the desktop shell as a dependency', () => {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--offline', '--no-deps',
    '--format-version', '1', '--manifest-path', path.join(root, 'nemo-mcp/Cargo.toml')], { encoding: 'utf8' }));
  const transport = metadata.packages.find(pkg => pkg.name === 'nemo-mcp');
  assert.ok(transport);
  // Cargo resolves renamed dependencies and target-specific declarations.
  assert.deepEqual(transport.dependencies.filter(dep => ['tauri', 'nemo'].includes(dep.name)), []);
});
