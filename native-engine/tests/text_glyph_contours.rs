//! Oracle tests for the glyph contour kernel (G38b / L06-B): every case of
//! `fixtures/text-glyph-contours/oracle.txt` (row grammar and provenance in
//! `text_glyph_contours_oracle.rs`) against the production functions, stage by
//! stage, so a failure names the stage that diverged.

use super::oracle::{assert_segments, case, parse_oracle, Case, Oracle};
use super::{
    contour_bbox, contour_containers, contours_to_merged_paths, glyph_paths, merge_plan,
    outline_to_contours, GlyphContourError, ItalicShear,
};

fn runnable(oracle: &Oracle) -> impl Iterator<Item = &Case> {
    oracle.cases.iter().filter(|c| !c.fails)
}

#[test]
fn oracle_contours_and_boxes_match_the_baseline() {
    let oracle = parse_oracle();
    for c in runnable(&oracle) {
        let got = outline_to_contours(&c.commands)
            .unwrap_or_else(|e| panic!("{}: refused {e:?}", c.name));
        assert_eq!(got.len(), c.contours.len(), "{}: contour count", c.name);
        for (i, (g, w)) in got.iter().zip(&c.contours).enumerate() {
            assert_segments(&format!("{} contour {i}", c.name), g, w);
        }
        assert_eq!(c.boxes.len(), c.contours.len(), "{}: BOX rows", c.name);
        for (i, (contour, want)) in got.iter().zip(&c.boxes).enumerate() {
            assert_eq!(contour_bbox(contour), *want, "{} box {i}", c.name);
        }
    }
}

#[test]
fn oracle_classification_reproduces_the_merge_calls_in_order() {
    let oracle = parse_oracle();
    for c in runnable(&oracle) {
        let contours = outline_to_contours(&c.commands).expect("contours");
        let containers = contour_containers(&contours);
        assert_eq!(
            merge_plan(&containers),
            c.merges,
            "{}: (exterior, hole) calls of _mergeHoleIntoExterior",
            c.name
        );
        let exteriors = containers.iter().filter(|k| k.is_none()).count();
        assert_eq!(
            exteriors, c.out_count,
            "{}: output path count is the containerless contours",
            c.name
        );
    }
}

#[test]
fn oracle_merged_paths_match_the_baseline() {
    let oracle = parse_oracle();
    for c in runnable(&oracle) {
        let contours = outline_to_contours(&c.commands).expect("contours");
        let got = contours_to_merged_paths(&contours);
        assert_eq!(got.len(), c.out_count, "{}: OUT", c.name);
        assert_eq!(c.merged.len(), c.out_count, "{}: P rows", c.name);
        for (k, (g, w)) in got.iter().zip(&c.merged).enumerate() {
            assert_eq!(w.len(), c.merged_counts[k], "{}: P {k} header", c.name);
            assert_segments(&format!("{} path {k}", c.name), g, w);
        }
    }
}

#[test]
fn oracle_sheared_paths_match_paper_transform() {
    let oracle = parse_oracle();
    let mut checked = 0;
    for c in runnable(&oracle) {
        let Some((skew, baseline)) = c.italic else {
            assert!(c.sheared.is_empty(), "{}: PS rows without italic", c.name);
            continue;
        };
        let shear = ItalicShear::new(skew, baseline).expect("finite shear");
        let got = glyph_paths(&c.commands, Some(&shear)).expect("glyph paths");
        assert_eq!(got.len(), c.sheared.len(), "{}: sheared path count", c.name);
        for (k, (g, w)) in got.iter().zip(&c.sheared).enumerate() {
            assert_eq!(w.len(), c.sheared_counts[k], "{}: PS {k} header", c.name);
            assert_segments(&format!("{} sheared {k}", c.name), &g.segments, w);
            checked += 1;
        }
    }
    assert!(checked >= 40, "only {checked} sheared paths were compared");
}

#[test]
fn oracle_unsheared_glyph_paths_equal_the_merged_output() {
    let oracle = parse_oracle();
    for c in runnable(&oracle) {
        let got = glyph_paths(&c.commands, None).expect("glyph paths");
        assert_eq!(got.len(), c.merged.len(), "{}: path count", c.name);
        for (k, (g, w)) in got.iter().zip(&c.merged).enumerate() {
            assert_segments(&format!("{} plain {k}", c.name), &g.segments, w);
        }
    }
}

#[test]
fn oracle_cases_where_the_baseline_throws_are_typed_refusals() {
    let oracle = parse_oracle();
    let failing: Vec<&Case> = oracle.cases.iter().filter(|c| c.fails).collect();
    let names: Vec<&str> = failing.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "line_before_move_fails",
            "quad_before_move_fails",
            "cubic_before_move_fails"
        ]
    );
    for c in failing {
        assert_eq!(
            outline_to_contours(&c.commands),
            Err(GlyphContourError::MissingMove { command: 0 }),
            "{}",
            c.name
        );
        assert_eq!(
            glyph_paths(&c.commands, None),
            Err(GlyphContourError::MissingMove { command: 0 })
        );
    }
}

#[test]
fn oracle_has_not_been_hollowed_out() {
    let oracle = parse_oracle();
    assert!(oracle.cases.len() >= 100, "cases: {}", oracle.cases.len());
    let italic = oracle.cases.iter().filter(|c| c.italic.is_some()).count();
    assert!(italic >= 30, "italic cases: {italic}");
    let merges: usize = oracle.cases.iter().map(|c| c.merges.len()).sum();
    assert!(merges >= 40, "recorded slit merges: {merges}");
    let segments: usize = oracle
        .cases
        .iter()
        .flat_map(|c| c.contours.iter())
        .map(Vec::len)
        .sum();
    assert!(segments >= 900, "contour segments: {segments}");
    let vanished = oracle
        .cases
        .iter()
        .filter(|c| !c.contours.is_empty() && c.out_count == 0)
        .count();
    assert!(
        vanished >= 2,
        "cases whose every contour became a hole: {vanished}"
    );
    let multi = oracle
        .cases
        .iter()
        .filter(|c| c.merges.windows(2).any(|w| w[0].0 == w[1].0))
        .count();
    assert!(
        multi >= 2,
        "cases merging two holes into one exterior: {multi}"
    );
    // Every quirk the controls rely on is a named case of the real run.
    for name in [
        "hole_o_probe",
        "island_in_hole_drops_island",
        "identical_bbox_pair_both_holes",
        "tie_two_equal_containers_first_wins",
        "z_twice_pops_twice",
        "quad_roundoff",
        "italic_curve_handles_not_linear",
        "italic_tiny_handles_skipped_like_paper",
    ] {
        let _ = case(&oracle, name);
    }
}

#[test]
fn oracle_tan12_equals_the_layout_kernel_shear_constant() {
    // The sibling L06-A kernel pins Math.tan(12 deg) as a constant; the real
    // Node run of the same expression must agree bit for bit.
    let oracle = parse_oracle();
    assert!(oracle.tan12 == super::super::object_text_layout_kernel::ITALIC_SHEAR);
    assert!(oracle.tan12 == 0.2125565616700221);
}
