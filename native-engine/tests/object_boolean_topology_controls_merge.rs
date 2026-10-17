//! Independent literal controls for the same-colour fill merge (L02-D):
//! candidate admission one rule at a time, the probe protocol and the
//! near-identical test. Hand-written expectations; nothing is read from the
//! oracle. Union planning, the commit slot arithmetic and the regeneration tags
//! are in `object_boolean_topology_controls_commit.rs`.

use super::controls::b;
use super::{
    is_near_identical_area, merge_candidates, select_absorbed, AreaFacts, FillFacts, MergeError,
    ProbeError, TouchProbe, UnavailableTouch,
};

fn plain(color: &str) -> FillFacts {
    FillFacts {
        is_path: true,
        closed: true,
        fill_hex8: Some(color.to_string()),
        ..Default::default()
    }
}

fn with(color: &str, edit: impl Fn(&mut FillFacts)) -> FillFacts {
    let mut facts = plain(color);
    edit(&mut facts);
    facts
}

#[test]
fn only_closed_unstroked_same_colour_paths_are_candidates() {
    let layer = vec![
        plain("#ff0000"),
        with("#ff0000", |f| f.stroked = true),
        with("#ff0000", |f| f.closed = false),
        with("#ff0000", |f| f.is_path = false),
        FillFacts {
            fill_hex8: None,
            ..plain("#ff0000")
        },
        with("", |_| {}),
        plain("#0000ff"),
        plain("#ff000080"),
        plain("#ff0000"),
        plain("#ff0000"),
    ];
    assert_eq!(merge_candidates(8, &layer, false), vec![0, 9]);
    assert_eq!(
        merge_candidates(9, &layer, false),
        vec![0, 8],
        "ascending, never the new fill itself"
    );
}

fn flagged(which: usize) -> FillFacts {
    let mut facts = plain("#ff0000");
    match which {
        0 => facts.is_vector_brush = true,
        1 => facts.is_linked_fill_companion = true,
        2 => facts.is_brush_texture_copy = true,
        3 => facts.is_revision_ghost = true,
        _ => facts.ghost_frame_defined = true,
    }
    facts
}

#[test]
fn each_exclusion_flag_removes_the_candidate() {
    let labels = [
        "vector brush",
        "linked fill companion",
        "brush texture copy",
        "revision ghost",
        "ghost frame (even frame 0)",
    ];
    for (which, label) in labels.iter().enumerate() {
        let layer = vec![flagged(which), plain("#ff0000")];
        assert!(merge_candidates(1, &layer, true).is_empty(), "{label}");
    }
    assert_eq!(
        merge_candidates(1, &[plain("#ff0000"), plain("#ff0000")], true),
        vec![0]
    );
}

#[test]
fn fill_brush_shapes_need_permission_or_a_traced_wall() {
    let shape = |id: Option<&str>| {
        with("#ff0000", |f| {
            f.is_fill_shape = true;
            f.stroke_id = id.map(str::to_string);
        })
    };
    let walled = |walls: Option<Vec<&str>>| FillFacts {
        fill_walls: walls.map(|w| w.into_iter().map(str::to_string).collect()),
        ..plain("#ff0000")
    };
    let probe =
        |child: FillFacts, new: FillFacts, allow: bool| merge_candidates(1, &[child, new], allow);
    assert!(probe(shape(Some("sA")), walled(None), false).is_empty());
    assert_eq!(
        probe(shape(Some("sA")), walled(None), true),
        vec![0],
        "brush commit allows absorption"
    );
    assert_eq!(
        probe(shape(Some("sA")), walled(Some(vec!["sX", "sA"])), false),
        vec![0]
    );
    assert!(probe(shape(Some("sA")), walled(Some(vec!["sX"])), false).is_empty());
    assert!(
        probe(shape(None), walled(Some(vec!["sA"])), false).is_empty(),
        "no id, cannot be traced"
    );
    assert!(probe(shape(Some("sA")), walled(Some(vec![])), false).is_empty());
    assert_eq!(probe(shape(None), walled(None), true), vec![0]);
}

#[test]
fn the_new_fill_needs_a_colour_and_a_slot_in_the_layer() {
    let no_colour = FillFacts {
        fill_hex8: None,
        ..plain("#ff0000")
    };
    assert!(merge_candidates(1, &[plain("#ff0000"), no_colour], false).is_empty());
    assert!(merge_candidates(1, &[plain("#ff0000"), with("", |_| {})], false).is_empty());
    assert!(merge_candidates(5, &[plain("#ff0000"), plain("#ff0000")], false).is_empty());
    assert!(merge_candidates(0, &[], false).is_empty());
}

/// One scripted answer per question, the same for every candidate.
struct Fixed {
    bi: Result<bool, ProbeError>,
    sb: Result<bool, ProbeError>,
    st: Result<bool, ProbeError>,
    nm: Result<bool, ProbeError>,
    log: Vec<&'static str>,
}

fn fixed(bi: bool, sb: bool, st: bool, nm: bool) -> Fixed {
    Fixed {
        bi: Ok(bi),
        sb: Ok(sb),
        st: Ok(st),
        nm: Ok(nm),
        log: Vec::new(),
    }
}

impl TouchProbe for Fixed {
    fn bounds_intersect(&mut self, _c: usize) -> Result<bool, ProbeError> {
        self.log.push("bounds");
        self.bi
    }
    fn stroke_between(&mut self, _c: usize) -> Result<bool, ProbeError> {
        self.log.push("divider");
        self.sb
    }
    fn strict_touch(&mut self, _c: usize) -> Result<bool, ProbeError> {
        self.log.push("strict");
        self.st
    }
    fn near_miss(&mut self, _c: usize) -> Result<bool, ProbeError> {
        self.log.push("near");
        self.nm
    }
}

#[test]
fn the_probe_protocol_short_circuits_in_the_baseline_order() {
    let mut p = fixed(false, true, true, true);
    assert!(select_absorbed(&[4], &mut p).unwrap().is_empty());
    assert_eq!(p.log, vec!["bounds"]);
    let mut p = fixed(true, true, true, true);
    assert!(select_absorbed(&[4], &mut p).unwrap().is_empty());
    assert_eq!(
        p.log,
        vec!["bounds", "divider"],
        "a divider between the fills blocks the merge"
    );
    let mut p = fixed(true, false, true, false);
    assert_eq!(select_absorbed(&[4], &mut p).unwrap(), vec![4]);
    assert_eq!(
        p.log,
        vec!["bounds", "divider", "strict"],
        "no near-miss question after a strict touch"
    );
    let mut p = fixed(true, false, false, true);
    assert_eq!(select_absorbed(&[4], &mut p).unwrap(), vec![4]);
    assert_eq!(p.log, vec!["bounds", "divider", "strict", "near"]);
    let mut p = fixed(true, false, false, false);
    assert!(select_absorbed(&[4, 7], &mut p).unwrap().is_empty());
    assert_eq!(p.log.len(), 8, "every candidate is asked in turn");
    let mut p = fixed(true, false, true, false);
    assert_eq!(select_absorbed(&[2, 5, 9], &mut p).unwrap(), vec![2, 5, 9]);
}

#[test]
fn a_throwing_strict_or_near_probe_counts_as_no_touch_like_the_baseline_catch() {
    let mut p = fixed(true, false, false, true);
    p.st = Err(ProbeError::Failed);
    assert_eq!(
        select_absorbed(&[1], &mut p).unwrap(),
        vec![1],
        "the near miss still gets its turn"
    );
    let mut p = fixed(true, false, false, true);
    p.st = Err(ProbeError::Failed);
    p.nm = Err(ProbeError::Failed);
    assert!(select_absorbed(&[1], &mut p).unwrap().is_empty());
}

#[test]
fn unavailable_geometry_aborts_and_unwrapped_failures_surface() {
    for stage in 0..4 {
        let mut p = fixed(true, false, false, false);
        match stage {
            0 => p.bi = Err(ProbeError::Unavailable),
            1 => p.sb = Err(ProbeError::Unavailable),
            2 => p.st = Err(ProbeError::Unavailable),
            _ => p.nm = Err(ProbeError::Unavailable),
        }
        assert_eq!(
            select_absorbed(&[1], &mut p),
            Err(MergeError::GeometryUnavailable),
            "stage {stage}"
        );
    }
    let mut p = fixed(true, false, false, false);
    p.bi = Err(ProbeError::Failed);
    assert_eq!(
        select_absorbed(&[1], &mut p),
        Err(MergeError::ProbeFailed("bounds_intersect"))
    );
    let mut p = fixed(true, false, false, false);
    p.sb = Err(ProbeError::Failed);
    assert_eq!(
        select_absorbed(&[1], &mut p),
        Err(MergeError::ProbeFailed("stroke_between"))
    );
    assert_eq!(
        select_absorbed(&[], &mut UnavailableTouch),
        Ok(vec![]),
        "nothing to ask, nothing to fail"
    );
    assert_eq!(
        select_absorbed(&[3], &mut UnavailableTouch),
        Err(MergeError::GeometryUnavailable)
    );
}

pub(crate) fn area(a: f64, x: f64, y: f64, w: f64, h: f64) -> AreaFacts {
    AreaFacts {
        area: a,
        bounds: b(x, y, w, h),
    }
}

#[test]
fn near_identical_tolerances_by_hand() {
    let base = area(10000.0, 0.0, 0.0, 100.0, 100.0);
    assert!(is_near_identical_area(&base, &base));
    assert!(is_near_identical_area(
        &base,
        &area(-10400.0, 4.0, 0.0, 100.0, 100.0)
    ));
    assert!(!is_near_identical_area(
        &base,
        &area(10000.0, 4.01, 0.0, 100.0, 100.0)
    ));
    assert!(!is_near_identical_area(
        &base,
        &area(10420.0, 0.0, 0.0, 100.0, 100.0)
    ));
    assert!(is_near_identical_area(
        &area(10.0, 0.0, 0.0, 5.0, 5.0),
        &area(14.0, 0.0, 0.0, 5.0, 5.0)
    ));
    assert!(!is_near_identical_area(
        &area(10.0, 0.0, 0.0, 5.0, 5.0),
        &area(14.0001, 0.0, 0.0, 5.0, 5.0)
    ));
    assert!(!is_near_identical_area(
        &area(0.0, 0.0, 0.0, 100.0, 100.0),
        &base
    ));
    assert!(!is_near_identical_area(
        &base,
        &area(0.0, 0.0, 0.0, 100.0, 100.0)
    ));
    assert!(!is_near_identical_area(
        &area(f64::NAN, 0.0, 0.0, 1.0, 1.0),
        &base
    ));
    assert!(!is_near_identical_area(
        &base,
        &area(10000.0, f64::NAN, 0.0, 100.0, 100.0)
    ));
    // The tolerance comes from the FIRST shape's longer side: 16 px on a 400 px side.
    let wide = area(160000.0, 0.0, 0.0, 400.0, 400.0);
    assert!(is_near_identical_area(
        &wide,
        &area(160000.0, 16.0, 0.0, 400.0, 400.0)
    ));
    assert!(is_near_identical_area(
        &area(160000.0, 16.0, 0.0, 400.0, 400.0),
        &wide
    ));
    assert!(!is_near_identical_area(
        &wide,
        &area(160000.0, 16.5, 0.0, 400.0, 400.0)
    ));
}
