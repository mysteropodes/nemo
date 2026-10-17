//! Hand-written literal pins of the evaluation join. The numbers were printed by `tools/pins.cjs` (baseline fbe4b8a3 run in a Node
//! vm) and are independent of the oracle parser and of `check`: a reader can hold the join to these values without trusting the
//! row machinery. Roughen values are compared with the rough-row bound `1e-8 * (1 + amount)`.
use super::super::{EvalSegment, EvaluatedVector};
use super::{bits, rows, snapshot};

const KAPPA10: f64 = 5.522847498;
const ROUGH_BOUND: f64 = 1e-8 * 9.0;

fn eval(id: &str) -> EvaluatedVector {
    let (_, all) = rows();
    let row = all.iter().find(|r| r.id == id).expect("oracle row");
    super::super::evaluate_vector(&snapshot(&row.doc), 0, row.frame).expect("evaluates")
}

fn bounds(v: &EvaluatedVector) -> [f64; 4] {
    let xs = v.segments.iter().map(|s| s.point[0]);
    let ys = v.segments.iter().map(|s| s.point[1]);
    [
        xs.clone().fold(f64::INFINITY, f64::min),
        ys.clone().fold(f64::INFINITY, f64::min),
        xs.fold(f64::NEG_INFINITY, f64::max),
        ys.fold(f64::NEG_INFINITY, f64::max),
    ]
}

fn anchor(v: &EvaluatedVector, i: usize, p: [f64; 2], hi: [f64; 2], ho: [f64; 2]) {
    assert_eq!(
        v.segments[i],
        EvalSegment {
            point: p,
            handle_in: hi,
            handle_out: ho
        },
        "anchor {i}"
    );
}

fn near(a: [f64; 4], b: [f64; 4], tol: f64) {
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() <= tol, "{a:?} vs {b:?}");
    }
}

#[test]
fn the_sampled_corner_radius_moves_the_rounded_rectangle() {
    // keys 0 -> 0 at frame 0 and 20 at frame 10: a square corner, then 10, then 20, then held at 20
    let z = [0.0, 0.0];
    let a0 = eval("rect-corner-tl-animated@0");
    assert_eq!((a0.closed, a0.segments.len()), (true, 7));
    anchor(&a0, 0, [0.0, 0.0], z, z);
    anchor(&a0, 1, [90.0, 0.0], z, [KAPPA10, 0.0]);
    anchor(&a0, 2, [100.0, 10.0], [0.0, -KAPPA10], z);
    let a5 = eval("rect-corner-tl-animated@5");
    assert_eq!(a5.segments.len(), 8);
    anchor(&a5, 0, [10.0, 0.0], [-KAPPA10, 0.0], z);
    anchor(&a5, 1, [90.0, 0.0], z, [KAPPA10, 0.0]);
    let a10 = eval("rect-corner-tl-animated@10");
    anchor(&a10, 0, [20.0, 0.0], [-11.045694996, 0.0], z);
    anchor(&a10, 2, [100.0, 10.0], [0.0, -KAPPA10], z);
    assert_eq!(bits(&a10), bits(&eval("rect-corner-tl-animated@12")));
    assert_eq!(bounds(&a10), [0.0, 0.0, 100.0, 60.0]);
}

#[test]
fn the_rebuild_takes_its_box_from_the_post_effect_anchors() {
    let z = [0.0, 0.0];
    let plain = eval("rect-corner-tl-animated@5");
    assert_eq!(bounds(&plain), [0.0, 0.0, 100.0, 60.0]);
    // zigzag size 4 pushes the stored rectangle's anchors out by 4 on every side; the rebuilt rounded rectangle lives in that box
    let after = eval("rect-zigzag-then-rebuild-uses-post-effect-bounds@5");
    assert_eq!((after.closed, after.segments.len()), (true, 8));
    assert_eq!(bounds(&after), [-4.0, -4.0, 104.0, 64.0]);
    anchor(&after, 0, [6.0, -4.0], [-KAPPA10, 0.0], z);
    anchor(&after, 1, [94.0, -4.0], z, [KAPPA10, 0.0]);
    anchor(&after, 2, [104.0, 6.0], [0.0, -KAPPA10], z);
    // a disabled entry keeps its slot but changes nothing: the box stays the stored one
    let disabled = eval("rect-disabled-effect-leaves-the-bounds@5");
    assert_eq!(bits(&disabled), bits(&plain));
}

#[test]
fn an_effect_without_an_animated_parameter_is_kept_and_a_static_shape_is_not_rebuilt() {
    let kept = eval("rect-zigzag-without-tracks-keeps-the-effect@5");
    assert_eq!(kept.segments.len(), 96);
    assert_eq!(bounds(&kept), [-4.0, -4.0, 104.0, 64.0]);
    let z = [0.0, 0.0];
    anchor(&kept, 0, [10.0, 0.0], z, z);
    anchor(&kept, 1, [11.574074074074074, -4.0], z, z);
    // paramShape stored, no track: the stored (here deliberately square) geometry is returned untouched
    let stored = eval("rect-without-tracks-is-not-rebuilt@5");
    assert_eq!(stored.segments.len(), 4);
    anchor(&stored, 0, [0.0, 0.0], z, z);
    anchor(&stored, 1, [100.0, 0.0], z, z);
    anchor(&stored, 2, [100.0, 60.0], z, z);
    anchor(&stored, 3, [0.0, 60.0], z, z);
    // a track on a plain vector (no paramShape) never triggers a rebuild: the stored square comes back
    let plain = eval("plain-vector-with-foreign-tracks-is-not-rebuilt@5");
    assert_eq!(plain.segments.len(), 4);
    anchor(&plain, 2, [100.0, 60.0], z, z);
    // tracks of another kind are ignored: the stored rounded rectangle comes back
    let foreign = eval("rect-foreign-kind-tracks-are-ignored@5");
    assert_eq!(foreign.segments.len(), 8);
    anchor(&foreign, 0, [10.0, 0.0], [-KAPPA10, 0.0], z);
}

#[test]
fn zigzag_is_frame_independent_and_roughen_follows_its_evolution() {
    let z = [0.0, 0.0];
    let (a, b) = (
        eval("plain-closed-zigzag@0"),
        eval("plain-closed-zigzag@12"),
    );
    assert_eq!(bits(&a), bits(&b));
    assert_eq!(a.segments.len(), 48);
    anchor(&a, 0, [0.0, 0.0], z, z);
    anchor(&a, 1, [1.9675925925925926, -4.0], z, z);
    anchor(&a, 2, [7.4074074074074066, 4.0], z, z);
    // evolution 50: frame 0 and frame 12 are different noise phases
    let (r0, r12) = (
        eval("plain-open-roughen-evolving@0"),
        eval("plain-open-roughen-evolving@12"),
    );
    assert!(!r0.closed && !r12.closed);
    assert_eq!((r0.segments.len(), r12.segments.len()), (7, 7));
    near(
        bounds(&r0),
        [
            -5.65685424949238,
            -5.65685424949238,
            102.52216842259756,
            27.381065967509034,
        ],
        ROUGH_BOUND,
    );
    near(
        bounds(&r12),
        [
            3.939298301762007,
            3.7848144814501348,
            102.27088868887009,
            23.770713181710263,
        ],
        ROUGH_BOUND,
    );
    // evolution 0 freezes the phase
    let (f0, f12) = (
        eval("plain-closed-roughen-frozen@0"),
        eval("plain-closed-roughen-frozen@12"),
    );
    assert_eq!((f0.closed, f0.segments.len()), (true, 12));
    near(
        bounds(&f0),
        [0.0, -6.524926639655989, 100.09855001691591, 60.0],
        ROUGH_BOUND,
    );
    near(bounds(&f12), bounds(&f0), 0.0);
}

#[test]
fn absent_empty_and_zero_size_stacks_return_the_stored_geometry() {
    let z = [0.0, 0.0];
    for id in [
        "plain-absent-stack@0",
        "plain-empty-stack@0",
        "plain-explicit-zero-size-is-identity@0",
    ] {
        let v = eval(id);
        assert_eq!((v.closed, v.segments.len()), (true, 4), "{id}");
        anchor(&v, 0, [0.0, 0.0], z, z);
        anchor(&v, 1, [100.0, 0.0], z, z);
        anchor(&v, 2, [100.0, 60.0], z, z);
        anchor(&v, 3, [0.0, 60.0], z, z);
    }
    // an open line with every parameter at its type default (size 12, 4 ridges, smooth 0)
    let line = eval("plain-line-zigzag-defaults@0");
    assert_eq!((line.closed, line.segments.len()), (false, 9));
    assert_eq!(
        bounds(&line),
        [0.0, -10.782335001743984, 79.472927500436, 31.64171000174398]
    );
}

#[test]
fn an_animated_zero_beats_the_stored_radius_and_a_single_key_is_constant() {
    let z = [0.0, 0.0];
    let square = eval("rect-animated-zero-beats-stored-radius@4");
    assert_eq!(square.segments.len(), 7);
    anchor(&square, 0, [0.0, 0.0], z, z);
    anchor(&square, 1, [90.0, 0.0], z, [KAPPA10, 0.0]);
    anchor(&square, 6, [0.0, 50.0], [0.0, KAPPA10], z);
    // bottom-right radius 14 from a single key at frame 3, held on both sides; top-left falls back to the stored 10
    let before = eval("rect-single-key-track@0");
    for id in ["rect-single-key-track@3", "rect-single-key-track@9"] {
        assert_eq!(bits(&eval(id)), bits(&before), "{id}");
    }
    anchor(&before, 0, [10.0, 0.0], [-KAPPA10, 0.0], z);
    anchor(&before, 4, [86.0, 60.0], [7.731986497199999, 0.0], z);
    anchor(&before, 5, [10.0, 60.0], z, [-KAPPA10, 0.0]);
}

#[test]
fn the_stored_box_member_is_never_read() {
    // the document stores box (0,0)-(10,10) beside anchors spanning (0,0)-(100,60): the rebuild frames itself on the anchors
    let (with_box, without) = (
        eval("rect-box-is-never-read@5"),
        eval("rect-corner-tl-animated@5"),
    );
    assert_eq!(bits(&with_box), bits(&without));
    assert_eq!(bounds(&with_box), [0.0, 0.0, 100.0, 60.0]);
}

#[test]
fn stack_order_changes_the_geometry() {
    let (zr, rz) = (
        eval("plain-stack-zigzag-then-roughen@10"),
        eval("plain-stack-roughen-then-zigzag@10"),
    );
    assert_eq!((zr.segments.len(), rz.segments.len()), (144, 144));
    near(
        bounds(&zr),
        [
            -4.631390198014327,
            -6.571389555238422,
            105.66287507576051,
            68.04097521359635,
        ],
        ROUGH_BOUND,
    );
    near(
        bounds(&rz),
        [
            -5.123348246517284,
            -5.136796794386587,
            108.01923551339698,
            66.51301320514389,
        ],
        ROUGH_BOUND,
    );
    assert!(bounds(&zr)[0] - bounds(&rz)[0] > 0.4);
}
