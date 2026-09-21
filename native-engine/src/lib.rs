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
    "codec" => codec, document;
    "commands" => commands, request_receipts, revision;
    "history" => history, transaction;
    "evaluation" => evaluation;
    "scheduler" => resource_leases, scheduler;
    "compositor" => compositor, render_scene;
    "viewport" => desktop_viewport;
    "export_job" => export_job, png_output;
    "application" => application, protocol, read_queries;
}

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
    "test-commands" => commands_tests = "../tests/commands.rs";
    "test-history" => history_tests = "../tests/history.rs";
    "test-evaluation" => evaluation_tests = "../tests/evaluation.rs";
    "test-scheduler" => scheduler_tests = "../tests/scheduler.rs";
    "test-compositor" => compositor_tests = "../tests/compositor.rs";
    "test-viewport" => viewport_tests = "../tests/desktop_viewport.rs";
    "test-export_job" => export_job_tests = "../tests/export_job.rs";
    "test-application" => application_tests = "../tests/application.rs";
    "test-application" => application_read_tests = "../tests/application_read.rs";
}
