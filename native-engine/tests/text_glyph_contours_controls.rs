//! Literal pins and mutation controls for the contour half of the glyph
//! contour kernel (G38b / L06-B): conversion, close, classification and the
//! slit. The shear, refusals and flags are in `text_glyph_contours_shear.rs`.
//! Literal expected values come from rows of the real baseline run in
//! `fixtures/text-glyph-contours/oracle.txt`; every mutation control rebuilds a
//! plausible "fixed" or "simplified" behavior and proves the oracle rejects it
//! while the production code equals the oracle.

use super::oracle::{assert_segments, case, parse_oracle, Oracle};
use super::{
    bbox_area, bbox_contains, contour_bbox, contour_containers, merge_plan, outline_to_contours,
    slit_merge, slit_merge_at, GlyphSegment,
};

fn contours_of(oracle: &Oracle, name: &str) -> Vec<Vec<GlyphSegment>> {
    outline_to_contours(&case(oracle, name).commands).expect("contours")
}

fn points(segments: &[GlyphSegment]) -> Vec<[f64; 2]> {
    segments.iter().map(|s| s.point).collect()
}

#[test]
fn quad_and_cubic_conversion_store_offsets_from_their_own_anchor() {
    let oracle = parse_oracle();
    let quad = contours_of(&oracle, "quad_simple");
    assert_eq!(quad.len(), 1);
    assert_eq!(quad[0][0].point, [0.0, 0.0]);
    assert_eq!(quad[0][0].handle_in, [0.0, 0.0]);
    assert_eq!(
        quad[0][0].handle_out,
        [3.333333333333333, 6.666666666666666]
    );
    assert_eq!(quad[0][1].point, [10.0, 0.0]);
    assert_eq!(
        quad[0][1].handle_in,
        [-3.333333333333333, 6.666666666666666]
    );
    assert_eq!(quad[0][1].handle_out, [0.0, 0.0]);
    // Cubic: handleOut on the previous segment, handleIn on the new one, both
    // relative to their own anchor (x2 - x, y2 - y).
    let cubic = contours_of(&oracle, "cubic_simple");
    assert_eq!(cubic[0].len(), 3);
    assert_eq!(cubic[0][0].handle_out, [0.0, 10.0]);
    assert_eq!(cubic[0][1].point, [10.0, 0.0]);
    assert_eq!(cubic[0][1].handle_in, [0.0, 10.0]);
    assert_eq!(cubic[0][2].point, [5.0, -5.0]);
}

#[test]
fn quad_handle_is_the_rounded_difference_and_the_direct_product_would_fail() {
    let oracle = parse_oracle();
    let c = case(&oracle, "quad_roundoff");
    let got = outline_to_contours(&c.commands).expect("contours");
    // Mutation: handleOut = 2/3 * (q - p0) computed directly.
    let direct = [2.0 / 3.0 * (151.9 - 137.4), 2.0 / 3.0 * (60.7 - 88.1)];
    assert_ne!(
        got[0][0].handle_out, direct,
        "the baseline handle carries (p0 + 2/3 d) - p0 rounding"
    );
    assert_eq!(got[0][0].handle_out, c.contours[0][0].handle_out);
    assert_eq!(
        got[0][0].handle_out,
        [9.666666666666657, -18.26666666666665]
    );
}

#[test]
fn quad_overwrites_the_previous_segments_handle_out() {
    let oracle = parse_oracle();
    let got = contours_of(&oracle, "quad_overwrites_previous_handle_out");
    // M(0,0) C(1,1,2,2,5,5) then Q(9,1,10,0) from (5,5): the cubic's handle on
    // (5,5) is replaced by the quad's first control offset.
    let c = case(&oracle, "quad_overwrites_previous_handle_out");
    assert_segments("overwrite", &got[0], &c.contours[0]);
    assert_eq!(got[0][1].point, [5.0, 5.0]);
    assert_ne!(got[0][1].handle_out, [0.0, 0.0]);
}

#[test]
fn close_drops_only_a_start_duplicate_within_the_threshold() {
    let oracle = parse_oracle();
    let dropped = contours_of(&oracle, "square_dup_start_dropped");
    assert_eq!(
        points(&dropped[0]),
        [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
    );
    let open = contours_of(&oracle, "square_open_no_dup");
    assert_eq!(open[0].len(), 4, "a far last point is kept");
    let within = contours_of(&oracle, "z_dup_within_threshold");
    assert_eq!(
        within[0].len(),
        3,
        "(0.009, 0.009) is a duplicate of (0, 0)"
    );
    let at = contours_of(&oracle, "z_dup_at_threshold_not_dropped");
    assert_eq!(at[0].len(), 4, "0.51 - 0.5 is not below 0.01");
    let one_axis = contours_of(&oracle, "z_dup_x_only_close");
    assert_eq!(one_axis[0].len(), 4, "close on x alone is not a duplicate");
    let none = contours_of(&oracle, "square_no_close_command");
    assert_eq!(none[0].len(), 4);
}

#[test]
fn close_hands_handle_in_to_segment_zero_and_edge_cases_stay_put() {
    let oracle = parse_oracle();
    let got = contours_of(&oracle, "z_moves_handle_in_onto_first");
    assert_eq!(got[0].len(), 4, "the closing (0, 0) was dropped");
    assert_eq!(
        got[0][0].handle_in,
        [-3.0, 3.0],
        "x2 - x, y2 - y of the dropped point"
    );
    assert_eq!(got[0][3].point, [0.0, 10.0]);
    assert_eq!(
        got[0][3].handle_out,
        [-3.0, -3.0],
        "the last curve's handleOut is kept"
    );
    let single = contours_of(&oracle, "z_single_point_contour");
    assert_eq!(single[0].len(), 1, "a one-point contour is left alone");
    let before = contours_of(&oracle, "z_alone_before_move");
    assert_eq!(before.len(), 1, "a Z before any M is a no-op");
    let twice = contours_of(&oracle, "z_twice_pops_twice");
    assert_eq!(twice[0].len(), 3, "a second Z applies the same test again");
    let moves = contours_of(&oracle, "two_moves_only");
    assert_eq!(moves.len(), 2);
    assert!(moves.iter().all(|c| c.len() == 1));
}

#[test]
fn classification_pins_tightest_container_ties_edges_and_vanishing_islands() {
    let oracle = parse_oracle();
    let nested = contours_of(&oracle, "island_in_hole_drops_island");
    let containers = contour_containers(&nested);
    assert_eq!(containers, [None, Some(0), Some(1)]);
    assert_eq!(merge_plan(&containers), [(0, 1)]);
    let c = case(&oracle, "island_in_hole_drops_island");
    assert_eq!(c.merged.len(), 1);
    assert_eq!(
        c.merged[0].len(),
        10,
        "ext + hole ring + two slit anchors; the island is gone"
    );
    let tight = contour_containers(&contours_of(&oracle, "tightest_container_not_first"));
    assert_eq!(tight, [None, Some(0), Some(1)]);
    let equal = contour_containers(&contours_of(&oracle, "identical_bbox_pair_both_holes"));
    assert_eq!(equal, [Some(1), Some(0)]);
    assert!(merge_plan(&equal).is_empty());
    let first = contour_containers(&contours_of(&oracle, "tie_two_equal_containers_first_wins"));
    assert_eq!(first, [None, None, Some(0)]);
    let swapped = contour_containers(&contours_of(&oracle, "tie_two_equal_containers_swapped"));
    assert_eq!(swapped, [None, None, Some(0)]);
    let edge = contour_containers(&contours_of(&oracle, "hole_touching_container_edge"));
    assert_eq!(
        edge,
        [None, Some(0)],
        "a shared edge still counts as contained"
    );
    let overlap = contour_containers(&contours_of(&oracle, "overlapping_not_contained"));
    assert_eq!(overlap, [None, None]);
    let listed = contour_containers(&contours_of(&oracle, "hole_listed_before_exterior"));
    assert_eq!(listed, [Some(1), None]);
    let zero_area = contour_containers(&contours_of(&oracle, "collinear_zero_area_contour"));
    assert_eq!(zero_area, [None, Some(0)]);
}

/// Mutation: pick the container with a different tie-break / area rule.
fn containers_with(
    contours: &[Vec<GlyphSegment>],
    replace: fn(f64, f64) -> bool,
) -> Vec<Option<usize>> {
    let boxes: Vec<_> = contours.iter().map(|c| contour_bbox(c)).collect();
    let mut out: Vec<Option<usize>> = vec![None; boxes.len()];
    for (i, inner) in boxes.iter().enumerate() {
        for (j, outer) in boxes.iter().enumerate() {
            if i == j || !bbox_contains(outer, inner) {
                continue;
            }
            let take = match out[i] {
                None => true,
                Some(k) => replace(bbox_area(&boxes[k]), bbox_area(outer)),
            };
            if take {
                out[i] = Some(j);
            }
        }
    }
    out
}

#[test]
fn container_tie_break_and_area_rule_mutations_fail_the_oracle() {
    let oracle = parse_oracle();
    let strictly_greater: fn(f64, f64) -> bool = |current, candidate| current > candidate;
    let greater_or_equal: fn(f64, f64) -> bool = |current, candidate| current >= candidate;
    let smaller: fn(f64, f64) -> bool = |current, candidate| current < candidate;
    for c in oracle.cases.iter().filter(|c| !c.fails) {
        let contours = contours_of(&oracle, &c.name);
        assert_eq!(
            containers_with(&contours, strictly_greater),
            contour_containers(&contours),
            "{}: the rebuilt baseline rule equals production",
            c.name
        );
    }
    let tie = contours_of(&oracle, "tie_two_equal_containers_first_wins");
    assert_eq!(
        containers_with(&tie, greater_or_equal)[2],
        Some(1),
        "`>=` picks the last equal container"
    );
    assert_ne!(
        containers_with(&tie, greater_or_equal),
        contour_containers(&tie)
    );
    let tight = contours_of(&oracle, "tightest_container_not_first");
    assert_eq!(
        containers_with(&tight, smaller)[2],
        Some(0),
        "picking the largest is not tightest"
    );
    assert_ne!(containers_with(&tight, smaller), contour_containers(&tight));
}

#[test]
fn the_slit_lands_at_index_zero_zero_with_the_exact_merged_order() {
    let oracle = parse_oracle();
    let contours = contours_of(&oracle, "hole_o_probe");
    let merged = slit_merge(&contours[0], &contours[1]);
    assert_eq!(
        points(&merged),
        [
            [0.0, 0.0],
            [4.0, 4.0],
            [6.0, 4.0],
            [6.0, 6.0],
            [4.0, 6.0],
            [4.0, 4.0],
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0]
        ]
    );
    let c = case(&oracle, "hole_o_probe");
    assert_segments("probe", &merged, &c.merged[0]);
    // The hole's first segment appears twice and the exterior's first twice.
    assert_eq!(merged[1], merged[5]);
    assert_eq!(merged[0], merged[6]);
}

/// Mutation: the "corrected" `_polyClosestPair` over real anchors (squared
/// distance, first minimum wins), which the baseline never actually runs.
fn corrected_pair(ext: &[GlyphSegment], hole: &[GlyphSegment]) -> (usize, usize) {
    let (mut best, mut pi, mut pj) = (f64::INFINITY, 0, 0);
    for (i, a) in ext.iter().enumerate() {
        for (j, b) in hole.iter().enumerate() {
            let (dx, dy) = (a.point[0] - b.point[0], a.point[1] - b.point[1]);
            let d = dx * dx + dy * dy;
            if d < best {
                best = d;
                pi = i;
                pj = j;
            }
        }
    }
    (pi, pj)
}

fn corrected_merge(contours: &[Vec<GlyphSegment>]) -> Vec<Vec<GlyphSegment>> {
    let containers = contour_containers(contours);
    let plan = merge_plan(&containers);
    let mut out = Vec::new();
    for (index, contour) in contours.iter().enumerate() {
        if containers[index].is_some() {
            continue;
        }
        let mut merged = contour.clone();
        for (_, hole) in plan.iter().filter(|(exterior, _)| *exterior == index) {
            let (pi, pj) = corrected_pair(&merged, &contours[*hole]);
            merged = slit_merge_at(&merged, &contours[*hole], pi, pj).expect("indices in range");
        }
        out.push(merged);
    }
    out
}

#[test]
fn a_corrected_closest_pair_fails_the_oracle_while_production_matches_it() {
    let oracle = parse_oracle();
    let near = case(&oracle, "hole_near_far_corner");
    let contours = contours_of(&oracle, "hole_near_far_corner");
    assert_eq!(corrected_pair(&contours[0], &contours[1]), (2, 2));
    let fixed = corrected_merge(&contours);
    assert_eq!(
        points(&fixed[0]),
        [
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [9.0, 9.0],
            [7.0, 9.0],
            [7.0, 7.0],
            [9.0, 7.0],
            [9.0, 9.0],
            [10.0, 10.0],
            [0.0, 10.0]
        ]
    );
    assert_ne!(
        points(&fixed[0]),
        points(&near.merged[0]),
        "the fix changes the order"
    );
    assert_eq!(
        points(&near.merged[0]),
        [
            [0.0, 0.0],
            [7.0, 7.0],
            [9.0, 7.0],
            [9.0, 9.0],
            [7.0, 9.0],
            [7.0, 7.0],
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0]
        ],
        "the baseline slit is still at (0, 0)"
    );
    let mut differing = 0;
    for c in oracle
        .cases
        .iter()
        .filter(|c| !c.fails && !c.merges.is_empty())
    {
        let contours = contours_of(&oracle, &c.name);
        let fixed = corrected_merge(&contours);
        let same =
            fixed.len() == c.merged.len() && fixed.iter().zip(&c.merged).all(|(a, b)| a == b);
        if !same {
            differing += 1;
        }
        let production = super::contours_to_merged_paths(&contours);
        assert!(
            production.iter().zip(&c.merged).all(|(a, b)| a == b),
            "{}",
            c.name
        );
    }
    assert!(
        differing >= 10,
        "only {differing} oracle cases tell the two pairs apart"
    );
}
