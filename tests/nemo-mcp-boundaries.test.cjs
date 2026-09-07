'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { checkSourceSizes } = require('../scripts/nemo/lib/boundaries-size.cjs');
const rustProfile = require('../engineering/boundaries/profiles/mcp-rust.profile.json');
const applicationProfile = require('../engineering/boundaries/profiles/app-js.profile.json');
const { checkProfile } = require('../scripts/nemo/lib/boundaries.cjs');
const root = path.join(__dirname, '..');

function rustFiles(dir) {
  return fs.readdirSync(path.join(root, dir), { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? rustFiles(`${dir}/${entry.name}`) : entry.name.endsWith('.rs') ? [`${dir}/${entry.name}`] : []);
}
test('all production MCP Rust modules have actual size enforcement without legacy waivers', () => {
  const declared = rustProfile.modules.flatMap(module => module.files.map(file => `${module.dir}/${file}`)).sort();
  const discovered = [...rustFiles('nemo-mcp/src'), 'nemo-mcp/build.rs', 'src-tauri/src/application_mcp.rs'].sort();
  assert.deepEqual(declared, discovered);
  assert.deepEqual(rustProfile.exceptions, []);
  const result = checkSourceSizes(rustProfile, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
});

test('opacity application slice declares domain, application, bootstrap and native-adapter boundaries', () => {
  const expected = [
    ['app.opacity.domain', 'domain', 'src/js/domain/animation/opacity.js', 'Domain kernel'],
    ['app.opacity.application', 'application', 'src/js/application/opacity-application.js', 'Domain/application'],
    ['app.opacity.bootstrap', 'bootstrap', 'src/js/bootstrap/opacity-application.js', 'Handwritten config/bootstrap'],
    ['app.application.mcp.adapter', 'adapters', 'src/js/adapters/application-mcp.js', 'Platform/engine adapter'],
  ];
  const modules = expected.map(([id, layer, file, sizeProfile]) => {
    const module = applicationProfile.modules.find(candidate => candidate.id === id);
    assert.ok(module, `missing ${id}`);
    assert.equal(module.layer, layer);
    assert.equal(module.sizeProfile, sizeProfile);
    assert.deepEqual(module.files.map(name => `${module.dir}/${name}`), [file]);
    assert.deepEqual(module.publicApi, module.files);
    return module;
  });
  const result = checkProfile({ ...applicationProfile, modules, exceptions: [] }, { root });
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
