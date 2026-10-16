//! Mutation controls for the SVG explode kernel (L10-D, G35).
//!
//! Each deliberate mistake below is re-implemented here and run through the SAME oracle
//! comparison as the shipped kernel. It must be rejected by exactly as many rows as the
//! equivalent source edit of the baseline functions changes in the JS oracle (the
//! `# control` lines of `oracle.txt`, measured by `tools/gen.mjs`): the Rust mutant is a faithful
//! model of the JS mutant, and neither is vacuous. The unmutated model must agree with
//! production on every row, otherwise a mutant could "fail" for a reason other than its defect.

use super::rows::{check, control_count, production, Impl};
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Contain {
    Contains,
    Swapped,
    Intersects,
}

#[derive(Clone, Copy)]
struct Mutation {
    contain: Contain,
    rule_swapped: bool,
    opacity_dropped: bool,
    islands_unstyled: bool,
    merge_at_zero: bool,
    slit_revisit_dropped: bool,
    hole_start_not_repeated: bool,
    tie_last_wins: bool,
}

const BASE: Mutation = Mutation {
    contain: Contain::Contains,
    rule_swapped: false,
    opacity_dropped: false,
    islands_unstyled: false,
    merge_at_zero: false,
    slit_revisit_dropped: false,
    hole_start_not_repeated: false,
    tie_last_wins: false,
};

/// Paper `Rectangle#intersects(rect)` with its default tolerance of 0.
fn intersects(this: &Bounds, other: &Bounds) -> bool {
    other.x + other.width > this.x
        && other.y + other.height > this.y
        && other.x < this.x + this.width
        && other.y < this.y + this.height
}

fn closest_mut(a: &[Pt], b: &[Pt], m: Mutation) -> (usize, usize) {
    let mut best_d = f64::INFINITY;
    let mut best = (0usize, 0usize);
    for (i, p) in a.iter().enumerate() {
        for (j, q) in b.iter().enumerate() {
            let dx = p[0] - q[0];
            let dy = p[1] - q[1];
            let d = dx * dx + dy * dy;
            let better = if m.tie_last_wins {
                d <= best_d
            } else {
                d < best_d
            };
            if better {
                best_d = d;
                best = (i, j);
            }
        }
    }
    best
}

fn pick_mut(pairing: HolePairing, ext: &[Seg], hole: &[Seg], m: Mutation) -> (usize, usize) {
    match pairing {
        HolePairing::BaselineArrayPoints => (0, 0),
        HolePairing::ClosestPoints => {
            if m.merge_at_zero {
                return (0, 0);
            }
            let a: Vec<Pt> = ext.iter().map(|s| s.point).collect();
            let b: Vec<Pt> = hole.iter().map(|s| s.point).collect();
            closest_mut(&a, &b, m)
        }
    }
}

fn merge_mut(ext: &[Seg], hole: &[Seg], pair: (usize, usize), m: Mutation) -> Option<Vec<Seg>> {
    let (i, j) = pair;
    // The `hole-start-not-repeated` mutant (`h < holeSegs.length`) never indexes the hole when it is
    // empty, so only the other variants refuse an empty hole (JS: valid slit-only path, not invalid).
    if ext.is_empty()
        || i >= ext.len()
        || (!m.hole_start_not_repeated && (hole.is_empty() || j >= hole.len()))
    {
        return None;
    }
    let mut out: Vec<Seg> = Vec::new();
    out.extend_from_slice(&ext[..=i]);
    let visits = if m.hole_start_not_repeated {
        hole.len()
    } else {
        hole.len() + 1
    };
    for h in 0..visits {
        out.push(hole[(j + h) % hole.len()]);
    }
    if !m.slit_revisit_dropped {
        out.push(ext[i]);
    }
    out.extend_from_slice(&ext[i + 1..]);
    Some(out)
}

fn contained_mut(ext: &Bounds, hole: &Bounds, m: Mutation) -> bool {
    match m.contain {
        Contain::Contains => ext.contains(hole),
        Contain::Swapped => hole.contains(ext),
        Contain::Intersects => intersects(ext, hole),
    }
}

fn explode_mut(leaf: &ImportLeaf, m: Mutation) -> Result<ExplodedLeaf, ExplodeError> {
    let (children, style) = match leaf {
        ImportLeaf::Path { .. } => return explode_leaf(leaf),
        ImportLeaf::Compound { children, style } => (children, style),
    };
    let carried = |style: &LeafStyle| LeafStyle {
        opacity: if m.opacity_dropped {
            1.0
        } else {
            style.opacity
        },
        ..style.clone()
    };
    let island_style = if m.islands_unstyled {
        LeafStyle {
            fill: None,
            stroke: None,
            stroke_width: 1.0,
            opacity: 1.0,
        }
    } else {
        carried(style)
    };
    let exteriors: Vec<usize> = (0..children.len())
        .filter(|&k| children[k].clockwise != m.rule_swapped)
        .collect();
    let holes: Vec<usize> = (0..children.len())
        .filter(|&k| children[k].clockwise == m.rule_swapped)
        .collect();
    let mut paths: Vec<ExplodedPath> = Vec::new();
    if holes.is_empty() {
        let source: Vec<usize> = if exteriors.is_empty() {
            (0..children.len()).collect()
        } else {
            exteriors
        };
        for k in source {
            paths.push(ExplodedPath {
                origin: PathOrigin::Island { child: k },
                contour: children[k].contour.clone(),
                style: island_style.clone(),
            });
        }
        return Ok(ExplodedLeaf {
            paths,
            unclaimed_holes: Vec::new(),
        });
    }
    for &e in &exteriors {
        let mine: Vec<usize> = holes
            .iter()
            .copied()
            .filter(|&h| contained_mut(&children[e].bounds, &children[h].bounds, m))
            .collect();
        let mut segments: Vec<Seg> = children[e].contour.segments.clone();
        for &h in &mine {
            let hole = &children[h].contour.segments;
            let pair = pick_mut(HolePairing::BaselineArrayPoints, &segments, hole, m);
            segments = match merge_mut(&segments, hole, pair, m) {
                Some(out) => out,
                None => return Err(ExplodeError::EmptyContour { child: h }),
            };
        }
        paths.push(ExplodedPath {
            origin: PathOrigin::Merged {
                exterior: e,
                holes: mine,
            },
            contour: Contour {
                segments,
                closed: true,
            },
            style: carried(style),
        });
    }
    Ok(ExplodedLeaf {
        paths,
        unclaimed_holes: Vec::new(),
    })
}

/// Rejected rows of `kind` when the kernel is replaced by the mutant model.
fn rejected(kind: &str, m: Mutation) -> usize {
    let closest = |a: &[Pt], b: &[Pt]| closest_mut(a, b, m);
    let pick = |p: HolePairing, e: &[Seg], h: &[Seg]| pick_mut(p, e, h, m);
    let merge = |e: &[Seg], h: &[Seg], pair: (usize, usize)| merge_mut(e, h, pair, m);
    let explode = |leaf: &ImportLeaf| explode_mut(leaf, m);
    let imp = Impl {
        closest: &closest,
        pick: &pick,
        merge: &merge,
        explode: &explode,
    };
    check(kind, &imp).1
}

const KINDS: [&str; 4] = ["P", "Mo", "Ma", "E"];

/// The mutant must be rejected by the same rows as the JS source edit named `name`.
fn assert_matches_js(name: &str, m: Mutation) -> usize {
    let mut total = 0;
    for kind in KINDS {
        let got = rejected(kind, m);
        assert_eq!(
            got,
            control_count(name, kind),
            "{}: rejected {} rows",
            name,
            kind
        );
        total += got;
    }
    assert!(total > 0, "{} must not be vacuous", name);
    total
}

#[test]
fn the_unmutated_model_agrees_with_production_on_every_row() {
    for kind in KINDS {
        assert_eq!(rejected(kind, BASE), 0, "model vs oracle on {}", kind);
        assert_eq!(
            check(kind, &production()).1,
            0,
            "production vs oracle on {}",
            kind
        );
    }
}

#[test]
fn wrong_containment_test_is_rejected() {
    let swapped = Mutation {
        contain: Contain::Swapped,
        ..BASE
    };
    let intersecting = Mutation {
        contain: Contain::Intersects,
        ..BASE
    };
    assert_matches_js("contain-swapped", swapped);
    assert_matches_js("contain-intersects", intersecting);
    // Only compound explosion reads bounds; the merge and pair rows are untouched.
    for kind in ["P", "Mo", "Ma"] {
        assert_eq!(rejected(kind, swapped), 0);
        assert_eq!(rejected(kind, intersecting), 0);
    }
}

#[test]
fn swapped_exterior_hole_rule_is_rejected() {
    let swapped = Mutation {
        rule_swapped: true,
        ..BASE
    };
    let total = assert_matches_js("rule-swapped", swapped);
    assert!(
        total >= 200,
        "the rule decides nearly every compound row: {}",
        total
    );
}

#[test]
fn merge_at_index_zero_instead_of_the_closest_pair_is_rejected_where_the_pair_engages() {
    let at_zero = Mutation {
        merge_at_zero: true,
        ..BASE
    };
    assert_matches_js("merge-at-zero", at_zero);
    // `explodeLeaf` hands the helper [x, y] arrays, so the real search never engages there:
    // the shipped baseline behaves as "always index 0" and this mutant is invisible to E and Ma.
    assert_eq!(rejected("E", at_zero), 0);
    assert_eq!(rejected("Ma", at_zero), 0);
    assert!(
        rejected("Mo", at_zero) > 40,
        "the object-point rows do engage the search"
    );
}

#[test]
fn engaging_the_closest_pair_inside_explode_is_not_the_baseline() {
    let engaged = |leaf: &ImportLeaf| explode_leaf_with(leaf, HolePairing::ClosestPoints);
    let imp = Impl {
        closest: &closest_pair,
        pick: &pick_pair,
        merge: &merge_hole_into_exterior,
        explode: &engaged,
    };
    let (rows, bad, _) = check("E", &imp);
    assert_eq!(rows, 277);
    assert_eq!(
        bad, 168,
        "the nearest-pair slit moves the seam in these baseline rows"
    );
}

#[test]
fn dropped_slit_revisit_is_rejected() {
    let dropped = Mutation {
        slit_revisit_dropped: true,
        ..BASE
    };
    assert_matches_js("slit-revisit-dropped", dropped);
    let unrepeated = Mutation {
        hole_start_not_repeated: true,
        ..BASE
    };
    assert_matches_js("hole-start-not-repeated", unrepeated);
    assert_eq!(rejected("P", dropped) + rejected("P", unrepeated), 0);
}

#[test]
fn tie_breaking_toward_the_last_pair_is_rejected() {
    let last = Mutation {
        tie_last_wins: true,
        ..BASE
    };
    assert_matches_js("tie-last-wins", last);
    assert_eq!(
        rejected("E", last),
        0,
        "array points never reach the search"
    );
}

#[test]
fn dropping_the_carried_style_is_rejected() {
    assert_matches_js(
        "opacity-not-carried",
        Mutation {
            opacity_dropped: true,
            ..BASE
        },
    );
    assert_matches_js(
        "islands-not-restyled",
        Mutation {
            islands_unstyled: true,
            ..BASE
        },
    );
}
