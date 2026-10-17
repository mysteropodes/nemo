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
  'engineering/boundaries/profiles/mcp-rust.profile.json',
  'native-engine/tests/object_property_metadata_dispatch.rs',
  'nemo-mcp/src/native_property_metadata_contract.rs',
  'nemo-mcp/src/native_property_metadata_wire_tests.rs',
  'src-tauri/src/native_property_metadata_dispatch_tests.rs',

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
  'native-engine/src/font_asset_admission.rs',
  'native-engine/tests/font_asset_admission.rs',
  'native-engine/src/font_asset_project.rs',
  'native-engine/src/project_asset_payload.rs',
  'native-engine/tests/font_asset_wire.rs',
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
  'native-engine/src/object_property_metadata_read.rs',
  'native-engine/tests/object_property_metadata_read.rs',
  'engineering/application/examples/native-property-metadata-v1/cases.json',
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
  const objectCodecText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_codec.rs'), 'utf8');
  const fontText = fs.readFileSync(path.join(ROOT, 'native-engine/src/font_asset_admission.rs'), 'utf8');
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
  assert.deepEqual(members.font_asset_admission, ['project_media_codec', 'dep:read-fonts']);
  assert.deepEqual(members.font_asset_wire, ['font_asset_admission']);
  assert.match(cargoText, /^read-fonts = \{ version = "=0\.39\.2", optional = true \}$/m);
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
      feature === 'project_media_codec' || feature === 'font_asset_admission' || feature === 'font_asset_wire' || feature === 'test-codec' || feature === 'test-application',
      `${feature} must not activate the private PNG selector`);
  }
  for (const feature of cargo.features.filter((name) => name !== 'project_media_codec')) {
    assert.equal(reaches(feature, 'project_media_codec'), feature === 'font_asset_admission' || feature === 'font_asset_wire' || feature === 'test-codec' || feature === 'test-application',
      `${feature} must not activate the private attached-codec selector`);
  }
  for (const feature of cargo.features.filter((name) => name !== 'font_asset_admission')) {
    assert.equal(reaches(feature, 'font_asset_admission'), feature === 'font_asset_wire',
      `${feature} must not activate the private font-admission selector`);
  }
  for (const feature of cargo.features.filter((name) => name !== 'font_asset_wire')) {
    assert.equal(reaches(feature, 'font_asset_wire'), false,
      `${feature} must not activate the private paired-font wire selector`);
  }
  assert.match(objectCodecText, /#\[cfg\(feature = "font_asset_admission"\)\]\s*#\[path = "font_asset_admission\.rs"\]\s*mod font_assets;/);
  assert.equal((objectCodecText.match(/mod font_assets;/g) || []).length, 1);
  assert.match(fontText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/font_asset_admission\.rs"\]\s*mod tests;/);
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

test('D35 snapshot metadata children remain private and registered', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.provenance.nativePropertyMetadataReadRegistration, /D35\/#2099/);
  for (const [id, dir] of [
    ['rust.native.engine.read-queries', 'src'], ['rust.native.engine.tests', 'tests'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    const file = 'object_property_metadata_read.rs';
    assert.equal(owner.files.filter((name) => name === file).length, 1);
    assert.equal(owner.publicApi.includes(file), false);
    assert.ok(CANDIDATE_FILES.includes(`native-engine/${dir}/${file}`));
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

test('D35 public metadata children keep their existing private owners', () => {
  const profile = readJson(ROOT, PROFILE);
  assert.match(readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json')
    .provenance.nativePropertyMetadataPublicRegistration, /D35\/#2099/);
  for (const [id, relative] of [["rust.native.engine.tests", "native-engine/tests/object_property_metadata_dispatch.rs"], ["rust.mcp.transport", "nemo-mcp/src/native_property_metadata_contract.rs"], ["rust.mcp.transport", "nemo-mcp/src/native_property_metadata_wire_tests.rs"], ["rust.desktop.native.host.release.tests", "src-tauri/src/native_property_metadata_dispatch_tests.rs"]]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    const file = relative.slice(owner.dir.length + 1);
    assert.equal(owner.files.filter((name) => name === file).length, 1);
    assert.equal(owner.publicApi.includes(file), false);
    assert.ok(CANDIDATE_FILES.includes(relative));
  }
  const mcp = readJson(ROOT, 'engineering/boundaries/profiles/mcp-rust.profile.json');
  const owner = mcp.modules.find((entry) => entry.id === 'mcp.transport');
  for (const file of ['src/native_property_metadata_contract.rs', 'src/native_property_metadata_wire_tests.rs']) {
    assert.equal(owner.files.filter((name) => name === file).length, 1);
    assert.equal(owner.publicApi.includes(file), false);
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

test('G22 parametric-shape kernel stays a private L01 aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeParametricGeometryRegistration, /L01-B\/#2130/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_vector_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_parametric_geometry\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_parametric_geometry;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_vector_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_vector_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_parametric_geometry.rs'), 'utf8');
  assert.match(kernelText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_parametric_geometry\.rs"\]\s*mod tests;/);
  assert.match(kernelText, /#\[path = "object_parametric_geometry_shapes\.rs"\]/);
  assert.match(kernelText, /#\[path = "object_parametric_geometry_box\.rs"\]/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_parametric_geometry.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_parametric_geometry_shapes.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_parametric_geometry_box.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_parametric_geometry_anim.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_parametric_geometry.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_parametric_geometry_box.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_parametric_geometry_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_parametric_geometry_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_parametric_geometry_pins_box.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_parametric_geometry_pin_data.rs'],
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

test('L02-B combine operand-plan kernel stays a private aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectGroupCombinePlanRegistration, /L02-B\/#2118/);
  const kernelsText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_kernels.rs'), 'utf8');
  assert.match(kernelsText, /#\[path = "object_group_combine_plan\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_group_combine_plan;/);
  const planText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_combine_plan.rs'), 'utf8');
  assert.match(planText, /#\[path = "object_group_combine_flatten\.rs"\]\s*mod flatten;/);
  for (const [name, mod] of [['object_group_combine_plan', 'tests'], ['object_group_combine_plan_controls', 'controls'], ['object_group_combine_plan_ports', 'ports']]) {
    assert.match(planText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const flattenText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_combine_flatten.rs'), 'utf8');
  assert.match(flattenText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_group_combine_flatten_controls\.rs"\]\s*mod controls;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_group_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_group_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_group_combine_plan.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_group_combine_flatten.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_group_combine_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_group_combine_plan_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_group_combine_plan_ports.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_group_combine_flatten_controls.rs'],
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

test('L03-C two-bone IK kernel stays a private image-mesh-family child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeRigIkSolveRegistration, /L03-C\/#2140/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_image_mesh_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "rig_ik_solve\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod rig_ik_solve;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_image_mesh_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_image_mesh_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/rig_ik_solve.rs'), 'utf8');
  for (const [name, mod] of [['rig_ik_solve_rows', 'oracle_rows'], ['rig_ik_solve', 'tests'], ['rig_ik_solve_find', 'find_tests'], ['rig_ik_solve_controls', 'controls_tests']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const oracle = fs.readFileSync(path.join(ROOT, 'native-engine/tests/fixtures/rig-ik-solve/oracle.txt'), 'utf8');
  assert.equal(oracle.trim().split('\n').length, 451, 'oracle row count');
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/rig_ik_solve.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_ik_solve.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_ik_solve_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_ik_solve_find.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_ik_solve_controls.rs'],
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

test('L05-F frame-edit range kernel stays a private document child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeFrameEditRangesRegistration, /L05-F\/#2133/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_frame_edit_kernels.rs'), 'utf8');
  assert.match(documentText, /#\[path = "object_frame_edit_ranges\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_frame_edit_ranges;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_frame_edit_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_frame_edit_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_frame_edit_ranges.rs'), 'utf8');
  for (const [name, mod] of [['object_frame_edit_ranges', 'tests'], ['object_frame_edit_ranges_controls', 'controls'], ['object_frame_edit_ranges_literals', 'literals']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_frame_edit_kernels.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_frame_edit_ranges.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_ranges.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_ranges_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_ranges_literals.rs'],
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

test('L05-H frame clipboard packet kernel stays a private commands child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeFrameClipboardPacketRegistration, /L05-H\/#2134/);
  const commandsText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_frame_edit_kernels.rs'), 'utf8');
  assert.match(commandsText, /#\[path = "object_frame_clipboard_packet\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_frame_clipboard_packet;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_frame_edit_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_frame_edit_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_frame_clipboard_packet.rs'), 'utf8');
  for (const [name, mod] of [['object_frame_clipboard_packet', 'tests'], ['object_frame_clipboard_packet_render', 'rendering'],
    ['object_frame_clipboard_packet_literals', 'literals'], ['object_frame_clipboard_packet_bounds', 'bounds'],
    ['object_frame_clipboard_packet_controls', 'controls']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_frame_clipboard_packet.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_clipboard_packet.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_clipboard_packet_render.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_clipboard_packet_literals.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_clipboard_packet_bounds.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_clipboard_packet_controls.rs'],
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

test('L06-C raster item value stays a private document aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectRasterAssetItemRegistration, /L06-C\/#2137/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(documentText, /#\[path = "object_raster_asset_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_raster_asset_kernels;/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_raster_asset_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_raster_asset_item\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_raster_asset_item;/);
  const itemText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_raster_asset_item.rs'), 'utf8');
  assert.match(itemText, /#\[path = "object_raster_asset_geometry\.rs"\]\s*pub\(crate\) mod geometry;/);
  for (const [name, decl] of [
    ['object_raster_asset_item_oracle', 'pub\\(crate\\) mod oracle;'],
    ['object_raster_asset_item', 'mod tests;'],
    ['object_raster_asset_item_codec', 'mod codec_tests;'],
    ['object_raster_asset_item_controls', 'mod controls;'],
  ]) {
    assert.match(itemText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*${decl}`));
  }
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_raster_asset_kernels.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_raster_asset_item.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_raster_asset_geometry.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_item.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_item_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_item_codec.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_item_controls.rs'],
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

test('L08-C effect-stack pass planner stays a private lane-8 child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeSceneEffectParamsRegistration, /L08-C\/#2146/);
  const aggregator = fs.readFileSync(path.join(ROOT, 'native-engine/src/scene_effect_kernels.rs'), 'utf8');
  assert.match(aggregator, /#\[path = "scene_effect_params\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod scene_effect_params;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_render_scene.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "scene_effect_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod scene_effect_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/scene_effect_params.rs'), 'utf8');
  for (const [name, mod] of [['scene_effect_params', 'tests'], ['scene_effect_params_cases', 'cases'], ['scene_effect_params_controls', 'controls'], ['scene_effect_params_guards', 'guards']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.render-scene', 'native-engine/src/scene_effect_params.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_params.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_params_cases.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_params_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/scene_effect_params_guards.rs'],
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

test('L10-B shot chain kernel stays a private history child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectShotChainRegistration, /L10-B\/#2124/);
  const historyText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_kernels.rs'), 'utf8');
  assert.match(historyText, /#\[path = "object_shot_chain\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_shot_chain;/);
  const chainText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_chain.rs'), 'utf8');
  assert.match(chainText, /#\[path = "object_shot_modules\.rs"\]\s*pub\(crate\) mod modules;/);
  const modulesText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_modules.rs'), 'utf8');
  for (const [text, children] of [
    [chainText, [['object_shot_chain_rows', 'rows'], ['object_shot_chain', 'tests'], ['object_shot_chain_controls', 'controls']]],
    [modulesText, [['object_shot_modules_rows', 'rows'], ['object_shot_modules', 'tests'], ['object_shot_modules_controls', 'controls']]],
  ]) {
    for (const [name, mod] of children) {
      assert.match(text, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
    }
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_history.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_shot_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_shot_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.history', 'native-engine/src/object_shot_chain.rs'],
    ['rust.native.engine.history', 'native-engine/src/object_shot_modules.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_shot_chain.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_shot_chain_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_shot_chain_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_shot_modules.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_shot_modules_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_shot_modules_controls.rs'],
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

test('L10-C drop/paste router kernel stays a private history-aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeImportDropRouteRegistration, /L10-C\/#2125/);
  const historyText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_kernels.rs'), 'utf8');
  assert.match(historyText, /#\[path = "native_import_drop_route\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod native_import_drop_route;/);
  const dropText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_import_drop_route.rs'), 'utf8');
  assert.match(dropText, /#\[path = "native_import_paste_route\.rs"\]\s*pub\(crate\) mod paste;/);
  const pasteText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_import_paste_route.rs'), 'utf8');
  assert.doesNotMatch(dropText, /^use super::/m);
  assert.doesNotMatch(pasteText, /^use super::/m);
  assert.match(pasteText, /use crate::history::object_history::object_shot_kernels::native_import_drop_route::/);
  for (const [text, children] of [
    [dropText, [['native_import_drop_route_rows', 'rows'], ['native_import_drop_route', 'tests'], ['native_import_drop_route_ports', 'ports_tests'], ['native_import_drop_route_controls', 'controls']]],
    [pasteText, [['native_import_paste_route', 'tests'], ['native_import_paste_route_controls', 'controls']]],
  ]) {
    for (const [name, mod] of children) {
      assert.match(text, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
    }
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_history.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_shot_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_shot_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.history', 'native-engine/src/native_import_drop_route.rs'],
    ['rust.native.engine.history', 'native-engine/src/native_import_paste_route.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_drop_route.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_drop_route_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_drop_route_ports.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_drop_route_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_paste_route.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_paste_route_controls.rs'],
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

test('L07-D charged bitmap-brush store stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeBitmapBrushResourcesStoreRegistration, /L07-D\/#2155/);
  const declaring = ['native-engine/src/object_scene_style_kernels.rs', 'native-engine/src/evaluation.rs']
    .filter((file) => fs.existsSync(path.join(ROOT, file)))
    .map((file) => fs.readFileSync(path.join(ROOT, file), 'utf8'))
    .join('\n');
  for (const name of ['bitmap_brush_resources_policy', 'bitmap_brush_resources_bitmap', 'bitmap_brush_resources_store']) {
    assert.match(declaring, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  const storeText = fs.readFileSync(path.join(ROOT, 'native-engine/src/bitmap_brush_resources_store.rs'), 'utf8');
  for (const [name, mod] of [
    ['bitmap_brush_resources_store_controls', 'controls'],
    ['bitmap_brush_resources_store_rows', 'rows'],
    ['bitmap_brush_resources_store', 'tests'],
  ]) {
    assert.match(storeText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/bitmap_brush_resources_policy.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/bitmap_brush_resources_bitmap.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/bitmap_brush_resources_store.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_store.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_store_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_store_controls.rs'],
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

test('L01-C serP/desP oracle tests stay a private L01 aggregator test child and fail closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeVectorSerpDesPOracleRegistration, /L01-C/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_vector_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_vector_oracle\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod object_vector_oracle;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_vector_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_vector_kernels;/);
  const owner = profile.modules.find((entry) => entry.id === 'rust.native.engine.tests');
  for (const name of ['object_vector_oracle.rs', 'object_vector_oracle_controls.rs', 'object_vector_oracle_pins.rs', 'object_vector_oracle_rules.rs',
    'object_vector_oracle_tables.rs', 'object_vector_oracle_tables_more.rs', 'object_vector_oracle_value.rs']) {
    const file = `native-engine/tests/${name}`;
    assert.equal(owner?.files.filter((entry) => entry === name).length, 1, `rust.native.engine.tests must own ${file}`);
    assert.equal(owner.publicApi.includes(name), false, `${file} stays private`);
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === 'rust.native.engine.tests').files = owner.files.filter((entry) => entry !== name);
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      `${file} must fail when unregistered`);
  }
});

test('L02-C clipboard/duplicate remap kernel stays a private aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectClipboardRemapRegistration, /L02-C\/#2119/);
  const kernelsText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_kernels.rs'), 'utf8');
  assert.match(kernelsText, /#\[path = "object_clipboard_remap\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_clipboard_remap;/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_clipboard_remap.rs'), 'utf8');
  for (const [name, mod] of [['object_clipboard_remap_groups', 'groups'], ['object_clipboard_remap_layer', 'layer']]) {
    assert.match(coreText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [name, mod] of [
    ['object_clipboard_remap', 'tests'], ['object_clipboard_remap_controls', 'controls'],
    ['object_clipboard_remap_layer_controls', 'layer_controls'], ['object_clipboard_remap_layer_replay', 'layer_replay'],
    ['object_clipboard_remap_mutations', 'mutations'],
    ['object_clipboard_remap_paste_controls', 'paste_controls'],
  ]) {
    assert.match(coreText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_group_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_group_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_clipboard_remap.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_clipboard_remap_groups.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_clipboard_remap_layer.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_clipboard_remap.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_clipboard_remap_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_clipboard_remap_layer_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_clipboard_remap_layer_replay.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_clipboard_remap_mutations.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_clipboard_remap_paste_controls.rs'],
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

test('L02-D boolean result topology kernel stays a private aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectBooleanTopologyRegistration, /L02-D\/#2151/);
  const kernelsText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_kernels.rs'), 'utf8');
  assert.match(kernelsText, /#\[path = "object_boolean_topology\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_boolean_topology;/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_boolean_topology.rs'), 'utf8');
  for (const [name, mod] of [
    ['object_boolean_erase', 'erase'], ['object_boolean_fill_commit', 'fill_commit'],
    ['object_boolean_fill_merge', 'fill_merge'], ['object_boolean_insert', 'insert'],
  ]) {
    assert.match(coreText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [name, mod] of [
    ['object_boolean_topology', 'tests'], ['object_boolean_topology_controls', 'controls'],
    ['object_boolean_topology_controls_commit', 'controls_commit'], ['object_boolean_topology_controls_erase', 'controls_erase'],
    ['object_boolean_topology_controls_merge', 'controls_merge'], ['object_boolean_topology_erase', 'erase_tests'],
    ['object_boolean_topology_merge', 'merge_tests'], ['object_boolean_topology_mutations', 'mutations'],
  ]) {
    assert.match(coreText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_group_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_group_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_boolean_topology.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_boolean_insert.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_boolean_erase.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_boolean_fill_merge.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_boolean_fill_commit.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology_controls_commit.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology_controls_erase.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology_controls_merge.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology_erase.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology_merge.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_boolean_topology_mutations.rs'],
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

test('L02-E layer lifecycle plan kernel stays a private aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectLayerLifecyclePlanRegistration, /L02-E\/#2168/);
  const kernelsText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_group_kernels.rs'), 'utf8');
  assert.match(kernelsText, /#\[path = "object_layer_lifecycle_plan\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_layer_lifecycle_plan;/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_layer_lifecycle_plan.rs'), 'utf8');
  for (const [name, mod] of [
    ['object_layer_lifecycle_delete', 'delete'], ['object_layer_lifecycle_duplicate', 'duplicate'],
    ['object_layer_lifecycle_fields', 'fields'], ['object_layer_lifecycle_order', 'order'],
  ]) {
    assert.match(coreText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [name, mod] of [
    ['object_layer_lifecycle_plan', 'tests'], ['object_layer_lifecycle_plan_controls', 'controls'],
    ['object_layer_lifecycle_plan_duplicate', 'duplicate_tests'], ['object_layer_lifecycle_plan_fixture', 'fixture'],
    ['object_layer_lifecycle_plan_mutations', 'mutations'], ['object_layer_lifecycle_plan_policy', 'policy'],
    ['object_layer_lifecycle_plan_replay_fields', 'replay_fields'],
  ]) {
    assert.match(coreText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_group_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_group_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_layer_lifecycle_plan.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_layer_lifecycle_delete.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_layer_lifecycle_duplicate.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_layer_lifecycle_fields.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_layer_lifecycle_order.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_layer_lifecycle_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_layer_lifecycle_plan_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_layer_lifecycle_plan_duplicate.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_layer_lifecycle_plan_fixture.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_layer_lifecycle_plan_mutations.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_layer_lifecycle_plan_policy.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_layer_lifecycle_plan_replay_fields.rs'],
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

test('L03-D rig weights and anchors kernel stays a private image-mesh-family child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeRigPoseWeightsRegistration, /L03-D\/#2152/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_image_mesh_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "rig_pose_weights\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod rig_pose_weights;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_image_mesh_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_image_mesh_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/rig_pose_weights.rs'), 'utf8');
  for (const [name, mod] of [['rig_pose_weights_contract', 'contract_tests'], ['rig_pose_weights_controls', 'controls_tests'], ['rig_pose_weights_flawed', 'flawed'], ['rig_pose_weights_rows', 'oracle_rows'], ['rig_pose_weights', 'tests']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const oracle = fs.readFileSync(path.join(ROOT, 'native-engine/tests/fixtures/rig-pose-weights/oracle.txt'), 'utf8');
  assert.equal(oracle.trim().split('\n').length, 1029, 'oracle row count');
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/rig_pose_weights.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_weights.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_weights_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_weights_contract.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_weights_flawed.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_weights_controls.rs'],
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

test('L04-C duplicator clone placement and effector kernels stay private aggregator children and fail closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeDuplicatorCloneEffectorRegistration, /L04-C\/#2122/);
  const declarationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_component_kernels.rs'), 'utf8');
  for (const name of ['duplicator_placement_clone', 'object_effector_falloff', 'object_effector_layer']) {
    assert.match(declarationText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  for (const [name, children] of [
    ['duplicator_placement_clone', [['duplicator_placement_clone', 'tests'], ['duplicator_placement_clone_controls', 'controls_tests'], ['duplicator_placement_clone_pins', 'pins_tests'], ['duplicator_placement_clone_codec', 'test_codec']]],
    ['object_effector_falloff', [['object_effector_falloff', 'tests'], ['object_effector_falloff_controls', 'controls_tests'], ['object_effector_falloff_pins', 'pins_tests'], ['object_effector_falloff_codec', 'test_codec']]],
    ['object_effector_layer', [['object_effector_layer', 'tests'], ['object_effector_layer_controls', 'controls_tests'], ['object_effector_layer_codec', 'test_codec']]],
  ]) {
    const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_component_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_component_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, `native-engine/src/${name}.rs`), 'utf8');
    for (const [file, mod] of children) {
      assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${file}\\.rs"\\]\\s*(?:pub\\(crate\\) )?mod ${mod};`));
    }
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/duplicator_placement_clone.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_effector_falloff.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_effector_layer.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_clone.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_clone_codec.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_clone_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/duplicator_placement_clone_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_effector_falloff.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_effector_falloff_codec.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_effector_falloff_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_effector_falloff_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_effector_layer.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_effector_layer_codec.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_effector_layer_controls.rs'],
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

test('L06-B glyph contour kernel stays a private lane-6 child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeTextGlyphContoursRegistration, /L06-B\/#2136/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_text_layout_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "text_glyph_contours\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod text_glyph_contours;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_text_layout_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_text_layout_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/text_glyph_contours.rs'), 'utf8');
  assert.match(kernelText, /#\[path = "text_glyph_contours_shear\.rs"\]\s*pub\(crate\) mod shear;/);
  for (const [name, decl] of [
    ['text_glyph_contours', 'mod tests;'],
    ['text_glyph_contours_oracle', 'pub\\(crate\\) mod oracle;'],
    ['text_glyph_contours_controls', 'mod controls;'],
    ['text_glyph_contours_shear', 'mod shear_tests;'],
  ]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*${decl}`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/text_glyph_contours.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/text_glyph_contours_shear.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/text_glyph_contours.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/text_glyph_contours_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/text_glyph_contours_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/text_glyph_contours_shear.rs'],
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

test('L06-D raster asset admission policy stays a private document aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectRasterAssetAdmissionRegistration, /L06-D\/#2154/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_raster_asset_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_raster_asset_admission\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_raster_asset_admission;/);
  const admissionText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_raster_asset_admission.rs'), 'utf8');
  assert.match(admissionText, /#\[path = "object_raster_asset_sniff\.rs"\]\s*pub\(crate\) mod sniff;/);
  assert.match(admissionText, /#\[cfg\(feature = "reference_image_asset"\)\]\s*#\[path = "object_raster_asset_png_port\.rs"\]\s*pub\(crate\) mod png_port;/);
  for (const [name, decl] of [
    ['object_raster_asset_admission_controls', 'mod controls;'],
    ['object_raster_asset_admission_oracle', 'pub\\(crate\\) mod oracle;'],
    ['object_raster_asset_admission_sniff', 'mod sniff_tests;'],
    ['object_raster_asset_admission', 'mod tests;'],
  ]) {
    assert.match(admissionText, new RegExp(`#\\[cfg\\(all\\(test, feature = "commands"\\)\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*${decl}`));
  }
  assert.match(admissionText, /#\[cfg\(all\(test, feature = "commands", feature = "reference_image_asset"\)\)\]\s*#\[path = "\.\.\/tests\/object_raster_asset_admission_real\.rs"\]\s*mod real_validator;/);
  // Layer discipline: the document-layer policy and sniffer import nothing outside the document group.
  const sniffText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_raster_asset_sniff.rs'), 'utf8');
  for (const text of [admissionText, sniffText]) {
    assert.doesNotMatch(text, /crate::(?!object_document::)/);
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_raster_asset_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_raster_asset_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_raster_asset_admission.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_raster_asset_sniff.rs'],
    ['rust.native.engine.codec', 'native-engine/src/object_raster_asset_png_port.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_admission.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_admission_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_admission_real.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_admission_sniff.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_admission_controls.rs'],
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

test('L07-C scene producer pin join stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectSceneProducerPinRegistration, /L07-C\/#2143/);
  const declaring = ['native-engine/src/object_scene_style_kernels.rs', 'native-engine/src/evaluation.rs']
    .filter((file) => fs.existsSync(path.join(ROOT, file)))
    .map((file) => fs.readFileSync(path.join(ROOT, file), 'utf8'))
    .join('\n');
  assert.match(declaring, /#\[path = "object_scene_producer_pin\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_scene_producer_pin;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_scene_producer_pin.rs'), 'utf8');
  for (const [name, mod] of [
    ['object_scene_producer_pin_controls', 'controls'],
    ['object_scene_producer_pin_rows', 'rows'],
    ['object_scene_producer_pin', 'tests'],
  ]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_scene_producer_pin.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_pin.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_pin_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_pin_rows.rs'],
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

test('L09-C media link state kernel stays a private aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectMediaLinkRegistration, /L09-C\/#2128/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_media_timing_core.rs'), 'utf8');
  for (const name of ['object_media_link_state', 'object_media_link_reopen', 'object_media_link_join']) {
    assert.match(coreText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  const children = {
    object_media_link_state: [['object_media_link_state', 'tests'], ['object_media_link_state_controls', 'controls'],
      ['object_media_link_state_pins', 'pins'], ['object_media_link_state_world', 'world']],
    object_media_link_reopen: [['object_media_link_reopen', 'tests']],
    object_media_link_join: [['object_media_link_join', 'tests']],
  };
  for (const [owner, tests] of Object.entries(children)) {
    const text = fs.readFileSync(path.join(ROOT, `native-engine/src/${owner}.rs`), 'utf8');
    for (const [name, mod] of tests) {
      assert.match(text, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
    }
  }
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_media_link_state.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_media_link_reopen.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_media_link_join.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_state.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_state_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_state_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_state_world.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_reopen.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_join.rs'],
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

test('L10-D SVG import explode kernel stays a private history-aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeImportSvgExplodeRegistration, /L10-D\/#2158/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "native_import_svg_explode\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod native_import_svg_explode;/);
  const explodeText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_import_svg_explode.rs'), 'utf8');
  assert.match(explodeText, /#\[path = "native_import_svg_plan\.rs"\]\s*pub\(crate\) mod plan;/);
  const planText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_import_svg_plan.rs'), 'utf8');
  assert.doesNotMatch(explodeText, /^use /m);
  assert.doesNotMatch(planText, /^use super::/m);
  assert.match(planText, /use crate::history::object_history::object_shot_kernels::native_import_svg_explode::/);
  for (const [text, children] of [
    [explodeText, [['native_import_svg_explode_rows', 'rows'], ['native_import_svg_explode', 'tests'], ['native_import_svg_explode_pins', 'pins'], ['native_import_svg_explode_controls', 'controls']]],
    [planText, [['native_import_svg_plan_rows', 'rows'], ['native_import_svg_plan', 'tests'], ['native_import_svg_plan_controls', 'controls']]],
  ]) {
    for (const [name, mod] of children) {
      assert.match(text, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
    }
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_history.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_shot_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_shot_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.history', 'native-engine/src/native_import_svg_explode.rs'],
    ['rust.native.engine.history', 'native-engine/src/native_import_svg_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_svg_explode.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_svg_explode_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_svg_explode_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_svg_explode_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_svg_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_svg_plan_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_svg_plan_controls.rs'],
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

test('L10-G import id remap kernel stays a private history-aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeImportRemapIdsRegistration, /L10-G\/#2176/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_import_remap_ids\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_import_remap_ids;/);
  const remapText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_import_remap_ids.rs'), 'utf8');
  assert.doesNotMatch(remapText, /^use (super|crate)::/m);
  for (const [name, mod] of [
    ['object_import_remap_ids_rows', 'rows'],
    ['object_import_remap_ids', 'tests'],
    ['object_import_remap_ids_graph', 'graph'],
    ['object_import_remap_ids_owner', 'owner'],
    ['object_import_remap_ids_controls', 'controls'],
  ]) {
    assert.match(remapText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_history.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_shot_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_shot_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.history', 'native-engine/src/object_import_remap_ids.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_import_remap_ids.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_import_remap_ids_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_import_remap_ids_graph.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_import_remap_ids_owner.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_import_remap_ids_controls.rs'],
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

test('G14 editable-vector record extension stays private document and codec children and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeVectorRecordRegistration, /L01-D\/#2150/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(documentText, /#\[path = "object_vector_record\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod vector_record;/);
  const codecText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_codec.rs'), 'utf8');
  assert.match(codecText, /#\[path = "object_vector_codec\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod object_vector_codec;/);
  const vectorCodecText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_vector_codec.rs'), 'utf8');
  assert.match(vectorCodecText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_vector_record\.rs"\]\s*mod object_vector_record_tests;/);
  const hostText = fs.readFileSync(path.join(ROOT, 'native-engine/tests/object_vector_record.rs'), 'utf8');
  for (const name of ['object_vector_record_controls', 'object_vector_record_data', 'object_vector_record_fill']) {
    assert.match(hostText, new RegExp(`#\\[path = "${name}\\.rs"\\]`));
  }
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_vector_record.rs'],
    ['rust.native.engine.codec', 'native-engine/src/object_vector_codec.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_record.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_record_data.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_record_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_record_fill.rs'],
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

test('L07-F contour producer join stays a private render-scene child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectSceneProducerContourRegistration, /L07-F\/#2161/);
  const parentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_render_scene.rs'), 'utf8');
  assert.match(parentText, /#\[path = "object_scene_producer_contour\.rs"\]\s*mod object_scene_producer_contour;/);
  assert.match(parentText, /object_scene_producer_contour::produce\(packet\)\.into_admitted\(\)\?/);
  const planText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_paint_plan.rs'), 'utf8');
  assert.match(planText, /object_scene_producer_contour::command_sequence\(scene, expected\)\?/);
  const compositorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_compositor.rs'), 'utf8');
  assert.match(compositorText, /\.solid\(\)\s*\.rgba_f32\(\)/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_scene_producer_contour.rs'), 'utf8');
  assert.doesNotMatch(kernelText, /super::/);
  for (const [name, mod] of [
    ['object_scene_producer_contour_controls', 'controls'],
    ['object_scene_producer_contour_rows', 'rows'],
    ['object_scene_producer_contour', 'tests'],
  ]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.render-scene', 'native-engine/src/object_scene_producer_contour.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_contour.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_contour_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_contour_rows.rs'],
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

test('L08-D scene resource declaration set stays a private render-scene child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectSceneResourceSetRegistration, /L08-D\/#2156/);
  const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
  assert.match(read('native-engine/src/scene_effect_kernels.rs'), /#\[path = "object_scene_resource_set\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_scene_resource_set;/);
  const kernelText = read('native-engine/src/object_scene_resource_set.rs');
  for (const [name, cfg] of [
    ['object_scene_resource_set', 'test'],
    ['object_scene_resource_set_controls', 'test'],
    ['object_scene_resource_set_permit', 'all\\(test, feature = "history"\\)'],
  ]) {
    assert.match(kernelText, new RegExp('#\\[cfg\\(' + cfg + '\\)\\]\\s*#\\[path = "\\.\\./tests/' + name + '\\.rs"\\]\\s*mod \\w+;'));
  }
  assert.doesNotMatch(kernelText, /crate::(revision|resource_leases|image_resource_budget|scheduler)/, 'render-scene layer: no lease or budget import');
  const scheduled = read('native-engine/src/object_scheduled_scene.rs');
  assert.match(scheduled, /verify_scene_resources\(key\.declared_resource_versions\(\)\)/);
  assert.doesNotMatch(scheduled, /declared_resource_versions\(\)\.is_empty\(\)/, 'blanket declared-resource refusal stays replaced');
  assert.match(read('native-engine/src/object_render_scene.rs'), /resources: Option<Arc<ObjectSceneResourceSet>>/);
  for (const [id, file] of [
    ['rust.native.engine.render-scene', 'native-engine/src/object_scene_resource_set.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_resource_set.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_resource_set_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_resource_set_permit.rs'],
  ]) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((name) => name === path.basename(file)).length, 1, id + ' must own ' + file);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, file + ' stays private');
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((name) => name !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      file + ' must fail when unregistered');
  }
});

test('L08-E viewport/export same-packet tests stay private viewport-consumer children and the consumption boundary keeps one door', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeViewportScenePacketParityRegistration, /L08-E\/#2163/);
  const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
  const consumer = read('native-engine/src/object_viewport_consumer.rs');
  const names = ["viewport_scene_controls","viewport_scene_packet_parity","viewport_scene_stale_matrix","viewport_scene_support"];
  for (const name of names) {
    assert.match(consumer, new RegExp('#\\[cfg\\(all\\(test, feature = "history", feature = "export_job"\\)\\)\\]\\s*#\\[path = "\\.\\./tests/' + name + '\\.rs"\\]\\s*mod ' + name + ';'));
  }
  // Viewport, PNG and export reach pixels only through the scheduled-object door.
  const guards = [
    [
      "native-engine/src/object_viewport_consumer.rs",
      [
        "compose_scheduled_object\\(live\\)",
        "present_scheduled_object\\(compositor, &self\\.result, live\\)"
      ],
      [
        "\\bcompose_object\\(",
        "\\breadback_object_rgba8\\(",
        "\\breadback_texture\\(",
        "Compositor::new",
        "FrameScheduler"
      ]
    ],
    [
      "native-engine/src/object_png_output.rs",
      [
        "readback_scheduled_object_rgba8\\(result, live\\)",
        "is_viewport\\(\\)"
      ],
      [
        "\\bcompose_object\\(",
        "\\breadback_object_rgba8\\(",
        "\\breadback_texture\\(",
        "Compositor::new"
      ]
    ],
    [
      "native-engine/src/png_output.rs",
      [
        "\\.compose_scheduled_object\\(live\\)",
        "encode_scheduled_object_png\\(self, &result, live"
      ],
      [
        "\\bcompose_object\\(",
        "\\breadback_object_rgba8\\("
      ]
    ],
    [
      "native-engine/src/object_export_job.rs",
      [
        "prepare_scheduled_object_composition_from_witness as prepare",
        "encoder\\.encode_object_png\\(&live"
      ],
      [
        "\\bCompositor\\b",
        "\\bcompose_",
        "readback",
        "wgpu"
      ]
    ],
    [
      "native-engine/src/object_compositor.rs",
      [
        "fn scheduled_object_surface[^}]{0,400}check_live\\(live, &result\\.prepared\\)",
        "fn compose_scheduled_object[^}]{0,400}check_live\\(live, prepared\\)",
        "fn readback_scheduled_object_rgba8[^}]{0,400}check_live\\(live, &result\\.prepared\\)"
      ],
      []
    ],
    [
      "native-engine/src/desktop_viewport.rs",
      [
        "\\.scheduled_object_surface\\(result, live\\)",
        "fn present_scheduled_object"
      ],
      [
        "\\bcompose_object\\(",
        "\\breadback_object_rgba8\\(",
        "\\breadback_texture\\("
      ]
    ]
  ];
  const code = (file) => read(file).split('\n').filter((line) => !/^\s*\/\//.test(line)).join('\n');
  for (const [file, must, mustNot] of guards) {
    const text = code(file);
    for (const source of must) {
      const hit = text.match(new RegExp(source));
      assert.ok(hit, file + ' must keep ' + source);
      assert.ok(!new RegExp(source).test(text.split(hit[0]).join('')), file + ' guard ' + source + ' must fail when removed');
    }
    for (const source of mustNot) {
      assert.doesNotMatch(text, new RegExp(source), file + ' must not reach ' + source);
      const mutant = source.replace(/\\b/g, '').replace(/\\\(/g, '(');
      assert.match(text + '\n' + mutant, new RegExp(source), file + ' guard ' + source + ' must trip on a mutant');
    }
  }
  for (const name of names) {
    const file = 'native-engine/tests/' + name + '.rs';
    const owner = profile.modules.find((entry) => entry.id === 'rust.native.engine.tests');
    assert.equal(owner?.files.filter((entry) => entry === name + '.rs').length, 1, 'rust.native.engine.tests must own ' + file);
    assert.equal(owner.publicApi.includes(name + '.rs'), false, file + ' stays private');
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === 'rust.native.engine.tests').files = owner.files.filter((entry) => entry !== name + '.rs');
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      file + ' must fail when unregistered');
  }
});

test('L09-D native SVG frame serializer stays a private export-job aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeVectorExportRegistration, /L09-D\/#2157/);
  const parentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/export_job.rs'), 'utf8');
  assert.match(parentText, /#\[path = "native_vector_export_core\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod native_vector_export;/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_vector_export_core.rs'), 'utf8');
  assert.match(coreText, /#\[path = "native_vector_export_svg\.rs"\]\s*pub\(crate\) mod native_vector_export_svg;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_vector_export_svg.rs'), 'utf8');
  for (const [name, mod] of [['native_vector_export_svg', 'tests'], ['native_vector_export_svg_controls', 'controls'], ['native_vector_export_svg_fixture', 'fixture'], ['native_vector_export_svg_oracle', 'oracle'], ['native_vector_export_svg_reader', 'reader']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.export-job', 'native-engine/src/native_vector_export_core.rs'],
    ['rust.native.engine.export-job', 'native-engine/src/native_vector_export_svg.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_vector_export_svg.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_vector_export_svg_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_vector_export_svg_fixture.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_vector_export_svg_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_vector_export_svg_reader.rs'],
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

test('L09-E native media export plan kernel stays a private export-job aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeMediaExportPlanRegistration, /L09-E\/#2162/);
  const parentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/export_job.rs'), 'utf8');
  assert.match(parentText, /#\[path = "native_media_export_core\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod native_media_export;/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_media_export_core.rs'), 'utf8');
  for (const name of ['native_media_export_plan', 'native_media_export_verify', 'native_media_export_video']) {
    assert.match(coreText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  for (const [src, children] of [
    ['native-engine/src/native_media_export_plan.rs', [['native_media_export_plan_controls', 'controls'], ['native_media_export_plan_oracle', 'oracle'], ['native_media_export_plan', 'tests']]],
    ['native-engine/src/native_media_export_verify.rs', [['native_media_export_verify_controls', 'controls'], ['native_media_export_verify_stage', 'stage'], ['native_media_export_verify', 'tests']]],
  ]) {
    const text = fs.readFileSync(path.join(ROOT, src), 'utf8');
    for (const [name, mod] of children) {
      assert.match(text, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
    }
  }
  for (const [id, file] of [
    ['rust.native.engine.export-job', 'native-engine/src/native_media_export_core.rs'],
    ['rust.native.engine.export-job', 'native-engine/src/native_media_export_plan.rs'],
    ['rust.native.engine.export-job', 'native-engine/src/native_media_export_verify.rs'],
    ['rust.native.engine.export-job', 'native-engine/src/native_media_export_video.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_media_export_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_media_export_plan_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_media_export_plan_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_media_export_verify.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_media_export_verify_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_media_export_verify_stage.rs'],
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

test('L05-B document frame-edit kernel stays a private lane-5 aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeFrameEditDocumentRegistration, /L05-B\/#2160/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_frame_edit_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_frame_edit_document\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_frame_edit_document;/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(documentText, /pub\(crate\) use timing::TimingEdit;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_frame_edit_document.rs'), 'utf8');
  assert.doesNotMatch(kernelText.split('#[cfg(test)]')[0], /super::/);
  for (const [name, mod] of [['object_frame_edit_document', 'tests'], ['object_frame_edit_document_build', 'build'],
    ['object_frame_edit_document_reach', 'reach'], ['object_frame_edit_document_literals', 'literals'],
    ['object_frame_edit_document_controls', 'controls']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_frame_edit_document.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_document.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_document_build.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_document_reach.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_document_literals.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_frame_edit_document_controls.rs'],
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

test('L03-E rig deform kernel stays a private image-mesh-family child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeRigPoseDeformRegistration, /L03-E\/#2188 \(lane #2109/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_image_mesh_kernels.rs'), 'utf8');
  for (const name of ['rig_pose_deform_anchor', 'rig_pose_deform']) {
    assert.match(aggregatorText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_image_mesh_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_image_mesh_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/rig_pose_deform.rs'), 'utf8');
  for (const [name, mod] of [['rig_pose_deform_controls', 'controls_tests'], ['rig_pose_deform_mesh', 'mesh_tests'], ['rig_pose_deform_rows', 'oracle_rows'], ['rig_pose_deform', 'tests']]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const oracle = fs.readFileSync(path.join(ROOT, 'native-engine/tests/fixtures/rig-pose-deform/oracle.txt'), 'utf8');
  assert.equal(oracle.trim().split('\n').length, 414, 'oracle row count');
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/rig_pose_deform_anchor.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/rig_pose_deform.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_deform.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_deform_mesh.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_deform_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/rig_pose_deform_controls.rs'],
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

test('L04-E exposed component property kernels stay private aggregator children and fail closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectComponentExposedRegistration, /L04-E\/#2189/);
  const declarationText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_component_kernels.rs'), 'utf8');
  for (const name of ['object_component_exposed_key', 'object_component_exposed', 'object_component_exposed_apply']) {
    assert.match(declarationText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  for (const [name, children] of [
    ['object_component_exposed_key', [['object_component_exposed_key', 'tests'], ['object_component_exposed_key_controls', 'controls_tests']]],
    ['object_component_exposed', [['object_component_exposed', 'tests']]],
    ['object_component_exposed_apply', [['object_component_exposed_apply', 'tests'], ['object_component_exposed_controls', 'controls_tests'], ['object_component_exposed_support', 'support']]],
  ]) {
    const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_component_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_component_kernels;/);
  const kernelText = fs.readFileSync(path.join(ROOT, `native-engine/src/${name}.rs`), 'utf8');
    for (const [file, mod] of children) {
      assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${file}\\.rs"\\]\\s*(?:pub\\(crate\\) )?mod ${mod};`));
    }
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_component_exposed_key.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_component_exposed.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_component_exposed_apply.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_exposed_key.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_exposed_key_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_exposed.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_exposed_apply.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_exposed_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_exposed_support.rs'],
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

test('L06-F text block record stays a private lane-6 child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectTextLayoutRecordRegistration, /L06-F\/#2175/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_text_layout_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "object_text_layout_record\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_text_layout_record;/);
  const recordText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_text_layout_record.rs'), 'utf8');
  for (const [file, name] of [['object_text_layout_font', 'font'], ['text_glyph_pin', 'pin'], ['object_text_layout_request', 'request'], ['object_text_layout_root', 'root']]) {
    assert.match(recordText, new RegExp(`#\\[path = "${file}\\.rs"\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  for (const [name, decl] of [
    ['object_text_layout_record_capture', 'mod capture_tests;'],
    ['object_text_layout_record_codec', 'mod codec_tests;'],
    ['object_text_layout_record_controls', 'mod controls;'],
    ['object_text_layout_record_oracle', 'pub\\(crate\\) mod oracle;'],
    ['object_text_layout_record_refusals', 'mod refusals;'],
    ['object_text_layout_record', 'mod tests;'],
  ]) {
    assert.match(recordText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*${decl}`));
  }
  for (const [source, name] of [['object_text_layout_font', 'object_text_layout_font'], ['text_glyph_pin', 'text_glyph_pin']]) {
    const childText = fs.readFileSync(path.join(ROOT, `native-engine/src/${source}.rs`), 'utf8');
    assert.match(childText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod tests;`));
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/evaluation.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_text_layout_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_text_layout_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_text_layout_record.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_text_layout_font.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_text_layout_request.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_text_layout_root.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/text_glyph_pin.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_record.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_record_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_record_codec.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_record_capture.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_record_refusals.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_record_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_text_layout_font.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/text_glyph_pin.rs'],
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

test('L07-E bitmap-brush stamp plan stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeBitmapBrushResourcesStampRegistration, /L07-E\/#2165/);
  const declaring = ['native-engine/src/object_scene_style_kernels.rs', 'native-engine/src/evaluation.rs']
    .filter((file) => fs.existsSync(path.join(ROOT, file)))
    .map((file) => fs.readFileSync(path.join(ROOT, file), 'utf8'))
    .join('\n');
  for (const name of ['bitmap_brush_resources_stamp', 'bitmap_brush_resources_stamp_bounds']) {
    assert.match(declaring, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  const stampText = fs.readFileSync(path.join(ROOT, 'native-engine/src/bitmap_brush_resources_stamp.rs'), 'utf8');
  for (const [name, mod] of [
    ['bitmap_brush_resources_stamp_controls', 'controls'],
    ['bitmap_brush_resources_stamp_rows', 'rows'],
    ['bitmap_brush_resources_stamp', 'tests'],
  ]) {
    assert.match(stampText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/bitmap_brush_resources_stamp.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/bitmap_brush_resources_stamp_bounds.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_stamp.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_stamp_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_stamp_controls.rs'],
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

test('L07-H component evaluated-shape kernel stays a private evaluation child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectSceneProducerComponentRegistration, /L07-H\/#2193/);
  const declaring = ['native-engine/src/object_scene_style_kernels.rs', 'native-engine/src/evaluation.rs']
    .filter((file) => fs.existsSync(path.join(ROOT, file)))
    .map((file) => fs.readFileSync(path.join(ROOT, file), 'utf8'))
    .join('\n');
  for (const name of ['object_scene_producer_component_shape', 'object_scene_producer_component']) {
    assert.match(declaring, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  const kernelText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_scene_producer_component.rs'), 'utf8');
  for (const [name, mod] of [
    ['object_scene_producer_component_controls', 'controls'],
    ['object_scene_producer_component_rows', 'rows'],
    ['object_scene_producer_component', 'tests'],
  ]) {
    assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  assert.doesNotMatch(kernelText, /\bsuper::/);
  for (const [id, file] of [
    ['rust.native.engine.evaluation', 'native-engine/src/object_scene_producer_component.rs'],
    ['rust.native.engine.evaluation', 'native-engine/src/object_scene_producer_component_shape.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_component.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_component_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_scene_producer_component_controls.rs'],
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

test('L09-G media binding plan stays a private aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectMediaLinkPlanRegistration, /L09-G\/#2194/);
  const coreText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_media_timing_core.rs'), 'utf8');
  assert.match(coreText, /#\[path = "object_media_link_plan\.rs"\]\s*pub\(crate\) mod object_media_link_plan;/);
  const planText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_media_link_plan.rs'), 'utf8');
  for (const [name, mod] of [['object_media_link_plan', 'tests'], ['object_media_link_plan_controls', 'controls'],
    ['object_media_link_plan_links', 'links'], ['object_media_link_plan_pins', 'pins']]) {
    assert.match(planText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  assert.doesNotMatch(planText, /\buse super::/, 'production code imports through the absolute module chain');
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_media_link_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_plan_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_plan_links.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_media_link_plan_pins.rs'],
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

test('L10-E Figma node-walk kernel stays a private history-aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeImportFigmaWalkRegistration, /L10-E\/#2195 \(lane #2116/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "native_import_figma_walk\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod native_import_figma_walk;/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_history.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_shot_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_shot_kernels;/);
  const read = (name) => fs.readFileSync(path.join(ROOT, `native-engine/src/native_import_figma_${name}.rs`), 'utf8');
  const walkText = read('walk');
  for (const name of ['jsnum', 'model', 'paint', 'plan', 'requests', 'ids']) {
    assert.match(walkText, new RegExp(`#\\[path = "native_import_figma_${name}\\.rs"\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  const children = {
    walk: [['native_import_figma_walk_rows', 'rows'], ['native_import_figma_walk_decode', 'decode'], ['native_import_figma_walk_flows', 'flows'], ['native_import_figma_walk', 'tests'], ['native_import_figma_walk_controls', 'controls']],
    jsnum: [['native_import_figma_jsnum', 'tests']],
    model: [],
    paint: [['native_import_figma_paint', 'tests']],
    plan: [['native_import_figma_plan', 'tests'], ['native_import_figma_plan_controls', 'controls']],
    requests: [['native_import_figma_requests', 'tests']],
    ids: [['native_import_figma_ids', 'tests']],
  };
  for (const [name, tests] of Object.entries(children)) {
    const text = read(name);
    assert.doesNotMatch(text, /^\s*use super::/m, `${name} must import by the absolute chain (ruling R5)`);
    if (['paint', 'plan', 'requests', 'ids'].includes(name)) {
      assert.match(text, /use crate::history::object_history::object_shot_kernels::native_import_figma_walk::/);
    }
    for (const [testName, mod] of tests) {
      assert.match(text, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${testName}\\.rs"\\]\\s*mod ${mod};`));
    }
  }
  const sources = ['walk', 'jsnum', 'model', 'paint', 'plan', 'requests', 'ids'].map((n) => `native-engine/src/native_import_figma_${n}.rs`);
  const tests = ['walk', 'walk_rows', 'walk_decode', 'walk_flows', 'walk_controls', 'jsnum', 'paint', 'plan', 'plan_controls', 'requests', 'ids'].map((n) => `native-engine/tests/native_import_figma_${n}.rs`);
  for (const [id, file] of [
    ...sources.map((f) => ['rust.native.engine.history', f]),
    ...tests.map((f) => ['rust.native.engine.tests', f]),
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

test('L10-F PSD import plan kernel stays a private history-aggregator child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeImportPsdPlanRegistration, /L10-F\/#2196/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_shot_kernels.rs'), 'utf8');
  assert.match(aggregatorText, /#\[path = "native_import_psd_plan\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod native_import_psd_plan;/);
  const planText = fs.readFileSync(path.join(ROOT, 'native-engine/src/native_import_psd_plan.rs'), 'utf8');
  assert.doesNotMatch(planText, /^use super::/m);
  assert.match(planText, /use crate::history::object_history::object_shot_kernels::native_import_drop_route::ProviderState;/);
  assert.match(planText, /use crate::history::object_history::object_shot_kernels::object_import_remap_ids::\{\s*psd_layer_key, psd_layer_name,?\s*\};/);
  for (const [name, mod] of [
    ['native_import_psd_plan_rows', 'rows'],
    ['native_import_psd_plan', 'tests'],
    ['native_import_psd_plan_oracle', 'oracle'],
    ['native_import_psd_plan_controls', 'controls'],
  ]) {
    assert.match(planText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_history.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_shot_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_shot_kernels;/);
  for (const [id, file] of [
    ['rust.native.engine.history', 'native-engine/src/native_import_psd_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_psd_plan.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_psd_plan_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_psd_plan_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/native_import_psd_plan_controls.rs'],
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

test('G14/G22/G13 vector extension records stay private document and codec children and fail closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeVectorExtensionsRegistration, /L01-E\/#2182/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  for (const [file, name] of [
    ['object_vector_pressure', 'vector_pressure'],
    ['object_parametric_record', 'parametric_record'],
    ['object_path_effects_record', 'path_effects_record'],
    ['object_vector_ext', 'vector_ext'],
  ]) {
    assert.match(documentText, new RegExp(`#\\[path = "${file}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*mod ${name};`));
  }
  const codecText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_codec.rs'), 'utf8');
  assert.match(codecText, /#\[path = "object_vector_ext_codec\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod object_vector_ext_codec;/);
  const extText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_vector_ext_codec.rs'), 'utf8');
  assert.match(extText, /#\[cfg\(test\)\]\s*#\[path = "\.\.\/tests\/object_vector_extensions\.rs"\]\s*mod object_vector_extensions_tests;/);
  const hostText = fs.readFileSync(path.join(ROOT, 'native-engine/tests/object_vector_extensions.rs'), 'utf8');
  for (const name of ['data', 'layers', 'pair_controls', 'pins', 'controls']) {
    assert.match(hostText, new RegExp(`#\\[path = "object_vector_extensions_${name}\\.rs"\\]`));
  }
  assert.match(hostText, /#\[cfg\(feature = "evaluation"\)\]\s*#\[path = "object_vector_extensions_kernel\.rs"\]/);
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_vector_pressure.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_parametric_record.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_path_effects_record.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_vector_ext.rs'],
    ['rust.native.engine.codec', 'native-engine/src/object_vector_ext_codec.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_extensions.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_extensions_data.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_extensions_pins.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_extensions_layers.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_extensions_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_extensions_pair_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_vector_extensions_kernel.rs'],
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

test('L03-K widget layer record stays a private document child wired into the shared layer admission and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectWidgetRecordRegistration, /L03-K\/#2173/);
  const documentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(documentText, /#\[path = "object_widget_record\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod object_widget_record;/);
  assert.match(documentText, /pub\(crate\) widget: Presence<WidgetRecord>,/);
  const recordText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_widget_record.rs'), 'utf8');
  assert.doesNotMatch(recordText.split('#[cfg(test)]')[0], /super::/);
  for (const [name, mod] of [['object_widget_record_rows', 'oracle_rows'], ['object_widget_record', 'tests'], ['object_widget_record_controls', 'controls_tests']]) {
    assert.match(recordText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  const codecText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_codec.rs'), 'utf8');
  assert.equal((codecText.match(/validate_widget_layers\(declared_layers, objects\)/g) || []).length, 1, 'single shared admission call');
  const oracle = fs.readFileSync(path.join(ROOT, 'native-engine/tests/fixtures/object-widget-record/oracle.txt'), 'utf8');
  assert.equal(oracle.trim().split('\n').length, 61, 'oracle row count');
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_widget_record.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_widget_record.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_widget_record_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_widget_record_controls.rs'],
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

test('L04-G component, StoryBoard and duplicator records stay private document children and fail closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectComponentRecordsRegistration, /L04-G\/#2174/);
  const familyParentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_document.rs'), 'utf8');
  assert.match(familyParentText, /#\[path = "object_component_record_kernels\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_component_record_kernels;/);
  const aggregatorText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_component_record_kernels.rs'), 'utf8');
  for (const name of ['object_component_records', 'object_storyboard_records', 'object_duplicator_records']) {
    assert.match(aggregatorText, new RegExp(`#\\[path = "${name}\\.rs"\\]\\s*#\\[allow\\(dead_code\\)\\]\\s*pub\\(crate\\) mod ${name};`));
  }
  for (const [name, children, nested] of [
    ['object_component_records', [['object_component_records', 'tests'], ['object_component_records_controls', 'controls_tests']], ['object_component_instance', 'instance']],
    ['object_storyboard_records', [['object_storyboard_records', 'tests'], ['object_storyboard_records_controls', 'controls_tests']], null],
    ['object_duplicator_records', [['object_duplicator_records', 'tests'], ['object_duplicator_records_controls', 'controls_tests']], ['object_duplicator_effector', 'effector']],
  ]) {
    const kernelText = fs.readFileSync(path.join(ROOT, `native-engine/src/${name}.rs`), 'utf8');
    for (const [file, mod] of children) {
      assert.match(kernelText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${file}\\.rs"\\]\\s*mod ${mod};`));
    }
    if (nested) {
      assert.match(kernelText, new RegExp(`#\\[path = "${nested[0]}\\.rs"\\]\\s*mod ${nested[1]};`));
    }
  }
  for (const [id, file] of [
    ['rust.native.engine.document', 'native-engine/src/object_component_record_kernels.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_component_records.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_component_instance.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_storyboard_records.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_duplicator_records.rs'],
    ['rust.native.engine.document', 'native-engine/src/object_duplicator_effector.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_records.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_component_records_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_storyboard_records.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_storyboard_records_controls.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_duplicator_records.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_duplicator_records_controls.rs'],
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

test('L06-E per-asset decoded-pixel handle stays a private read-queries child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeObjectRasterAssetResourcesRegistration, /L06-E/);
  const snapshotText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_snapshot.rs'), 'utf8');
  assert.match(snapshotText, /#\[cfg\(all\(feature = "history", feature = "project_media_codec"\)\)\]\s*#\[path = "object_raster_asset_resources\.rs"\]\s*#\[allow\(dead_code\)\]\s*pub\(crate\) mod object_raster_asset_resources;/);
  const codecText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_codec.rs'), 'utf8');
  assert.match(codecText, /#\[cfg\(all\(feature = "history", feature = "project_media_codec"\)\)\]\s*pub\(crate\) use crate::object_document::\{AssetId, AssetInventory, AssetKind\};/);
  const resourcesText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_raster_asset_resources.rs'), 'utf8');
  // Layer discipline: the read-queries file reaches document types only through the codec re-export, never with super::.
  const production = resourcesText.split('#[cfg(test)]')[0];
  assert.doesNotMatch(production, /super::/);
  assert.doesNotMatch(production, /crate::object_document/);
  for (const [name, decl] of [
    ['object_raster_asset_resources_oracle', 'pub\\(crate\\) mod oracle;'],
    ['object_raster_asset_resources', 'mod tests;'],
    ['object_raster_asset_resources_refusals', 'mod refusals;'],
    ['object_raster_asset_resources_cancel', 'mod cancel;'],
    ['object_raster_asset_resources_controls', 'mod controls;'],
  ]) {
    assert.match(resourcesText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*${decl}`));
  }
  for (const [id, file] of [
    ['rust.native.engine.read-queries', 'native-engine/src/object_raster_asset_resources.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_resources.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_resources_oracle.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_resources_refusals.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_resources_cancel.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/object_raster_asset_resources_controls.rs'],
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

test('L07-G brush asset publication stays a private read-queries child and fails closed without registration', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeBitmapBrushResourcesPublishRegistration, /L07-G\/#2192/);
  const parentText = fs.readFileSync(path.join(ROOT, 'native-engine/src/read_queries.rs'), 'utf8');
  assert.match(parentText, /#\[path = "bitmap_brush_resources_publish\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod bitmap_brush_resources_publish;/);
  const codecText = fs.readFileSync(path.join(ROOT, 'native-engine/src/object_codec.rs'), 'utf8');
  assert.match(codecText, /pub\(crate\) use crate::object_document::\{AssetId, AssetInventory, AssetKind\};/);
  const storeText = fs.readFileSync(path.join(ROOT, 'native-engine/src/bitmap_brush_resources_store.rs'), 'utf8');
  assert.match(storeText, /pub\(crate\) fn budget\(&self\) -> &ImageResourceBudget/);
  const moduleText = fs.readFileSync(path.join(ROOT, 'native-engine/src/bitmap_brush_resources_publish.rs'), 'utf8');
  assert.doesNotMatch(moduleText, /super::/);
  assert.doesNotMatch(moduleText, /shared_image_resource_budget/);
  for (const [name, mod] of [
    ['bitmap_brush_resources_publish_controls', 'controls'],
    ['bitmap_brush_resources_publish_rows', 'rows'],
    ['bitmap_brush_resources_publish', 'tests'],
  ]) {
    assert.match(moduleText, new RegExp(`#\\[cfg\\(test\\)\\]\\s*#\\[path = "\\.\\./tests/${name}\\.rs"\\]\\s*mod ${mod};`));
  }
  for (const [id, file] of [
    ['rust.native.engine.read-queries', 'native-engine/src/bitmap_brush_resources_publish.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_publish.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_publish_rows.rs'],
    ['rust.native.engine.tests', 'native-engine/tests/bitmap_brush_resources_publish_controls.rs'],
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

test('L08-F viewport-only overlay gate stays a private desktop-viewport child, imports only the witness owner and never reaches export code', () => {
  const profile = readJson(ROOT, PROFILE);
  const coverage = readJson(ROOT, 'engineering/boundaries/profiles/rust.coverage.json');
  assert.match(coverage.nativeViewportSceneOverlayRegistration, /L08-F\/#2164/);
  const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');
  assert.match(read('native-engine/src/desktop_viewport.rs'),
    /#\[path = "viewport_scene_overlay\.rs"\]\s*#\[allow\(dead_code\)\]\s*mod viewport_scene_overlay;/);
  const gate = read('native-engine/src/viewport_scene_overlay.rs');
  const tests = ["viewport_scene_overlay","viewport_scene_overlay_controls","viewport_scene_overlay_gate","viewport_scene_overlay_viewport"];
  for (const name of tests) {
    assert.match(gate, new RegExp('#\\[cfg\\([^\\]]*test[^\\]]*\\)\\]\\s*#\\[path = "\\.\\./tests/' + name + '\\.rs"\\]\\s*mod \\w+;'));
  }
  // The only crate import is the witness owner; the only class discriminator is is_viewport().
  assert.deepEqual(gate.split('\n').filter((line) => /^use /.test(line)),
    ['use crate::object_render_scene::ObjectCompositionWitness;']);
  // Export, PNG and readback code never name the overlay types; the viewport witness stays refused there.
  const guards = [
    [
      "native-engine/src/viewport_scene_overlay.rs",
      [
        "witness\\.is_viewport\\(\\)"
      ],
      [
        "\\bsuper::",
        "FrameScheduler|Compositor|ExportJob|ScheduledFrameIdentity|\\bwgpu\\b|\\bvello\\b|\\bserde\\b"
      ]
    ],
    [
      "native-engine/src/object_export_job.rs",
      [
        "is_viewport\\(\\)"
      ],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/object_png_output.rs",
      [
        "is_viewport\\(\\)"
      ],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/png_output.rs",
      [],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/export_job.rs",
      [],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/object_compositor.rs",
      [],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/compositor_readback.rs",
      [],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/object_scheduled_scene.rs",
      [],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/object_render_scene.rs",
      [],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ],
    [
      "native-engine/src/render_scene.rs",
      [],
      [
        "viewport_scene_overlay|ViewportOverlay|OverlayCategory|EditorDisplayMode|OverlayDisposition"
      ]
    ]
  ];
  const code = (file) => read(file).split('\n').filter((line) => !/^\s*\/\//.test(line)).join('\n');
  for (const [file, must, mustNot] of guards) {
    const body = code(file);
    for (const source of must) {
      const hit = body.match(new RegExp(source));
      assert.ok(hit, file + ' must keep ' + source);
      assert.ok(!new RegExp(source).test(body.split(hit[0]).join('')), file + ' guard ' + source + ' must fail when removed');
    }
    for (const source of mustNot) {
      assert.doesNotMatch(body, new RegExp(source), file + ' must not reach ' + source);
      const mutant = source.replace(/\\b/g, '').replace(/\\\(/g, '(').replace(/\\\./g, '.');
      assert.match(body + '\n' + mutant, new RegExp(source), file + ' guard ' + source + ' must trip on a mutant');
    }
  }
  const owners = [['rust.native.engine.desktop-viewport', 'native-engine/src/' + "viewport_scene_overlay.rs"]]
    .concat(tests.map((name) => ['rust.native.engine.tests', 'native-engine/tests/' + name + '.rs']));
  for (const [id, file] of owners) {
    const owner = profile.modules.find((entry) => entry.id === id);
    assert.equal(owner?.files.filter((entry) => entry === path.basename(file)).length, 1, id + ' must own ' + file);
    assert.equal(owner.publicApi.includes(path.basename(file)), false, file + ' stays private');
    const missing = structuredClone(profile);
    missing.modules.find((entry) => entry.id === id).files = owner.files.filter((entry) => entry !== path.basename(file));
    const result = checkSourceCoverage(missing, { root: ROOT, sourcePaths: [file] });
    assert.ok(result.violations.some((entry) => entry.rule === 'coverage-unprofiled-source' && entry.file === file),
      file + ' must fail when unregistered');
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
    // Use the full discovered Rust inventory. A partial source list would
    // report every other declared member as missing and hide restoration bugs.
    const d35Profile = readJson(clone, PROFILE);
    const d35Discovered = discoverRepositoryFiles({ root: clone });
    assert.equal(d35Discovered.ok, true);
    assert.deepEqual(d35Discovered.diagnostics, []);
    const d35Sources = d35Discovered.entries.filter((entry) => entry.path.endsWith('.rs')).map((entry) => entry.path);
    const d35Exclusions = readJson(clone, 'engineering/boundaries/profiles/rust.coverage.json').exclusions;
    const d35Coverage = () => checkRustSourceCoverage(readJson(clone, PROFILE), {
      root: clone, sourcePaths: d35Sources, exclusions: d35Exclusions,
    });
    assert.deepEqual(d35Coverage().violations, []);
    for (const [id, dir] of [
      ['rust.native.engine.read-queries', 'src'], ['rust.native.engine.tests', 'tests'],
    ]) {
      const file = 'object_property_metadata_read.rs';
      const relative = `native-engine/${dir}/${file}`;
      assert.ok(d35Sources.includes(relative));
      const sourceBytes = fs.readFileSync(path.join(clone, relative));
      const missing = structuredClone(d35Profile);
      const owner = missing.modules.find((entry) => entry.id === id);
      owner.files = owner.files.filter((name) => name !== file);
      fs.writeFileSync(path.join(clone, PROFILE), JSON.stringify(missing));
      assert.deepEqual(d35Coverage().violations.map((entry) => [entry.rule, entry.file]),
        [['coverage-unprofiled-source', relative]]);
      restore(clone, new Map([[PROFILE, original.get(PROFILE)]]));
      assert.deepEqual(fs.readFileSync(path.join(clone, relative)), sourceBytes);
      assert.deepEqual(fs.readFileSync(path.join(clone, PROFILE)), original.get(PROFILE));
      assert.deepEqual(d35Coverage().violations, []);
    }

    for (const [id, relative] of [["rust.native.engine.tests", "native-engine/tests/object_property_metadata_dispatch.rs"], ["rust.mcp.transport", "nemo-mcp/src/native_property_metadata_contract.rs"], ["rust.mcp.transport", "nemo-mcp/src/native_property_metadata_wire_tests.rs"], ["rust.desktop.native.host.release.tests", "src-tauri/src/native_property_metadata_dispatch_tests.rs"]]) {
      assert.ok(d35Sources.includes(relative));
      const sourceBytes = fs.readFileSync(path.join(clone, relative));
      const missing = structuredClone(d35Profile);
      const owner = missing.modules.find((entry) => entry.id === id);
      const file = relative.slice(owner.dir.length + 1);
      owner.files = owner.files.filter((name) => name !== file);
      fs.writeFileSync(path.join(clone, PROFILE), JSON.stringify(missing));
      assert.deepEqual(d35Coverage().violations.map((entry) => [entry.rule, entry.file]),
        [['coverage-unprofiled-source', relative]]);
      restore(clone, new Map([[PROFILE, original.get(PROFILE)]]));
      assert.deepEqual(fs.readFileSync(path.join(clone, relative)), sourceBytes);
      assert.deepEqual(fs.readFileSync(path.join(clone, PROFILE)), original.get(PROFILE));
      assert.deepEqual(d35Coverage().violations, []);
    }

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
