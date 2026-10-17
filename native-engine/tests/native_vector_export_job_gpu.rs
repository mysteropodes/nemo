//! Real headless Metal: the PNG and the SVG of one pinned frame come out of the same
//! job mechanism with the same pins, the same dimensions and the same covered area.
//! No installed-display acceptance; the memory port is not filesystem publication.
use super::fixture::{begin_request, output, owner, pins, plan, staged_files, witness, Port};
use super::*;
use crate::compositor::Compositor;
use crate::export_job::native_media_export::native_media_export_plan::{
    ExportPins, FrameRange, MediaExportPlan, OutputKind,
};
use crate::export_job::native_media_export::native_media_export_verify::verify_output;
use crate::export_job::{ExportJobErrorKind, JobReceipt, JobStatus, ObjectExportJob};
use crate::scheduler::FrameScheduler;
use std::io::Cursor;

fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    (info.width, info.height, rgba)
}

#[test]
fn gpu_png_and_svg_of_one_frame_share_pins_dimensions_and_covered_pixels() {
    let owner = owner("svg-job-gpu", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let mut gpu = Compositor::new().expect("requires real headless Metal");

    let png_port = Port::default();
    let mut png_job_port = png_port.clone();
    let mut png_scheduler = FrameScheduler::new();
    let png_receipt: JobReceipt = {
        let mut job = ObjectExportJob::new(&mut png_job_port, &mut png_scheduler);
        job.begin(
            &owner,
            begin_request(&snapshot, 7, 65536),
            witness(&snapshot, 7),
        )
        .unwrap();
        let pending = job.start_frame(&mut gpu).unwrap().unwrap();
        job.finish_frame(pending).unwrap()
    };

    let svg_port = Port::default();
    let mut svg_job_port = svg_port.clone();
    let mut svg_scheduler = FrameScheduler::new();
    let svg_receipt: JobReceipt = {
        let mut job =
            ObjectExportJob::with_format(&mut svg_job_port, &mut svg_scheduler, FrameFormat::Svg);
        job.begin(
            &owner,
            begin_request(&snapshot, 7, 65536),
            witness(&snapshot, 7),
        )
        .unwrap();
        let pending = job.start_svg_frame(&mut NativeSvgEncoder).unwrap().unwrap();
        job.finish_frame(pending).unwrap()
    };

    // Same pinned frame: identical receipt fields except the artifact file name.
    assert_eq!(png_receipt.status, JobStatus::Succeeded);
    assert_eq!(svg_receipt.status, JobStatus::Succeeded);
    assert_eq!(png_receipt.pinned_revision, svg_receipt.pinned_revision);
    assert_eq!(
        png_receipt.document_snapshot_id,
        svg_receipt.document_snapshot_id
    );
    assert_eq!(png_receipt.progress, svg_receipt.progress);
    let (png_artifact, svg_artifact) =
        (png_receipt.artifact.unwrap(), svg_receipt.artifact.unwrap());
    assert_eq!(png_artifact.target, svg_artifact.target);
    assert_eq!(png_artifact.files, ["frame_0001.png"]);
    assert_eq!(svg_artifact.files, ["frame_0001.svg"]);
    assert_eq!(
        (
            png_scheduler.lease_counters().acquired(),
            svg_scheduler.lease_counters().acquired()
        ),
        (1, 1)
    );

    // Same dimensions, checked independently on both byte streams.
    let png_bytes = png_port.st().published[0].1.clone();
    let (width, height, rgba) = decode(&png_bytes);
    assert_eq!((width, height), (64, 64));
    let svg_files = svg_port.st().published.clone();
    let verified = verify_output(
        &MediaExportPlan::new(
            OutputKind::Still,
            FrameFormat::Svg,
            ExportPins::from_schedule(&pins(&snapshot, 7)),
            output(),
            FrameRange::new(7, 7).unwrap(),
            32,
        )
        .unwrap(),
        &staged_files(&svg_files),
    )
    .unwrap();
    assert_eq!(
        (verified.files[0].width, verified.files[0].height),
        (64, 64)
    );

    // Same covered area: the SVG square (from the plan's contour) against PNG pixels.
    let paint_plan = plan(&snapshot, 7);
    let command = paint_plan.commands().next().unwrap();
    let segments = command.contour().segments();
    let xs = segments.iter().map(|segment| segment.point[0]);
    let ys = segments.iter().map(|segment| segment.point[1]);
    let (x0, x1) = (
        xs.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
    );
    let (y0, y1) = (
        ys.clone().fold(f64::MAX, f64::min),
        ys.fold(f64::MIN, f64::max),
    );
    assert_eq!((x0, x1, y0, y1), (11.0, 35.0, 4.0, 28.0));
    for (x, y, covered) in [
        (20usize, 10usize, true),
        (30, 20, true),
        (12, 5, true),
        (5, 5, false),
        (40, 40, false),
        (60, 60, false),
        (20, 2, false),
        (36, 10, false),
    ] {
        let center = (x as f64 + 0.5, y as f64 + 0.5);
        let inside = x0 <= center.0 && center.0 < x1 && y0 <= center.1 && center.1 < y1;
        assert_eq!(inside, covered, "svg geometry at ({x},{y})");
        let pixel = &rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        let painted = pixel.iter().any(|&channel| channel != 255);
        assert_eq!(painted, covered, "png pixel at ({x},{y}) = {pixel:?}");
    }
}

#[test]
fn gpu_a_png_pending_frame_cannot_be_finished_by_an_svg_job() {
    let owner = owner("svg-job-gpu-cross", &[7]);
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let mut gpu = Compositor::new().expect("requires real headless Metal");
    let (png_port, svg_port) = (Port::default(), Port::default());
    let (mut png_job_port, mut svg_job_port) = (png_port.clone(), svg_port.clone());
    let (mut png_scheduler, mut svg_scheduler) = (FrameScheduler::new(), FrameScheduler::new());
    {
        let mut png_job = ObjectExportJob::new(&mut png_job_port, &mut png_scheduler);
        let mut svg_job =
            ObjectExportJob::with_format(&mut svg_job_port, &mut svg_scheduler, FrameFormat::Svg);
        png_job
            .begin(
                &owner,
                begin_request(&snapshot, 7, 65536),
                witness(&snapshot, 7),
            )
            .unwrap();
        svg_job
            .begin(
                &owner,
                begin_request(&snapshot, 7, 65536),
                witness(&snapshot, 7),
            )
            .unwrap();
        let png_pending = png_job.start_frame(&mut gpu).unwrap().unwrap();
        let svg_pending = svg_job
            .start_svg_frame(&mut NativeSvgEncoder)
            .unwrap()
            .unwrap();
        // Equal job id and work number, different scheduler (work provenance refuses it first; the
        // format check in finish_frame is defense in depth): refused, nothing written.
        let error = svg_job.finish_frame(png_pending).unwrap_err();
        assert_eq!(error.kind, ExportJobErrorKind::InvalidRequest);
        assert_eq!(
            svg_job.finish_frame(svg_pending).unwrap().status,
            JobStatus::Succeeded
        );
        png_job.cancel().unwrap();
    }
    assert_eq!(svg_port.st().events, ["begin", "write", "publish"]);
    assert_eq!(png_port.st().events, ["begin", "cleanup"]);
    assert_eq!(svg_port.st().published.len(), 1);
    assert!(png_port.st().published.is_empty());
    assert_eq!(
        (
            png_scheduler.lease_counters().live(),
            svg_scheduler.lease_counters().live()
        ),
        (0, 0)
    );
}
