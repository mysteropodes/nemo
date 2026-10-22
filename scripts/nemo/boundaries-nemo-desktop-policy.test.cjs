'use strict';
// Adopted desktop Rust graph and exact external-port policy acceptance.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const R = require('./lib/boundaries-rust.cjs');
const ROOT = path.resolve(__dirname, '../..');
const read = (name) => JSON.parse(fs.readFileSync(path.join(ROOT, 'engineering/boundaries/profiles', name), 'utf8'));

test('adopted nemo-desktop policy holds at HEAD: exact edges, no debt, MCP and native-engine ports pinned', () => {
  const policy = read('nemo-desktop.edges.json');
  const r = R.checkRustCrate(read('rust.profile.json'), policy, { root: ROOT });
  assert.equal(r.ok, true, JSON.stringify(r.violations));
  assert.equal(r.moduleCount, policy.profileModules.length);
  assert.deepEqual(r.unsupported, []);
  assert.deepEqual(r.exceptionsApplied, []);
  assert.deepEqual(r.exportedPort, []);
  assert.deepEqual(r.edges.map((e) => `${e.from}->${e.to}`).sort(), [
    'rust.desktop.mcp.adapter->rust.desktop.native.dispatch',
    'rust.desktop.mcp.adapter.tests->rust.desktop.mcp.adapter',
    'rust.desktop.mcp.adapter.tests->rust.desktop.native.application',
    'rust.desktop.mcp.adapter.tests->rust.desktop.native.application.contract',
    'rust.desktop.native.application->rust.desktop.native.application.contract',
    'rust.desktop.native.application->rust.desktop.native.application.ports',
    'rust.desktop.native.application->rust.desktop.native.dispatch',
    'rust.desktop.native.application.commands->rust.desktop.mcp.adapter',
    'rust.desktop.native.application.commands->rust.desktop.native.application',
    'rust.desktop.native.application.commands->rust.desktop.native.application.contract',
    'rust.desktop.native.application.commands->rust.desktop.native.application.ports',
    'rust.desktop.native.application.commands->rust.desktop.native.application.viewport',
    'rust.desktop.native.application.commands->rust.desktop.native.dispatch',
    'rust.desktop.native.application.contract->rust.desktop.native.application.ports',
    'rust.desktop.native.application.ports.tests->rust.desktop.native.application.ports',
    'rust.desktop.native.application.tests->rust.desktop.native.application',
    'rust.desktop.native.application.tests->rust.desktop.native.application.contract',
    'rust.desktop.native.application.tests->rust.desktop.native.application.ports',
    'rust.desktop.native.application.tests->rust.desktop.native.application.viewport',
    'rust.desktop.native.application.tests->rust.desktop.native.viewport',
    'rust.desktop.native.application.viewport->rust.desktop.native.application.contract',
    'rust.desktop.native.application.viewport->rust.desktop.native.viewport',
    'rust.desktop.native.host.release.tests->rust.desktop.mcp.adapter',
    'rust.desktop.native.host.release.tests->rust.desktop.media',
    'rust.desktop.native.host.release.tests->rust.desktop.native.application',
    'rust.desktop.native.host.release.tests->rust.desktop.native.application.commands',
    'rust.desktop.native.host.release.tests->rust.desktop.native.application.contract',
    'rust.desktop.native.host.release.tests->rust.desktop.native.application.ports',
    'rust.desktop.native.host.release.tests->rust.desktop.native.application.viewport',
    'rust.desktop.native.host.release.tests->rust.desktop.native.dispatch',
    'rust.desktop.native.host.release.tests->rust.desktop.shell',
    'rust.desktop.shell->rust.desktop.mcp.adapter', 'rust.desktop.shell->rust.desktop.media',
    'rust.desktop.shell->rust.desktop.native.application',
    'rust.desktop.shell->rust.desktop.native.application.commands',
    'rust.desktop.shell->rust.desktop.native.application.contract',
    'rust.desktop.shell->rust.desktop.native.application.ports',
    'rust.desktop.shell->rust.desktop.native.application.viewport',
    'rust.desktop.shell->rust.desktop.native.dispatch',
    'rust.desktop.shell->rust.desktop.native.viewport', 'rust.desktop.shell->rust.desktop.play-observer', 'rust.desktop.shell->rust.desktop.tasks', 'rust.desktop.tasks->rust.desktop.play-observer',
    'rust.desktop.tasks.tests->rust.desktop.play-observer', 'rust.desktop.tasks.tests->rust.desktop.tasks']);
  assert.deepEqual(r.unanalyzedModules.slice().sort(),
    ['rust.desktop.build', 'rust.mcp.transport', 'rust.native.engine']);

  const application = fs.readFileSync(path.join(ROOT, 'src-tauri/src/application_mcp.rs'), 'utf8');
  const applicationTests = fs.readFileSync(path.join(ROOT, 'src-tauri/src/application_mcp_tests.rs'), 'utf8');
  const viewport = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_viewport.rs'), 'utf8');
  const ports = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_application_ports.rs'), 'utf8');
  const bootstrap = fs.readFileSync(path.join(ROOT, 'src-tauri/src/native_application.rs'), 'utf8');
  assert.match(application, /use nemo_mcp::\{/);
  assert.match(application, /registry::\{self, Endpoint, Registration\}/);
  assert.match(application, /\bwire,/);
  assert.match(application, /nemo_mcp::BUILD_SOURCE_ID/);
  for (const symbol of ['ApplicationRequest', 'ApplicationResponse', 'NativeApplicationError',
    'NativeApplicationRequest', 'NativeApplicationResponse', 'NativeHostStatus', 'NativeStatusRequest',
    'NATIVE_API_VERSION', 'Operation']) assert.match(application, new RegExp(`\\b${symbol}\\b`));
  assert.deepEqual(policy.externalCratePorts.nemo_mcp.items.slice().sort(),
    ['BUILD_SOURCE_ID', 'contract::ApplicationRequest', 'contract::ApplicationResponse',
      'contract::NativeApplicationError', 'contract::NativeApplicationRequest',
      'contract::NativeApplicationResponse', 'contract::NativeHostStatus',
      'contract::NativeStatusRequest', 'contract::NATIVE_API_VERSION', 'contract::Operation',
      'registry', 'wire'].sort());
  assert.deepEqual(policy.externalCratePorts.nemo_mcp.allowedModules.slice().sort(), [
    'rust.desktop.mcp.adapter', 'rust.desktop.native.application.commands',
    'rust.desktop.native.application.contract', 'rust.desktop.native.application.tests',
    'rust.desktop.native.host.release.tests',
  ]);

  assert.match(applicationTests, /use crate::application_mcp::\*/);
  assert.match(application, /use native_engine::\{/);
  assert.match(applicationTests, /use native_engine::\{/);
  assert.match(viewport, /use native_engine::compositor::\{/);
  assert.match(viewport, /use native_engine::desktop_viewport::\{/);
  assert.match(ports, /use native_engine::application::\{/);
  assert.match(ports, /use native_engine::png_output::\{/);
  assert.match(bootstrap, /use native_engine::\{/);
  assert.equal(policy.externalCratePorts.native_engine.allowedModules, undefined);
  assert.equal(policy.externalCratePorts.native_engine.items, undefined);
  assert.deepEqual(policy.externalCratePorts.native_engine.moduleItems, {
    'rust.desktop.mcp.adapter': [
      'application::ExportResourceResolver', 'application::NativeApplication', 'commands::OpacityRequest',
      'compositor::Compositor', 'desktop_viewport::DesktopViewportHost',
      'desktop_viewport::PresentationReceipt', 'desktop_viewport::SurfacePort', 'document::OpacityDocument',
      'object_document::ObjectDocument', 'object_frame_packet::ObjectFrameSelector',
      'export_job::ExportCompositor', 'export_job::JobReceipt', 'export_job::StagedArtifactPort',
      'protocol::OP_JOB_EXPORT_PNG_BEGIN', 'scheduler::OutputSpec', 'object_document::FrameScopeKind',
      'export_job::MAX_OBJECT_EXPORT_FRAMES'],
    'rust.desktop.mcp.adapter.tests': [
      'application::REPRODUCTION_FIXTURE',
      'application::ResourceResolutionError', 'application::ResourceResolutionErrorKind',
      'codec::decode_project', 'export_job::ExportArtifact', 'export_job::ExportReadback',
      'protocol::OpaqueResourceHandle', 'render_scene::GeometryPaintInput', 'render_scene::RenderScene'],
    'rust.desktop.native.viewport': [
      'compositor::CompositionResult', 'compositor::Compositor', 'compositor::CompositorError',
      'compositor::CompositorInstance', 'desktop_viewport::AcquiredSurfaceFrame',
      'desktop_viewport::CssPoint', 'desktop_viewport::DesktopViewportHost', 'desktop_viewport::PresentationReceipt',
      'desktop_viewport::SettledSurfaceFrame', 'desktop_viewport::SurfaceAttempt',
      'desktop_viewport::SurfacePort', 'desktop_viewport::SurfacePresentation',
      'desktop_viewport::SurfaceRecoveryError', 'desktop_viewport::ViewportError',
      'desktop_viewport::ViewportMapping', 'render_scene::ScheduledFrameIdentity', 'scheduler::WorkId'],
    'rust.desktop.native.application.ports': [
      'application::ExportResourceResolver', 'application::ResourceResolutionError',
      'application::ResourceResolutionErrorKind', 'compositor::CompositionResult', 'compositor::Compositor',
      'png_output::ExportArtifact', 'png_output::ExportCompositor', 'png_output::ExportReadback',
      'png_output::StagedArtifactPort', 'protocol::OpaqueResourceHandle',
      'render_scene::GeometryPaintInput', 'render_scene::RenderScene'],
    'rust.desktop.native.application.contract': [
      'codec::decode_project', 'desktop_viewport::CssBounds', 'desktop_viewport::PhysicalExtent',
      'desktop_viewport::ViewportMapping', 'document::OpacityDocument', 'export_job::CleanupStatus',
      'export_job::ExternalEffectDisposition', 'export_job::JobReceipt', 'export_job::JobStatus',
      'render_geometry::ClosedCubicPath', 'render_geometry::CubicSegment',
      'render_scene::GeometryPaintInput', 'render_scene::LayerGeometry', 'render_scene::OpaqueSrgbPaint',
      'resource_leases::WorkId', 'scheduler::OutputSpec'],
    'rust.desktop.native.application.viewport': [
      'compositor::CompositionResult', 'compositor::Compositor', 'desktop_viewport::DeferredAction',
      'desktop_viewport::FatalAction', 'desktop_viewport::ViewportMapping', 'desktop_viewport::ViewportStatus',
      'render_scene::ScheduledFrameIdentity', 'resource_leases::WorkId'],
    'rust.desktop.native.application': [
      'application::ApplicationReleaseReceipt', 'application::NativeApplication',
      'application::ReproductionStatus', 'application::REPRODUCTION_FIXTURE',
      'commands::OpacityRequest', 'commands::ResponseEnvelope',
      'compositor::CompositionResult', 'document::OpacityDocument',
      'export_job::JobReceipt', 'export_job::PendingFrame', 'export_job::ReconciliationStage',
      'render_scene',
      'resource_leases::FrameFailure', 'resource_leases::FrameFailureKind', 'resource_leases::WorkId',
      'scheduler::EvaluationKey', 'scheduler::FrameScheduler'],
    'rust.desktop.native.application.commands': [
      'application::ReproductionFixture', 'application::ReproductionStatus',
      'application::REPRODUCTION_FIXTURE', 'application::replay_reproduction_bundle',
      'commands::OpacityRequest', 'commands::ResponseEnvelope', 'compositor::Compositor',
      'desktop_viewport::DesktopViewportHost', 'desktop_viewport::SurfacePort',
      'desktop_viewport::ViewportMapping', 'export_job::ExportArtifact',
      'export_job::ExternalEffectDisposition', 'export_job::JobStatus', 'export_job::ObjectHostExportOutcome',
      'export_job::StagedArtifactPort', 'export_job::MAX_OBJECT_EXPORT_FRAMES',
      'history::PreparedProjectReplacement', 'object_codec::decode_storage_project',
      'object_document::FrameScopeKind', 'object_frame_packet::ObjectFrameSelector', 'resource_leases::WorkId',
      'scheduler::OutputSpec', 'desktop_viewport::ObjectPreviewView',
      'desktop_viewport::ObjectPreviewNavigation', 'desktop_viewport::ObjectViewProjection',
      'desktop_viewport::ObjectPresentedFrame'],
    'rust.desktop.native.application.tests': [
      'application::ExportResourceResolver', 'compositor::Compositor',
      'desktop_viewport::AcquiredSurfaceFrame', 'desktop_viewport::DesktopViewportHost',
      'desktop_viewport::SettledSurfaceFrame', 'desktop_viewport::SurfaceAttempt',
      'desktop_viewport::SurfacePort', 'desktop_viewport::SurfacePresentation',
      'desktop_viewport::SurfaceRecoveryError', 'desktop_viewport::ViewportMapping',
      'export_job::JobStatus'],
    'rust.desktop.native.host.release.tests': [
      'application::ApplicationReleaseReceipt', 'codec::decode_project', 'commands::NativeOpacityApplication',
      'commands::OpacityRequest', 'commands::ResponseEnvelope', 'compositor::Compositor',
      'desktop_viewport::AcquiredSurfaceFrame', 'desktop_viewport::CssBounds',
      'desktop_viewport::DesktopViewportHost', 'desktop_viewport::PhysicalExtent',
      'desktop_viewport::PresentationReceipt', 'desktop_viewport::SettledSurfaceFrame',
      'desktop_viewport::SurfaceAttempt', 'desktop_viewport::SurfacePort',
      'desktop_viewport::SurfacePresentation', 'desktop_viewport::SurfaceRecoveryError',
      'desktop_viewport::ViewportMapping', 'desktop_viewport::ViewportStatus', 'document::OpacityDocument',
      'export_job::CleanupStatus', 'export_job::ExportArtifact', 'export_job::ExportReleaseReconciliation',
      'export_job::ExternalEffectDisposition', 'export_job::JobReceipt', 'export_job::JobStatus',
      'export_job::PendingFrame', 'export_job::ReconciliationStage', 'export_job::StagedArtifactPort',
      'history::PreparedProjectReplacement', 'object_codec::decode_project',
      'object_codec::decode_storage_project', 'object_document::FrameScopeKind',
      'object_frame_packet::ObjectFrameSelector', 'object_snapshot::ObjectSnapshot',
      'render_scene::ScheduledFrameIdentity', 'resource_leases::WorkId', 'scheduler::OutputSpec',
      'object_codec::encode_storage_project', 'desktop_viewport::ObjectPreviewView',
      'object_render_scene::prepare_object_camera_viewport_composition'],
    'rust.desktop.native.dispatch': [
      'application::ApplicationReleaseReceipt', 'application::ExportResourceResolver',
      'application::NativeApplication', 'commands::DispatchErrorCode', 'commands::OpacityRequest',
      'commands::ResponseEnvelope', 'compositor::Compositor', 'desktop_viewport::DesktopViewportHost',
      'desktop_viewport::ObjectViewportFrame', 'desktop_viewport::PresentationReceipt',
      'desktop_viewport::SurfacePort', 'desktop_viewport::ViewportStatus', 'document::OpacityDocument',
      'export_job::CleanupStatus', 'export_job::ExportCompositor', 'export_job::ExportReleaseReconciliation',
      'export_job::ExternalEffectDisposition', 'export_job::JobReceipt', 'export_job::JobStatus',
      'export_job::ObjectHostExportOutcome', 'export_job::ObjectHostExportRequest', 'export_job::PendingFrame',
      'export_job::ReconciliationStage', 'export_job::StagedArtifactPort',
      'export_job::export_object_svg_frames', 'history::InactiveProject', 'history::NativeObjectHistory',
      'history::PreparedProjectReplacement', 'native_media_frame_export::NativeMediaRelease',
      'native_media_frame_export::NativeVideoDecoder', 'native_media_frame_export::NativeVideoInfo',
      'native_media_frame_export::NativeVideoPort', 'native_media_frame_export::begin_native_media_frames',
      'native_media_frame_export::declared_scene_resources',
      'native_media_frame_export::with_placement_source_frame_count', 'object_document::ObjectDocument',
      'object_frame_packet::ObjectFrameSelector', 'object_render_scene::prepare_object_paint_plan',
      'object_render_scene::ObjectPaintPlan', 'object_render_scene::prepare_object_composition',
      'object_render_scene::prepare_object_render_scene',
      'object_render_scene::prepare_scheduled_object_composition_from_witness',
      'object_render_scene::ObjectCompositionWitness', 'object_snapshot::ObjectSnapshot',
      'resource_leases::WorkId', 'scheduler::EvaluationKey', 'scheduler::FrameScheduler',
      'scheduler::ObjectSchedulePins', 'scheduler::ObjectScheduleScope', 'scheduler::OutputSpec',
      'scheduler::ScheduledFrame', 'application::prepare_object_motion_exposure_frame_for_control',
      'application::prepare_object_motion_fallback_frame_for_control', 'application::ObjectMotionFrameControl',
      'application::finalize_object_motion_frame_effects_for_owner', 'application::ObjectMotionEffectsCommit',
      'object_document::PropertyHolderTarget', 'export_job::MAX_OBJECT_EXPORT_FRAMES',
      'desktop_viewport::ObjectPreviewView', 'desktop_viewport::ObjectPreviewNavigation',
      'desktop_viewport::ObjectViewProjection', 'desktop_viewport::ObjectPresentedFrame',
      'object_render_scene::prepare_object_navigated_viewport_composition', 'application::ObjectBrushRuntime',
      'object_render_scene::ObjectBrushAdmission', 'export_job::export_object_frames_with_materializer'],
    'rust.desktop.media': [
      'native_media_frame_export::NativeVideoDecoder', 'native_media_frame_export::NativeVideoInfo'],
  });
  const rustProfile = read('rust.profile.json');
  for (const [moduleId, items] of Object.entries(policy.externalCratePorts.native_engine.moduleItems)) {
    const module = rustProfile.modules.find((entry) => entry.id === moduleId);
    const refs = module.files.flatMap((file) => {
      const source = fs.readFileSync(path.join(ROOT, module.dir, file), 'utf8');
      const analysis = R.analyzeRustSource(source, { externalCrates: ['native_engine'] });
      return analysis.uses.concat(analysis.inline)
        .filter((entry) => entry.segments[0] === 'native_engine')
        .map((entry) => entry.segments.slice(1).join('::'));
    });
    for (const item of items) assert.ok(refs.some((ref) => ref === item || ref.startsWith(`${item}::`)),
      `${moduleId} grants unused native_engine item ${item}`);
  }
});

test('temporal preview and export require the precise owner-controlled exposure port, not an exact-frame fallback grant', () => {
  const profile = read('rust.profile.json'), policy = read('nemo-desktop.edges.json');
  const moduleId = 'rust.desktop.native.dispatch';
  const item = 'application::prepare_object_motion_exposure_frame_for_control';
  const items = policy.externalCratePorts.native_engine.moduleItems[moduleId];
  assert.equal(items.filter((value) => value === item).length, 1);
  assert.equal(items.includes('object_frame_packet::prepare_object_frame'), false);
  const admitted = R.checkRustCrate(profile, policy, { root: ROOT });
  assert.equal(admitted.ok, true, JSON.stringify(admitted.violations));
  const denied = structuredClone(policy);
  denied.externalCratePorts.native_engine.moduleItems[moduleId] = items.filter((value) => value !== item)
    .concat('object_frame_packet::prepare_object_frame');
  const result = R.checkRustCrate(profile, denied, { root: ROOT });
  assert.equal(result.ok, false);
  assert.equal(result.violations.length, 2, JSON.stringify(result.violations));
  assert.ok(result.violations.every((violation) => violation.rule === 'private-port-access'
    && violation.module === moduleId && violation.detail.path === item));
  assert.deepEqual(result.violations.map((violation) => violation.file).sort(),
    ['src-tauri/src/native_object_export.rs', 'src-tauri/src/native_object_preview_prepare.rs']);
});
