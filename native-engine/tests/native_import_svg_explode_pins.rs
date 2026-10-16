//! Literal pins of the SVG explode kernel (L10-D, G35): the documented examples, each proven to be a
//! row of the baseline oracle (`oracle_has`) before the production output is compared to it.

use super::rows::{enc_leaf, enc_outs, enc_segs, oracle_has, oracle_rows};
use super::*;

fn seg(x: f64, y: f64) -> Seg {
    Seg {
        point: [x, y],
        handle_in: [0.0, 0.0],
        handle_out: [0.0, 0.0],
    }
}

/// The oracle's rectangle contours: clockwise walks right then down, counter-clockwise down then right.
fn square(x: f64, y: f64, w: f64, h: f64, clockwise: bool) -> Vec<Seg> {
    if clockwise {
        vec![seg(x, y), seg(x + w, y), seg(x + w, y + h), seg(x, y + h)]
    } else {
        vec![seg(x, y), seg(x, y + h), seg(x + w, y + h), seg(x + w, y)]
    }
}

fn child(x: f64, y: f64, w: f64, h: f64, clockwise: bool) -> CompoundChild {
    CompoundChild {
        contour: Contour {
            segments: square(x, y, w, h, clockwise),
            closed: true,
        },
        clockwise,
        bounds: Bounds {
            x,
            y,
            width: w,
            height: h,
        },
    }
}

fn red() -> LeafStyle {
    LeafStyle {
        fill: Some("rgb(255,0,0)".to_string()),
        stroke: None,
        stroke_width: 1.0,
        opacity: 1.0,
    }
}

fn translucent() -> LeafStyle {
    LeafStyle {
        fill: Some("rgba(0,128,255,0.5)".to_string()),
        stroke: Some("rgb(0,0,0)".to_string()),
        stroke_width: 2.5,
        opacity: 0.75,
    }
}

fn yellow() -> LeafStyle {
    LeafStyle {
        fill: Some("rgb(255,255,0)".to_string()),
        stroke: Some("rgba(255,0,255,0.25)".to_string()),
        stroke_width: 0.5,
        opacity: 0.3,
    }
}

fn merged(
    points: &[[f64; 2]],
    exterior: usize,
    holes: Vec<usize>,
    style: LeafStyle,
) -> ExplodedPath {
    ExplodedPath {
        origin: PathOrigin::Merged { exterior, holes },
        contour: Contour {
            segments: points.iter().map(|p| seg(p[0], p[1])).collect(),
            closed: true,
        },
        style,
    }
}

fn compound(children: Vec<CompoundChild>, style: LeafStyle) -> ImportLeaf {
    ImportLeaf::Compound { children, style }
}

/// Assert the literal is a baseline row: the leaf's `E` line with exactly these outputs.
fn assert_baseline_row(leaf: &ImportLeaf, paths: &[ExplodedPath]) {
    let line = format!("E {} => {}", enc_leaf(leaf), enc_outs(paths, true));
    assert!(oracle_has(&line), "not an oracle row: {}", line);
}

#[test]
fn donut_slits_at_segment_zero_and_revisits_each_anchor_once() {
    let leaf = compound(
        vec![
            child(0.0, 0.0, 100.0, 100.0, true),
            child(40.0, 40.0, 20.0, 20.0, false),
        ],
        red(),
    );
    let points = [
        [0.0, 0.0],
        [40.0, 40.0],
        [40.0, 60.0],
        [60.0, 60.0],
        [60.0, 40.0],
        [40.0, 40.0],
        [0.0, 0.0],
        [100.0, 0.0],
        [100.0, 100.0],
        [0.0, 100.0],
    ];
    let want = vec![merged(&points, 0, vec![1], red())];
    assert_baseline_row(&leaf, &want);
    let out = explode_leaf(&leaf).expect("donut explodes");
    assert_eq!(
        out,
        ExplodedLeaf {
            paths: want,
            unclaimed_holes: vec![]
        }
    );
    let count = |p: [f64; 2]| {
        out.paths[0]
            .contour
            .segments
            .iter()
            .filter(|s| s.point == p)
            .count()
    };
    assert_eq!(count([0.0, 0.0]), 2, "exterior slit anchor visited twice");
    assert_eq!(count([40.0, 40.0]), 2, "hole start visited twice");
    assert_eq!(count([100.0, 100.0]), 1);
    assert!(out.paths[0].contour.closed);
}

#[test]
fn closest_points_pairing_slits_at_the_nearest_corner() {
    let leaf = compound(
        vec![
            child(0.0, 0.0, 100.0, 100.0, true),
            child(80.0, 80.0, 10.0, 10.0, false),
        ],
        translucent(),
    );
    let baseline = [
        [0.0, 0.0],
        [80.0, 80.0],
        [80.0, 90.0],
        [90.0, 90.0],
        [90.0, 80.0],
        [80.0, 80.0],
        [0.0, 0.0],
        [100.0, 0.0],
        [100.0, 100.0],
        [0.0, 100.0],
    ];
    let nearest = [
        [0.0, 0.0],
        [100.0, 0.0],
        [100.0, 100.0],
        [90.0, 90.0],
        [90.0, 80.0],
        [80.0, 80.0],
        [80.0, 90.0],
        [90.0, 90.0],
        [100.0, 100.0],
        [0.0, 100.0],
    ];
    let want_default = vec![merged(&baseline, 0, vec![1], translucent())];
    assert_baseline_row(&leaf, &want_default);
    assert_eq!(explode_leaf(&leaf).expect("default").paths, want_default);
    let closest = explode_leaf_with(&leaf, HolePairing::ClosestPoints).expect("closest");
    assert_eq!(
        closest.paths,
        vec![merged(&nearest, 0, vec![1], translucent())]
    );
    // The closest-pair merge IS the baseline function called with {x, y} points: an oracle `Mo` row.
    let ext = square(0.0, 0.0, 100.0, 100.0, true);
    let hole = square(80.0, 80.0, 10.0, 10.0, false);
    let want: Vec<Seg> = nearest.iter().map(|p| seg(p[0], p[1])).collect();
    let line = format!(
        "Mo {} {} => {}",
        enc_segs(&ext),
        enc_segs(&hole),
        enc_segs(&want)
    );
    assert!(oracle_has(&line), "not an oracle row: {}", line);
    assert_eq!(pick_pair(HolePairing::ClosestPoints, &ext, &hole), (2, 2));
    assert_eq!(
        pick_pair(HolePairing::BaselineArrayPoints, &ext, &hole),
        (0, 0)
    );
    assert_eq!(closest.paths[0].contour.segments.len(), baseline.len());
}

#[test]
fn compound_without_holes_promotes_every_exterior_with_the_compound_style() {
    let mut open = child(300.0, 0.0, 10.0, 10.0, true);
    open.contour.closed = false;
    let leaf = compound(
        vec![
            child(0.0, 0.0, 100.0, 100.0, true),
            child(200.0, 0.0, 50.0, 50.0, true),
            open,
        ],
        yellow(),
    );
    let out = explode_leaf(&leaf).expect("islands");
    assert_eq!(out.unclaimed_holes, Vec::<usize>::new());
    let ImportLeaf::Compound { children, .. } = &leaf else {
        panic!("compound")
    };
    let want: Vec<ExplodedPath> = (0..3)
        .map(|k| ExplodedPath {
            origin: PathOrigin::Island { child: k },
            contour: children[k].contour.clone(),
            style: yellow(),
        })
        .collect();
    assert_eq!(out.paths, want);
    assert_baseline_row(&leaf, &want);
    assert!(
        !out.paths[2].contour.closed,
        "the island keeps its own closed flag"
    );
}

#[test]
fn each_exterior_merges_only_the_holes_its_bounds_contain_in_hole_order() {
    let leaf = compound(
        vec![
            child(0.0, 0.0, 100.0, 100.0, true),
            child(30.0, 30.0, 20.0, 20.0, false),
            child(200.0, 0.0, 100.0, 100.0, true),
            child(230.0, 30.0, 20.0, 20.0, false),
        ],
        yellow(),
    );
    let first = [
        [0.0, 0.0],
        [30.0, 30.0],
        [30.0, 50.0],
        [50.0, 50.0],
        [50.0, 30.0],
        [30.0, 30.0],
        [0.0, 0.0],
        [100.0, 0.0],
        [100.0, 100.0],
        [0.0, 100.0],
    ];
    let second = [
        [200.0, 0.0],
        [230.0, 30.0],
        [230.0, 50.0],
        [250.0, 50.0],
        [250.0, 30.0],
        [230.0, 30.0],
        [200.0, 0.0],
        [300.0, 0.0],
        [300.0, 100.0],
        [200.0, 100.0],
    ];
    let want = vec![
        merged(&first, 0, vec![1], yellow()),
        merged(&second, 2, vec![3], yellow()),
    ];
    assert_baseline_row(&leaf, &want);
    assert_eq!(explode_leaf(&leaf).expect("two donuts").paths, want);
}

#[test]
fn a_hole_inside_two_exterior_bounds_is_merged_into_both() {
    let leaf = compound(
        vec![
            child(0.0, 0.0, 100.0, 100.0, true),
            child(10.0, 10.0, 80.0, 80.0, true),
            child(40.0, 40.0, 20.0, 20.0, false),
        ],
        translucent(),
    );
    let out = explode_leaf(&leaf).expect("explodes");
    assert_eq!(out.paths.len(), 2);
    assert_eq!(
        out.paths[0].origin,
        PathOrigin::Merged {
            exterior: 0,
            holes: vec![2]
        }
    );
    assert_eq!(
        out.paths[1].origin,
        PathOrigin::Merged {
            exterior: 1,
            holes: vec![2]
        }
    );
    assert_eq!(out.unclaimed_holes, Vec::<usize>::new());
    assert_eq!(out.paths[1].contour.segments[0].point, [10.0, 10.0]);
    let outs = enc_outs(&out.paths, true);
    let line = format!("E {} => {}", enc_leaf(&leaf), outs);
    assert!(oracle_has(&line), "not an oracle row: {}", line);
}

#[test]
fn unclaimed_holes_are_reported_not_silently_dropped() {
    // A hole that pokes out of the only exterior's bounds: the exterior comes out unmerged.
    let poking = compound(
        vec![
            child(0.0, 0.0, 100.0, 100.0, true),
            child(90.0, 90.0, 30.0, 30.0, false),
        ],
        red(),
    );
    let out = explode_leaf(&poking).expect("explodes");
    let plain = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    assert_eq!(out.paths, vec![merged(&plain, 0, vec![], red())]);
    assert_eq!(out.unclaimed_holes, vec![1]);
    assert_baseline_row(&poking, &out.paths);
    // An all counter-clockwise compound has no exterior: the baseline inserts nothing at all.
    let ccw = compound(
        vec![
            child(0.0, 0.0, 100.0, 100.0, false),
            child(40.0, 40.0, 20.0, 20.0, false),
        ],
        red(),
    );
    let out = explode_leaf(&ccw).expect("explodes");
    assert!(out.paths.is_empty());
    assert_eq!(out.unclaimed_holes, vec![0, 1]);
    assert_baseline_row(&ccw, &[]);
}

#[test]
fn plain_paths_and_empty_compounds_follow_the_baseline() {
    let open = ImportLeaf::Path {
        contour: Contour {
            segments: square(10.0, 10.0, 40.0, 30.0, true)[..3].to_vec(),
            closed: false,
        },
        style: LeafStyle {
            fill: None,
            stroke: Some("rgb(51,102,153)".to_string()),
            stroke_width: 4.0,
            opacity: 1.0,
        },
    };
    let out = explode_leaf(&open).expect("path");
    assert_eq!(out.paths.len(), 1);
    assert_eq!(out.paths[0].origin, PathOrigin::Passthrough);
    let ImportLeaf::Path { contour, style } = &open else {
        panic!("path")
    };
    assert_eq!(&out.paths[0].contour, contour);
    assert_eq!(&out.paths[0].style, style);
    assert_baseline_row(&open, &out.paths);
    let empty = compound(vec![], red());
    let out = explode_leaf(&empty).expect("empty compound");
    assert!(out.paths.is_empty() && out.unclaimed_holes.is_empty());
    assert_baseline_row(&empty, &[]);
    // An empty exterior with a hole far away comes out as an empty closed merged path.
    let leaf = compound(
        vec![
            CompoundChild {
                contour: Contour {
                    segments: vec![],
                    closed: true,
                },
                clockwise: true,
                bounds: Bounds {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
            },
            child(40.0, 40.0, 20.0, 20.0, false),
        ],
        red(),
    );
    let out = explode_leaf(&leaf).expect("explodes");
    assert_eq!(out.paths, vec![merged(&[], 0, vec![], red())]);
    assert_eq!(out.unclaimed_holes, vec![1]);
    assert_baseline_row(&leaf, &out.paths);
}

#[test]
fn an_empty_contour_reaching_a_merge_is_a_typed_refusal_matching_the_oracle_invalid_rows() {
    let hole_empty = CompoundChild {
        contour: Contour {
            segments: vec![],
            closed: true,
        },
        clockwise: false,
        bounds: Bounds {
            x: 40.0,
            y: 40.0,
            width: 20.0,
            height: 20.0,
        },
    };
    let leaf = compound(vec![child(0.0, 0.0, 100.0, 100.0, true), hole_empty], red());
    assert_eq!(
        explode_leaf(&leaf),
        Err(ExplodeError::EmptyContour { child: 1 })
    );
    let ext_empty = CompoundChild {
        contour: Contour {
            segments: vec![],
            closed: true,
        },
        clockwise: true,
        bounds: Bounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    };
    let leaf = compound(vec![ext_empty, child(40.0, 40.0, 20.0, 20.0, false)], red());
    assert_eq!(
        explode_leaf(&leaf),
        Err(ExplodeError::EmptyContour { child: 0 })
    );
    // The pure merge reports the same condition as None, as the oracle's `invalid` rows do.
    let ext = square(0.0, 0.0, 100.0, 100.0, true);
    assert_eq!(merge_hole_into_exterior(&ext, &[], (0, 0)), None);
    assert_eq!(merge_hole_into_exterior(&[], &ext, (0, 0)), None);
    assert_eq!(merge_hole_into_exterior(&ext, &ext, (4, 0)), None);
    assert_eq!(merge_hole_into_exterior(&ext, &ext, (0, 4)), None);
    let invalid = oracle_rows("Mo")
        .iter()
        .filter(|f| f[2] == "=>" && f[3] == "invalid")
        .count();
    assert_eq!(invalid, 3, "empty ext, empty hole, both empty");
}
