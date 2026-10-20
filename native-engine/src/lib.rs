//! Scaffold for the native opacity slice; no production document authority.
//!
//! Feature names reserve the N07–N15 module seams. Their implementation files
//! arrive in their owning leaves; enabling an unfinished feature must fail.
//! See the crate README and ADR-001 for dependency and activation boundaries.

// rustfmt does not expand this declaration-only macro. Cargo can therefore
// format the scaffold before later leaves add the disabled module files.
macro_rules! engine_modules {
    ($($feature:literal => $($module:ident),+;)*) => {
        $( $( #[cfg(feature = $feature)] pub mod $module; )+ )*
    };
}

engine_modules! {
    "codec" => codec, document, project_structure, object_document, object_codec;
    "commands" => commands, request_receipts, revision, object_snapshot;
    "history" => history, transaction;
    "evaluation" => evaluation, animation_curve, object_frame_packet;
    "scheduler" => resource_leases, scheduler;
    "compositor" => compositor, render_scene, render_geometry, object_render_scene;
    "viewport" => desktop_viewport;
    "export_job" => export_job, export_job_lifecycle, png_output;
    "application" => application, protocol, read_queries, native_media_frame_export;
}

#[cfg(feature = "reference_image_asset")]
pub(crate) mod reference_image_asset;

#[cfg(feature = "object_expression_pin")]
pub(crate) mod object_expression_pin;
#[cfg(feature = "object_expression_runtime")]
pub(crate) mod object_expression_runtime;

// Each named Cargo target uses this existing file as a harness. Selecting its
// test feature resolves the real future test file; absent tests fail compilation.
// No feature enabled means no test module, not a placeholder passing test.
#[cfg(test)]
macro_rules! test_modules {
    ($($feature:literal => $module:ident = $path:literal;)*) => {
        $( #[cfg(feature = $feature)] #[path = $path] mod $module; )*
    };
}

#[cfg(test)]
test_modules! {
    "test-codec" => codec_tests = "../tests/codec.rs";
    "test-codec" => project_structure_tests = "../tests/project_structure.rs";
    "test-codec" => object_codec_tests = "../tests/object_codec.rs";
    "test-codec" => reference_image_asset_tests = "../tests/reference_image_asset.rs";
    "test-commands" => commands_tests = "../tests/commands.rs";
    "test-commands" => object_read_tests = "../tests/object_read.rs";
    "test-commands" => object_fill_tests = "../tests/object_fill.rs";
    "test-history" => history_tests = "../tests/history.rs";
    "test-history" => object_history_tests = "../tests/object_history.rs";
    "test-evaluation" => evaluation_tests = "../tests/evaluation.rs";
    "test-evaluation" => object_frame_packet_evaluation_tests = "../tests/object_frame_packet.rs";
    "test-evaluation" => expression_noise_tests = "../tests/expression_noise.rs";
    "test-evaluation" => expression_random_tests = "../tests/expression_random.rs";
    "test-evaluation" => expression_math_tests = "../tests/expression_math.rs";
    "test-evaluation" => expression_time_tests = "../tests/expression_time.rs";
    "test-evaluation" => motion_parent_chain_tests = "../tests/motion_parent_chain.rs";
    "test-evaluation" => motion_parent_chain_controls_tests = "../tests/motion_parent_chain_controls.rs";
    "test-evaluation" => property_expression_runtime_tests = "../tests/property_expression_runtime.rs";
    "test-evaluation" => property_expression_runtime_controls_tests = "../tests/property_expression_runtime_controls.rs";
    "test-evaluation" => property_transform_eval_tests = "../tests/property_transform_eval.rs";
    "test-evaluation" => property_transform_eval_controls_tests = "../tests/property_transform_eval_controls.rs";
    "test-evaluation" => property_follow_path_sample_tests = "../tests/property_follow_path_sample.rs";
    "test-evaluation" => property_follow_path_sample_controls_tests = "../tests/property_follow_path_sample_controls.rs";
    "test-evaluation" => property_expression_views_tests = "../tests/property_expression_views.rs";
    "test-evaluation" => property_expression_lookup_tests = "../tests/property_expression_lookup.rs";
    "test-evaluation" => property_expression_views_controls_tests = "../tests/property_expression_views_controls.rs";
    "test-scheduler" => scheduler_tests = "../tests/scheduler.rs";
    "test-compositor" => compositor_tests = "../tests/compositor.rs";
    "test-compositor" => render_geometry_tests = "../tests/render_geometry.rs";
    "test-compositor" => object_render_scene_compositor_tests = "../tests/object_render_scene.rs";
    "test-viewport" => viewport_tests = "../tests/desktop_viewport.rs";
    "test-export_job" => export_job_tests = "../tests/export_job.rs";
    "test-export_job" => authored_curve_consumers_tests = "../tests/authored_curve_consumers.rs";
    "test-export_job" => export_geometry_tests = "../tests/render_geometry.rs";
    "test-export_job" => export_job_lifecycle_tests = "../tests/export_job_lifecycle.rs";
    "test-application" => application_tests = "../tests/application.rs";
    "test-application" => application_read_tests = "../tests/application_read.rs";
    "test-application" => application_replacement_tests = "../tests/application_replacement.rs";
    "test-application" => object_application_read_tests = "../tests/object_read.rs";
    "test-application" => object_frame_packet_application_tests = "../tests/object_frame_packet.rs";
    "test-application" => object_render_scene_application_tests = "../tests/object_render_scene.rs";
}

#[cfg(all(test, feature = "test-object_expression_pin"))]
#[path = "../tests/object_expression_pin.rs"]
mod object_expression_pin_tests;

#[cfg(all(test, feature = "test-object_expression_runtime"))]
#[path = "../tests/object_expression_runtime.rs"]
mod object_expression_runtime_tests;

#[cfg(all(test, feature = "export_job", feature = "project_media_codec"))]
#[path = "../tests/object_stored_gradient.rs"]
mod object_stored_gradient_tests;
