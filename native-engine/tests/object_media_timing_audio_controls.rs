//! Controls for the audio timing kernel: each mutation below is one plausible
//! mistake spliced around the REAL kernel (an input changed before the call,
//! or an output changed after it) and the oracle comparison must name rows
//! that catch it. `Mutation::None` is the unmutated pass-through, so a green
//! control proves the comparison can fail, not that it was skipped.

use super::tests::run;
use super::{
    plan_scrub, plan_start, AudioTimingRefusal, AudioTrackTiming, PlaybackEnv, ScrubOutcome,
    SilentReason, StartOutcome, TrackGain, VolumeSampler,
};

/// A deliberate mistake the comparison must catch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// `frame + offsetFrames` instead of `frame - offsetFrames`.
    OffsetSignFlip,
    /// `trimStart` ignored (no clamp of `playFrom` to the trim window).
    TrimStartIgnored,
    /// The volume curve capped at 1999 samples instead of 2000.
    CapOffByOne,
    /// Playback: `tSec > trim.end` (non-strict) instead of `tSec >= trim.end`.
    PlayEndNonStrict,
    /// Scrub: `tSec <= trim.end` accepted (the end sample still blips).
    ScrubEndInclusive,
    /// The delay of a later start has the wrong sign.
    DelaySignFlip,
    /// The 45 ms blip floor dropped (`1 / fps` alone).
    ScrubFloorIgnored,
}

/// The smallest `f64` above a finite `x`.
fn next_up(x: f64) -> f64 {
    if x == 0.0 {
        f64::from_bits(1)
    } else if x > 0.0 {
        f64::from_bits(x.to_bits() + 1)
    } else {
        f64::from_bits(x.to_bits() - 1)
    }
}

impl Mutation {
    fn track(self, t: &AudioTrackTiming) -> AudioTrackTiming {
        let mut m = *t;
        match self {
            Mutation::OffsetSignFlip => m.offset_frames = -t.offset_frames,
            Mutation::TrimStartIgnored => m.trim_start = f64::NAN,
            Mutation::ScrubEndInclusive => {
                let end = t.trim_end.or(t.buffer_duration);
                if let Some(e) = end.filter(|e| e.is_finite()) {
                    m.trim_end = Some(next_up(e));
                }
            }
            _ => {}
        }
        m
    }

    /// `tSec == trim.end` exactly, the only input where strictness shows.
    fn at_end(env: &PlaybackEnv, t: &AudioTrackTiming, frame: f64) -> bool {
        let offset = if t.offset_frames.is_nan() {
            0.0
        } else {
            t.offset_frames
        };
        let end = t.trim_end.or(t.buffer_duration);
        end == Some((frame - offset) / env.fps)
    }

    pub(crate) fn plan_start(
        self,
        env: &PlaybackEnv,
        t: &AudioTrackTiming,
        frame: f64,
        sampler: &dyn VolumeSampler,
    ) -> Result<StartOutcome, AudioTimingRefusal> {
        let got = plan_start(env, &self.track(t), frame, sampler)?;
        Ok(match (self, got) {
            (Mutation::PlayEndNonStrict, StartOutcome::Silent(SilentReason::PastTrimEnd))
                if Self::at_end(env, t, frame) =>
            {
                StartOutcome::Silent(SilentReason::EmptyWindow)
            }
            (Mutation::CapOffByOne, StartOutcome::Play(mut s)) => {
                if let TrackGain::Curve(c) = &mut s.gain {
                    if c.values.len() == 2000 {
                        c.values.truncate(1999);
                        c.duration_s = 1998.0 / env.fps;
                    }
                }
                StartOutcome::Play(s)
            }
            (Mutation::DelaySignFlip, StartOutcome::Play(mut s)) => {
                s.delay_s = -s.delay_s;
                StartOutcome::Play(s)
            }
            (_, got) => got,
        })
    }

    pub(crate) fn plan_scrub(
        self,
        env: &PlaybackEnv,
        t: &AudioTrackTiming,
        frame: f64,
        sampler: &dyn VolumeSampler,
    ) -> Result<ScrubOutcome, AudioTimingRefusal> {
        let got = plan_scrub(env, &self.track(t), frame, sampler)?;
        Ok(match (self, got) {
            (Mutation::ScrubFloorIgnored, ScrubOutcome::Blip(mut b)) => {
                b.duration_s = 1.0 / env.fps;
                ScrubOutcome::Blip(b)
            }
            (_, got) => got,
        })
    }
}

fn caught_by(m: Mutation) -> Vec<String> {
    run(m).bad
}

/// The rows a control names must include the kind it is meant to break.
fn assert_caught(m: Mutation, kind: &str) {
    let bad = caught_by(m);
    assert!(!bad.is_empty(), "{m:?} slipped through the oracle");
    assert!(
        bad.iter().any(|b| b.starts_with(&format!("{kind}: "))),
        "{m:?} was not caught by a {kind} row:\n{}",
        bad.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn unmutated_pass_through_is_clean() {
    assert!(caught_by(Mutation::None).is_empty());
}

#[test]
fn mutation_offset_sign_flip_is_caught() {
    assert_caught(Mutation::OffsetSignFlip, "P");
    assert_caught(Mutation::OffsetSignFlip, "B");
}

#[test]
fn mutation_trim_start_ignored_is_caught() {
    assert_caught(Mutation::TrimStartIgnored, "P");
    assert_caught(Mutation::TrimStartIgnored, "B");
}

#[test]
fn mutation_cap_off_by_one_is_caught() {
    assert_caught(Mutation::CapOffByOne, "P");
}

#[test]
fn mutation_non_strict_end_comparison_is_caught() {
    // Playback: only the silent reason differs at `tSec == trim.end`.
    assert_caught(Mutation::PlayEndNonStrict, "P");
    // Scrub: the end sample would still blip.
    assert_caught(Mutation::ScrubEndInclusive, "B");
}

#[test]
fn mutation_delay_sign_flip_is_caught() {
    assert_caught(Mutation::DelaySignFlip, "P");
}

#[test]
fn mutation_scrub_floor_ignored_is_caught() {
    assert_caught(Mutation::ScrubFloorIgnored, "B");
}
