//! Negative controls of the media-needs join: each mutant is spliced around the
//! REAL join's answer (one plausible mistake per mutant) and the oracle replay must
//! see it. The expected mismatch counts are literals taken from the oracle's class
//! counts (1650 single, 462 blend, 264 refused), so a mutant that the oracle does
//! not catch, or catches in the wrong rows, fails here.
use super::tests::{run_oracle, Candidate, Run};
use super::{needs_for_frame, MediaNeed, NeedsRefusal};

fn replay(candidate: &Candidate<'_>) -> Run {
    run_oracle(candidate)
}

/// Rewrites the real answer of every exported frame.
fn spliced(
    edit: impl Fn(Result<Vec<MediaNeed>, NeedsRefusal>) -> Result<Vec<MediaNeed>, NeedsRefusal>,
) -> Run {
    replay(&|input, frame| edit(needs_for_frame(input, frame)))
}

#[test]
fn unmutated_pass_through_is_clean() {
    let run = spliced(|answer| answer);
    assert_eq!(run.mismatches, 0);
    assert_eq!(run.entries, 2376);
}

#[test]
fn mutation_off_by_one_source_frame_is_caught() {
    // Every single-frame entry is wrong; blend pairs and refusals are untouched.
    let run = spliced(|answer| {
        answer.map(|needs| {
            needs
                .into_iter()
                .map(|need| match need {
                    MediaNeed::Single {
                        placement_id,
                        asset_id,
                        frame,
                    } => MediaNeed::Single {
                        placement_id,
                        asset_id,
                        frame: frame + 1,
                    },
                    other => other,
                })
                .collect()
        })
    });
    assert_eq!(run.mismatches, 1650);
}

#[test]
fn mutation_swapped_blend_pair_is_caught() {
    // A pair is only used when its two frames differ, so every swap changes the need.
    let run = spliced(|answer| {
        answer.map(|needs| {
            needs
                .into_iter()
                .map(|need| match need {
                    MediaNeed::Blend {
                        placement_id,
                        asset_id,
                        from,
                        to,
                        weight,
                    } => MediaNeed::Blend {
                        placement_id,
                        asset_id,
                        from: to,
                        to: from,
                        weight,
                    },
                    other => other,
                })
                .collect()
        })
    });
    assert_eq!(run.mismatches, 462);
}

#[test]
fn mutation_wrong_blend_weight_is_caught() {
    let run = spliced(|answer| {
        answer.map(|needs| {
            needs
                .into_iter()
                .map(|need| match need {
                    MediaNeed::Blend {
                        placement_id,
                        asset_id,
                        from,
                        to,
                        weight,
                    } => MediaNeed::Blend {
                        placement_id,
                        asset_id,
                        from,
                        to,
                        weight: 1.0 - weight,
                    },
                    other => other,
                })
                .collect()
        })
    });
    // 1 - w equals w only at 0.5. The oracle's 462 pairs carry the weights 0.5 (198 pairs),
    // 1/32 (132) and 1 (132), so exactly the 132 + 132 others are caught.
    assert_eq!(run.mismatches, 264);
}

#[test]
fn mutation_swallowed_refusal_is_caught() {
    // A mutant that turns every typed refusal into "nothing to draw" misses exactly the
    // refused entries.
    let run = spliced(|answer| answer.or(Ok(Vec::new())));
    assert_eq!(run.mismatches, 264);
}

#[test]
fn mutation_floor_instead_of_refuse_is_caught() {
    // Flooring a fractional partner would make the refused entries decodable.
    let run = spliced(|answer| match answer {
        Err(NeedsRefusal::InvalidSourceFrame {
            placement_id,
            frame,
        }) => Ok(vec![MediaNeed::Single {
            placement_id,
            asset_id: super::tests::aid("vid"),
            frame: frame as u32,
        }]),
        other => other,
    });
    assert_eq!(run.mismatches, 264);
}

#[test]
fn an_empty_answer_fails_every_decodable_entry() {
    let run = replay(&|_, _| Ok(Vec::new()));
    assert_eq!(run.mismatches, 2376);
}
