//! Mutation controls for the audio export disposition (G50, L09-F).
//!
//! Each control splices ONE plausible mistake into the output of the REAL
//! planner and replays the whole baseline oracle. The mistake must be caught on
//! exactly the rows whose result it changes; that set is derived here from the
//! oracle's own result tokens (not from the code under test), so a control that
//! stops biting, or a checker that stops looking, fails. The pass-through
//! control proves the comparison is not vacuous.

use super::tests::{hex, plan, rendered, rows, wanted, Row};
use super::TrackAudio;
use crate::object_document::media_timing::object_media_timing_audio::{SilentReason, TrackGain};

#[derive(Clone, Copy, Debug)]
enum Mutation {
    PassThrough,
    /// A span that ends after its duration alone, ignoring when it starts.
    EndOmitsDelay,
    /// A delayed track reported as audible from the first frame.
    StartZeroed,
    /// The buffer read position lost.
    OffsetZeroed,
    /// The static gain replaced by unity.
    GainReset,
    /// The last envelope sample lost.
    CurveTruncated,
    /// Muted/undecoded and past-end/empty-window reasons exchanged.
    ReasonSwapped,
    /// Excluded tracks omitted from the manifest.
    ExcludedDropped,
    /// Entries returned in reverse input order.
    OrderReversed,
    /// A kernel refusal reported as a silent exclusion.
    RefusalHidden,
}

impl Mutation {
    fn apply(self, tracks: &mut Vec<TrackAudio>) {
        match self {
            Mutation::PassThrough => {}
            Mutation::OrderReversed => tracks.reverse(),
            Mutation::ExcludedDropped => {
                tracks.retain(|entry| !matches!(entry, TrackAudio::Excluded { .. }));
            }
            _ => {
                for entry in tracks.iter_mut() {
                    self.apply_one(entry);
                }
            }
        }
    }

    fn apply_one(self, entry: &mut TrackAudio) {
        if let Mutation::RefusalHidden = self {
            if matches!(entry, TrackAudio::Unresolved { .. }) {
                let track_id = entry.track_id().to_string();
                *entry = TrackAudio::Excluded {
                    track_id,
                    reason: SilentReason::NoAudioContext,
                };
            }
            return;
        }
        match (self, entry) {
            (Mutation::EndOmitsDelay, TrackAudio::Audible(span)) => {
                span.end_s = span.start.duration_s;
            }
            (Mutation::StartZeroed, TrackAudio::Audible(span)) => span.start_s = 0.0,
            (Mutation::OffsetZeroed, TrackAudio::Audible(span)) => span.start.offset_s = 0.0,
            (Mutation::GainReset, TrackAudio::Audible(span)) => {
                if let TrackGain::Value(v) = &mut span.start.gain {
                    *v = 1.0;
                }
            }
            (Mutation::CurveTruncated, TrackAudio::Audible(span)) => {
                if let TrackGain::Curve(curve) = &mut span.start.gain {
                    curve.values.pop();
                }
            }
            (Mutation::ReasonSwapped, TrackAudio::Excluded { reason, .. }) => {
                *reason = match *reason {
                    SilentReason::Muted => SilentReason::NoBuffer,
                    SilentReason::NoBuffer => SilentReason::Muted,
                    SilentReason::PastTrimEnd => SilentReason::EmptyWindow,
                    SilentReason::EmptyWindow => SilentReason::PastTrimEnd,
                    other => other,
                };
            }
            _ => {}
        }
    }
}

/// How many oracle rows the mutated planner output disagrees with.
fn caught_rows(mutation: Mutation) -> usize {
    rows()
        .iter()
        .filter(|row| {
            let manifest = plan(row);
            let mut tracks = manifest.tracks().to_vec();
            mutation.apply(&mut tracks);
            rendered(&tracks) != wanted(row)
        })
        .count()
}

/// Rows holding at least one result token for which `pred(fields)` is true.
fn rows_with(pred: impl Fn(&[&str]) -> bool) -> usize {
    let hit = |row: &Row| {
        row.expect
            .iter()
            .any(|t| pred(&t.split(':').collect::<Vec<_>>()))
    };
    rows().iter().filter(|row| hit(row)).count()
}

fn is_span(f: &[&str]) -> bool {
    f[0] == "Y"
}

#[test]
fn unmutated_pass_through_is_clean() {
    assert_eq!(caught_rows(Mutation::PassThrough), 0);
}

#[test]
fn mutation_end_omitting_the_delay_is_caught() {
    let want = rows_with(|f| is_span(f) && hex(f[1]) != 0.0);
    assert!(want > 0, "the oracle has delayed spans");
    assert_eq!(caught_rows(Mutation::EndOmitsDelay), want);
}

#[test]
fn mutation_zeroed_start_is_caught() {
    let want = rows_with(|f| is_span(f) && hex(f[1]) != 0.0);
    assert_eq!(caught_rows(Mutation::StartZeroed), want);
}

#[test]
fn mutation_zeroed_buffer_offset_is_caught() {
    let want = rows_with(|f| is_span(f) && hex(f[2]) != 0.0);
    assert!(
        want > 0,
        "the oracle has spans that read from inside the buffer"
    );
    assert_eq!(caught_rows(Mutation::OffsetZeroed), want);
}

#[test]
fn mutation_gain_reset_is_caught() {
    let want = rows_with(|f| is_span(f) && f[4] == "v" && hex(f[5]) != 1.0);
    assert!(want > 0, "the oracle has non-unity static gains");
    assert_eq!(caught_rows(Mutation::GainReset), want);
}

#[test]
fn mutation_truncated_envelope_is_caught() {
    let want = rows_with(|f| is_span(f) && f[4] == "c");
    assert!(want > 0, "the oracle has animated envelopes");
    assert_eq!(caught_rows(Mutation::CurveTruncated), want);
}

#[test]
fn mutation_swapped_exclusion_reasons_are_caught() {
    let want = rows_with(|f| {
        f[0] == "N" && ["Muted", "NoBuffer", "PastTrimEnd", "EmptyWindow"].contains(&f[1])
    });
    assert!(want > 0);
    assert_eq!(caught_rows(Mutation::ReasonSwapped), want);
}

#[test]
fn mutation_dropped_exclusions_are_caught() {
    let want = rows_with(|f| f[0] == "N");
    assert!(want > 0);
    assert_eq!(caught_rows(Mutation::ExcludedDropped), want);
}

#[test]
fn mutation_reversed_order_is_caught_on_every_multi_track_row() {
    let want = rows().iter().filter(|row| row.tracks.len() >= 2).count();
    assert!(want > 0);
    assert_eq!(caught_rows(Mutation::OrderReversed), want);
}

#[test]
fn mutation_refusal_reported_as_silence_is_caught() {
    let want = rows_with(|f| f[0] == "R");
    assert!(want > 0);
    assert_eq!(caught_rows(Mutation::RefusalHidden), want);
}
