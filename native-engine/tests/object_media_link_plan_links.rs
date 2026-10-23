//! The plan's link half: an unresolved linked image/video is a typed
//! `MissingLink` (asset id + placement id) at every instant, a resolved one
//! binds, and the link gate runs before any media fact is read.

use super::tests::{aid, drawing, embedded, env, inventory, linked, place, plan_embedded, Facts};
use super::{
    first_missing_link, plan_frame, Binding, BoundMedia, PlanEntry, PlanInput, VideoFacts,
};
use crate::object_document::media_timing::object_media_link_join::{
    LinkReadiness, LocatorNamespace, MissingLink, MissingLinkReason,
};
use crate::object_document::media_timing::object_media_link_state::{
    LinkedRef, ResolveOutcome, ResolveTable,
};
use crate::object_document::media_timing::object_media_timing_video::FrameSelection;
use crate::object_document::{AssetInventory, AssetKind};

const PATH: &str = "/media/clip.mp4";

fn path_ref() -> LinkedRef<'static> {
    LinkedRef {
        linked_path: Some(PATH),
        linked_handle_id: None,
    }
}

fn handle_ref(id: &'static str) -> LinkedRef<'static> {
    LinkedRef {
        linked_path: None,
        linked_handle_id: Some(id),
    }
}

fn failed(r: &LinkedRef<'_>, outcome: ResolveOutcome, at_ms: i64) -> ResolveTable {
    let mut table = ResolveTable::new();
    table.begin(r, at_ms);
    table.finish(&mut [], r, outcome, at_ms);
    table
}

fn linked_video_inv() -> AssetInventory {
    inventory(
        vec![linked(AssetKind::Video, "vid", PATH)],
        vec![place("pv", "vid", drawing("lv"), 3)],
    )
}

fn plan_with(
    inv: &AssetInventory,
    table: &ResolveTable,
    ns: LocatorNamespace,
    now_ms: i64,
    facts: &Facts,
) -> Vec<PlanEntry> {
    let input = PlanInput {
        inventory: inv,
        links: table,
        namespace: ns,
        now_ms,
        env: env(24.0),
        facts,
    };
    plan_frame(&input, 5.0).expect("finite frame")
}

fn missing(asset: &str, placement: &str, reason: MissingLinkReason) -> PlanEntry {
    PlanEntry::MissingLink(MissingLink {
        asset_id: aid(asset),
        placement_id: placement.to_string(),
        reason,
    })
}

fn vfacts() -> VideoFacts {
    VideoFacts {
        frame_count: 10.0,
        time_remap: false,
        interp_mode: None,
        frame_blend: false,
    }
}

fn video_facts() -> Facts {
    let mut f = Facts::default();
    f.entry("pv").video = Some(vfacts());
    f
}

#[test]
fn an_unresolved_link_is_a_typed_missing_link_never_a_default_frame() {
    let table = ResolveTable::new();
    // No facts at all: the gate answers first, so the media facts are never asked for.
    let got = plan_with(
        &linked_video_inv(),
        &table,
        LocatorNamespace::DesktopPath,
        0,
        &Facts::default(),
    );
    assert_eq!(
        got,
        vec![missing("vid", "pv", MissingLinkReason::NotResolved)]
    );
    let image = inventory(
        vec![linked(AssetKind::Image, "img", "/media/a.png")],
        vec![place("pi", "img", drawing("li"), 0)],
    );
    let got = plan_with(
        &image,
        &table,
        LocatorNamespace::DesktopPath,
        0,
        &Facts::default(),
    );
    assert_eq!(
        got,
        vec![missing("img", "pi", MissingLinkReason::NotResolved)]
    );
}

#[test]
fn a_resolved_link_binds_with_resolved_readiness_and_the_placement_offset() {
    let mut table = ResolveTable::new();
    table.prime(&path_ref(), true);
    let got = plan_with(
        &linked_video_inv(),
        &table,
        LocatorNamespace::DesktopPath,
        0,
        &video_facts(),
    );
    assert_eq!(
        got,
        vec![PlanEntry::Bound(Binding {
            placement_id: "pv".to_string(),
            asset_id: aid("vid"),
            link: LinkReadiness::Resolved,
            media: BoundMedia::Video(FrameSelection::Single { frame: 2.0 }),
        })]
    );
}

#[test]
fn a_failed_resolve_reports_its_reason_cooling_or_not_and_across_a_clock_that_ran_backwards() {
    let table = failed(&path_ref(), ResolveOutcome::ReadFailed, 1000);
    let inv = linked_video_inv();
    for now in [1000, 4999, 5000, 9000, -5] {
        let got = plan_with(
            &inv,
            &table,
            LocatorNamespace::DesktopPath,
            now,
            &Facts::default(),
        );
        assert_eq!(
            got,
            vec![missing("vid", "pv", MissingLinkReason::Missing)],
            "at {now}"
        );
    }
    // a web handle that needs the user's permission keeps that reason
    let handle = "handle-1";
    let table = failed(&handle_ref(handle), ResolveOutcome::PermissionNeeded, 1000);
    let web = inventory(
        vec![linked(AssetKind::Video, "vid", handle)],
        vec![place("pv", "vid", drawing("lv"), 3)],
    );
    for now in [1500, 4999, 5000, 60_000] {
        let got = plan_with(
            &web,
            &table,
            LocatorNamespace::WebHandle,
            now,
            &Facts::default(),
        );
        assert_eq!(
            got,
            vec![missing("vid", "pv", MissingLinkReason::PermissionNeeded)],
            "at {now}"
        );
    }
}

#[test]
fn a_gone_web_handle_is_missing_and_a_later_success_binds() {
    let handle = "handle-2";
    let mut table = failed(&handle_ref(handle), ResolveOutcome::HandleMissing, 0);
    let web = inventory(
        vec![linked(AssetKind::Video, "vid", handle)],
        vec![place("pv", "vid", drawing("lv"), 3)],
    );
    let got = plan_with(
        &web,
        &table,
        LocatorNamespace::WebHandle,
        10_000,
        &Facts::default(),
    );
    assert_eq!(got, vec![missing("vid", "pv", MissingLinkReason::Missing)]);
    // the retry gate opened, the resolver ran again and succeeded
    table.begin(&handle_ref(handle), 10_000);
    table.finish(
        &mut [],
        &handle_ref(handle),
        ResolveOutcome::Resolved,
        10_001,
    );
    let got = plan_with(
        &web,
        &table,
        LocatorNamespace::WebHandle,
        10_002,
        &video_facts(),
    );
    assert!(matches!(got.as_slice(), [PlanEntry::Bound(b)] if b.link == LinkReadiness::Resolved));
}

#[test]
fn the_locator_namespace_is_part_of_the_link_identity() {
    let mut table = ResolveTable::new();
    table.prime(&path_ref(), true);
    let got = plan_with(
        &linked_video_inv(),
        &table,
        LocatorNamespace::WebHandle,
        0,
        &video_facts(),
    );
    assert_eq!(
        got,
        vec![missing("vid", "pv", MissingLinkReason::NotResolved)]
    );
}

#[test]
fn embedded_assets_need_no_link_whatever_the_table_holds() {
    let table = failed(&path_ref(), ResolveOutcome::ReadFailed, 0);
    let inv = inventory(
        vec![embedded(AssetKind::Video, "vid")],
        vec![place("pv", "vid", drawing("lv"), 3)],
    );
    let got = plan_with(
        &inv,
        &table,
        LocatorNamespace::DesktopPath,
        0,
        &video_facts(),
    );
    assert!(matches!(got.as_slice(), [PlanEntry::Bound(b)] if b.link == LinkReadiness::Embedded));
    assert_eq!(plan_embedded(&inv, &video_facts(), 24.0, 5.0), got);
}

#[test]
fn a_missing_link_does_not_stop_other_placements_and_the_first_is_in_placement_order() {
    let inv = inventory(
        vec![
            linked(AssetKind::Image, "img", "/media/a.png"),
            embedded(AssetKind::Video, "vid"),
            linked(AssetKind::Video, "vid2", PATH),
        ],
        vec![
            place("p1", "img", drawing("l1"), 0),
            place("p2", "vid", drawing("l2"), 0),
            place("p3", "vid2", drawing("l3"), 0),
        ],
    );
    let mut facts = Facts::default();
    for pid in ["p2", "p3"] {
        facts.entry(pid).video = Some(vfacts());
    }
    let table = ResolveTable::new();
    let got = plan_with(&inv, &table, LocatorNamespace::DesktopPath, 0, &facts);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], missing("img", "p1", MissingLinkReason::NotResolved));
    assert!(matches!(&got[1], PlanEntry::Bound(b) if b.placement_id == "p2"));
    assert_eq!(
        got[2],
        missing("vid2", "p3", MissingLinkReason::NotResolved)
    );
    assert_eq!(
        first_missing_link(&got).map(|m| m.placement_id.as_str()),
        Some("p1")
    );
    // once the image resolves, the next one in placement order is the first
    let mut table = ResolveTable::new();
    table.prime(
        &LinkedRef {
            linked_path: Some("/media/a.png"),
            linked_handle_id: None,
        },
        true,
    );
    let got = plan_with(&inv, &table, LocatorNamespace::DesktopPath, 0, &facts);
    assert_eq!(
        first_missing_link(&got).map(|m| m.placement_id.as_str()),
        Some("p3")
    );
    table.prime(&path_ref(), true);
    let got = plan_with(&inv, &table, LocatorNamespace::DesktopPath, 0, &facts);
    assert_eq!(first_missing_link(&got), None);
}

#[test]
fn planning_leaves_the_resolve_table_as_it_was() {
    let table = failed(&path_ref(), ResolveOutcome::ReadFailed, 1000);
    let before = table.failure(&path_ref());
    let _ = plan_with(
        &linked_video_inv(),
        &table,
        LocatorNamespace::DesktopPath,
        9000,
        &Facts::default(),
    );
    assert_eq!(table.failure(&path_ref()), before);
    assert!(!table.in_flight(&path_ref()) && !table.cached(&path_ref()));
}
