//! Chain-discovery tests for the two-bone IK kernel (L03-C, I12): `findIKChain` tolerance, roles,
//! rejections, rig-order precedence and the falsy empty id, on hand-built topologies (the oracle
//! rows for the same function run in `tests`).

use super::oracle_rows::{bone, r};
use super::{find_ik_chain, IkBone};

fn elbow_with_gap(d: f64) -> Vec<IkBone> {
    vec![
        bone("boneA", &[[0.0, 0.0], [50.0, 50.0]]),
        bone("boneB", &[[50.0 + d, 50.0], [100.0, 0.0]]),
    ]
}

#[test]
fn coincidence_tolerance_is_strictly_below_two() {
    // the gap is what the vertices make of it: 50 + 1.999999999999999 rounds to 52, a gap of exactly 2
    for (d, found) in [
        (0.0, true),
        (1.9999999, true),
        (1.99999999999999, true),
        (2.0, false),
        (3.0, false),
    ] {
        assert_eq!(
            find_ik_chain(&elbow_with_gap(d), "boneB", 1).is_some(),
            found,
            "gap {d}"
        );
    }
    // the distance is a 2-D one: (51.2, 51.6) is 2.0000000000000027 from (50, 50), so no chain
    let diag = vec![
        bone("boneA", &[[0.0, 0.0], [50.0, 50.0]]),
        bone("boneB", &[[51.2, 51.6], [100.0, 0.0]]),
    ];
    assert!(find_ik_chain(&diag, "boneB", 1).is_none());
    let inside = vec![
        bone("boneA", &[[0.0, 0.0], [50.0, 50.0]]),
        bone("boneB", &[[51.0, 51.0], [100.0, 0.0]]),
    ];
    assert!(find_ik_chain(&inside, "boneB", 1).is_some(), "sqrt(2) < 2");
}

#[test]
fn chain_roles_for_the_far_end_at_either_index() {
    let rig = elbow_with_gap(0.0);
    let s = find_ik_chain(&rig, "boneB", 1).expect("chain");
    assert_eq!(
        (s.root, s.joint, s.end_near, s.end),
        (r("boneA", 0), r("boneA", 1), r("boneB", 0), r("boneB", 1))
    );
    // dragging the FIRST vertex of the end bone: its near end is the last vertex
    let flipped = vec![
        bone("boneA", &[[0.0, 0.0], [50.0, 50.0]]),
        bone("boneB", &[[100.0, 0.0], [50.0, 50.0]]),
    ];
    let s = find_ik_chain(&flipped, "boneB", 0).expect("chain");
    assert_eq!((s.end_near, s.end), (r("boneB", 1), r("boneB", 0)));
    // from boneA's first vertex the root is boneB's far end
    let s = find_ik_chain(&rig, "boneA", 0).expect("chain");
    assert_eq!((s.root, s.joint), (r("boneB", 1), r("boneB", 0)));
}

#[test]
fn closed_short_interior_and_unknown_bones_have_no_chain() {
    let mut closed_end = elbow_with_gap(0.0);
    closed_end[1].closed = true;
    let mut closed_root = elbow_with_gap(0.0);
    closed_root[0].closed = true;
    let three = vec![
        bone("boneA", &[[0.0, 0.0], [50.0, 50.0]]),
        bone("boneB", &[[50.0, 50.0], [75.0, 20.0], [100.0, 0.0]]),
    ];
    assert!(find_ik_chain(&closed_end, "boneB", 1).is_none());
    assert!(find_ik_chain(&closed_root, "boneB", 1).is_none());
    assert!(
        find_ik_chain(&three, "boneB", 1).is_none(),
        "interior vertex"
    );
    assert!(find_ik_chain(&three, "boneB", 2).is_some());
    assert!(find_ik_chain(&elbow_with_gap(0.0), "boneB", 2).is_none());
    assert!(find_ik_chain(&elbow_with_gap(0.0), "nope", 0).is_none());
    let lone = vec![
        bone("boneA", &[[50.0, 50.0]]),
        bone("boneB", &[[50.0, 50.0], [100.0, 0.0]]),
    ];
    assert!(
        find_ik_chain(&lone, "boneB", 1).is_none(),
        "a one-vertex candidate is skipped"
    );
}

#[test]
fn first_coinciding_bone_in_rig_order_wins_and_the_empty_id_is_falsy() {
    let c1 = bone("boneC1", &[[50.0, 50.0], [10.0, 90.0]]);
    let c2 = bone("boneC2", &[[0.0, 0.0], [50.0, 50.0]]);
    let end = bone("boneB", &[[50.0, 50.0], [100.0, 0.0]]);
    let id_of = |rig: &[IkBone]| find_ik_chain(rig, "boneB", 1).expect("chain").joint.bone_id;
    assert_eq!(id_of(&[c1.clone(), c2.clone(), end.clone()]), "boneC1");
    assert_eq!(id_of(&[c2.clone(), c1.clone(), end.clone()]), "boneC2");
    // a bone whose both ends are within tolerance: its first vertex wins
    let both = bone("boneC3", &[[50.0, 50.0], [50.5, 50.5]]);
    assert_eq!(
        find_ik_chain(&[both, end.clone()], "boneB", 1)
            .expect("chain")
            .joint
            .vi,
        0
    );
    // an empty id is falsy: it neither stops the search nor counts as found
    let blank = bone("", &[[0.0, 0.0], [50.0, 50.0]]);
    assert!(find_ik_chain(&[blank.clone(), end.clone()], "boneB", 1).is_none());
    assert_eq!(id_of(&[blank, c1, end]), "boneC1");
}
