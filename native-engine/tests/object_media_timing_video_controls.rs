//! Controls for the video source-frame kernel: each mutation below is one
//! plausible mistake spliced around the REAL kernel (an input changed before
//! the call, or an output changed after it) and the oracle comparison must
//! name rows that catch it. `Mutation::None` is the unmutated pass-through, so
//! a green control proves the comparison can fail, not that it was skipped.

use super::tests::run;
use super::{
    blend_weight, interp_mode, select_frame, source_frame, source_frame_exact, FrameSelection,
    InterpMode, SourceFrame, TimeRemapPort, VideoLayerTiming, VideoTimingRefusal,
};

/// A deliberate mistake the comparison must catch.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mutation {
    None,
    /// `frame + offsetFrames` instead of `frame - offsetFrames`.
    OffsetSignFlip,
    /// The last source frame one too high (`frameCount` instead of `- 1`).
    LastFrameOffByOne,
    /// `Math.floor` where `Math.round` belongs (rounded frame and weight).
    FloorInsteadOfRound,
    /// `max(0, frameCount - 1)` without the `frameCount || 1` fallback.
    MissingCountFallback,
    /// The legacy `frameBlend` boolean is not read.
    FrameBlendIgnored,
}

impl Mutation {
    /// The layer as the mutated kernel would be fed.
    fn layer<'a>(self, l: &VideoLayerTiming<'a>) -> VideoLayerTiming<'a> {
        let mut m = *l;
        match self {
            Mutation::OffsetSignFlip => m.offset_frames = -l.offset_frames,
            Mutation::LastFrameOffByOne => m.frame_count = l.frame_count + 1.0,
            Mutation::FrameBlendIgnored => m.frame_blend = false,
            _ => {}
        }
        m
    }

    /// Without the `|| 1` fallback a `NaN` count makes `last`, and so every
    /// clamp, `NaN` (`Math.max(0, NaN - 1)`); other counts are unaffected.
    fn lost_fallback(self, l: &VideoLayerTiming<'_>) -> bool {
        self == Mutation::MissingCountFallback && l.frame_count.is_nan()
    }

    pub(crate) fn source(
        self,
        l: &VideoLayerTiming<'_>,
        frame: f64,
        port: &dyn TimeRemapPort,
    ) -> Result<SourceFrame, VideoTimingRefusal> {
        let got = source_frame(&self.layer(l), frame, port)?;
        Ok(match self {
            _ if self.lost_fallback(l) => SourceFrame {
                exact: f64::NAN,
                rounded: f64::NAN,
            },
            Mutation::FloorInsteadOfRound => SourceFrame {
                rounded: got.exact.floor(),
                ..got
            },
            _ => got,
        })
    }

    pub(crate) fn weight(self, exact: f64) -> Result<f64, VideoTimingRefusal> {
        let got = blend_weight(exact)?;
        Ok(if self == Mutation::FloorInsteadOfRound {
            ((exact - exact.floor()) * 32.0).floor() / 32.0
        } else {
            got
        })
    }

    pub(crate) fn mode(self, l: &VideoLayerTiming<'_>) -> InterpMode {
        interp_mode(&self.layer(l))
    }

    pub(crate) fn select(
        self,
        l: &VideoLayerTiming<'_>,
        frame: f64,
        port: &dyn TimeRemapPort,
    ) -> Result<FrameSelection, VideoTimingRefusal> {
        let mutated = self.layer(l);
        let got = select_frame(&mutated, frame, port)?;
        Ok(match (self, got) {
            _ if self.lost_fallback(l) => FrameSelection::Single { frame: f64::NAN },
            (Mutation::FloorInsteadOfRound, FrameSelection::Single { .. }) => {
                FrameSelection::Single {
                    frame: source_frame_exact(&mutated, frame, port)?.floor(),
                }
            }
            (_, got) => got,
        })
    }
}

/// The rows a control names must include the op it is meant to break.
fn caught_by(m: Mutation) -> (usize, Vec<String>) {
    let r = run(m);
    (r.bad.len(), r.bad)
}

fn assert_caught(m: Mutation, op: &str) {
    let (n, bad) = caught_by(m);
    assert!(n > 0, "{m:?} slipped through the oracle");
    assert!(
        bad.iter().any(|b| b.contains(&format!(": {op} "))),
        "{m:?} was not caught by an {op} row:\n{}",
        bad.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn unmutated_pass_through_is_clean() {
    assert_eq!(caught_by(Mutation::None).0, 0);
}

#[test]
fn mutation_offset_sign_flip_is_caught() {
    assert_caught(Mutation::OffsetSignFlip, "X");
    assert_caught(Mutation::OffsetSignFlip, "S");
}

#[test]
fn mutation_last_frame_off_by_one_is_caught() {
    assert_caught(Mutation::LastFrameOffByOne, "X");
    assert_caught(Mutation::LastFrameOffByOne, "S");
}

#[test]
fn mutation_floor_instead_of_round_is_caught() {
    assert_caught(Mutation::FloorInsteadOfRound, "X");
    assert_caught(Mutation::FloorInsteadOfRound, "W");
    assert_caught(Mutation::FloorInsteadOfRound, "S");
}

#[test]
fn mutation_missing_frame_count_fallback_is_caught() {
    assert_caught(Mutation::MissingCountFallback, "X");
    assert_caught(Mutation::MissingCountFallback, "S");
}

#[test]
fn mutation_frame_blend_ignored_is_caught() {
    assert_caught(Mutation::FrameBlendIgnored, "M");
    assert_caught(Mutation::FrameBlendIgnored, "S");
}
