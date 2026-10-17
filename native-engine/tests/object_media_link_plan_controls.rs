//! Controls for the media binding plan.
//!
//! Part 1, mutations: each [`Mutation`] is one plausible mistake spliced around
//! the REAL plan (an input changed before the call) and the oracle comparison
//! or a pinned check must name it. `Mutation::None` is the unmutated
//! pass-through, so a green control proves the checks can fail.
//!
//! Part 2, preconditions: the plan relies on `AssetInventory`/`AssetPlacement`
//! construction to refuse a dangling asset id, a kind/target mismatch, a
//! duplicate placement and an offset beyond +-(2^53 - 1) (the largest i64 an
//! f64 holds exactly). These tests show the refusals happen before a plan can
//! ever see such a value, and that the plan emits exactly one entry for each
//! accepted (target, kind) pair, none for a font.

use super::tests::{
    aid, drawing, embedded, env, font, inventory, linked, place, plan_embedded, run, track, Facts,
};
use super::{plan_frame, BoundMedia, PlanEntry, PlanInput};
use crate::object_document::media_timing::object_media_link_join::{
    LinkReadiness, LocatorNamespace, MissingLinkReason,
};
use crate::object_document::media_timing::object_media_link_state::{LinkedRef, ResolveTable};
use crate::object_document::{AssetInventory, AssetKind, AssetPlacement, AssetTarget};

/// A deliberate mistake the checks must catch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// The placement offset is negated before it reaches the kernels.
    OffsetSignFlip,
    /// Entries come out in reverse placement order.
    ReversedOrder,
    /// The caller names the wrong locator namespace.
    NamespaceSwapped,
    /// The link verdict is ignored: every link reads as resolved.
    TableIgnored,
}

impl Mutation {
    pub(crate) fn offset(self, off: i64) -> i64 {
        if self == Mutation::OffsetSignFlip {
            -off
        } else {
            off
        }
    }

    fn order(self, mut placements: Vec<AssetPlacement>) -> Vec<AssetPlacement> {
        if self == Mutation::ReversedOrder {
            placements.reverse();
        }
        placements
    }

    fn namespace(self, ns: LocatorNamespace) -> LocatorNamespace {
        match (self, ns) {
            (Mutation::NamespaceSwapped, LocatorNamespace::DesktopPath) => {
                LocatorNamespace::WebHandle
            }
            (Mutation::NamespaceSwapped, LocatorNamespace::WebHandle) => {
                LocatorNamespace::DesktopPath
            }
            _ => ns,
        }
    }

    fn table(self, primed: bool) -> ResolveTable {
        let mut table = ResolveTable::new();
        if primed || self == Mutation::TableIgnored {
            table.prime(&path_ref(), true);
        }
        table
    }
}

const PATH: &str = "/media/clip.mp4";

fn path_ref() -> LinkedRef<'static> {
    LinkedRef {
        linked_path: Some(PATH),
        linked_handle_id: None,
    }
}

/// Entries come out in placement order `pa`, `pv`, `pi`.
fn order_clean(m: Mutation) -> bool {
    let placements = m.order(vec![
        place("pa", "aud", track("ta"), 0),
        place("pv", "vid", drawing("lv"), 0),
        place("pi", "img", drawing("li"), 0),
    ]);
    let inv = inventory(
        vec![
            embedded(AssetKind::Audio, "aud"),
            embedded(AssetKind::Video, "vid"),
            embedded(AssetKind::Image, "img"),
        ],
        placements,
    );
    let ids: Vec<String> = plan_embedded(&inv, &Facts::default(), 24.0, 5.0)
        .iter()
        .map(|e| match e {
            PlanEntry::Unavailable(u) => u.placement_id.clone(),
            PlanEntry::Bound(b) => b.placement_id.clone(),
            other => panic!("unexpected entry {other:?}"),
        })
        .collect();
    ids == ["pa", "pv", "pi"]
}

fn link_plan(m: Mutation, primed: bool) -> Vec<PlanEntry> {
    let inv = inventory(
        vec![linked(AssetKind::Image, "img", PATH)],
        vec![place("pi", "img", drawing("li"), 0)],
    );
    let table = m.table(primed);
    let facts = Facts::default();
    let input = PlanInput {
        inventory: &inv,
        links: &table,
        namespace: m.namespace(LocatorNamespace::DesktopPath),
        now_ms: 0,
        env: env(24.0),
        facts: &facts,
    };
    plan_frame(&input, 5.0).expect("finite frame")
}

/// A primed desktop link binds as resolved.
fn resolved_clean(m: Mutation) -> bool {
    matches!(
        link_plan(m, true).as_slice(),
        [PlanEntry::Bound(b)]
            if b.link == LinkReadiness::Resolved && b.media == BoundMedia::Still
    )
}

/// An unprimed link is a typed missing link, not a bound frame.
fn unresolved_clean(m: Mutation) -> bool {
    matches!(
        link_plan(m, false).as_slice(),
        [PlanEntry::MissingLink(l)]
            if l.reason == MissingLinkReason::NotResolved
                && l.placement_id == "pi"
                && l.asset_id.as_str() == "img"
    )
}

fn failures(m: Mutation) -> (usize, [bool; 3]) {
    (
        run(m).bad.len(),
        [order_clean(m), resolved_clean(m), unresolved_clean(m)],
    )
}

#[test]
fn unmutated_pass_through_is_clean() {
    assert_eq!(failures(Mutation::None), (0, [true, true, true]));
}

#[test]
fn mutation_offset_sign_flip_is_caught() {
    assert!(failures(Mutation::OffsetSignFlip).0 > 0);
}

#[test]
fn mutation_reversed_order_is_caught() {
    assert_eq!(failures(Mutation::ReversedOrder), (0, [false, true, true]));
}

#[test]
fn mutation_namespace_swapped_is_caught() {
    assert_eq!(
        failures(Mutation::NamespaceSwapped),
        (0, [true, false, true])
    );
}

#[test]
fn mutation_table_ignored_is_caught() {
    assert_eq!(failures(Mutation::TableIgnored), (0, [true, true, false]));
}

// ---- part 2: preconditions the plan relies on --------------------------------

fn err_of(inv: Result<AssetInventory, impl std::fmt::Debug>) -> String {
    format!("{:?}", inv.expect_err("construction must refuse"))
}

#[test]
fn a_dangling_asset_id_cannot_enter_a_pinned_inventory() {
    let dangling = place("pv", "ghost", drawing("lv"), 0);
    let r = AssetInventory::new(
        vec![embedded(AssetKind::Video, "vid")],
        vec![dangling],
        8,
        8,
    );
    assert_eq!(err_of(r), "MissingAsset");
}

#[test]
fn a_kind_mismatch_cannot_enter_a_pinned_inventory() {
    let cases = [
        (AssetKind::Audio, drawing("lv")),
        (AssetKind::Font, drawing("lv")),
        (AssetKind::Video, track("ta")),
        (AssetKind::Image, track("ta")),
        (AssetKind::Image, font("lt")),
        (AssetKind::Audio, font("lt")),
    ];
    for (kind, target) in cases {
        let r = AssetInventory::new(
            vec![embedded(kind, "x")],
            vec![place("p", "x", target.clone(), 0)],
            8,
            8,
        );
        assert_eq!(err_of(r), "KindMismatch", "{kind:?} on {target:?}");
    }
}

#[test]
fn a_duplicate_placement_cannot_enter_a_pinned_inventory() {
    let records = || {
        vec![
            embedded(AssetKind::Image, "a"),
            embedded(AssetKind::Image, "b"),
        ]
    };
    // same placement id on two targets
    let same_id = vec![
        place("p", "a", drawing("l1"), 0),
        place("p", "b", drawing("l2"), 0),
    ];
    assert_eq!(
        err_of(AssetInventory::new(records(), same_id, 8, 8)),
        "DuplicatePlacement"
    );
    // two placements on one target
    let same_target = vec![
        place("p1", "a", drawing("l1"), 0),
        place("p2", "b", drawing("l1"), 0),
    ];
    assert_eq!(
        err_of(AssetInventory::new(records(), same_target, 8, 8)),
        "DuplicatePlacement"
    );
}

#[test]
fn an_offset_beyond_two_pow_53_minus_one_cannot_be_placed() {
    const MAX: i64 = 9_007_199_254_740_991;
    for ok in [MAX, -MAX, 0, 1, -1] {
        assert!(
            AssetPlacement::new("p".to_string(), aid("a"), drawing("l"), ok).is_ok(),
            "{ok}"
        );
    }
    for bad in [MAX + 1, -MAX - 1, i64::MAX, i64::MIN] {
        let r = AssetPlacement::new("p".to_string(), aid("a"), drawing("l"), bad);
        assert_eq!(
            format!("{:?}", r.expect_err("must refuse")),
            "Placement",
            "{bad}"
        );
    }
}

#[test]
fn the_plan_emits_one_entry_per_accepted_target_and_kind_and_none_for_a_font() {
    let kinds = [
        AssetKind::Image,
        AssetKind::Video,
        AssetKind::Audio,
        AssetKind::Font,
    ];
    let targets: [fn() -> AssetTarget; 3] = [|| drawing("l"), || track("t"), || font("f")];
    let mut accepted = Vec::new();
    for (ti, target) in targets.iter().enumerate() {
        for kind in kinds {
            let r = AssetInventory::new(
                vec![embedded(kind, "x")],
                vec![place("p", "x", target(), 0)],
                8,
                8,
            );
            if let Ok(inv) = r {
                let n = plan_embedded(&inv, &Facts::default(), 24.0, 5.0).len();
                accepted.push((ti, kind, n));
            }
        }
    }
    assert_eq!(
        accepted,
        [
            (0, AssetKind::Image, 1),
            (0, AssetKind::Video, 1),
            (1, AssetKind::Audio, 1),
            (2, AssetKind::Font, 0),
        ]
    );
}
