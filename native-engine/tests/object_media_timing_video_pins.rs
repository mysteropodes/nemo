//! Pinned literal tests for the video source-frame kernel (G28, L09-A).
//!
//! The oracle file covers the baseline cross product; these tests pin the
//! cases a reader should see spelled out, with f64 bit-pattern literals, plus
//! the properties no oracle row can express (the port is only consulted for a
//! video layer with a time remap, and with the timeline frame itself).

use std::cell::RefCell;

use super::tests::Fixed;
use super::{
    blend_weight, interp_mode, js_round, last_frame, select_frame, source_frame,
    source_frame_exact, BlendPlan, FrameSelection, InterpMode, NoTimeRemap, TimeRemapPort,
    VideoLayerTiming, VideoTimingRefusal,
};

/// A port that records every frame it is asked about.
struct Recording(RefCell<Vec<f64>>, f64);

impl TimeRemapPort for Recording {
    fn remap_value(&self, frame: f64) -> Option<f64> {
        self.0.borrow_mut().push(frame);
        Some(self.1)
    }
}

/// A port that must never be consulted.
struct Untouchable;

impl TimeRemapPort for Untouchable {
    fn remap_value(&self, frame: f64) -> Option<f64> {
        panic!("time remap consulted for frame {frame}");
    }
}

fn layer(frame_count: f64, offset_frames: f64) -> VideoLayerTiming<'static> {
    VideoLayerTiming {
        frame_count,
        offset_frames,
        native_video: true,
        time_remap: false,
        interp_mode: None,
        frame_blend: false,
    }
}

fn remapped(frame_count: f64) -> VideoLayerTiming<'static> {
    VideoLayerTiming {
        time_remap: true,
        ..layer(frame_count, 0.0)
    }
}

fn exact(l: &VideoLayerTiming<'_>, frame: f64, p: &dyn TimeRemapPort) -> u64 {
    source_frame_exact(l, frame, p).expect("finite").to_bits()
}

#[test]
fn offset_maps_and_clamps_with_pinned_bits() {
    let l = layer(10.0, 3.0);
    assert_eq!(exact(&l, 5.0, &NoTimeRemap), 0x4000_0000_0000_0000); // 2
    assert_eq!(exact(&l, 3.0, &NoTimeRemap), 0); // +0
    assert_eq!(exact(&l, 1.0, &NoTimeRemap), 0); // -2 clamps to +0
    assert_eq!(exact(&l, 2.5, &NoTimeRemap), 0); // -0.5 clamps to +0
    assert_eq!(exact(&l, 12.0, &NoTimeRemap), 0x4022_0000_0000_0000); // 9
    assert_eq!(exact(&l, 13.0, &NoTimeRemap), 0x4022_0000_0000_0000); // 10 -> last
    assert_eq!(exact(&l, 5.5, &NoTimeRemap), 0x4004_0000_0000_0000); // 2.5
    let neg = layer(10.0, -3.0);
    assert_eq!(exact(&neg, 2.0, &NoTimeRemap), 0x4014_0000_0000_0000); // 5
    assert_eq!(exact(&neg, 20.0, &NoTimeRemap), 0x4022_0000_0000_0000);
}

#[test]
fn last_frame_is_count_minus_one_with_the_one_frame_fallback() {
    assert_eq!(last_frame(10.0).to_bits(), 0x4022_0000_0000_0000);
    assert_eq!(last_frame(10.5).to_bits(), 0x4023_0000_0000_0000);
    assert_eq!(last_frame(1.0).to_bits(), 0);
    assert_eq!(last_frame(0.5).to_bits(), 0); // truthy, 0.5 - 1 clamps up
    assert_eq!(last_frame(-4.0).to_bits(), 0);
    assert_eq!(last_frame(f64::INFINITY), f64::INFINITY);
    for falsy in [f64::NAN, 0.0, -0.0] {
        assert_eq!(
            last_frame(falsy).to_bits(),
            0,
            "frameCount {falsy:?} reads as 1"
        );
        let l = layer(falsy, 0.0);
        assert_eq!(exact(&l, 7.0, &NoTimeRemap), 0, "one frame long, never NaN");
    }
}

#[test]
fn huge_and_infinite_offsets_clamp_instead_of_wrapping() {
    assert_eq!(exact(&layer(10.0, 1e18), 5.0, &NoTimeRemap), 0);
    assert_eq!(
        exact(&layer(10.0, -1e18), 5.0, &NoTimeRemap),
        0x4022_0000_0000_0000
    );
    assert_eq!(exact(&layer(10.0, f64::INFINITY), 5.0, &NoTimeRemap), 0);
    assert_eq!(
        exact(&layer(10.0, f64::NEG_INFINITY), 5.0, &NoTimeRemap),
        0x4022_0000_0000_0000
    );
    assert_eq!(
        exact(&layer(10.0, f64::NAN), 5.0, &NoTimeRemap),
        0x4014_0000_0000_0000
    );
    assert_eq!(
        exact(&layer(1e300, 0.0), 1e9, &NoTimeRemap),
        0x41CD_CD65_0000_0000
    );
    assert_eq!(
        exact(&layer(f64::INFINITY, 0.0), 1e18, &NoTimeRemap),
        1e18f64.to_bits()
    );
}

#[test]
fn non_finite_frame_is_a_typed_refusal_before_any_port_call() {
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let l = remapped(10.0);
        let want = Err(VideoTimingRefusal::NonFiniteFrame);
        assert_eq!(source_frame_exact(&l, frame, &Untouchable), want);
        assert_eq!(source_frame(&l, frame, &Untouchable).map(|s| s.exact), want);
        assert_eq!(
            select_frame(&l, frame, &Untouchable).map(|_| ()),
            want.map(|_: f64| ())
        );
    }
}

#[test]
fn time_remap_value_overrides_the_offset_and_clamps() {
    let l = VideoLayerTiming {
        offset_frames: 3.0,
        ..remapped(10.0)
    };
    let at = |v: f64| exact(&l, 100.0, &Fixed(Some(v)));
    assert_eq!(at(3.5), 0x400C_0000_0000_0000);
    assert_eq!(at(100.0), 0x4022_0000_0000_0000); // clamped to last
    assert_eq!(at(-1.0), 0);
    assert_eq!(at(-0.0), 0); // +0, never -0
    assert_eq!(exact(&l, 5.0, &Fixed(None)), 0x4000_0000_0000_0000); // falls through
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            source_frame_exact(&l, 5.0, &Fixed(Some(bad))),
            Err(VideoTimingRefusal::NonFiniteTimeRemap),
            "remap {bad:?} must be refused, not read as 0 or clamped"
        );
    }
}

#[test]
fn time_remap_port_receives_the_timeline_frame_once() {
    let l = VideoLayerTiming {
        offset_frames: 3.0,
        ..remapped(10.0)
    };
    let p = Recording(RefCell::new(Vec::new()), 1.0);
    source_frame_exact(&l, 42.0, &p).expect("finite");
    assert_eq!(*p.0.borrow(), vec![42.0]);
}

#[test]
fn port_is_only_consulted_with_a_native_video_record_and_a_time_remap() {
    let no_remap = layer(10.0, 3.0);
    assert_eq!(exact(&no_remap, 5.0, &Untouchable), 0x4000_0000_0000_0000);
    let bare = VideoLayerTiming {
        native_video: false,
        time_remap: true,
        ..layer(10.0, 3.0)
    };
    assert_eq!(exact(&bare, 5.0, &Untouchable), 0x4000_0000_0000_0000);
}

#[test]
fn rounding_is_half_up_like_math_round() {
    assert_eq!(js_round(0.5), 1.0);
    assert_eq!(js_round(1.5), 2.0);
    assert_eq!(js_round(2.5), 3.0); // not banker's rounding
    assert_eq!(js_round(0.499_999_999_999_999_94), 0.0); // floor(x + 0.5) would be 1
    assert_eq!(js_round(-1.5).to_bits(), (-1.0f64).to_bits());
    assert_eq!(js_round(-0.5).to_bits(), 0x8000_0000_0000_0000);
    assert_eq!(js_round(4_503_599_627_370_497.0), 4_503_599_627_370_497.0);
    let l = remapped(10.0);
    let r = |v: f64| source_frame(&l, 0.0, &Fixed(Some(v))).expect("finite");
    assert_eq!(r(2.5).rounded.to_bits(), 0x4008_0000_0000_0000);
    assert_eq!(r(2.5).exact.to_bits(), 0x4004_0000_0000_0000);
    assert_eq!(
        r(3.499_999_999_999_999_6).rounded.to_bits(),
        0x4008_0000_0000_0000
    );
    assert_eq!(r(0.5).rounded.to_bits(), 0x3FF0_0000_0000_0000);
}

#[test]
fn blend_weight_is_quantized_to_32nds_half_up() {
    let w = |e: f64| blend_weight(e).expect("finite").to_bits();
    assert_eq!(w(0.0), 0);
    assert_eq!(w(5.0), 0);
    assert_eq!(w(2.015625), 0x3FA0_0000_0000_0000); // tie 0.5/32 rounds up to 1/32
    assert_eq!(w(0.25), 0x3FD0_0000_0000_0000);
    assert_eq!(w(3.7), 0x3FE6_0000_0000_0000); // 22.4/32 -> 22/32
    assert_eq!(w(0.984375), 0x3FF0_0000_0000_0000); // 31.5/32 reaches a full 1
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(blend_weight(bad), Err(VideoTimingRefusal::NonFiniteFrame));
    }
}

#[test]
fn interp_mode_needs_native_video_and_time_remap() {
    let mode = |native_video, time_remap, m: Option<&str>, fb| {
        interp_mode(&VideoLayerTiming {
            native_video,
            time_remap,
            interp_mode: m,
            frame_blend: fb,
            ..layer(10.0, 0.0)
        })
    };
    assert_eq!(mode(true, true, Some("motion"), false), InterpMode::Motion);
    assert_eq!(mode(true, true, Some("blend"), false), InterpMode::Blend);
    assert_eq!(mode(true, true, Some("none"), true), InterpMode::None); // explicit wins
    assert_eq!(mode(true, true, None, true), InterpMode::Blend); // legacy flag
    assert_eq!(mode(true, true, None, false), InterpMode::None);
    assert_eq!(mode(true, true, Some("Blend"), true), InterpMode::Blend); // case-sensitive
    assert_eq!(mode(true, true, Some("Blend"), false), InterpMode::None);
    assert_eq!(mode(true, false, Some("motion"), true), InterpMode::None);
    assert_eq!(mode(false, true, Some("motion"), true), InterpMode::None);
}

#[test]
fn selection_is_a_blend_pair_or_the_rounded_single_frame() {
    let l = VideoLayerTiming {
        frame_blend: true,
        ..remapped(10.0)
    };
    let pick = |v: f64| select_frame(&l, 0.0, &Fixed(Some(v))).expect("finite");
    let blend =
        |from: f64, to: f64, weight: f64| FrameSelection::Blend(BlendPlan { from, to, weight });
    assert_eq!(pick(3.5), blend(3.0, 4.0, 0.5));
    assert_eq!(pick(8.99), blend(8.0, 9.0, 1.0)); // weight reaches 1
    assert_eq!(pick(9.4), FrameSelection::Single { frame: 9.0 }); // clamped to last
    assert_eq!(pick(4.0), FrameSelection::Single { frame: 4.0 }); // weight 0
    assert_eq!(pick(4.01), FrameSelection::Single { frame: 4.0 }); // 0.32/32 rounds to 0
    let one = VideoLayerTiming {
        frame_blend: true,
        ..remapped(1.0)
    };
    let p = Fixed(Some(0.5));
    assert_eq!(
        select_frame(&one, 0.0, &p),
        Ok(FrameSelection::Single { frame: 0.0 })
    );
    let off = VideoLayerTiming {
        time_remap: false,
        interp_mode: Some("blend"),
        ..layer(10.0, 0.0)
    };
    assert_eq!(
        select_frame(&off, 4.5, &NoTimeRemap),
        Ok(FrameSelection::Single { frame: 5.0 }) // mode none: rounded half up
    );
}
