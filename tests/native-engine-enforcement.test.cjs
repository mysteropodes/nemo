'use strict';

// N19 (#1351) — native-engine boundaries and normal local validation.
// Positive assertions pin the adopted graph and Cargo/module mapping. Negative
// controls run only in an isolated local clone, then prove both clone and source
// bytes were restored, so validation never edits the working candidate.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { checkRustCrate } = require('../scripts/nemo/lib/boundaries-rust.cjs');
const { checkSourceCoverage } = require('../scripts/nemo/lib/boundaries-coverage.cjs');
const { checkRustSourceCoverage } = require('../scripts/nemo/lib/boundaries-rust-vendor.cjs');
const { discoverRepositoryFiles } = require('../scripts/nemo/lib/boundaries-repository.cjs');
const { JOBS, PROFILES } = require('../scripts/nemo/lib/jobs.cjs');
const ci = require('../scripts/nemo/ci.cjs');

const ROOT = path.resolve(__dirname, '..');
const PROFILE = 'engineering/boundaries/profiles/rust.profile.json';
const POLICY = 'engineering/boundaries/profiles/native-engine.edges.json';
const CANDIDATE_FILES = [
  POLICY,
  PROFILE,
  'engineering/boundaries/profiles/rust.coverage.json',
  'engineering/boundaries/profiles/rust.vendor.json',
  'native-engine/Cargo.toml',
  'native-engine/src/lib.rs',
  'native-engine/src/commands.rs',
  'native-engine/src/history.rs',
  'native-engine/src/export_job.rs',
  'native-engine/src/object_export_host.rs',
  'native-engine/src/export_job_lifecycle.rs',
  'native-engine/tests/export_job_lifecycle.rs',
  'native-engine/src/request_receipts.rs',
  'native-engine/src/revision.rs',
  'native-engine/src/read_queries.rs',
  'native-engine/src/reference_image_resources.rs',
  'native-engine/tests/reference_image_resources.rs',
  'native-engine/tests/reference_png_rows.rs',
  'native-engine/src/resource_leases.rs',
  'native-engine/tests/object_resource_leases.rs',
  'native-engine/src/object_document.rs',
  'native-engine/src/project_media_envelope.rs',
  'native-engine/src/object_codec.rs',
  'native-engine/src/project_media_codec.rs',
  'native-engine/tests/project_media_codec.rs',
  'native-engine/tests/object_project_admission.rs',
  'native-engine/src/object_project_admission.rs',
  'native-engine/src/object_project_history.rs',
  'native-engine/src/object_reference_dispatch.rs',
  'native-engine/tests/object_project_history.rs',
  'native-engine/tests/project_media_workflow.rs',
  'native-engine/tests/object_reference_dispatch.rs',
  'native-engine/src/reference_image_asset.rs',
  'native-engine/src/reference_png_scanlines.rs',
  'native-engine/tests/reference_image_asset.rs',
  'native-engine/tests/reference_png_scanlines.rs',
  'native-engine/src/object_snapshot.rs',
  'native-engine/src/object_reference_revision.rs',
  'native-engine/src/object_frame_packet.rs',
  'native-engine/src/object_expression_pin.rs',
  'native-engine/tests/object_expression_pin.rs',
  'native-engine/src/object_expression_runtime.rs',
  'native-engine/tests/object_expression_runtime.rs',
  'native-engine/tests/object_frame_packet.rs',
  'native-engine/src/object_render_scene.rs',
  'native-engine/tests/object_render_scene.rs',
  'native-engine/src/object_compositor.rs',
  'native-engine/tests/object_compositor.rs',
  'native-engine/src/object_fill.rs',
  'native-engine/src/object_fill_owner.rs',
  'native-engine/src/object_property_curve_edits.rs',
  'native-engine/src/object_property_track_edit.rs',
  'native-engine/tests/object_property_curve_edits.rs',
  'native-engine/tests/object_property_track_edit.rs',
  'native-engine/src/object_revision.rs',
  'native-engine/src/encoded_retention_budget.rs',
  'native-engine/src/image_resource_budget.rs',
  'native-engine/tests/image_resource_budget.rs',
  'native-engine/src/object_history.rs',
  'native-engine/tests/object_history.rs',
  'native-engine/tests/object_codec.rs',
  'native-engine/tests/object_read.rs',
  'native-engine/tests/object_fill.rs',
  'native-engine/tests/application_read.rs',
  'native-engine/src/application_replacement.rs',
  'native-engine/src/application_construction.rs',
  'native-engine/src/application.rs',
  'native-engine/src/application_diagnostics.rs',
  'native-engine/src/application_reproduction.rs',
  'native-engine/src/reproduction_bundle.rs',
  'native-engine/src/reproduction_replay.rs',
  'native-engine/src/reproduction_catalog.rs',
  'native-engine/fixtures/reproduction-opacity-v1.json',
  'native-engine/tests/application_diagnostics.rs',
  'native-engine/tests/application_replacement.rs',
  'native-engine/tests/application_reproduction.rs',
  'native-engine/tests/reproduction_lifecycle.rs',
  'native-engine/tests/reproduction_decode.rs',
  'native-engine/tests/reproduction_replay.rs',
  'native-engine/tests/reproduction_catalog.rs',
  'src-tauri/src/native_object_desktop_preview_extent_tests.rs',
  'src-tauri/src/native_object_desktop_export_extent_tests.rs',
  'src-tauri/src/native_object_reference.rs',
  'src-tauri/src/native_object_project_route.rs',
  'src-tauri/src/native_object_project_tests.rs',
  'src-tauri/src/native_object_reference_tests.rs',
  'tests/native-engine-enforcement.test.cjs',
  'scripts/nemo/ci.cjs',
  'scripts/nemo/ci.test.cjs',
  'scripts/nemo/lib/jobs.cjs',
  'scripts/nemo/lib/rust-test-jobs.cjs',
  'package.json',
  'engineering/inventory/baseline-manifest.json',
  'engineering/inventory/surfaces.json',
  'engineering/inventory/surfaces.csv',
  'engineering/inventory/SURFACES.md',
  'engineering/boundaries/profiles/app-js.coverage.json',
  'engineering/boundaries/profiles/app-js.profile.json',
  'engineering/remediation/EXECUTION_PLAN.en.md',
  'engineering/remediation/EXECUTION_PLAN.fr.md',
];

const EXPECTED_MODULES = [
  'rust.native.engine', 'rust.native.engine.document', 'rust.native.engine.codec',
  'rust.native.engine.request-receipts', 'rust.native.engine.revision', 'rust.native.engine.commands',
  'rust.native.engine.history', 'rust.native.engine.evaluation', 'rust.native.engine.resource-leases',
  'rust.native.engine.scheduler', 'rust.native.engine.render-scene', 'rust.native.engine.compositor',
  'rust.native.engine.desktop-viewport', 'rust.native.engine.png-output', 'rust.native.engine.export-job',
  'rust.native.engine.protocol', 'rust.native.engine.application',
  'rust.native.engine.read-queries',
];

const EXPECTED_EDGES = [
  'application->codec', 'application->commands', 'application->document', 'application->export-job', 'application->history',
  'application->evaluation', 'application->protocol', 'application->render-scene', 'application->request-receipts', 'application->revision',
  'codec->document', 'commands->document', 'commands->request-receipts', 'commands->revision',
  'compositor->render-scene', 'desktop-viewport->compositor', 'desktop-viewport->render-scene',
  'desktop-viewport->scheduler', 'evaluation->document', 'evaluation->revision', 'export-job->history',
  'export-job->compositor', 'export-job->png-output', 'export-job->render-scene', 'export-job->resource-leases',
  'export-job->revision', 'export-job->scheduler', 'history->commands', 'history->document',
  'history->request-receipts', 'history->revision', 'png-output->compositor', 'png-output->render-scene',
  'protocol->commands', 'protocol->export-job', 'render-scene->evaluation', 'render-scene->read-queries', 'render-scene->scheduler',
  'resource-leases->revision', 'revision->document', 'root->application', 'root->codec',
  'scheduler->resource-leases', 'scheduler->revision',
  'commands->read-queries', 'read-queries->codec', 'read-queries->evaluation',
  'read-queries->request-receipts', 'read-queries->revision',
].sort();

const PRODUCTION = {
  codec: ['codec', 'document', 'project_structure', 'object_document', 'object_codec'],
  commands: ['commands', 'request_receipts', 'revision', 'object_snapshot'],
  history: ['history', 'transaction'],
  evaluation: ['evaluation', 'animation_curve', 'object_frame_packet'],
  scheduler: ['resource_leases', 'scheduler'],
  compositor: ['compositor', 'render_scene', 'render_geometry', 'object_render_scene'],
  viewport: ['desktop_viewport'],
  export_job: ['export_job', 'export_job_lifecycle', 'png_output'],
  application: ['application', 'protocol', 'read_queries'],
};

const TEST_TARGETS = {
  codec: '../tests/codec.rs', commands: '../tests/commands.rs', history: '../tests/history.rs',
  evaluation: '../tests/evaluation.rs', scheduler: '../tests/scheduler.rs', compositor: '../tests/compositor.rs',
  viewport: '../tests/desktop_viewport.rs', export_job: '../tests/export_job.rs', application: '../tests/application.rs',
};
const ENGINE_MACRO = '($($feature:literal => $($module:ident),+;)*) => { $( $( #[cfg(feature = $feature)] pub mod $module; )+ )* };';
const TEST_MACRO = '($($feature:literal => $module:ident = $path:literal;)*) => { $( #[cfg(feature = $feature)] #[path = $path] mod $module; )* };';

function readJson(root, relative) { return JSON.parse(fs.readFileSync(path.join(root, relative), 'utf8')); }

function section(text, start) {
  const at = text.indexOf(start);
  assert.notEqual(at, -1, `${start} missing`);
  const rest = text.slice(at + start.length);
  const next = rest.search(/^\s*\[/m);
  return next === -1 ? rest : rest.slice(0, next);
}

function macroMappings(text, name) {
  const match = new RegExp(`${name}!\\s*\\{([\\s\\S]*?)\\n\\}`).exec(text);
  assert.ok(match, `${name}! invocation missing`);
  const result = {};
  for (const entry of match[1].matchAll(/"([^"]+)"\s*=>\s*([^;]+);/g)) {
    (result[entry[1]] ||= []).push(...entry[2].split(',').map((item) => item.trim()));
  }
  return result;
}

function macroDefinition(text, name) {
  const at = text.indexOf(`macro_rules! ${name}`);
  assert.notEqual(at, -1, `${name} definition missing`);
  const open = text.indexOf('{', at);
  let depth = 0, close = -1;
  for (let i = open; i < text.length; i++) {
    if (text[i] === '{') depth++;
    else if (text[i] === '}' && --depth === 0) { close = i; break; }
  }
  assert.notEqual(close, -1, `${name} definition is unclosed`);
  return text.slice(open + 1, close).replace(/\s+/g, ' ').trim();
}

function assertMacroDefinitions(text) {
  assert.equal(macroDefinition(text, 'engine_modules'), ENGINE_MACRO, 'engine_modules definition drift');
  assert.equal(macroDefinition(text, 'test_modules'), TEST_MACRO, 'test_modules definition drift');
}

function cargoContract(text) {
  const features = [...section(text, '[features]').matchAll(/^([A-Za-z0-9_-]+)\s*=/gm)].map((match) => match[1]);
  const targets = {};
  for (const match of text.matchAll(/\[\[test\]\]([\s\S]*?)(?=\n\[\[test\]\]|$)/g)) {
    const name = /^name\s*=\s*"([^"]+)"/m.exec(match[1])?.[1];
    targets[name] = {
      path: /^path\s*=\s*"([^"]+)"/m.exec(match[1])?.[1],
      feature: /^required-features\s*=\s*\["([^"]+)"\]/m.exec(match[1])?.[1],
    };
  }
  return { features, targets };
}

function cargoFeatureMembers(text) {
  const members = {};
  for (const match of section(text, '[features]').matchAll(/^([A-Za-z0-9_-]+)\s*=\s*\[([^\]\n]*)\]$/gm)) {
    assert.match(match[2], /^\s*(?:"[^"]+"(?:\s*,\s*"[^"]+")*)?\s*$/, `malformed feature list: ${match[1]}`);
    members[match[1]] = [...match[2].matchAll(/"([^"]+)"/g)].map((entry) => entry[1]);
  }
  return members;
}

function snapshot(root, files) {
  return new Map(files.map((relative) => [relative, fs.existsSync(path.join(root, relative))
    ? fs.readFileSync(path.join(root, relative)) : null]));
}

function restore(root, bytes) {
  for (const [relative, value] of bytes) {
    const target = path.join(root, relative);
    if (value === null) fs.rmSync(target, { force: true });
    else { fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, value); }
  }
}

function copyCandidate(root) {
  for (const relative of CANDIDATE_FILES) {
    const target = path.join(root, relative);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(path.join(ROOT, relative), target);
  }
}

function nativeResult(root) {
  const result = ci.rustBoundaries(root);
  return result.crates.find((crate) => crate.crate === 'nemo-native-engine');
}

test('native-engine policy adopts the exact clean internal graph with no exceptions', () => {
  const profile = readJson(ROOT, PROFILE), policy = readJson(ROOT, POLICY);
  assert.deepEqual(policy.profileModules, EXPECTED_MODULES);
  assert.deepEqual(policy.exceptions, []);
  const result = checkRustCrate(profile, policy, { root: ROOT });
  assert.equal(result.ok, true, JSON.stringify(result.violations, null, 2));
  assert.equal(result.moduleCount, 18);
  assert.equal(result.edges.length, 49);
  assert.deepEqual(result.exceptionsApplied, []);
  assert.deepEqual(result.unsupported, []);
  const short = (id) => id.replace('rust.native.engine.', '').replace('rust.native.engine', 'root');
  assert.deepEqual(result.edges.map((edge) => `${short(edge.from)}->${short(edge.to)}`).sort(), EXPECTED_EDGES);
});


test('desktop extent test children stay private and missing membership fails closed', (t) => {
  const files = ['native_object_desktop_preview_extent_tests.rs', 'native_object_desktop_export_extent_tests.rs'];
  const id = 'rust.desktop.native.host.release.tests';
  const profile = readJson(ROOT, PROFILE);
  const owner = profile.modules.find((entry) => entry.id === id);
  assert.match(readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json').provenance.nativeObjectDesktopExtentRegistration, /F04\/#2080/);
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-extent-membership-'));
  const clone = path.join(temp, 'repo');
  t.after(() => fs.rmSync(temp, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));
  const cloned = spawnSync('git', ['clone', '--quiet', '--local', ROOT, clone], { encoding: 'utf8' });
  assert.equal(cloned.status, 0, cloned.stderr);
  copyCandidate(clone);
  const original = fs.readFileSync(path.join(clone, PROFILE));
  const discovered = discoverRepositoryFiles({ root: clone });
  assert.equal(discovered.ok, true);
  assert.deepEqual(discovered.diagnostics, []);
  const selected = discovered.entries
    .filter((entry) => entry.path.endsWith('.rs')).map((entry) => entry.path);
  const exclusions = readJson(clone, 'engineering/boundaries/profiles/rust.coverage.json').exclusions;
  const coverage = (candidate) => checkRustSourceCoverage(candidate, { root: clone, sourcePaths: selected, exclusions });
  assert.deepEqual(coverage(profile).violations, []);
  for (const file of files) {
    assert.equal(owner.files.filter((entry) => entry === file).length, 1);
    assert.equal(owner.publicApi.includes(file), false);
    const relative = `src-tauri/src/${file}`;
    assert.ok(CANDIDATE_FILES.includes(relative));
    const source = fs.readFileSync(path.join(clone, relative));
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((entry) => entry !== file);
    fs.writeFileSync(path.join(clone, PROFILE), JSON.stringify(missing));
    const result = coverage(readJson(clone, PROFILE));
    assert.deepEqual(result.violations.map((entry) => [entry.rule, entry.file]), [['coverage-unprofiled-source', relative]]);
    fs.writeFileSync(path.join(clone, PROFILE), original);
    assert.deepEqual(fs.readFileSync(path.join(clone, relative)), source);
    assert.deepEqual(coverage(readJson(clone, PROFILE)).violations, []);
  }
  assert.deepEqual(fs.readFileSync(path.join(clone, PROFILE)), original);
});

test('Cargo features, targets and declaration macros are one exact contract', () => {
  const cargoText = fs.readFileSync(path.join(ROOT, 'native-engine/Cargo.toml'), 'utf8');
  const rootText = fs.readFileSync(path.join(ROOT, 'native-engine/src/lib.rs'), 'utf8');
  const compositorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/compositor.rs'), 'utf8');
  const objectCompositorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_compositor.rs'), 'utf8');
  const policy = readJson(ROOT, POLICY), cargo = cargoContract(cargoText);
  assert.deepEqual(cargo.features, policy.features.declared);
  const members = cargoFeatureMembers(cargoText);
  assert.deepEqual(Object.keys(members), cargo.features);
  assert.deepEqual(members.default, []);
  assert.deepEqual(members.codec, ['dep:serde', 'dep:serde_json']);
  assert.deepEqual(members.reference_image_asset, ['dep:png', 'dep:flate2', 'dep:miniz_oxide']);
  assert.match(cargoText, /^flate2 = \{ version = "=1\.1\.10", default-features = false, features = \["rust_backend"\], optional = true \}$/m);
  assert.match(cargoText, /^miniz_oxide = \{ version = "=0\.9\.1", default-features = false, optional = true \}$/m);
  assert.deepEqual(members.project_media_codec, ['codec', 'reference_image_asset', 'dep:base64']);
  assert.deepEqual(members['test-codec'], ['project_media_codec']);
  assert.deepEqual(members['test-compositor'], ['compositor']);
  assert.deepEqual(members['test-application'], ['application', 'project_media_codec']);
  assert.deepEqual(members.object_expression_pin, ['history', 'evaluation']);
  assert.deepEqual(members['test-object_expression_pin'], ['object_expression_pin']);
  assert.deepEqual(members.object_expression_runtime, ['object_expression_pin', 'dep:rquickjs']);
  assert.deepEqual(members['test-object_expression_runtime'], ['object_expression_runtime']);
  assert.match(cargoText, /^rquickjs = \{ version = "=0\.14\.0", default-features = false, features = \["std"\], optional = true \}$/m);
  const reaches = (feature, target, graph = members, seen = new Set()) => {
    if (feature === target) return true;
    if (seen.has(feature)) return false;
    seen.add(feature);
    return (graph[feature] || []).some((member) => graph[member] && reaches(member, target, graph, seen));
  };
  for (const feature of cargo.features.filter((name) => name !== 'reference_image_asset')) {
    assert.equal(reaches(feature, 'reference_image_asset'),
      feature === 'project_media_codec' || feature === 'test-codec' || feature === 'test-application',
      `${feature} must not activate the private PNG selector`);
  }
  for (const feature of cargo.features.filter((name) => name !== 'project_media_codec')) {
    assert.equal(reaches(feature, 'project_media_codec'), feature === 'test-codec' || feature === 'test-application',
      `${feature} must not activate the private attached-codec selector`);
  }
  for (const feature of cargo.features.filter((name) => name !== 'object_expression_pin')) {
    assert.equal(reaches(feature, 'object_expression_pin'),
      ['test-object_expression_pin', 'object_expression_runtime', 'test-object_expression_runtime'].includes(feature),
      `${feature} must not activate the private expression-pin selector`);
  }
  for (const feature of cargo.features.filter((name) => name !== 'object_expression_runtime')) {
    assert.equal(reaches(feature, 'object_expression_runtime'), feature === 'test-object_expression_runtime',
      `${feature} must not activate the private expression-runtime selector`);
  }
  assert.equal(reaches('codec', 'reference_image_asset', { ...members,
    codec: [...members.codec, 'reference_image_asset'] }), true, 'feature-isolation mutant must be detected');
  assert.match(rootText, /#\[cfg\(feature = "reference_image_asset"\)\]\s*pub\(crate\) mod reference_image_asset;/);
  assert.equal((rootText.match(/pub\(crate\) mod reference_image_asset;/g) || []).length, 1);
  assert.match(rootText, /#\[cfg\(feature = "object_expression_pin"\)\]\s*pub\(crate\) mod object_expression_pin;/);
  assert.equal((rootText.match(/pub\(crate\) mod object_expression_pin;/g) || []).length, 1);
  assert.match(rootText, /#\[cfg\(all\(test, feature = "test-object_expression_pin"\)\)\]\s*#\[path = "\.\.\/tests\/object_expression_pin\.rs"\]\s*mod object_expression_pin_tests;/);
  assert.match(rootText, /#\[cfg\(feature = "object_expression_runtime"\)\]\s*pub\(crate\) mod object_expression_runtime;/);
  assert.equal((rootText.match(/pub\(crate\) mod object_expression_runtime;/g) || []).length, 1);
  assert.match(rootText, /#\[cfg\(all\(test, feature = "test-object_expression_runtime"\)\)\]\s*#\[path = "\.\.\/tests\/object_expression_runtime\.rs"\]\s*mod object_expression_runtime_tests;/);
  assert.match(compositorText, /#\[path = "object_compositor\.rs"\]\s*mod object_compositor;/);
  assert.match(objectCompositorText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_compositor\.rs"\]\s*mod tests;/);
  assertMacroDefinitions(rootText);
  assert.deepEqual(macroMappings(rootText, 'engine_modules'), PRODUCTION);
  const testMacros = macroMappings(rootText, 'test_modules');
  assert.deepEqual(Object.keys(testMacros), Object.keys(TEST_TARGETS).map((name) => `test-${name}`));
  for (const [name, source] of Object.entries(TEST_TARGETS)) {
    const expected = [`${name}_tests = "${source}"`];
    if (name === 'codec') expected.push('project_structure_tests = "../tests/project_structure.rs"',
      'object_codec_tests = "../tests/object_codec.rs"',
      'reference_image_asset_tests = "../tests/reference_image_asset.rs"');
    if (name === 'commands') expected.push('object_read_tests = "../tests/object_read.rs"',
      'object_fill_tests = "../tests/object_fill.rs"');
    if (name === 'history') expected.push('object_history_tests = "../tests/object_history.rs"');
    if (name === 'evaluation') expected.push('object_frame_packet_evaluation_tests = "../tests/object_frame_packet.rs"',
      'expression_noise_tests = "../tests/expression_noise.rs"',
      'expression_random_tests = "../tests/expression_random.rs"',
      'expression_math_tests = "../tests/expression_math.rs"',
      'expression_time_tests = "../tests/expression_time.rs"',
      'motion_parent_chain_tests = "../tests/motion_parent_chain.rs"',
      'motion_parent_chain_controls_tests = "../tests/motion_parent_chain_controls.rs"',
      'property_expression_runtime_tests = "../tests/property_expression_runtime.rs"',
      'property_expression_runtime_controls_tests = "../tests/property_expression_runtime_controls.rs"',
      'property_transform_eval_tests = "../tests/property_transform_eval.rs"',
      'property_transform_eval_controls_tests = "../tests/property_transform_eval_controls.rs"',
      'property_follow_path_sample_tests = "../tests/property_follow_path_sample.rs"',
      'property_follow_path_sample_controls_tests = "../tests/property_follow_path_sample_controls.rs"',
      'property_expression_views_tests = "../tests/property_expression_views.rs"',
      'property_expression_views_controls_tests = "../tests/property_expression_views_controls.rs"');
    if (name === 'compositor') expected.push('render_geometry_tests = "../tests/render_geometry.rs"',
      'object_render_scene_compositor_tests = "../tests/object_render_scene.rs"');
    if (name === 'export_job') expected.push('authored_curve_consumers_tests = "../tests/authored_curve_consumers.rs"',
      'export_geometry_tests = "../tests/render_geometry.rs"',
      'export_job_lifecycle_tests = "../tests/export_job_lifecycle.rs"');
    if (name === 'application') expected.push('application_read_tests = "../tests/application_read.rs"',
      'application_replacement_tests = "../tests/application_replacement.rs"',
      'object_application_read_tests = "../tests/object_read.rs"',
      'object_frame_packet_application_tests = "../tests/object_frame_packet.rs"',
      'object_render_scene_application_tests = "../tests/object_render_scene.rs"');
    assert.deepEqual(testMacros[`test-${name}`], expected);
    assert.deepEqual(cargo.targets[name], { path: 'src/lib.rs', feature: `test-${name}` });
  }
  assert.deepEqual(cargo.targets.object_expression_pin,
    { path: 'src/lib.rs', feature: 'test-object_expression_pin' });
  assert.deepEqual(cargo.targets.object_expression_runtime,
    { path: 'src/lib.rs', feature: 'test-object_expression_runtime' });
  assert.deepEqual(Object.keys(cargo.targets), [...Object.keys(TEST_TARGETS), 'object_expression_pin', 'object_expression_runtime']);
  assert.match(fs.readFileSync(path.join(ROOT, 'scripts/nemo/lib/rust-test-jobs.cjs'), 'utf8'),
    /const targets = \[[^\n]*'application', 'object_expression_pin', 'object_expression_runtime'\];/);
  const privateModules = rootText.replace('pub mod $module;', 'mod $module;');
  assert.notEqual(privateModules, rootText, 'engine module visibility token was not found');
  assert.throws(() => assertMacroDefinitions(privateModules), /engine_modules definition drift/);
});

test('native-engine validation is required in normal graphs and staged adapters stay covered', () => {
  assert.equal(JOBS['test:rust-native-engine']?.required, true);
  assert.ok(PROFILES.quick.includes('test:rust-native-engine'));
  assert.ok(PROFILES.full.includes('test:rust-native-engine'));
  assert.ok(PROFILES.quick.includes('test:unit') && PROFILES.full.includes('test:unit'));
  assert.ok(ci.QUICK.includes('test:rust-native-engine') && ci.QUICK.includes('test:unit'));
  assert.equal(readJson(ROOT, 'package.json').scripts['test:rust-native-engine'],
    'node scripts/nemo/job.cjs test:rust-native-engine');
  for (const file of ['native-viewport-adapter.test.cjs', 'native-application-transport.test.cjs',
    'native-opacity-editor-adapter.test.cjs', 'native-opacity-render-export-adapter.test.cjs']) {
    assert.equal(fs.existsSync(path.join(ROOT, 'tests', file)), true, `${file} missing from test:unit discovery`);
  }
});

test('P37C1C4 retained-reference CPU, PNG, and managed-wire children have exact private source coverage', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  const children = [
    ['rust.native.engine.revision', 'native-engine/src/image_resource_budget.rs'],
    ['rust.native.engine.read-queries', 'native-engine/src/reference_image_resources.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/image_resource_budget.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/reference_image_resources.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/reference_png_rows.rs'],
    ['rust.native.engine.codec', 'native-engine/src/reference_png_scanlines.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/reference_png_scanlines.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_project_admission.rs'],
  ];
  assert.match(coverage.provenance.nativeReferenceCpuPayloadBudgetRegistration, /P37C1C4A\/#1843/);
  assert.match(coverage.provenance.nativeReferencePngScanlineStrictnessRegistration, /P37C1C4B\/#1848/);
  assert.match(coverage.provenance.nativeReferenceInflateWorkspaceRegistration, /P37C1C4C\/#1851/);
  assert.match(coverage.provenance.nativeManagedProjectWireWorkspaceRegistration, /P37C1C4D\/#1856/);
  assert.match(coverage.provenance.nativeReferencePngRowOutputRegistration, /P37C1C4E\/#1860/);
  for (const [id, file] of children) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.ok(owner?.files.includes(path.basename(file)), `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('N25F3C2A1P expression pins remain private and fail closed without source registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.provenance.nativeObjectExpressionPinRegistration, /N25F3C2A1P\/#1700/);
  for (const [id, file] of [
    ['rust.native.engine.application', 'native-engine/src/object_expression_pin.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_expression_pin.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.ok(owner?.files.includes(path.basename(file)), `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('N25F3C2A1R expression runtime remains private and fails closed without source registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.provenance.nativeObjectExpressionRuntimeRegistration, /N25F3C2A1R\/#1701/);
  for (const [id, file] of [
    ['rust.native.engine.application', 'native-engine/src/object_expression_runtime.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_expression_runtime.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.ok(owner?.files.includes(path.basename(file)), `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('P37C1-DESKTOP project history children are private and fail closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.provenance.nativeStillReferenceHistoryWorkflowRegistration, /P37C1-DESKTOP\/#1864/);
  for (const [id, file] of [
    ['rust.native.engine.revision', 'native-engine/src/object_reference_revision.rs'],
    ['rust.native.engine.read-queries', 'native-engine/src/object_project_admission.rs'],
    ['rust.native.engine.history', 'native-engine/src/object_project_history.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_project_history.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/project_media_workflow.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.ok(owner?.files.includes(path.basename(file)), `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('P37C1-DESKTOP host and reference dispatch children remain private and registered', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.provenance.nativeStillReferenceDesktopRouteRegistration, /P37C1-DESKTOP\/#1864/);
  for (const [id, file] of [
    ['rust.native.engine.history', 'native-engine/src/object_reference_dispatch.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_reference_dispatch.rs'],
    ['rust.desktop.native.application.commands', 'src-tauri/src/native_object_reference.rs'],
    ['rust.desktop.native.application.commands', 'src-tauri/src/native_object_project_route.rs'],
    ['rust.desktop.native.host.release.tests', 'src-tauri/src/native_object_project_tests.rs'],
    ['rust.desktop.native.host.release.tests', 'src-tauri/src/native_object_reference_tests.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.ok(owner?.files.includes(path.basename(file)), `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('D12 property curve children remain registered and private', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.provenance.nativePropertyTrackEditRegistration, /D12\/#2094/);
  for (const [id, dir] of [
    ['rust.native.engine.commands', 'src'], ['rust.native.engine.tests', 'tests'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    for (const file of ['object_property_curve_edits.rs', 'object_property_track_edit.rs']) {
      assert.equal(owner.files.filter((name) => name === file).length, 1);
      assert.equal(owner.publicApi.includes(file), false);
      assert.ok(CANDIDATE_FILES.includes(`native-engine/${dir}/${file}`));
    }
  }
});

test('D25 camera kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativePropertyCameraEvalRegistration, /D25\/#2095/);
  const evaluationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(evaluationText, /#\[path = "property_camera_eval\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod property_camera_eval;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/property_camera_eval.rs'), 'utf8');
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/property_camera_eval\.rs"\]\s*mod tests;/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/property_camera_eval.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/property_camera_eval.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('D31 3D projection kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativePropertyProjection3dRegistration, /D31\/#2096/);
  const evaluationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(evaluationText, /#\[path = "property_projection_3d\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod property_projection_3d;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/property_projection_3d.rs'), 'utf8');
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/property_projection_3d\.rs"\]\s*mod tests;/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/property_projection_3d.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/property_projection_3d.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L09-A media timing video kernel stays a private document child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectMediaTimingRegistration, /L09-A\/#2126/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(documentText, /#\[path = "object_media_timing_core\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod media_timing;/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_media_timing_core.rs'), 'utf8');
  assert.match(coreText, /#\[path = "object_media_timing_video\.rs"\]\s*pub\(crate\) mod object_media_timing_video;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_media_timing_video.rs'), 'utf8');
  for (const [name, mod] of [['object_media_timing_video', 'tests'], ['object_media_timing_video_controls', 'controls'], ['object_media_timing_video_pins', 'pins']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_media_timing_core.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_media_timing_video.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_timing_video.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_timing_video_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_timing_video_pins.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L10-A montage timing kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectMontageTimingRegistration, /L10-A\/#2123/);
  const evaluationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(evaluationText, /#\[path = "object_montage_timing\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_montage_timing;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_montage_timing.rs'), 'utf8');
  for (const [name, mod] of [['object_montage_timing_rows', 'rows'], ['object_montage_timing', 'tests'], ['object_montage_timing_controls', 'controls']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_montage_timing.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_montage_timing.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_montage_timing_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_montage_timing_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('G13 path-effect kernel stays a private L01 aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativePathEffectsKernelRegistration, /L01-A\/#2129/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_vector_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_path_effects_kernel\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_path_effects_kernel;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_vector_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_vector_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_path_effects_kernel.rs'), 'utf8');
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_path_effects_kernel\.rs"\]\s*mod tests;/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_path_effects_kernel.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_path_effects_kernel.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_path_effects_kernel_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_path_effects_kernel_pins.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L02-A group hierarchy kernel stays a private document child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectGroupHierarchyRegistration, /L02-A\/#2117/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_group_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_group_kernels;/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_kernels.rs'), 'utf8');
  assert.match(documentText, /#\[path = "object_group_hierarchy\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_group_hierarchy;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_hierarchy.rs'), 'utf8');
  assert.match(kernelText, /#\[path = "object_group_hierarchy_selection\.rs"\]\s*mod selection;/);
  for (const name of ['object_group_hierarchy', 'object_group_hierarchy_controls', 'object_group_hierarchy_mutations']) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod `));
  }
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_group_hierarchy.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_group_hierarchy_selection.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_group_hierarchy.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_group_hierarchy_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_group_hierarchy_mutations.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L03-A image-mesh geometry kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectImageMeshGeometryRegistration, /L03-A\/#2138/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_image_mesh_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_image_mesh_kernels;/);
  const evaluationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_image_mesh_kernels.rs'), 'utf8');
  assert.match(evaluationText, /#\[path = "object_image_mesh_geometry\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_image_mesh_geometry;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_image_mesh_geometry.rs'), 'utf8');
  for (const child of ['object_image_mesh_geometry', 'object_image_mesh_geometry_rows', 'object_image_mesh_geometry_controls']) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${child}\\.rs"\\]\\s*mod [a-z_]+;`));
  }
  const oracle = fs.readFileSync(path.join(ROOT, 'native-engine/tests/fixtures/object-image-mesh-geometry/oracle.txt'), 'utf8');
  assert.equal(oracle.trim().split('\n').length, 710, 'oracle row count');
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_image_mesh_geometry.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_image_mesh_geometry.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_image_mesh_geometry_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_image_mesh_geometry_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L03-B widget geometry kernel stays a private image-mesh-family child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectWidgetGeometryRegistration, /L03-B\/#2139/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_image_mesh_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_widget_geometry\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_widget_geometry;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_widget_geometry.rs'), 'utf8');
  for (const [name, mod] of [['object_widget_geometry_rows', 'oracle_rows'], ['object_widget_geometry', 'tests'], ['object_widget_geometry_controls', 'controls_tests']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const oracle = fs.readFileSync(path.join(ROOT, 'native-engine/tests/fixtures/object-widget-geometry/oracle.txt'), 'utf8');
  assert.equal(oracle.trim().split('\n').length, 814, 'oracle row count');
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_widget_geometry.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_widget_geometry.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_widget_geometry_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_widget_geometry_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L04-A component playback kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectComponentPlaybackRegistration, /L04-A\/#2120/);
  // R4: the declaration lives in the L04 evaluation aggregator (name chosen by the lead) or, without one, in evaluation.rs.
  const declarationText = ['native-engine/src/object_component_kernels.rs', 'native-engine/src/evaluation.rs']
    .filter((file) => fs.existsSync(path.join(ROOT, file)))
    .map((file) => fs.readFileSync(path.join(ROOT, file), 'utf8'))
    .join('\n');
  assert.match(declarationText, /#\[path = "object_component_playback\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_component_playback;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_component_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_component_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_component_playback.rs'), 'utf8');
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_component_playback\.rs"\]\s*mod tests;/);
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_component_playback_controls\.rs"\]\s*mod controls_tests;/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_component_playback.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_playback.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_playback_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L04-B duplicator placement kernels stay private aggregator children and fail closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeDuplicatorPlacementRegistration, /L04-B\/#2121/);
  // R4: the declarations live in the L04 evaluation aggregator or, without one, in evaluation.rs.
  const declarationText = ['native-engine/src/object_component_kernels.rs', 'native-engine/src/evaluation.rs']
    .filter((file) => fs.existsSync(path.join(ROOT, file)))
    .map((file) => fs.readFileSync(path.join(ROOT, file), 'utf8'))
    .join('\n');
  for (const name of ['duplicator_placement_modes', 'duplicator_placement_rng']) {
    assert.match(declarationText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  for (const [name, children] of [
    ['duplicator_placement_modes', [['duplicator_placement_modes', 'tests'], ['duplicator_placement_modes_controls', 'controls_tests'], ['duplicator_placement_modes_pins', 'pins_tests']]],
    ['duplicator_placement_rng', [['duplicator_placement_rng', 'tests'], ['duplicator_placement_rng_controls', 'controls_tests']]],
  ]) {
    const kernelText = fs.readFileSync(path.join(ROOT, `native-engine/src/${name}.rs`), 'utf8');
    for (const [file, mod] of children) {
      assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${file}\\.rs"\\]\\s*mod ${mod};`));
    }
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/duplicator_placement_modes.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/duplicator_placement_rng.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_modes.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_modes_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_modes_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_rng.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_rng_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L05-A sparse frame-edit kernel stays a private timing child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeFrameEditSparseRegistration, /L05-A\/#2132/);
  const timingText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_layer_timing.rs'), 'utf8');
  assert.match(timingText, /#\[path = "object_frame_edit_sparse\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod object_frame_edit_sparse;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_frame_edit_sparse.rs'), 'utf8');
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_frame_edit_sparse\.rs"\]\s*mod tests;/);
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_frame_edit_sparse_controls\.rs"\]\s*mod controls;/);
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_frame_edit_sparse_literals\.rs"\]\s*mod literals;/);
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_frame_edit_sparse.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_sparse.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_sparse_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_sparse_literals.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L06-A text layout kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectTextLayoutKernelRegistration, /L06-A\/#2135/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_text_layout_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_text_layout_kernels;/);
  const evaluationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_text_layout_kernels.rs'), 'utf8');
  assert.match(evaluationText, /#\[path = "object_text_layout_kernel\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_text_layout_kernel;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_text_layout_kernel.rs'), 'utf8');
  assert.match(kernelText, /#\[path = "object_text_layout_flow\.rs"\]\s*pub\(crate\) mod flow;/);
  assert.match(kernelText, /#\[path = "object_text_layout_model\.rs"\]\s*pub\(crate\) mod model;/);
  for (const [name, decl] of [
    ['object_text_layout_kernel', 'mod tests;'],
    ['object_text_layout_kernel_oracle', 'pub\\(crate\\) mod oracle;'],
    ['object_text_layout_kernel_controls', 'mod controls;'],
    ['object_text_layout_kernel_refusals', 'mod refusals;'],
  ]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*${decl}`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_text_layout_kernel.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_text_layout_flow.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_text_layout_model.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_kernel.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_kernel_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_kernel_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_kernel_refusals.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L07-A scene paint kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectSceneStylePaintRegistration, /L07-A\/#2141/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_scene_style_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_scene_style_kernels;/);
  const evaluationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_scene_style_kernels.rs'), 'utf8');
  assert.match(evaluationText, /#\[path = "object_scene_style_paint\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_scene_style_paint;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_scene_style_paint.rs'), 'utf8');
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_scene_style_paint\.rs"\]\s*mod tests;/);
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_scene_style_paint_controls\.rs"\]\s*mod controls;/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_scene_style_paint.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_style_paint.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_style_paint_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L07-B scene stroke kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectSceneStyleStrokeRegistration, /L07-B\/#2142/);
  const declaring = ['native-engine/src/object_scene_style_kernels.rs', 'native-engine/src/evaluation.rs']
    .filter((file) => fs.existsSync(path.join(ROOT, file)))
    .map((file) => fs.readFileSync(path.join(ROOT, file), 'utf8'))
    .join('\n');
  assert.match(declaring, /#\[path = "object_scene_style_stroke\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_scene_style_stroke;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_scene_style_stroke.rs'), 'utf8');
  for (const [name, mod] of [['object_scene_style_stroke_controls', 'controls'], ['object_scene_style_stroke', 'tests']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_scene_style_stroke.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_style_stroke.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_style_stroke_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L08-A scene effect order planner stays a private render-scene child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeSceneEffectOrderRegistration, /L08-A\/#2144/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_render_scene.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "scene_effect_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod scene_effect_kernels;/);
  const sceneText = fs.readFileSync(path.join(ROOT, 'native-engine/src/scene_effect_kernels.rs'), 'utf8');
  assert.match(sceneText, /#\[path = "scene_effect_order\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod scene_effect_order;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/scene_effect_order.rs'), 'utf8');
  for (const name of ['scene_effect_order', 'scene_effect_order_cases', 'scene_effect_order_controls']) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod \\w+;`));
  }
  for (const [id, file] of [
    ['rust.native.engine.render-scene', 'native-engine/src/scene_effect_order.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_order.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_order_cases.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_order_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L08-B blend/matte/mask CPU reference stays a private lane-8 child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeSceneEffectBlendRefRegistration, /L08-B\/#2145/);
  const aggregator = fs.readFileSync(path.join(ROOT, 'native-engine/src/scene_effect_kernels.rs'), 'utf8');
  assert.match(aggregator, /#\[path = "scene_effect_blend_ref\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod scene_effect_blend_ref;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/scene_effect_blend_ref.rs'), 'utf8');
  assert.match(kernelText, /#\[path = "scene_effect_blend_ref_mask\.rs"\]\s*pub\(crate\) mod mask;/);
  for (const name of ['scene_effect_blend_ref', 'scene_effect_blend_ref_cases', 'scene_effect_blend_ref_controls']) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod \\w+;`));
  }
  for (const [id, file] of [
    ['rust.native.engine.render-scene', 'native-engine/src/scene_effect_blend_ref.rs'],
    ['rust.native.engine.render-scene', 'native-engine/src/scene_effect_blend_ref_mask.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_blend_ref.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_blend_ref_cases.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_blend_ref_controls.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L09-B media timing audio kernel stays a private aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectMediaTimingAudioRegistration, /L09-B\/#2127/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_media_timing_core.rs'), 'utf8');
  assert.match(coreText, /#\[path = "object_media_timing_audio\.rs"\]\s*pub\(crate\) mod object_media_timing_audio;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_media_timing_audio.rs'), 'utf8');
  for (const [name, mod] of [['object_media_timing_audio', 'tests'], ['object_media_timing_audio_controls', 'controls'], ['object_media_timing_audio_pins', 'pins'], ['object_media_timing_audio_scrub', 'scrub']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_media_timing_audio.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_timing_audio.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_timing_audio_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_timing_audio_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_timing_audio_scrub.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, `${id} must own ${file}`);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('isolated negative controls fail closed and restore every touched byte', (t) => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'nemo-n19-enforcement-'));
  const clone = path.join(temp, 'repo');
  t.after(() => fs.rmSync(temp, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));
  const cloned = spawnSync('git', ['clone', '--quiet', '--local', ROOT, clone], { encoding: 'utf8' });
  assert.equal(cloned.status, 0, cloned.stderr);
  copyCandidate(clone);
  const touched = [...new Set(CANDIDATE_FILES.concat([
    'native-engine/src/document.rs', 'native-engine/src/lib.rs', 'src/js/adapters/n19-unregistered.js',
    'native-engine/src/n19_unregistered.rs',
  ]))];
  const original = snapshot(clone, touched), source = snapshot(ROOT, CANDIDATE_FILES);
  try {
    // Remove real memberships from the cloned profile one at a time. Source
    // files remain present, so each missing registration must be detected.
    const registered = readJson(clone, PROFILE);
    for (const [id, dir] of [
      ['rust.native.engine.commands', 'src'], ['rust.native.engine.tests', 'tests'],
    ]) {
      for (const file of ['object_property_curve_edits.rs', 'object_property_track_edit.rs']) {
        const missing = structuredClone(registered);
        const owner = missing.modules.find((entry) => entry.id === id);
        owner.files = owner.files.filter((name) => name !== file);
        fs.writeFileSync(path.join(clone, PROFILE), JSON.stringify(missing));
        const relative = `native-engine/${dir}/${file}`;
        const result = checkSourceCoverage(readJson(clone, PROFILE), { root: clone, sourcePaths: [relative] });
        assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === relative));
        restore(clone, new Map([[PROFILE, original.get(PROFILE)]]));
      }
    }

    const rustExtra = 'native-engine/src/n19_unregistered.rs';
    fs.writeFileSync(path.join(clone, rustExtra), 'pub fn unregistered() {}\n');
    const repo = discoverRepositoryFiles({ root: clone });
    const selected = repo.entries.filter((entry) => entry.path.endsWith('.rs')).map((entry) => entry.path);
    const rustCoverage = checkRustSourceCoverage(readJson(clone, PROFILE), {
      root: clone, sourcePaths: selected, exclusions: readJson(clone, 'engineering/boundaries/profiles/rust.coverage.json').exclusions,
    });
    assert.equal(rustCoverage.ok, false);
    assert.ok(rustCoverage.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === rustExtra));
    restore(clone, new Map([[rustExtra, original.get(rustExtra)]]));

    const appExtra = 'src/js/adapters/n19-unregistered.js';
    fs.writeFileSync(path.join(clone, appExtra), 'export const unregistered = true;\n');
    const appScratch = fs.mkdtempSync(path.join(temp, 'app-baseline-'));
    const base = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: clone, encoding: 'utf8' }).stdout.trim();
    assert.throws(() => ci.applicationBoundaries(base, appScratch, clone),
      /fresh discovery found \d+ source\(s\), but the policy accounts for \d+ retained \+ \d+ excluded/);
    restore(clone, new Map([[appExtra, original.get(appExtra)]]));

    const documentPath = path.join(clone, 'native-engine/src/document.rs');
    fs.appendFileSync(documentPath, '\nuse crate::application;\n');
    const edge = nativeResult(clone);
    assert.equal(edge.ok, false);
    assert.ok(edge.violations.some((entry) => entry.rule === 'layer-violation'
      && entry.file === 'native-engine/src/document.rs' && entry.detail.targetModule === 'rust.native.engine.application'));
    restore(clone, new Map([['native-engine/src/document.rs', original.get('native-engine/src/document.rs')]]));

    for (const [file, importPath, target] of [
      ['native-engine/src/read_queries.rs', 'crate::history::NativeObjectHistory', 'rust.native.engine.history'],
      ['native-engine/src/revision.rs', 'crate::codec::decode_project', 'rust.native.engine.codec'],
      ['native-engine/src/object_codec.rs', 'crate::revision::ImageResourceBudget', 'rust.native.engine.revision'],
    ]) {
      fs.appendFileSync(path.join(clone, file), `\nuse ${importPath};\n`);
      const denied = nativeResult(clone);
      assert.ok(denied.violations.some((entry) => entry.rule === 'layer-violation'
        && entry.file === file && entry.detail.targetModule === target), `${file} must reject ${importPath}`);
      restore(clone, new Map([[file, original.get(file)]]));
    }

    const rootPath = path.join(clone, 'native-engine/src/lib.rs');
    fs.appendFileSync(rootPath, '\n#[cfg(feature = "n19-undeclared")] pub mod n19_undeclared {}\n');
    const feature = nativeResult(clone);
    assert.equal(feature.ok, false);
    assert.ok(feature.violations.some((entry) => entry.rule === 'undeclared-feature'
      && entry.detail.feature === 'n19-undeclared'));
    restore(clone, new Map([['native-engine/src/lib.rs', original.get('native-engine/src/lib.rs')]]));

    const jobsPath = path.join(clone, 'scripts/nemo/lib/jobs.cjs');
    const jobsText = fs.readFileSync(jobsPath, 'utf8');
    const removed = jobsText.replace(/^\s*'test:rust-native-engine': \{ run: jobTestNativeEngine, required: true \},\n/m, '');
    assert.notEqual(removed, jobsText, 'native-engine job registry line was not found');
    fs.writeFileSync(jobsPath, removed);
    const verify = spawnSync(process.execPath, ['scripts/nemo/verify.cjs', '--jobs', 'test:rust-native-engine', '--json'],
      { cwd: clone, encoding: 'utf8' });
    assert.notEqual(verify.status, 0);
    assert.match(verify.stderr, /unknown job "test:rust-native-engine"/);
  } finally {
    restore(clone, original);
    for (const [relative, bytes] of original) {
      const current = fs.existsSync(path.join(clone, relative)) ? fs.readFileSync(path.join(clone, relative)) : null;
      assert.deepEqual(current, bytes, `isolated clone did not restore ${relative}`);
    }
    for (const [relative, bytes] of source) assert.deepEqual(fs.readFileSync(path.join(ROOT, relative)), bytes,
      `negative control changed source worktree ${relative}`);
  }
});
