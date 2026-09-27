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
test('all MCP transport and desktop-host Rust modules have actual size enforcement without legacy waivers', () => {
  const declared = rustProfile.modules.flatMap(module => module.files.map(file => `${module.dir}/${file}`)).sort();
  const discovered = [
    ...rustFiles('nemo-mcp/src'),
    'nemo-mcp/build.rs',
    'src-tauri/src/application_mcp.rs',
    'src-tauri/src/application_mcp_tests.rs',
    'src-tauri/src/native_revision_sync.rs',
    'src-tauri/src/native_revision_sync_tests.rs',
  ].sort();
  assert.deepEqual(declared, discovered);
  assert.deepEqual(rustProfile.exceptions, []);
  const result = checkSourceSizes(rustProfile, { root });
  assert.equal(result.ok, true, JSON.stringify(result.violations));
});

test('opacity application slice declares contract, lifecycle, operations, bootstrap and adapter boundaries', () => {
  const expected = [
    ['app.opacity.domain', 'domain', 'src/js/domain/animation/opacity.js', 'Domain kernel'],
    ['app.native.opacity.contract', 'domain', 'src/js/application/native-opacity-contract.js', 'Domain/application'],
    ['app.native.opacity.lifecycle', 'application', 'src/js/application/native-opacity-lifecycle.js', 'Domain/application'],
    ['app.native.opacity.operations', 'application', 'src/js/application/native-opacity-operations.js', 'Domain/application'],
    ['app.opacity.application', 'application', 'src/js/application/opacity-application.js', 'Domain/application'],
    ['app.native.opacity.legacy-surface.adapter', 'adapters', 'src/js/adapters/native-opacity-legacy-surface.js', 'Platform/engine adapter'],
    ['app.native.opacity.motion-surface.adapter', 'adapters', 'src/js/adapters/native-opacity-motion-surface.js', 'Platform/engine adapter'],
    ['app.motion.canvas-intent.adapter', 'adapters', 'src/js/adapters/motion-canvas-intent.js', 'Platform/engine adapter'],
    ['app.select.canvas-intent.adapter', 'adapters', 'src/js/adapters/select-canvas-intent.js', 'Platform/engine adapter'],
    ['app.opacity.bootstrap', 'bootstrap', 'src/js/bootstrap/opacity-application.js', 'Handwritten config/bootstrap'],
    ['app.native.opacity.bootstrap', 'bootstrap', 'src/js/bootstrap/native-opacity-application.js', 'Handwritten config/bootstrap'],
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
  const size = checkSourceSizes({ ...applicationProfile, modules, exceptions: [] }, { root });
  assert.equal(size.ok, true, JSON.stringify(size.violations));
});

test('component exposed-property domain has its own exact public, load and no-exception boundary', () => {
  const module = applicationProfile.modules.find((candidate) => candidate.id === 'app.component.exposed-properties.domain');
  assert.ok(module);
  assert.equal(module.layer, 'domain'); assert.equal(module.sizeProfile, 'Domain kernel');
  assert.equal(module.dir, 'src/js/domain/component');
  assert.deepEqual(module.files, ['exposed-properties.js']); assert.deepEqual(module.publicApi, module.files);
  const sourcePath = `${module.dir}/${module.files[0]}`;
  const policy = require('../engineering/boundaries/profiles/app-js.coverage.json');
  const entry = policy.retainedSources.find((record) => record.path === sourcePath);
  assert.equal(entry.moduleId, module.id); assert.equal(entry.executionClass, 'document-classic');
  const html = fs.readFileSync(path.join(root, 'src/index.html'), 'utf8').split('\n');
  let ordinal = 0;
  html.forEach((line, index) => {
    if (!line.includes('<script')) return;
    ordinal++;
    if (!line.includes('js/domain/component/exposed-properties.js')) return;
    assert.deepEqual(entry.loadSites, [{ path: 'src/index.html', line: index + 1, scriptOrdinal: ordinal }]);
    assert.equal(html[index + 1], '<script src="js/app.js"></script>');
  });
  const isolated = { ...applicationProfile, modules: [module], exceptions: [] };
  for (const result of [checkProfile(isolated, { root }), checkSourceSizes(isolated, { root })]) {
    assert.equal(result.ok, true, JSON.stringify(result.violations));
  }
});

test('MCP transport crate cannot acquire the desktop shell as a dependency', () => {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--offline', '--no-deps',
    '--format-version', '1', '--manifest-path', path.join(root, 'nemo-mcp/Cargo.toml')], { encoding: 'utf8' }));
  const transport = metadata.packages.find(pkg => pkg.name === 'nemo-mcp');
  assert.ok(transport);
  // Cargo resolves renamed dependencies and target-specific declarations.
  assert.deepEqual(
    transport.dependencies.filter(dep => ['tauri', 'nemo', 'nemo-native-engine'].includes(dep.name)),
    [],
  );
});
