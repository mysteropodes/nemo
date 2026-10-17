//! Baseline rows: `src/js/application/export-job.js` and the `export.js` SVG file
//! naming at fbe4b8a3, run unmodified in a Node vm (tools/gen.js). Each lifecycle
//! row is replayed through the native SVG job and compared field by field; the one
//! row native deliberately answers differently is asserted as such.
use super::fixture::{
    begin, expected_svg, owner, plan, recolor_layer_two, step, witness, Port, RejectSvg,
};
use super::*;
use crate::export_job::native_media_export::native_media_export_plan::frame_file_name;
use crate::export_job::{CleanupStatus, JobReceipt, JobStatus, ObjectExportJob};
use crate::revision::ObjectSnapshot;
use crate::scheduler::FrameScheduler;

const ORACLE: &str = include_str!("fixtures/native-vector-export-job/oracle.txt");

fn rows(op: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|cols| cols[0] == op)
        .map(|cols| cols[1..].to_vec())
        .collect()
}

struct Observed {
    receipt: JobReceipt,
    port: Port,
    snapshot: ObjectSnapshot,
}

/// The native analogue of one baseline scenario over the frames 5, 6, 7.
fn native(scenario: &str) -> Observed {
    let frames = [5u32, 6, 7];
    let mut history = owner("svg-oracle", &frames);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let port = Port::default();
    match scenario {
        "write_fails_second" => port.set().fail_write_at = Some(2),
        "cleanup_fails" => {
            let mut state = port.set();
            state.fail_write_at = Some(2);
            state.fail_cleanup = true;
        }
        _ => {}
    }
    let mut job_port = port.clone();
    let mut scheduler = FrameScheduler::new();
    let receipt = {
        let mut job = ObjectExportJob::with_format(&mut job_port, &mut scheduler, FrameFormat::Svg);
        begin(&mut job, &history, &snapshot, &frames, 65536);
        match scenario {
            "succeed3" | "changed_between_batches" => {
                for index in 0..3 {
                    step(&mut job, &snapshot, &frames, index);
                    if scenario == "changed_between_batches" && index == 0 {
                        recolor_layer_two(&mut history);
                    }
                }
            }
            "cancel_after_first" => {
                step(&mut job, &snapshot, &frames, 0);
                job.next_frame(Ok(witness(&snapshot, 6))).unwrap();
                let pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
                job.cancel().unwrap();
                job.finish_frame(pending).unwrap();
            }
            "write_fails_second" | "cleanup_fails" => {
                step(&mut job, &snapshot, &frames, 0);
                step(&mut job, &snapshot, &frames, 1);
            }
            "evaluate_fails_first" => {
                assert!(job.start_svg_frame(&mut RejectSvg).unwrap().is_none());
            }
            "replaced_between_batches" => {
                step(&mut job, &snapshot, &frames, 0);
                job.replace_owner(&owner("svg-oracle-replacement", &frames))
                    .unwrap();
            }
            other => panic!("unknown scenario {other}"),
        }
        job.status().unwrap()
    };
    Observed {
        receipt,
        port,
        snapshot,
    }
}

fn status_text(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Running => "running",
        JobStatus::Succeeded => "succeeded",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

/// How each baseline error code is spelled by the native job.
fn native_code(status: &str, code: &str, cleanup_error: &str) -> Option<&'static str> {
    match (status, code, cleanup_error) {
        ("succeeded", "", _) => None,
        ("cancelled", "", _) => Some("cancelled"),
        // Native keeps an unfinished cleanup visible as THE error of the job.
        ("failed", "write_failed", "1") => Some("cleanup_failed"),
        ("failed", "write_failed", _) => Some("write_failed"),
        ("failed", "evaluate_failed", _) => Some("frame_failed"),
        ("cancelled", "document_replaced", _) => Some("authority_released"),
        other => panic!("unmapped baseline row {other:?}"),
    }
}

#[test]
fn baseline_lifecycle_rows_match_the_native_job_observables() {
    let lifecycle = rows("J");
    assert_eq!(lifecycle.len(), 7);
    for row in &lifecycle {
        let (scenario, status, code) = (row[0], row[1], row[2]);
        let (written, removed, artifact) = (row[3], row[4], row[5]);
        let (progress, cleanup_error) = (row[6], row[7]);
        if scenario == "changed_between_batches" {
            continue; // asserted on its own below
        }
        let observed = native(scenario);
        let receipt = &observed.receipt;
        let port = &observed.port;
        assert_eq!(status_text(receipt.status), status, "{scenario}");
        let code_now = receipt.error.as_ref().map(|error| error.code);
        assert_eq!(
            code_now,
            native_code(status, code, cleanup_error),
            "{scenario}"
        );
        assert_eq!(port.st().written.join(","), written, "{scenario} written");
        assert_eq!(port.removed().join(","), removed, "{scenario} removed");
        let files = receipt.artifact.as_ref().map(|a| a.files.join(","));
        assert_eq!(files.unwrap_or_default(), artifact, "{scenario} artifact");
        assert_eq!(
            format!("{}", receipt.progress),
            progress,
            "{scenario} progress"
        );
        let failed_cleanup = receipt.cleanup.status == CleanupStatus::Failed;
        assert_eq!(
            if failed_cleanup { "1" } else { "0" },
            cleanup_error,
            "{scenario}"
        );
        // A failed or cancelled job never reports partial output as an artifact.
        assert!(
            status == "succeeded" || receipt.artifact.is_none(),
            "{scenario}"
        );
        assert!(
            status == "succeeded" || port.st().published.is_empty(),
            "{scenario}"
        );
    }
}

#[test]
fn baseline_document_change_fails_but_the_pinned_native_job_exports_the_original_revision() {
    let row = rows("J")
        .into_iter()
        .find(|row| row[0] == "changed_between_batches")
        .unwrap();
    assert_eq!((row[1], row[2]), ("failed", "document_changed"));
    assert_eq!(row[3], "frame_0001.svg");
    let observed = native("changed_between_batches");
    assert_eq!(observed.receipt.status, JobStatus::Succeeded);
    assert_eq!(observed.receipt.pinned_revision, 0);
    let state = observed.port.st();
    assert_eq!(state.published.len(), 3);
    for ((_, bytes), frame) in state.published.iter().zip([5u32, 6, 7]) {
        let expected = expected_svg(&plan(&observed.snapshot, frame));
        assert_eq!(
            std::str::from_utf8(bytes).unwrap(),
            expected,
            "frame {frame}"
        );
    }
}

#[test]
fn baseline_frame_names_match_the_native_file_names_and_mutants_do_not() {
    let names = rows("N");
    assert_eq!(names.len(), 11);
    for row in &names {
        let ordinal: u64 = row[0].parse().unwrap();
        assert_eq!(frame_file_name(ordinal, FrameFormat::Svg), row[1]);
    }
    let staged = native("succeed3").port.st().written.clone();
    assert_eq!(staged, [names[0][1], names[1][1], "frame_0003.svg"]);
    let mutants: [fn(u64) -> String; 3] = [
        |ordinal| format!("frame_{ordinal}.svg"),
        |ordinal| format!("frame_{:04}.svg", ordinal - 1),
        |ordinal| format!("frame_{:04}.png", ordinal),
    ];
    for mutant in mutants {
        let caught = names
            .iter()
            .any(|row| mutant(row[0].parse().unwrap()) != row[1]);
        assert!(caught, "a naming mutant agrees with every baseline row");
    }
}
