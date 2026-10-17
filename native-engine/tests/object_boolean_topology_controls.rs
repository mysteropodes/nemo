//! Independent literal controls for the boolean result topology kernel
//! (G19 + G18, L02-D): 3 islands + 2 holes, a nested hole, unpaired holes,
//! polygon admission, insert slots and data carry. Every expected value is
//! written by hand from the baseline rules; none is read from the oracle. The
//! eraser controls live in `object_boolean_topology_controls_erase.rs`, the
//! candidate / probe controls in `..._controls_merge.rs`, the union / commit /
//! tag controls in `..._controls_commit.rs`.

use super::{
    admit_polygons, carry_boolean_data, classify, plan_insert, Bounds, DataValue, InsertRequest,
    IslandBuild, PaintArg, PathFacts, Polygon, ResultShape, Ring, SourceData, StrokeIdAction,
    StrokeInfo, TopologyError,
};

pub(crate) fn b(x: f64, y: f64, w: f64, h: f64) -> Bounds {
    Bounds {
        x,
        y,
        width: w,
        height: h,
    }
}

pub(crate) fn ext(x: f64, y: f64, w: f64, h: f64) -> Ring {
    Ring {
        clockwise: true,
        bounds: b(x, y, w, h),
        segments: 4,
    }
}

pub(crate) fn hole(x: f64, y: f64, w: f64, h: f64) -> Ring {
    Ring {
        clockwise: false,
        bounds: b(x, y, w, h),
        segments: 4,
    }
}

fn square(x: f64, y: f64, s: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
}

fn poly(exterior: Vec<[f64; 2]>, holes: Vec<Vec<[f64; 2]>>) -> Polygon {
    Polygon { exterior, holes }
}

pub(crate) fn text(v: &str) -> DataValue {
    DataValue::Text(v.to_string())
}

pub(crate) fn data(pairs: &[(&str, DataValue)]) -> SourceData {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn three_islands_two_holes_and_a_third_hole_are_all_retained() {
    let rings = vec![
        ext(0.0, 0.0, 100.0, 100.0),
        hole(10.0, 10.0, 20.0, 20.0),
        hole(50.0, 50.0, 20.0, 20.0),
        ext(200.0, 0.0, 50.0, 50.0),
        ext(300.0, 0.0, 50.0, 50.0),
        hole(310.0, 10.0, 10.0, 10.0),
    ];
    let plan = classify(&ResultShape::Compound(rings)).unwrap();
    let rows: Vec<(usize, Vec<usize>)> = plan
        .islands
        .iter()
        .map(|i| (i.ring, i.holes.clone()))
        .collect();
    assert_eq!(rows, vec![(0, vec![1, 2]), (3, vec![]), (4, vec![5])]);
    assert!(plan
        .islands
        .iter()
        .all(|i| i.build == IslandBuild::Merged && i.cleanup));
    assert_eq!((plan.exterior_count, plan.hole_count), (3, 3));
    assert!(plan.shared_holes.is_empty());
    let mut merged: Vec<usize> = plan.islands.iter().flat_map(|i| i.holes.clone()).collect();
    merged.sort_unstable();
    assert_eq!(
        merged,
        vec![1, 2, 5],
        "every hole is merged into exactly one island"
    );
}

#[test]
fn a_nested_island_keeps_its_own_hole_and_the_outer_pairing_is_reported() {
    // Outer square with a big hole, an island inside that hole, the island's own hole.
    let rings = vec![
        ext(0.0, 0.0, 300.0, 300.0),
        hole(50.0, 50.0, 200.0, 200.0),
        ext(100.0, 100.0, 100.0, 100.0),
        hole(120.0, 120.0, 20.0, 20.0),
    ];
    let plan = classify(&ResultShape::Compound(rings)).unwrap();
    assert_eq!(plan.islands[0].holes, vec![1, 3]);
    assert_eq!(plan.islands[1].holes, vec![3]);
    assert_eq!(
        plan.shared_holes,
        vec![3],
        "the baseline cuts hole 3 into the outer shape too"
    );
}

#[test]
fn a_hole_no_exterior_contains_is_refused_never_dropped() {
    let one = vec![ext(0.0, 0.0, 50.0, 50.0), hole(500.0, 500.0, 10.0, 10.0)];
    assert_eq!(
        classify(&ResultShape::Compound(one)),
        Err(TopologyError::UnpairedHoles(vec![1]))
    );
    let mixed = vec![
        ext(0.0, 0.0, 100.0, 100.0),
        hole(10.0, 10.0, 20.0, 20.0),
        hole(900.0, 900.0, 10.0, 10.0),
        hole(800.0, 0.0, 1.0, 1.0),
    ];
    assert_eq!(
        classify(&ResultShape::Compound(mixed)),
        Err(TopologyError::UnpairedHoles(vec![2, 3]))
    );
    let only_holes = vec![hole(0.0, 0.0, 20.0, 20.0), hole(50.0, 50.0, 20.0, 20.0)];
    assert_eq!(
        classify(&ResultShape::Compound(only_holes)),
        Err(TopologyError::UnpairedHoles(vec![0, 1]))
    );
}

#[test]
fn containment_is_inclusive_on_every_edge_and_strict_beyond() {
    let outer = b(0.0, 0.0, 100.0, 100.0);
    assert!(outer.contains(&b(0.0, 0.0, 100.0, 100.0)));
    assert!(outer.contains(&b(100.0, 100.0, 0.0, 0.0)));
    assert!(!outer.contains(&b(-0.5, 10.0, 10.0, 10.0)));
    assert!(!outer.contains(&b(10.0, -0.5, 10.0, 10.0)));
    assert!(!outer.contains(&b(95.0, 10.0, 5.5, 10.0)));
    assert!(!outer.contains(&b(10.0, 95.0, 10.0, 5.5)));
    let rings = vec![ext(0.0, 0.0, 100.0, 100.0), hole(0.0, 0.0, 100.0, 100.0)];
    assert_eq!(
        classify(&ResultShape::Compound(rings)).unwrap().islands[0].holes,
        vec![1]
    );
}

#[test]
fn a_result_without_holes_is_one_flat_island_per_exterior_in_order() {
    let rings = vec![ext(100.0, 0.0, 10.0, 10.0), ext(0.0, 0.0, 10.0, 10.0)];
    let plan = classify(&ResultShape::Compound(rings)).unwrap();
    let rows: Vec<(IslandBuild, usize, bool)> = plan
        .islands
        .iter()
        .map(|i| (i.build, i.ring, i.cleanup))
        .collect();
    assert_eq!(
        rows,
        vec![(IslandBuild::Flat, 0, false), (IslandBuild::Flat, 1, false)]
    );
    assert!(classify(&ResultShape::Compound(vec![]))
        .unwrap()
        .islands
        .is_empty());
    assert!(classify(&ResultShape::Empty).unwrap().islands.is_empty());
}

#[test]
fn a_single_path_is_one_cleaned_island() {
    let single = ResultShape::Single(PathFacts {
        bounds: b(0.0, 0.0, 40.0, 40.0),
        segments: 4,
    });
    let plan = classify(&single).unwrap();
    assert_eq!(plan.islands.len(), 1);
    assert_eq!(
        (
            plan.islands[0].build,
            plan.islands[0].ring,
            plan.islands[0].cleanup
        ),
        (IslandBuild::Single, 0, true)
    );
}

#[test]
fn bad_bounds_are_refused_with_the_ring_index() {
    let nan = Ring {
        bounds: b(f64::NAN, 0.0, 1.0, 1.0),
        ..ext(0.0, 0.0, 1.0, 1.0)
    };
    assert_eq!(
        classify(&ResultShape::Compound(vec![ext(0.0, 0.0, 9.0, 9.0), nan])),
        Err(TopologyError::NonFiniteBounds { ring: 1 })
    );
    let negative = Ring {
        bounds: b(0.0, 0.0, -1.0, 1.0),
        ..ext(0.0, 0.0, 1.0, 1.0)
    };
    assert_eq!(
        classify(&ResultShape::Compound(vec![negative])),
        Err(TopologyError::NegativeExtent { ring: 0 })
    );
}

#[test]
fn polygon_admission_orders_rings_exterior_then_its_holes() {
    let polys = vec![
        poly(
            square(0.0, 0.0, 100.0),
            vec![square(10.0, 10.0, 20.0), square(50.0, 50.0, 20.0)],
        ),
        poly(square(200.0, 0.0, 50.0), vec![]),
    ];
    let admission = admit_polygons(&polys).unwrap();
    let flags: Vec<bool> = match &admission.shape {
        ResultShape::Compound(rings) => rings.iter().map(|r| r.clockwise).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(flags, vec![true, false, false, true]);
    assert!(admission.dropped.is_empty());
}

#[test]
fn polygon_admission_drops_short_exteriors_and_reports_original_indices() {
    let polys = vec![
        poly(vec![[0.0, 0.0], [5.0, 5.0]], vec![]),
        poly(square(0.0, 0.0, 10.0), vec![]),
        poly(vec![], vec![]),
    ];
    let admission = admit_polygons(&polys).unwrap();
    assert_eq!(admission.dropped, vec![0, 2]);
    assert_eq!(
        admission.shape,
        ResultShape::Single(PathFacts {
            bounds: b(0.0, 0.0, 10.0, 10.0),
            segments: 4,
        })
    );
    let triangle = vec![poly(vec![[0.0, 0.0], [10.0, 0.0], [5.0, 8.0]], vec![])];
    assert!(matches!(
        admit_polygons(&triangle).unwrap().shape,
        ResultShape::Single(_)
    ));
    assert_eq!(admit_polygons(&[]).unwrap().shape, ResultShape::Empty);
}

#[test]
fn polygon_admission_refuses_a_degenerate_hole_and_a_non_finite_point() {
    let degenerate = vec![
        poly(square(0.0, 0.0, 10.0), vec![]),
        poly(
            square(0.0, 0.0, 100.0),
            vec![square(1.0, 1.0, 2.0), vec![[5.0, 5.0], [6.0, 6.0]]],
        ),
    ];
    assert_eq!(
        admit_polygons(&degenerate),
        Err(TopologyError::DegenerateHole {
            polygon: 1,
            hole: 1
        })
    );
    let nan = vec![poly(vec![[0.0, 0.0], [f64::NAN, 1.0], [2.0, 2.0]], vec![])];
    assert_eq!(
        admit_polygons(&nan),
        Err(TopologyError::NonFinitePoint { polygon: 0 })
    );
    // A dropped polygon's own holes are never inspected.
    let dropped = vec![
        poly(vec![[0.0, 0.0]], vec![vec![[1.0, 1.0]]]),
        poly(square(0.0, 0.0, 5.0), vec![]),
    ];
    assert!(admit_polygons(&dropped).is_ok());
}

#[test]
fn bounds_of_points_match_min_max_with_exact_subtraction() {
    assert_eq!(
        Bounds::of_points(&[[0.1, 0.2], [33.3, 0.2], [33.3, 77.7], [0.1, 77.7]]),
        b(0.1, 0.2, 33.3 - 0.1, 77.7 - 0.2)
    );
    assert_eq!(
        Bounds::of_points(&[[-5.0, 7.0], [3.0, -2.0], [0.0, 0.0]]),
        b(-5.0, -2.0, 8.0, 9.0)
    );
    assert_eq!(Bounds::of_points(&[]), Bounds::EMPTY);
}

fn island_request<'a>(
    at: usize,
    len: usize,
    src: Option<&'a SourceData>,
    stroke: Option<&'a StrokeInfo>,
) -> InsertRequest<'a> {
    InsertRequest {
        insert_at: at,
        layer_len: len,
        fill: PaintArg::Set("#ff0000".to_string()),
        opacity: Some(0.5),
        stroke,
        src,
        preserve_geometry: false,
    }
}

fn three_flat() -> ResultShape {
    ResultShape::Compound(vec![
        ext(0.0, 0.0, 1.0, 1.0),
        ext(5.0, 0.0, 1.0, 1.0),
        ext(9.0, 0.0, 1.0, 1.0),
    ])
}

#[test]
fn islands_land_at_insert_at_plus_k_clamped_to_the_layer() {
    let at = |insert_at: usize, len: usize| -> Vec<usize> {
        plan_insert(&three_flat(), &island_request(insert_at, len, None, None))
            .unwrap()
            .islands
            .iter()
            .map(|i| i.index)
            .collect()
    };
    assert_eq!(at(0, 3), vec![0, 1, 2]);
    assert_eq!(at(1, 3), vec![1, 2, 3]);
    assert_eq!(at(3, 3), vec![3, 4, 5]);
    assert_eq!(
        at(9, 2),
        vec![2, 3, 4],
        "insertChild clamps the first slot to the length"
    );
    assert_eq!(at(0, 0), vec![0, 1, 2]);
}

#[test]
fn first_island_keeps_the_stroke_id_later_islands_ask_for_a_fresh_one() {
    let src = data(&[
        ("strokeId", text("sSRC")),
        ("groupId", text("g1")),
        ("other", text("x")),
    ]);
    let plan = plan_insert(&three_flat(), &island_request(0, 0, Some(&src), None)).unwrap();
    let ids: Vec<StrokeIdAction> = plan
        .islands
        .iter()
        .map(|i| i.carry.stroke_id.clone())
        .collect();
    assert_eq!(
        ids,
        vec![
            StrokeIdAction::Carry(text("sSRC")),
            StrokeIdAction::Fresh,
            StrokeIdAction::Fresh
        ]
    );
    for island in &plan.islands {
        assert_eq!(island.carry.fields.get("groupId"), Some(&text("g1")));
        assert!(
            !island.carry.fields.contains_key("other"),
            "unlisted keys are not carried"
        );
        assert!(!island.carry.fields.contains_key("strokeId"));
    }
}

#[test]
fn stroke_is_dropped_unless_info_with_a_colour_is_supplied() {
    let info = StrokeInfo {
        color: "#222222".to_string(),
        width: 3.0,
        cap: "round".to_string(),
        join: "bevel".to_string(),
    };
    let kept = plan_insert(&three_flat(), &island_request(0, 0, None, Some(&info))).unwrap();
    assert!(kept
        .islands
        .iter()
        .all(|i| i.stroke.as_ref() == Some(&info)));
    let no_colour = StrokeInfo {
        color: String::new(),
        ..info.clone()
    };
    let cleared =
        plan_insert(&three_flat(), &island_request(0, 0, None, Some(&no_colour))).unwrap();
    assert!(cleared.islands.iter().all(|i| i.stroke.is_none()));
    let none = plan_insert(&three_flat(), &island_request(0, 0, None, None)).unwrap();
    assert!(none.islands.iter().all(|i| i.stroke.is_none()));
    assert!(none.islands.iter().all(|i| i.opacity == Some(0.5)));
}

#[test]
fn present_but_falsy_values_are_copied_and_absent_ones_are_not() {
    let src = data(&[
        ("groupId", text("")),
        ("ownerId", DataValue::Number(0.0)),
        ("tweenOn", DataValue::Bool(false)),
        ("channelTag", DataValue::Null),
        ("effects", DataValue::Opaque("[{\"t\":1}]".to_string())),
    ]);
    let carry = carry_boolean_data(Some(&src), false);
    assert_eq!(carry.fields.len(), 5);
    assert_eq!(carry.fields.get("channelTag"), Some(&DataValue::Null));
    assert!(!carry.fields.contains_key("boxAngle"));
    assert_eq!(carry.stroke_id, StrokeIdAction::Absent);
    assert_eq!(carry_boolean_data(None, true).fields.len(), 0);
}

#[test]
fn stroke_id_truthiness_decides_the_later_islands_only() {
    let with = |v: DataValue, first: bool| {
        carry_boolean_data(Some(&data(&[("strokeId", v)])), first).stroke_id
    };
    for falsy in [
        DataValue::Null,
        DataValue::Bool(false),
        DataValue::Number(0.0),
        text(""),
    ] {
        assert_eq!(
            with(falsy.clone(), true),
            StrokeIdAction::Carry(falsy.clone())
        );
        assert_eq!(with(falsy, false), StrokeIdAction::Absent);
    }
    for truthy in [text("0"), DataValue::Number(7.0), DataValue::Bool(true)] {
        assert_eq!(with(truthy.clone(), false), StrokeIdAction::Fresh);
        assert_eq!(with(truthy.clone(), true), StrokeIdAction::Carry(truthy));
    }
}

#[test]
fn js_truthiness_table() {
    assert!(!DataValue::Number(f64::NAN).truthy());
    assert!(!DataValue::Number(-0.0).truthy());
    assert!(DataValue::Number(-1.0).truthy());
    assert!(DataValue::Opaque("[]".to_string()).truthy());
    assert!(text(" ").truthy());
}
