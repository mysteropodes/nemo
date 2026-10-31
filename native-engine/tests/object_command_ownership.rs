//! CS2 (#2345) negative control: every `command.document.object.*` operation declared in
//! `engineering/application/capabilities-v2/*.json` is owned by exactly one of the hand-routed
//! legacy chain, a registered command family, or the pinned "routed elsewhere" list below.
//! A declared operation without a handler fails here, and so do two families (or a family and the
//! legacy chain) claiming one operation.
use super::{families, NativeObjectHistory};
use crate::request_receipts::OpacityRequest;

include!(concat!(env!("OUT_DIR"), "/declared_object_operations.rs"));

/// Declared operations this dispatcher does not route, each with the owner that does. Add an entry
/// only with a reason; prefer registering a family.
const ROUTED_ELSEWHERE: &[(&str, &str)] = &[
    (
        "command.document.object.asset.attach",
        "object_raster_asset_dispatch.rs (raster asset attach/remove route)",
    ),
    (
        "command.document.object.asset.remove",
        "object_raster_asset_dispatch.rs (raster asset attach/remove route)",
    ),
    (
        "command.document.object.asset.font.import",
        "object_font_asset_history.rs (font asset import route)",
    ),
    (
        "command.document.object.asset.video.link",
        "object_media_video_link_history.rs (host probe then dispatch_video_link route)",
    ),
    (
        "command.document.object.brush.bitmap.publish",
        "object_brush_bitmap_route.rs (ObjectBrushRuntime::dispatch_bitmap_resource: the host routes the brush resource publish/read to the session-owned brush store before the object command chain)",
    ),
    (
        "command.document.object.brush.bitmap.stroke",
        "object_brush_bitmap_stroke_history.rs (image byte-port route: the host reads the staged PNG)",
    ),
];

/// Declared operations whose only owner is a family gated on the `application` feature (the
/// tween family: it needs the app-level history and export modules). Without that feature no
/// target builds the owner, so the declaration is expected to be unowned there; the
/// `application` build must own every one of them exactly once.
const APPLICATION_ONLY: &[&str] = &[
    "command.document.object.tween.generate",
    "command.document.object.tween.regenerate",
    "command.document.object.tween.split",
    "command.document.object.tween.reassign",
];

fn probe(operation: &str) -> OpacityRequest {
    serde_json::from_value(
        serde_json::json!({"apiVersion":2,"requestId":"probe","instanceId":"probe",
        "documentId":"probe","operation":operation,"payload":{}}),
    )
    .expect("probe request")
}

fn owners(operation: &str) -> Vec<String> {
    let mut owners = Vec::new();
    if !NativeObjectHistory::legacy_chain_declines(&probe(operation)) {
        owners.push("legacy chain".to_owned());
    }
    owners.extend(
        families::FAMILIES
            .iter()
            .filter(|family| (family.owns)(operation))
            .map(|family| format!("family {}", family.name)),
    );
    if let Some((_, reason)) = ROUTED_ELSEWHERE.iter().find(|(op, _)| *op == operation) {
        assert!(
            !reason.is_empty(),
            "{operation}: routed-elsewhere needs a reason"
        );
        owners.push(format!("routed elsewhere ({reason})"));
    }
    owners
}

#[test]
fn every_declared_object_operation_has_exactly_one_owner() {
    let mut problems = Vec::new();
    for operation in DECLARED_OBJECT_OPERATIONS {
        if !cfg!(feature = "application") && APPLICATION_ONLY.contains(operation) {
            continue;
        }
        let owners = owners(operation);
        if owners.len() != 1 {
            problems.push(format!("{operation}: {} owners {owners:?}", owners.len()));
        }
    }
    assert!(
        problems.is_empty(),
        "operation ownership violations:\n{}",
        problems.join("\n")
    );
}

#[test]
fn routed_elsewhere_entries_are_declared_and_not_stale() {
    for (operation, reason) in ROUTED_ELSEWHERE {
        assert!(!reason.is_empty(), "{operation}: missing reason");
        assert!(
            DECLARED_OBJECT_OPERATIONS.contains(operation),
            "{operation} is pinned as routed elsewhere but no capability declares it"
        );
    }
}

#[test]
fn application_only_operations_are_declared_and_not_stale() {
    for operation in APPLICATION_ONLY {
        assert!(
            DECLARED_OBJECT_OPERATIONS.contains(operation),
            "{operation} is pinned as application-only but no capability declares it"
        );
        assert_eq!(
            families::owner(operation).is_some(),
            cfg!(feature = "application"),
            "{operation}"
        );
    }
}

#[test]
fn registered_families_are_named_once() {
    let mut names: Vec<_> = families::FAMILIES
        .iter()
        .map(|family| family.name)
        .collect();
    names.sort_unstable();
    let len = names.len();
    names.dedup();
    assert_eq!(names.len(), len, "duplicate command family name");
}

#[test]
fn tween_family_is_available_only_with_its_application_dependencies() {
    for operation in ["generate", "regenerate", "split", "reassign"] {
        let operation = format!("command.document.object.tween.{operation}");
        let owner = families::owner(&operation);
        assert_eq!(
            owner.is_some(),
            cfg!(feature = "application"),
            "{operation}"
        );
        if let Some(owner) = owner {
            assert_eq!(owner.name, "tween");
        }
    }
}
