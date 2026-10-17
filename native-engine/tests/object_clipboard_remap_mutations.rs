//! Mutation guards for the clipboard / duplicate remap kernel (L02-C).
//!
//! Each rule is a checker over a finished plan. The real kernel output must
//! satisfy it, and a hand-built mutant (the rule broken in one place) must
//! fail it, so a checker that accepted everything would be caught. The mixed
//! scenario below carries a brush anchor with dabs, a role-attached item, a
//! nested group, a solo group (dropped) and a source parent link.

use super::controls::{ent, it, plan, tagged};
use super::tests::Seq;
use super::{
    plan_layer_duplicate, AdmittedKinds, ClonePlan, DabSource, ItemSource, LayerPlan, LayerSource,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::GroupEntry;

fn table() -> Vec<GroupEntry> {
    vec![
        ent("gRoot", None, &["gIn", "sc"]),
        ent("gIn", Some("gRoot"), &["sa", "sb"]),
        ent("gSolo", None, &["sd", "sx"]),
    ]
}

fn items() -> Vec<ItemSource> {
    let anchor = ItemSource {
        brush_group_id: Some("bgA".into()),
        dabs: vec![DabSource::default(); 2],
        attached_to_role_id: Some("roleHead".into()),
        attach_offset: Some((2.0, 3.0)),
        ..tagged("sa", "gIn")
    };
    vec![
        anchor,
        tagged("sb", "gIn"),
        tagged("sc", "gRoot"),
        tagged("sd", "gSolo"),
        it("se"),
    ]
}

const SOURCE_IDS: [&str; 5] = ["sa", "sb", "sc", "sd", "se"];

fn mixed() -> ClonePlan {
    plan(&items(), Some(&table())).0.unwrap()
}

/// Fresh stroke ids are unique and never one of the source ids.
fn guard_fresh_ids_alias_nothing(p: &ClonePlan) -> bool {
    let ids: Vec<&str> = p.items.iter().map(|c| c.stroke_id.as_str()).collect();
    ids.iter().all(|id| !SOURCE_IDS.contains(id))
        && ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id))
}

/// Anchors and their dabs carry the SAME fresh brush group.
fn guard_dabs_share_the_anchor_group(p: &ClonePlan) -> bool {
    p.dabs
        .iter()
        .all(|d| p.items[d.source].brush_group_id.as_deref() == Some(d.brush_group_id.as_str()))
}

fn guard_track_role_cleared(p: &ClonePlan) -> bool {
    p.items.iter().all(|c| c.track_role_id.is_none())
}

fn guard_attachment_kept(p: &ClonePlan) -> bool {
    p.items[0].attached_to_role_id.as_deref() == Some("roleHead")
        && p.items[0].attach_offset == Some((2.0, 3.0))
}

/// No clone is tagged with a dropped group; every kept group has two operands.
fn guard_dropped_groups_untag(p: &ClonePlan) -> bool {
    let kept: Vec<&str> = p
        .groups
        .iter()
        .filter(|g| g.kept)
        .map(|g| g.fresh.as_str())
        .collect();
    p.groups.iter().all(|g| !g.kept || g.entry.order.len() >= 2)
        && p.items
            .iter()
            .all(|c| c.group_id.as_deref().map_or(true, |t| kept.contains(&t)))
}

/// Children are inserted before the parents that list them.
fn guard_children_first(p: &ClonePlan) -> bool {
    p.groups.iter().enumerate().all(|(at, g)| {
        g.entry
            .order
            .iter()
            .all(|m| match p.groups.iter().position(|o| &o.fresh == m) {
                Some(child) => child < at,
                None => true,
            })
    })
}

/// A recreated group never points at a source gid as its parent.
fn guard_no_stale_parent(p: &ClonePlan) -> bool {
    p.groups.iter().all(|g| {
        g.entry
            .parent
            .as_deref()
            .map_or(true, |x| !["gRoot", "gIn", "gSolo"].contains(&x))
    })
}

#[test]
fn the_real_plan_satisfies_every_guard() {
    let p = mixed();
    assert!(guard_fresh_ids_alias_nothing(&p));
    assert!(guard_dabs_share_the_anchor_group(&p) && !p.dabs.is_empty());
    assert!(guard_track_role_cleared(&p) && guard_attachment_kept(&p));
    assert!(
        guard_dropped_groups_untag(&p) && guard_children_first(&p) && guard_no_stale_parent(&p)
    );
    let kept: Vec<bool> = p.groups.iter().map(|g| g.kept).collect();
    assert_eq!(
        kept,
        [true, true, false],
        "gIn and gRoot survive, gSolo is dropped"
    );
    assert_eq!(
        p.groups[0].entry.parent.as_deref(),
        Some(p.groups[1].fresh.as_str())
    );
}

#[test]
fn guard_fresh_stroke_ids_never_alias_the_source() {
    let mut p = mixed();
    p.items[1].stroke_id = "sb".into();
    assert!(!guard_fresh_ids_alias_nothing(&p));
    let mut p = mixed();
    p.items[1].stroke_id = p.items[0].stroke_id.clone();
    assert!(!guard_fresh_ids_alias_nothing(&p));
}

#[test]
fn guard_dabs_share_one_brush_group_with_their_anchor() {
    let mut p = mixed();
    p.dabs[1].brush_group_id = "bgA".into();
    assert!(!guard_dabs_share_the_anchor_group(&p));
}

#[test]
fn guard_a_copy_never_claims_the_track_role() {
    let mut p = mixed();
    p.items[2].track_role_id = Some("roleBall".into());
    assert!(!guard_track_role_cleared(&p));
}

#[test]
fn guard_attachment_survives_the_clone() {
    let mut p = mixed();
    p.items[0].attach_offset = None;
    assert!(!guard_attachment_kept(&p));
    let mut p = mixed();
    p.items[0].attached_to_role_id = None;
    assert!(!guard_attachment_kept(&p));
}

#[test]
fn guard_dropped_groups_leave_no_tag_behind() {
    let mut p = mixed();
    let solo = p.groups[2].fresh.clone();
    p.items[3].group_id = Some(solo);
    assert!(!guard_dropped_groups_untag(&p));
    let mut p = mixed();
    p.groups[2].kept = true;
    assert!(
        !guard_dropped_groups_untag(&p),
        "a kept group below two operands"
    );
}

#[test]
fn guard_children_are_inserted_before_their_parents() {
    let mut p = mixed();
    p.groups.swap(0, 1);
    assert!(!guard_children_first(&p));
}

#[test]
fn guard_parent_links_are_repointed_at_fresh_ids() {
    let mut p = mixed();
    p.groups[0].entry.parent = Some("gRoot".into());
    assert!(!guard_no_stale_parent(&p));
}

fn guard_no_old_mesh_left(p: &LayerPlan, old: &[&str]) -> bool {
    let frames = p.frame_mesh_ids.iter().flatten().flatten();
    let rig = p.rig_bind_mesh_ids.iter().flatten().flatten();
    frames
        .chain(rig)
        .chain(p.video_mesh_id.iter())
        .all(|id| !old.contains(&id.as_str()))
}

fn guard_moved_keys_follow_unmoved(p: &LayerPlan) -> bool {
    let first_new = p
        .element_motion_keys
        .iter()
        .position(|k| p.mesh_map.iter().any(|m| &m.1 == k));
    match first_new {
        Some(at) => p.element_motion_keys[at..]
            .iter()
            .all(|k| p.mesh_map.iter().any(|m| &m.1 == k)),
        None => true,
    }
}

fn mesh_layer() -> LayerPlan {
    let ids = |v: &[&str]| v.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
    let src = LayerSource {
        frame_mesh_ids: vec![ids(&["m1", "m2"]), ids(&["m1"])],
        video_mesh_id: Some("m2".into()),
        element_motion_keys: vec!["m1".into(), "x".into(), "m2".into()],
        rig_bind_mesh_ids: Some(ids(&["m2"])),
        ..Default::default()
    };
    let all = AdmittedKinds {
        mesh: true,
        rig: true,
        component: true,
    };
    plan_layer_duplicate(&src, all, &mut Seq::default()).unwrap()
}

#[test]
fn guard_the_mesh_map_reaches_every_holder() {
    let p = mesh_layer();
    assert!(guard_no_old_mesh_left(&p, &["m1", "m2"]) && guard_moved_keys_follow_unmoved(&p));
    assert_eq!(p.video_mesh_id.as_deref(), Some("N2"));
    let mut bad = p.clone();
    bad.rig_bind_mesh_ids = Some(vec![Some("m2".into())]);
    assert!(!guard_no_old_mesh_left(&bad, &["m1", "m2"]));
    let mut bad = p.clone();
    bad.video_mesh_id = Some("m2".into());
    assert!(!guard_no_old_mesh_left(&bad, &["m1", "m2"]));
    let mut bad = p.clone();
    bad.frame_mesh_ids[1][0] = Some("m1".into());
    assert!(!guard_no_old_mesh_left(&bad, &["m1", "m2"]));
    let mut bad = p;
    bad.element_motion_keys = vec!["N1".into(), "x".into(), "N2".into()];
    assert!(!guard_moved_keys_follow_unmoved(&bad));
}
