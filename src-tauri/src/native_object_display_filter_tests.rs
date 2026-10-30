use super::support::{document, Fixture};
use crate::native_object_desktop_preview::admit_request;
use native_engine::desktop_viewport::ViewportDisplayFilter;
use serde_json::json;

#[test]
fn display_filter_toggle_acquires_fresh_work_without_changing_source_or_document() {
    let f = Fixture::with_document(
        json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":document(),"viewportReference":null,"assets":[]}),
    );
    let before = f.retained_project();
    let mut request = f.raw.clone();
    let mut works = Vec::new();
    for filter in ["none", "values", "none", "contrast", "dim", "mirrored", "values-mirrored"] {
        request["displayFilter"] = json!(filter);
        let receipt = f.run(&request.to_string()).unwrap();
        assert_eq!(receipt.status, "presented");
        assert_eq!(
            serde_json::to_value(receipt.display_filter).unwrap(),
            filter
        );
        assert!(!works.contains(&receipt.work_id));
        works.push(receipt.work_id.clone());
        // The support port reads the actual composition texture every time:
        // blue artwork and white pasteboard remain byte-identical before blit.
        assert_eq!(f.retained_project(), before);
    }
    assert_eq!(f.trace.borrow().acquisitions, 7);
    assert_eq!(
        f.trace.borrow().filters,
        [
            ViewportDisplayFilter::None,
            ViewportDisplayFilter::Values,
            ViewportDisplayFilter::None,
            ViewportDisplayFilter::Contrast,
            ViewportDisplayFilter::Dim,
            ViewportDisplayFilter::Mirrored,
            ViewportDisplayFilter::ValuesMirrored
        ]
    );
    assert_eq!(f.creations.get(), 1);
    f.release();
}

#[test]
fn deferred_filter_retry_keeps_work_but_changed_filter_schedules_new_work() {
    let f = Fixture::new();
    let mut request = f.raw.clone();
    request["displayFilter"] = json!("values");
    f.trace.borrow_mut().mode = 1;
    let first = f.run(&request.to_string()).unwrap();
    assert_eq!(first.status, "deferred-timeout");
    let retry = f.run(&request.to_string()).unwrap();
    assert_eq!(retry.work_id, first.work_id);
    assert_eq!(retry.view_generation, first.view_generation);
    request["displayFilter"] = json!("dim");
    f.trace.borrow_mut().mode = 0;
    let changed = f.run(&request.to_string()).unwrap();
    assert_eq!(changed.status, "presented");
    assert_ne!(changed.work_id, first.work_id);
    assert!(changed.view_generation > first.view_generation);
    assert_eq!(changed.display_filter, ViewportDisplayFilter::Dim);
    assert_eq!(
        f.trace.borrow().filters,
        [
            ViewportDisplayFilter::Values,
            ViewportDisplayFilter::Values,
            ViewportDisplayFilter::Dim
        ]
    );
    f.release();
}

#[test]
fn display_filter_admission_is_closed_before_target_creation() {
    let f = Fixture::new();
    for value in [
        json!("grayscale"),
        json!("blur(2px)"),
        json!("toString"),
        json!(null),
        json!(true),
        json!({"kind":"values"}),
    ] {
        let mut request = f.raw.clone();
        request["displayFilter"] = value;
        assert!(admit_request(&request.to_string(), f.state.instance_id()).is_err());
        assert!(f.run(&request.to_string()).is_err());
    }
    assert_eq!(f.creations.get(), 0);
    assert_eq!(f.trace.borrow().acquisitions, 0);
    f.release();
}

#[test]
fn failed_filter_cancellation_does_not_acquire_or_publish_replacement() {
    let f = Fixture::new();
    let mut request = f.raw.clone();
    request["displayFilter"] = json!("values");
    f.trace.borrow_mut().mode = 1;
    assert_eq!(
        f.run(&request.to_string()).unwrap().status,
        "deferred-timeout"
    );
    f.fail_cancel(true);
    request["displayFilter"] = json!("dim");
    assert!(f.run(&request.to_string()).is_err());
    assert_eq!(f.trace.borrow().acquisitions, 1);
    assert_eq!(f.trace.borrow().filters, [ViewportDisplayFilter::Values]);
    f.fail_cancel(false);
    f.release();
}

#[test]
fn surface_recovery_keeps_the_admitted_display_filter() {
    for mode in [3, 4] {
        let f = Fixture::new();
        let mut request = f.raw.clone();
        request["displayFilter"] = json!("values");
        f.trace.borrow_mut().mode = mode;
        let receipt = f.run(&request.to_string()).unwrap();
        assert_eq!(receipt.status, "presented");
        assert_eq!(receipt.display_filter, ViewportDisplayFilter::Values);
        assert_eq!(f.trace.borrow().filters, [ViewportDisplayFilter::Values; 2]);
        f.release();
    }
}
