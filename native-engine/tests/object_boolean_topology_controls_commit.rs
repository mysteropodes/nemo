//! Independent literal controls for the fill-merge union plan, commit slot
//! arithmetic and regeneration tags (L02-D). Hand-written expectations; nothing is
//! read from the oracle.

use super::controls::{b, ext, hole};
use super::controls_merge::area;
use super::{
    applied_tags, merge_tags, plan_merge_commit, plan_union, resolve_union, Absorbed, MergeCommit,
    MergeCommitInput, MergeDecision, MergeError, ParticipantTags, PathFacts, ResultShape,
    TopologyError, UniteDecision,
};

#[test]
fn union_planning_skips_copies_of_the_original_new_fill_only() {
    let new = area(10000.0, 0.0, 0.0, 100.0, 100.0);
    let absorbed = [
        Absorbed {
            index: 1,
            area: area(10000.0, 0.0, 0.0, 100.0, 100.0),
        },
        Absorbed {
            index: 4,
            area: area(2500.0, 0.0, 0.0, 50.0, 50.0),
        },
        Absorbed {
            index: 6,
            area: area(10100.0, 1.0, 1.0, 100.0, 100.0),
        },
    ];
    let plan = plan_union(&new, &absorbed);
    assert_eq!(plan.steps, vec![4]);
    assert_eq!(
        plan.near_identical,
        Some(1),
        "the FIRST near-identical child is the one kept"
    );
    assert_eq!(plan.skipped, vec![1, 6]);
    assert_eq!(
        resolve_union(&plan, &[true]),
        Ok(MergeDecision::Unite(UniteDecision {
            united: vec![4],
            failed: vec![]
        }))
    );
    assert_eq!(
        resolve_union(&plan, &[false]),
        Ok(MergeDecision::KeepExisting(1))
    );
    assert_eq!(
        resolve_union(&plan, &[]),
        Err(MergeError::OutcomeMismatch {
            steps: 1,
            outcomes: 0
        })
    );
    let far = plan_union(&new, &absorbed[1..2]);
    assert_eq!(resolve_union(&far, &[false]), Ok(MergeDecision::NoChange));
    let copies = plan_union(&new, &absorbed[0..1]);
    assert!(copies.steps.is_empty());
    assert_eq!(
        resolve_union(&copies, &[]),
        Ok(MergeDecision::KeepExisting(1))
    );
    let mixed = plan_union(
        &new,
        &[
            absorbed[1],
            Absorbed {
                index: 9,
                area: area(2400.0, 0.0, 0.0, 50.0, 50.0),
            },
        ],
    );
    assert_eq!(
        resolve_union(&mixed, &[false, true]),
        Ok(MergeDecision::Unite(UniteDecision {
            united: vec![9],
            failed: vec![4]
        }))
    );
}

fn square_shape() -> ResultShape {
    ResultShape::Single(PathFacts {
        bounds: b(0.0, 0.0, 10.0, 10.0),
        segments: 4,
    })
}

fn commit(
    layer_len: usize,
    new_index: usize,
    absorbed: &[usize],
    shape: &ResultShape,
) -> Result<MergeCommit, MergeError> {
    let unite = UniteDecision {
        united: absorbed.to_vec(),
        failed: vec![],
    };
    let participants = vec![ParticipantTags::default(); absorbed.len() + 1];
    plan_merge_commit(&MergeCommitInput {
        layer_len,
        new_index,
        absorbed,
        unite: &unite,
        shape,
        fill_hex8: "#ff0000",
        opacity: 0.5,
        data: None,
        participants: &participants,
    })
}

#[test]
fn the_united_shape_takes_the_topmost_participants_slot_less_removals_below() {
    let at = |len: usize, new: usize, absorbed: &[usize]| {
        let c = commit(len, new, absorbed, &square_shape()).unwrap();
        (c.removed, c.insert.islands[0].index, c.first_index)
    };
    assert_eq!(
        at(7, 3, &[1, 5]),
        (vec![1, 3, 5], 3, Some(3)),
        "sentinels stay in place around it"
    );
    assert_eq!(at(3, 2, &[0, 1]), (vec![0, 1, 2], 0, Some(0)));
    assert_eq!(at(3, 0, &[1, 2]), (vec![0, 1, 2], 0, Some(0)));
    assert_eq!(at(2, 0, &[1]), (vec![0, 1], 0, Some(0)));
    assert_eq!(
        at(10, 4, &[8]),
        (vec![4, 8], 7, Some(7)),
        "slot 8 minus the one removal below"
    );
    assert_eq!(at(6, 5, &[0]), (vec![0, 5], 4, Some(4)));
}

#[test]
fn islands_follow_the_slot_and_carry_the_preserve_flag_and_the_new_fill_style() {
    let two = ResultShape::Compound(vec![ext(0.0, 0.0, 5.0, 5.0), ext(9.0, 0.0, 5.0, 5.0)]);
    let c = commit(6, 2, &[1], &two).unwrap();
    let slots: Vec<usize> = c.insert.islands.iter().map(|i| i.index).collect();
    assert_eq!(slots, vec![1, 2]);
    assert!(c.insert.skip_refit);
    assert!(c
        .insert
        .islands
        .iter()
        .all(|i| i.stroke.is_none() && i.opacity == Some(0.5)));
    let none = commit(4, 0, &[1], &ResultShape::Empty).unwrap();
    assert_eq!(none.first_index, None);
    assert!(none.insert.islands.is_empty());
}

#[test]
fn commit_refuses_inconsistent_input_without_partial_output() {
    let unite = UniteDecision {
        united: vec![0],
        failed: vec![],
    };
    let tags = vec![ParticipantTags::default(); 1];
    let input = |absorbed: &[usize], participants: &[ParticipantTags]| -> Result<_, MergeError> {
        plan_merge_commit(&MergeCommitInput {
            layer_len: 3,
            new_index: 2,
            absorbed,
            unite: &unite,
            shape: &square_shape(),
            fill_hex8: "#ff0000",
            opacity: 1.0,
            data: None,
            participants,
        })
    };
    assert_eq!(
        input(&[0], &tags).unwrap_err(),
        MergeError::ParticipantMismatch {
            expected: 2,
            given: 1
        }
    );
    assert_eq!(
        input(
            &[7],
            &[ParticipantTags::default(), ParticipantTags::default()]
        )
        .unwrap_err(),
        MergeError::IndexOutOfRange(7)
    );
    assert_eq!(
        input(
            &[2],
            &[ParticipantTags::default(), ParticipantTags::default()]
        )
        .unwrap_err(),
        MergeError::DuplicateIndex(2)
    );
    assert_eq!(
        input(
            &[0, 0],
            &[
                ParticipantTags::default(),
                ParticipantTags::default(),
                ParticipantTags::default()
            ]
        )
        .unwrap_err(),
        MergeError::DuplicateIndex(0)
    );
    let unpaired = ResultShape::Compound(vec![ext(0.0, 0.0, 5.0, 5.0), hole(50.0, 50.0, 1.0, 1.0)]);
    assert_eq!(
        commit(3, 2, &[0], &unpaired).unwrap_err(),
        MergeError::Topology(TopologyError::UnpairedHoles(vec![1]))
    );
}

#[test]
fn failed_unions_are_reported_as_lost_children() {
    let unite = UniteDecision {
        united: vec![1],
        failed: vec![0],
    };
    let c = plan_merge_commit(&MergeCommitInput {
        layer_len: 3,
        new_index: 2,
        absorbed: &[0, 1],
        unite: &unite,
        shape: &square_shape(),
        fill_hex8: "#ff0000",
        opacity: 1.0,
        data: None,
        participants: &vec![ParticipantTags::default(); 3],
    })
    .unwrap();
    assert_eq!(
        c.removed,
        vec![0, 1, 2],
        "child 0 is removed although its union failed"
    );
    assert_eq!(c.lost, vec![0]);
}

fn tags(
    seeds: Option<Vec<[f64; 2]>>,
    seed: Option<[f64; 2]>,
    walls: Option<Vec<&str>>,
    gap: Option<f64>,
) -> ParticipantTags {
    ParticipantTags {
        fill_seeds: seeds,
        fill_seed: seed,
        fill_walls: walls.map(|w| w.into_iter().map(str::to_string).collect()),
        fill_gap_px: gap,
    }
}

#[test]
fn tags_gather_in_participant_order_with_deduplicated_walls_and_the_largest_gap() {
    let merged = merge_tags(&[
        tags(None, Some([7.0, 8.0]), Some(vec!["w9"]), Some(20.0)),
        tags(
            Some(vec![[1.0, 2.0], [3.0, 4.0]]),
            None,
            Some(vec!["w1", "w2"]),
            Some(10.0),
        ),
        tags(
            None,
            Some([5.0, 6.0]),
            Some(vec!["w2", "w3", "w3"]),
            Some(30.0),
        ),
    ]);
    assert_eq!(
        merged.seeds,
        vec![[7.0, 8.0], [1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]
    );
    assert_eq!(merged.walls, vec!["w9", "w1", "w2", "w3"]);
    assert_eq!(merged.gap_px, 30.0);
    let hidden = merge_tags(&[tags(Some(vec![]), Some([5.0, 6.0]), None, None)]);
    assert!(
        hidden.seeds.is_empty(),
        "an empty fillSeeds array hides fillSeed"
    );
    let ignored = merge_tags(&[
        tags(None, None, None, Some(f64::NAN)),
        tags(None, None, None, Some(-3.0)),
    ]);
    assert_eq!(ignored.gap_px, 0.0);
}

#[test]
fn tags_are_applied_only_when_seeds_exist_and_default_the_gap_to_24() {
    let no_seeds = merge_tags(&[tags(None, None, Some(vec!["w1"]), Some(12.0))]);
    assert_eq!(
        applied_tags(&no_seeds),
        None,
        "walls and gap alone tag nothing"
    );
    let zero_gap = applied_tags(&merge_tags(&[tags(
        None,
        Some([1.0, 1.0]),
        None,
        Some(0.0),
    )]))
    .unwrap();
    assert_eq!((zero_gap.gap_px, zero_gap.walls), (24.0, None));
    let full = applied_tags(&merge_tags(&[tags(
        None,
        Some([1.0, 1.0]),
        Some(vec!["w"]),
        Some(9.0),
    )]))
    .unwrap();
    assert_eq!(
        (full.gap_px, full.walls),
        (9.0, Some(vec!["w".to_string()]))
    );
}
