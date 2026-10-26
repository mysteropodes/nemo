//! nemo-test-harness: test-evaluation => property_follow_path_sample_controls_tests
//! Controls for the follow-path sampler: mutation detection, oracle coverage
//! and the cases Paper cannot be driven into by the oracle.

use crate::evaluation::property_follow_path_sample::{
    follow_path_geometry, path_curves, path_length, point_tangent_at, second_point, SegmentIn,
    StrokeIn,
};
use crate::property_follow_path_sample_tests::{bits, hex, run, ORACLE};

/// A deliberate mistake the comparison must catch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// The out-handle sign flipped.
    HandleSign,
    /// A closed path built without its closing curve.
    NoClosing,
    /// A wrong t-from-length: the offset off by one part in a million.
    TimeFromLength,
    /// A tangent left unnormalized (scaled by 1.5).
    TangentScale,
}

impl Mutation {
    pub(crate) fn handle(self, h: [f64; 2]) -> [f64; 2] {
        if self == Mutation::HandleSign {
            [-h[0], -h[1]]
        } else {
            h
        }
    }

    pub(crate) fn offset(self, o: f64) -> f64 {
        if self == Mutation::TimeFromLength {
            o * (1.0 + 1e-6)
        } else {
            o
        }
    }

    pub(crate) fn tangent(self, t: [f64; 2]) -> [f64; 2] {
        if self == Mutation::TangentScale {
            [t[0] * 1.5, t[1] * 1.5]
        } else {
            t
        }
    }
}

/// Every shape family is in the oracle; counts are what the generator emits.
#[test]
fn oracle_covers_the_families_and_edges() {
    let rows: Vec<Vec<&str>> = ORACLE.lines().map(|l| l.split(' ').collect()).collect();
    let pth = rows.iter().filter(|t| t[0] == "pth");
    assert_eq!(pth.clone().count(), 135);
    assert_eq!(rows.iter().filter(|t| t[0] == "sel").count(), 199);
    assert!(pth.clone().any(|t| t[3] == "1"), "closed paths");
    assert!(pth.clone().any(|t| hex(t[4]) == 0.0), "zero length");
    assert!(pth.clone().any(|t| hex(t[4]) > 1e8), "huge coordinates");
    assert!(
        pth.clone().any(|t| hex(t[4]) > 0.0 && hex(t[4]) < 1e-6),
        "tiny coordinates"
    );
    let nan = "7ff8000000000000=-";
    assert!(
        pth.clone().all(|t| t[5].contains(nan)),
        "NaN offset is null"
    );
    assert!(
        pth.clone()
            .any(|t| t[5].split(';').filter(|p| p.ends_with("=-")).count() > 3),
        "past-the-end offsets"
    );
    assert!(
        rows.iter()
            .any(|t| t[0] == "sel" && t[6] == "-" && t[2] == "~"),
        "no stroke"
    );
    assert!(
        rows.iter()
            .any(|t| t[0] == "sel" && t[6] == "-" && t[5] != "-"),
        "zero length / null point"
    );
}

/// Each deliberate mistake must make rows fail; counts are measured here.
#[test]
fn mutations_are_detected() {
    for (m, at_least) in [
        (Mutation::HandleSign, 112),
        (Mutation::NoClosing, 65),
        (Mutation::TimeFromLength, 260),
        (Mutation::TangentScale, 133),
    ] {
        let failed = run(m).bad.len();
        println!("{m:?}: {failed} rows rejected");
        assert!(failed >= at_least, "{m:?} slipped through: {failed} rows");
    }
}

fn seg(p: [f64; 2], hi: [f64; 2], ho: [f64; 2]) -> SegmentIn {
    SegmentIn {
        point: p,
        handle_in: Some(hi),
        handle_out: Some(ho),
    }
}

/// The `(10, 0)` second point applies only without a tangent; a zero
/// tangent (a truthy Point in JavaScript) keeps the point itself.
#[test]
fn second_point_fallback_and_zero_tangent() {
    assert_eq!(second_point([1.0, 2.0], None), [11.0, 2.0]);
    assert_eq!(second_point([1.0, 2.0], Some([0.0, 0.0])), [1.0, 2.0]);
    assert_eq!(second_point([1.0, 2.0], Some([0.0, 1.0])), [1.0, 12.0]);
}

/// A missing handle reads as `[0, 0]`, and strokes without segments, or with
/// one segment, are skipped; two coincident points have no length.
#[test]
fn missing_handles_and_stroke_selection() {
    let bare = [
        SegmentIn {
            point: [0.0, 0.0],
            handle_in: None,
            handle_out: None,
        },
        SegmentIn {
            point: [100.0, 0.0],
            handle_in: None,
            handle_out: None,
        },
    ];
    let zero = [
        seg([0.0; 2], [0.0; 2], [0.0; 2]),
        seg([0.0; 2], [0.0; 2], [0.0; 2]),
    ];
    let one = [seg([1.0; 2], [0.0; 2], [0.0; 2])];
    let strokes = [
        None,
        Some(StrokeIn {
            segments: None,
            closed: true,
        }),
        Some(StrokeIn {
            segments: Some(&one),
            closed: false,
        }),
        Some(StrokeIn {
            segments: Some(&bare),
            closed: false,
        }),
        Some(StrokeIn {
            segments: Some(&zero),
            closed: false,
        }),
    ];
    let g = follow_path_geometry(&strokes, |len| len * 0.25).expect("first long stroke");
    assert_eq!(
        (g.len, g.offset, g.point, g.tangent),
        (100.0, 25.0, [25.0, 0.0], Some([1.0, 0.0]))
    );
    assert_eq!(g.second, [35.0, 0.0]);
    assert!(
        follow_path_geometry(&strokes[4..], |l| l).is_none(),
        "zero length"
    );
    assert!(
        follow_path_geometry(&strokes[..3], |l| l).is_none(),
        "no stroke"
    );
}

/// Offsets 0 and the length, on an open and a closed path (the closed end
/// reads the first curve at time 0).
#[test]
fn ends_of_open_and_closed_paths() {
    let tri = [
        seg([0.0, 0.0], [0.0; 2], [0.0; 2]),
        seg([100.0, 0.0], [0.0; 2], [0.0; 2]),
        seg([100.0, 100.0], [0.0; 2], [0.0; 2]),
    ];
    for closed in [false, true] {
        let curves = path_curves(&tri, closed);
        let len = path_length(&curves);
        let end = point_tangent_at(&curves, closed, len).expect("end");
        let (want_p, want_t) = if closed {
            ([0.0, 0.0], [1.0, 0.0])
        } else {
            ([100.0, 100.0], [0.0, 1.0])
        };
        assert_eq!(
            (end.point, end.tangent),
            (want_p, Some(want_t)),
            "closed {closed}"
        );
        assert_eq!(
            bits(len),
            bits(if closed {
                100.0 + 100.0 + 100.0 * 2f64.sqrt()
            } else {
                200.0
            })
        );
    }
}
