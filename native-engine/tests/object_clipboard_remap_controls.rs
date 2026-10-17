//! Independent literal controls for the clipboard / duplicate remap kernel
//! (F15 + F08, L02-C): partial group copy, 3-level nest copy, brush anchor with
//! 5 dabs, self-referencing group, duplicate input ids and provider exhaustion
//! at every step. Every expected value is written by hand from the baseline
//! rules (and compared with the real functions by `tools/controls_check.js`);
//! none is read from the oracle. The test double issues `N1`, `N2` ... in
//! request order, so a literal such as `N3` also pins WHEN it was requested.

use super::tests::Seq;
use super::{
    plan_clone, ClonePlan, CloneRequest, DabSource, GroupRemap, IdKind, IdSource, ItemSource,
    RemapError,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupEntry,
};

pub(crate) fn it(sid: &str) -> ItemSource {
    ItemSource {
        stroke_id: Some(sid.into()),
        ..Default::default()
    }
}

pub(crate) fn tagged(sid: &str, gid: &str) -> ItemSource {
    ItemSource {
        group_id: Some(gid.into()),
        ..it(sid)
    }
}

pub(crate) fn ent(gid: &str, parent: Option<&str>, order: &[&str]) -> GroupEntry {
    GroupEntry {
        gid: gid.into(),
        name: None,
        combine_mode: None,
        parent: parent.map(str::to_string),
        order: order.iter().map(|s| s.to_string()).collect(),
    }
}

/// An expected recreated group.
fn gr(src: &str, fresh: &str, kept: bool, parent: Option<&str>, order: &[&str]) -> GroupRemap {
    GroupRemap {
        source: src.into(),
        fresh: fresh.into(),
        kept,
        entry: ent(fresh, parent, order),
    }
}

pub(crate) fn plan(
    items: &[ItemSource],
    groups: Option<&[GroupEntry]>,
) -> (Result<ClonePlan, RemapError>, Seq) {
    let mut ids = Seq::default();
    let got = plan_with(items, groups, &[], &mut ids);
    (got, ids)
}

pub(crate) fn plan_with<S: IdSource>(
    items: &[ItemSource],
    groups: Option<&[GroupEntry]>,
    dst: &[String],
    ids: &mut S,
) -> Result<ClonePlan, RemapError> {
    let req = CloneRequest {
        items,
        source_groups: groups,
        destination_group_ids: dst,
        offset: 0.0,
    };
    plan_clone(&req, ids)
}

fn tags(p: &ClonePlan) -> Vec<Option<&str>> {
    p.items.iter().map(|c| c.group_id.as_deref()).collect()
}

fn strokes(p: &ClonePlan) -> Vec<&str> {
    p.items.iter().map(|c| c.stroke_id.as_str()).collect()
}

/// Scripted provider: hands out the listed ids, then reports exhaustion.
pub(crate) struct Fixed {
    pub(crate) ids: Vec<&'static str>,
    pub(crate) at: usize,
}

impl IdSource for Fixed {
    fn fresh(&mut self, _kind: IdKind, _replaces: Option<&str>) -> Option<String> {
        let id = self.ids.get(self.at)?;
        self.at += 1;
        Some(id.to_string())
    }
}

#[test]
fn partial_group_copy_keeps_only_the_members_that_came_along() {
    let mut table = ent("gA", None, &["sa", "sb", "sc"]);
    table.name = Some("Trio".into());
    table.combine_mode = Some(CombineMode::Subtract);
    let (got, ids) = plan(&[tagged("sa", "gA"), tagged("sb", "gA")], Some(&[table]));
    let p = got.unwrap();
    assert_eq!(strokes(&p), ["N1", "N2"]);
    let map = [
        ("sa".to_string(), "N1".to_string()),
        ("sb".to_string(), "N2".to_string()),
    ];
    assert_eq!(p.stroke_map, map, "old to new stroke ids");
    assert_eq!(tags(&p), [Some("N3"), Some("N3")]);
    assert_eq!(p.groups.len(), 1);
    let g = &p.groups[0];
    assert_eq!(
        (g.source.as_str(), g.fresh.as_str(), g.kept),
        ("gA", "N3", true)
    );
    assert_eq!(g.entry.order, ["N1", "N2"], "the unselected sc is dropped");
    assert_eq!(g.entry.name.as_deref(), Some("Trio"));
    assert_eq!(g.entry.combine_mode, Some(CombineMode::Subtract));
    let asked: Vec<_> = ids.log.iter().map(|(k, r)| (*k, r.as_deref())).collect();
    assert_eq!(
        asked,
        [
            (IdKind::Stroke, Some("sa")),
            (IdKind::Stroke, Some("sb")),
            (IdKind::CombineGroup, Some("gA")),
        ]
    );
}

#[test]
fn a_group_left_with_one_operand_is_dropped_and_its_clone_untagged() {
    let table = ent("gA", None, &["sa", "sb"]);
    let (got, _) = plan(&[tagged("sa", "gA"), it("sz")], Some(&[table]));
    let p = got.unwrap();
    assert_eq!(tags(&p), [None, None]);
    assert_eq!(p.groups, [gr("gA", "N3", false, None, &["N1"])]);
}

#[test]
fn three_level_nest_copy_recreates_every_group_children_first() {
    let table = [
        ent("gRoot", None, &["gMid", "s4"]),
        ent("gMid", Some("gRoot"), &["gIn", "s3"]),
        ent("gIn", Some("gMid"), &["s1", "s2"]),
    ];
    let items = [
        tagged("s1", "gIn"),
        tagged("s2", "gIn"),
        tagged("s3", "gMid"),
        tagged("s4", "gRoot"),
    ];
    let p = plan(&items, Some(&table)).0.unwrap();
    assert_eq!(tags(&p), [Some("N5"), Some("N5"), Some("N6"), Some("N7")]);
    assert_eq!(
        p.groups,
        [
            gr("gIn", "N5", true, Some("N6"), &["N1", "N2"]),
            gr("gMid", "N6", true, Some("N7"), &["N5", "N3"]),
            gr("gRoot", "N7", true, None, &["N6", "N4"]),
        ]
    );
}

#[test]
fn copying_only_the_inner_group_recreates_its_ancestors_which_then_drop() {
    let table = [
        ent("gRoot", None, &["gMid", "s4"]),
        ent("gMid", Some("gRoot"), &["gIn", "s3"]),
        ent("gIn", Some("gMid"), &["s1", "s2"]),
    ];
    let p = plan(&[tagged("s1", "gIn"), tagged("s2", "gIn")], Some(&table))
        .0
        .unwrap();
    assert_eq!(tags(&p), [Some("N3"), Some("N3")]);
    assert_eq!(
        p.groups,
        [
            gr("gIn", "N3", true, Some("N4"), &["N1", "N2"]),
            gr("gMid", "N4", false, Some("N5"), &["N3"]),
            gr("gRoot", "N5", false, None, &["N4"]),
        ],
        "ancestors with one operand are dropped but stay named by their children"
    );
}

#[test]
fn brush_anchor_with_five_dabs_shares_one_fresh_brush_group() {
    let anchor = ItemSource {
        brush_group_id: Some("bgA".into()),
        dabs: vec![DabSource::default(); 5],
        ..it("sa")
    };
    let (got, ids) = plan(&[anchor], None);
    let p = got.unwrap();
    assert_eq!(p.items[0].brush_group_id.as_deref(), Some("N1"));
    assert_eq!(p.items[0].stroke_id, "N2");
    assert_eq!(p.dabs.len(), 5);
    assert!(p
        .dabs
        .iter()
        .all(|d| d.brush_group_id == "N1" && d.stroke_id.is_none()));
    assert_eq!(
        p.dabs.iter().map(|d| (d.source, d.dab)).collect::<Vec<_>>(),
        [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)]
    );
    let kinds: Vec<IdKind> = ids.log.iter().map(|(k, _)| *k).collect();
    assert_eq!(kinds, [IdKind::BrushGroup, IdKind::Stroke]);
}

#[test]
fn anchors_with_the_same_old_brush_group_share_a_fresh_one_others_get_their_own() {
    let a = |sid: &str, bg: &str| ItemSource {
        brush_group_id: Some(bg.into()),
        ..it(sid)
    };
    let p = plan(&[a("sa", "bgA"), a("sb", "bgA"), a("sc", "bgB")], None)
        .0
        .unwrap();
    let brush: Vec<_> = p
        .items
        .iter()
        .map(|c| c.brush_group_id.as_deref())
        .collect();
    assert_eq!(brush, [Some("N1"), Some("N1"), Some("N4")]);
    assert_eq!(strokes(&p), ["N2", "N3", "N5"]);
}

#[test]
fn track_role_is_cleared_while_attachment_is_kept() {
    let hat = ItemSource {
        track_role_id: Some("roleBall".into()),
        attached_to_role_id: Some("roleHead".into()),
        attach_offset: Some((1.5, -2.0)),
        ..it("sa")
    };
    let c = plan(&[hat], None).0.unwrap().items.remove(0);
    assert_eq!(c.track_role_id, None);
    assert_eq!(c.attached_to_role_id.as_deref(), Some("roleHead"));
    assert_eq!(c.attach_offset, Some((1.5, -2.0)));
}

#[test]
fn a_vector_brush_backdrop_gets_a_fresh_pair_id_and_never_the_old_one() {
    let vb = ItemSource {
        linked_fill_id: Some("lfOld".into()),
        vector_brush_with_centerline: true,
        linked_fill_snapshot: true,
        ..it("sa")
    };
    let (got, ids) = plan(&[vb], None);
    let c = got.unwrap().items.remove(0);
    assert_eq!(
        (
            c.stroke_id.as_str(),
            c.linked_fill_id.as_deref(),
            c.companion
        ),
        ("N1", Some("N2"), true)
    );
    let kinds: Vec<IdKind> = ids.log.iter().map(|(k, _)| *k).collect();
    assert_eq!(kinds, [IdKind::Stroke, IdKind::LinkedFill]);
    // Without the vbLinkedFill snapshot the baseline keeps the old id verbatim.
    let lone = ItemSource {
        linked_fill_id: Some("lfOld".into()),
        vector_brush_with_centerline: true,
        ..it("sa")
    };
    let c = plan(&[lone], None).0.unwrap().items.remove(0);
    assert_eq!(
        (c.linked_fill_id.as_deref(), c.companion),
        (Some("lfOld"), false)
    );
}

#[test]
fn a_self_referencing_group_that_is_its_own_first_owner_refuses_without_requesting_ids() {
    // parentOf(gS) is gS itself: the baseline ancestor loop never ends.
    let table = [ent("gS", None, &["gS", "sa", "sb"])];
    let (got, ids) = plan(&[tagged("sa", "gS")], Some(&table));
    assert_eq!(got, Err(RemapError::GroupCycle("gS".into())));
    assert!(
        ids.log.is_empty(),
        "input refusals precede the first id request"
    );
    // A mutual cycle refuses the same way.
    let table = [ent("gX", None, &["gY"]), ent("gY", None, &["gX", "sa"])];
    assert_eq!(
        plan(&[tagged("sa", "gY")], Some(&table)).0,
        Err(RemapError::GroupCycle("gY".into()))
    );
}

#[test]
fn a_self_listing_group_with_an_earlier_owner_terminates_like_the_baseline() {
    let table = [
        ent("gH", None, &["gS", "sz"]),
        ent("gS", None, &["gS", "sa", "sb"]),
    ];
    let p = plan(&[tagged("sa", "gS")], Some(&table)).0.unwrap();
    assert_eq!(tags(&p), [Some("N2")]);
    assert_eq!(
        p.groups,
        [
            gr("gS", "N2", true, None, &["N2", "N1"]),
            gr("gH", "N3", false, None, &["N2"]),
        ]
    );
}

#[test]
fn duplicate_source_stroke_ids_refuse_before_any_id_is_requested() {
    let (got, ids) = plan(&[it("sa"), it("sb"), it("sa")], None);
    assert_eq!(got, Err(RemapError::DuplicateSourceStrokeId("sa".into())));
    assert!(ids.log.is_empty());
    // Items without an id are not duplicates of each other.
    assert!(plan(&[ItemSource::default(), ItemSource::default()], None)
        .0
        .is_ok());
}

#[test]
fn provider_exhaustion_at_any_step_exposes_nothing() {
    let table = [ent("gA", None, &["sa", "sb"])];
    let items = [tagged("sa", "gA"), tagged("sb", "gA")];
    let want = [IdKind::Stroke, IdKind::Stroke, IdKind::CombineGroup];
    for (limit, kind) in want.iter().enumerate() {
        let mut ids = Seq {
            limit: Some(limit),
            ..Seq::default()
        };
        let got = plan_with(&items, Some(&table), &[], &mut ids);
        assert_eq!(got, Err(RemapError::IdUnavailable(*kind)), "limit {limit}");
        assert_eq!(
            ids.log.len(),
            limit,
            "the kernel stops at the first refusal"
        );
    }
    let mut ids = Seq {
        limit: Some(3),
        ..Seq::default()
    };
    assert!(plan_with(&items, Some(&table), &[], &mut ids).is_ok());
    // The kernel keeps no state: a failed call does not change the next call.
    assert_eq!(plan(&items, Some(&table)).0.unwrap().groups.len(), 1);
}

#[test]
fn provider_ids_that_alias_an_existing_identity_are_refused() {
    let table = [ent("gA", None, &["sa", "sb"])];
    let items = [tagged("sa", "gA"), tagged("sb", "gA")];
    let dst = ["gZ".to_string()];
    let run =
        |ids: Vec<&'static str>| plan_with(&items, Some(&table), &dst, &mut Fixed { ids, at: 0 });
    assert_eq!(run(vec!["sa"]), Err(RemapError::IdCollision("sa".into())));
    assert_eq!(
        run(vec!["n1", "n1"]),
        Err(RemapError::IdCollision("n1".into()))
    );
    assert_eq!(
        run(vec!["n1", "n2", "gA"]),
        Err(RemapError::IdCollision("gA".into()))
    );
    assert_eq!(
        run(vec!["n1", "n2", "gZ"]),
        Err(RemapError::IdCollision("gZ".into()))
    );
    assert_eq!(
        run(vec!["n1", "sb"]),
        Err(RemapError::IdCollision("sb".into()))
    );
    assert_eq!(run(vec![""]), Err(RemapError::EmptyFreshId(IdKind::Stroke)));
    assert!(run(vec!["n1", "n2", "n3"]).is_ok());
}

#[test]
fn dab_inputs_that_cannot_be_cloned_faithfully_refuse_or_get_a_fresh_identity() {
    let orphan = ItemSource {
        dabs: vec![DabSource::default()],
        ..it("sa")
    };
    assert_eq!(
        plan(&[it("sz"), orphan], None).0,
        Err(RemapError::DabsWithoutBrushGroup(1))
    );
    // A dab that carries a stroke id of its own never keeps it (identity must not alias).
    let anchor = ItemSource {
        brush_group_id: Some("bgA".into()),
        dabs: vec![DabSource {
            stroke_id: Some("sDab".into()),
            group_id: None,
        }],
        ..it("sa")
    };
    let (got, ids) = plan(&[anchor], None);
    assert_eq!(got.unwrap().dabs[0].stroke_id.as_deref(), Some("N3"));
    assert_eq!(ids.log[2], (IdKind::Stroke, Some("sDab".to_string())));
}

#[test]
fn empty_selection_and_non_finite_offsets_refuse() {
    assert_eq!(plan(&[], None).0, Err(RemapError::EmptySelection));
    for bad in [f64::NAN, f64::INFINITY] {
        let items = [it("sa")];
        let req = CloneRequest {
            items: &items,
            source_groups: None,
            destination_group_ids: &[],
            offset: bad,
        };
        assert_eq!(
            plan_clone(&req, &mut Seq::default()),
            Err(RemapError::NonFiniteOffset)
        );
    }
}

#[test]
fn malformed_group_tables_refuse_before_any_id_is_requested() {
    let dup = [ent("gA", None, &[]), ent("gA", None, &[])];
    let (got, ids) = plan(&[tagged("sa", "gA")], Some(&dup));
    assert_eq!(got, Err(RemapError::DuplicateGroupId("gA".into())));
    assert!(ids.log.is_empty());
    let unnamed = [ent("", None, &[])];
    assert_eq!(
        plan(&[it("sa")], Some(&unnamed)).0,
        Err(RemapError::EmptyGroupId)
    );
}

#[test]
fn tags_follow_the_source_table_a_missing_table_keeps_them_an_empty_one_deletes_them() {
    let items = [tagged("sa", "gA")];
    assert_eq!(tags(&plan(&items, None).0.unwrap()), [Some("gA")]);
    assert_eq!(tags(&plan(&items, Some(&[])).0.unwrap()), [None]);
    let table = [ent("gA", None, &["sb", "sc"])];
    let three = [tagged("sa", "gX"), tagged("sb", "gA"), tagged("sc", "gA")];
    let p = plan(&three, Some(&table)).0.unwrap();
    assert_eq!(tags(&p), [None, Some("N4"), Some("N4")]);
    assert_eq!(p.groups, [gr("gA", "N4", true, None, &["N2", "N3"])]);
}

#[test]
fn two_groups_into_a_layer_with_groups_get_distinct_fresh_gids() {
    let table = [
        ent("gA", None, &["sa", "sb"]),
        ent("gB", None, &["sc", "sd"]),
    ];
    let items = [
        tagged("sa", "gA"),
        tagged("sb", "gA"),
        tagged("sc", "gB"),
        tagged("sd", "gB"),
    ];
    let dst = ["gZ".to_string()];
    let p = plan_with(&items, Some(&table), &dst, &mut Seq::default()).unwrap();
    assert_eq!(tags(&p), [Some("N5"), Some("N5"), Some("N6"), Some("N6")]);
    assert!(p
        .groups
        .iter()
        .all(|g| g.fresh != "gA" && g.fresh != "gB" && g.fresh != "gZ"));
}
