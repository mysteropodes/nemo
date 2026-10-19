//! Hand-written literal pins for the media binding plan: the single offset
//! source, ordering, typed unavailability and refusals, immutability. Every
//! expected number below was worked out by hand from the baseline formulas
//! (`frame - offset` clamped to `[0, frameCount - 1]`, `tSec = (frame - offset) / fps`)
//! and agrees with the generated oracle rows for the same inputs.

use std::sync::Arc;

use super::tests::{
    aid, drawing, embedded, env, font, inventory, place, plan_embedded, track, Facts, Fixed,
    Profile,
};
use super::{
    plan_frame, AudioFacts, Binding, BoundMedia, PlanEntry, PlanError, PlanInput, Refusal,
    RefusedBinding, Unavailable, UnavailableBinding, VideoFacts,
};
use crate::object_document::media_timing::object_media_link_join::{
    LinkGateError, LinkReadiness, LocatorNamespace,
};
use crate::object_document::media_timing::object_media_link_state::ResolveTable;
use crate::object_document::media_timing::object_media_timing_audio::{
    AudioTimingRefusal, SilentReason, StartOutcome, TrackGain, TrackVolume,
};
use crate::object_document::media_timing::object_media_timing_video::{
    BlendPlan, FrameSelection, VideoTimingRefusal,
};
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetRecord, AssetSource,
};

fn video_facts(frame_count: f64) -> VideoFacts {
    VideoFacts {
        frame_count,
        time_remap: false,
        interp_mode: None,
        frame_blend: false,
    }
}

fn audio_facts() -> AudioFacts {
    AudioFacts {
        muted: false,
        buffer_duration: Some(10.0),
        trim_start: f64::NAN,
        trim_end: None,
        volume: TrackVolume::Flat(1.0),
    }
}

fn video_inv(off: i64) -> AssetInventory {
    inventory(
        vec![embedded(AssetKind::Video, "vid")],
        vec![place("pv", "vid", drawing("lv"), off)],
    )
}

fn audio_inv(off: i64) -> AssetInventory {
    inventory(
        vec![embedded(AssetKind::Audio, "aud")],
        vec![place("pa", "aud", track("ta"), off)],
    )
}

fn media_of(entries: &[PlanEntry]) -> BoundMedia {
    match entries {
        [PlanEntry::Bound(b)] => b.media.clone(),
        other => panic!("expected one bound entry, got {other:?}"),
    }
}

fn single(frame: f64) -> BoundMedia {
    BoundMedia::Video(FrameSelection::Single { frame })
}

fn facts_video(fc: f64) -> Facts {
    let mut f = Facts::default();
    f.entry("pv").video = Some(video_facts(fc));
    f
}

fn facts_audio(a: AudioFacts) -> Facts {
    let mut f = Facts::default();
    f.entry("pa").audio = Some(a);
    f
}

#[test]
fn embedded_image_binds_as_a_still_with_no_timing() {
    let inv = inventory(
        vec![embedded(AssetKind::Image, "img")],
        vec![place("pi", "img", drawing("li"), 7)],
    );
    let entries = plan_embedded(&inv, &Facts::default(), 24.0, 3.0);
    assert_eq!(
        entries,
        vec![PlanEntry::Bound(Binding {
            placement_id: "pi".to_string(),
            asset_id: aid("img"),
            link: LinkReadiness::Embedded,
            media: BoundMedia::Still,
        })]
    );
}

#[test]
fn video_offset_comes_from_the_placement_alone() {
    let facts = facts_video(10.0);
    // source frame = clamp(frame - offset, 0, frameCount - 1)
    assert_eq!(
        media_of(&plan_embedded(&video_inv(3), &facts, 24.0, 5.0)),
        single(2.0)
    );
    assert_eq!(
        media_of(&plan_embedded(&video_inv(3), &facts, 24.0, 1.0)),
        single(0.0)
    );
    assert_eq!(
        media_of(&plan_embedded(&video_inv(3), &facts, 24.0, 20.0)),
        single(9.0)
    );
    assert_eq!(
        media_of(&plan_embedded(&video_inv(0), &facts, 24.0, 5.0)),
        single(5.0)
    );
    assert_eq!(
        media_of(&plan_embedded(&video_inv(-3), &facts, 24.0, 5.0)),
        single(8.0)
    );
}

#[test]
fn time_remap_value_overrides_the_placement_offset() {
    let mut facts = facts_video(10.0);
    let e = facts.entry("pv");
    e.video.as_mut().unwrap().time_remap = true;
    e.remap = Some(Fixed(Some(3.5)));
    assert_eq!(
        media_of(&plan_embedded(&video_inv(3), &facts, 24.0, 5.0)),
        single(4.0)
    );
    // the port answers nothing: the offset path applies
    facts.entry("pv").remap = Some(Fixed(None));
    assert_eq!(
        media_of(&plan_embedded(&video_inv(3), &facts, 24.0, 5.0)),
        single(2.0)
    );
}

#[test]
fn a_blend_pair_follows_the_kernel_with_the_remapped_frame() {
    let mut facts = facts_video(10.0);
    let e = facts.entry("pv");
    {
        let v = e.video.as_mut().unwrap();
        v.time_remap = true;
        v.interp_mode = Some("blend".to_string());
    }
    e.remap = Some(Fixed(Some(3.5)));
    let want = BoundMedia::Video(FrameSelection::Blend(BlendPlan {
        from: 3.0,
        to: 4.0,
        weight: 0.5,
    }));
    assert_eq!(
        media_of(&plan_embedded(&video_inv(0), &facts, 24.0, 5.0)),
        want
    );
}

#[test]
fn audio_start_span_is_derived_from_the_placement_offset() {
    let facts = facts_audio(audio_facts());
    // tSec = (48 - 24) / 24 = 1 s, window [0, 10) -> audible now from 1 s for 9 s
    let StartOutcome::Play(s) = audio_of(&plan_embedded(&audio_inv(24), &facts, 24.0, 48.0)) else {
        panic!("expected a playing track");
    };
    assert!(s.starts_now);
    assert_eq!(
        (s.delay_s, s.offset_s, s.duration_s, s.start_frame),
        (0.0, 1.0, 9.0, 48.0)
    );
    assert_eq!(s.gain, TrackGain::Value(1.0));
}

fn audio_of(entries: &[PlanEntry]) -> StartOutcome {
    match media_of(entries) {
        BoundMedia::Audio(o) => o,
        other => panic!("expected an audio binding, got {other:?}"),
    }
}

#[test]
fn audio_before_trim_start_is_a_delayed_start_and_past_the_end_is_silent() {
    let mut a = audio_facts();
    a.trim_start = 1.5;
    let facts = facts_audio(a);
    // tSec = 1 s < trimStart 1.5 s: wait 0.5 s (12 frames), read from 1.5 s for 8.5 s
    let StartOutcome::Play(s) = audio_of(&plan_embedded(&audio_inv(0), &facts, 24.0, 24.0)) else {
        panic!("expected a playing track");
    };
    assert!(!s.starts_now);
    assert_eq!(
        (s.delay_s, s.offset_s, s.duration_s, s.start_frame),
        (0.5, 1.5, 8.5, 36.0)
    );
    let late = audio_of(&plan_embedded(&audio_inv(0), &facts, 24.0, 300.0));
    assert_eq!(late, StartOutcome::Silent(SilentReason::PastTrimEnd));
}

#[test]
fn muted_and_undecoded_audio_are_typed_silence_not_omissions() {
    let mut muted = audio_facts();
    muted.muted = true;
    let entries = plan_embedded(&audio_inv(0), &facts_audio(muted), 24.0, 5.0);
    assert_eq!(
        audio_of(&entries),
        StartOutcome::Silent(SilentReason::Muted)
    );
    let mut raw = audio_facts();
    raw.buffer_duration = None;
    let entries = plan_embedded(&audio_inv(0), &facts_audio(raw), 24.0, 5.0);
    assert_eq!(
        audio_of(&entries),
        StartOutcome::Silent(SilentReason::NoBuffer)
    );
}

#[test]
fn animated_volume_binds_a_curve_through_the_injected_sampler() {
    let mut a = audio_facts();
    a.volume = TrackVolume::Animated;
    a.buffer_duration = Some(3.0);
    let mut facts = facts_audio(a);
    facts.entry("pa").sampler = Some(Profile(3));
    let StartOutcome::Play(s) = audio_of(&plan_embedded(&audio_inv(0), &facts, 24.0, 0.0)) else {
        panic!("expected a playing track");
    };
    // ceil(3 s * 24) + 1 = 73 samples of 0.1, spanning 72 / 24 = 3 s
    let TrackGain::Curve(c) = s.gain else {
        panic!("expected a curve");
    };
    assert_eq!((c.values.len(), c.duration_s), (73, 3.0));
    assert!(c.values.iter().all(|v| *v == 0.1_f32));
}

#[test]
fn entries_follow_the_placement_order_and_font_placements_are_skipped() {
    let records = vec![
        embedded(AssetKind::Audio, "aud"),
        embedded(AssetKind::Font, "fnt"),
        embedded(AssetKind::Video, "vid"),
        embedded(AssetKind::Image, "img"),
    ];
    let mut facts = Facts::default();
    facts.entry("pv").video = Some(video_facts(10.0));
    facts.entry("pa").audio = Some(audio_facts());
    let order = |ps: Vec<AssetPlacement>| -> Vec<String> {
        let inv = inventory(records.clone(), ps);
        plan_embedded(&inv, &facts, 24.0, 5.0)
            .iter()
            .map(|e| match e {
                PlanEntry::Bound(b) => b.placement_id.clone(),
                other => panic!("unexpected entry {other:?}"),
            })
            .collect()
    };
    let make = || {
        vec![
            place("pa", "aud", track("ta"), 0),
            place("pf", "fnt", font("lt"), 0),
            place("pv", "vid", drawing("lv"), 0),
            place("pi", "img", drawing("li"), 0),
        ]
    };
    assert_eq!(order(make()), ["pa", "pv", "pi"]);
    let mut reversed = make();
    reversed.reverse();
    assert_eq!(order(reversed), ["pi", "pv", "pa"]);
}

#[test]
fn missing_providers_are_typed_unavailable_never_a_default() {
    let unavailable = |entries: Vec<PlanEntry>, pid: &str, asset: &str, what: Unavailable| {
        assert_eq!(
            entries,
            vec![PlanEntry::Unavailable(UnavailableBinding {
                placement_id: pid.to_string(),
                asset_id: aid(asset),
                what,
            })]
        );
    };
    let none = Facts::default();
    unavailable(
        plan_embedded(&video_inv(0), &none, 24.0, 5.0),
        "pv",
        "vid",
        Unavailable::VideoFacts,
    );
    unavailable(
        plan_embedded(&audio_inv(0), &none, 24.0, 5.0),
        "pa",
        "aud",
        Unavailable::AudioFacts,
    );
    // a remapped layer without its sampler must not fall back to the offset path
    let mut remapped = facts_video(10.0);
    remapped.entry("pv").video.as_mut().unwrap().time_remap = true;
    let entries = plan_embedded(&video_inv(3), &remapped, 24.0, 5.0);
    unavailable(entries, "pv", "vid", Unavailable::TimeRemapSampler);
    // an animated volume without its sampler
    let mut a = audio_facts();
    a.volume = TrackVolume::Animated;
    let entries = plan_embedded(&audio_inv(0), &facts_audio(a), 24.0, 5.0);
    unavailable(entries, "pa", "aud", Unavailable::VolumeSampler);
    // a flat volume never needs a sampler
    let flat = plan_embedded(&audio_inv(0), &facts_audio(audio_facts()), 24.0, 5.0);
    assert!(matches!(flat.as_slice(), [PlanEntry::Bound(_)]));
}

#[test]
fn kernel_refusals_are_typed_per_placement_and_do_not_stop_the_others() {
    let inv = inventory(
        vec![
            embedded(AssetKind::Video, "vid"),
            embedded(AssetKind::Audio, "aud"),
            embedded(AssetKind::Image, "img"),
        ],
        vec![
            place("pv", "vid", drawing("lv"), 0),
            place("pa", "aud", track("ta"), 0),
            place("pi", "img", drawing("li"), 0),
        ],
    );
    let mut facts = facts_video(10.0);
    let e = facts.entry("pv");
    e.video.as_mut().unwrap().time_remap = true;
    e.remap = Some(Fixed(Some(f64::NAN)));
    facts.entry("pa").audio = Some(audio_facts());
    // fps 0 refuses the audio kernel; the NaN remap value refuses the video kernel
    let entries = plan_embedded(&inv, &facts, 0.0, 5.0);
    let refused = |pid: &str, asset: &str, why: Refusal| {
        PlanEntry::Refused(RefusedBinding {
            placement_id: pid.to_string(),
            asset_id: aid(asset),
            why,
        })
    };
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries[0],
        refused(
            "pv",
            "vid",
            Refusal::Video(VideoTimingRefusal::NonFiniteTimeRemap)
        )
    );
    assert_eq!(
        entries[1],
        refused("pa", "aud", Refusal::Audio(AudioTimingRefusal::InvalidFps))
    );
    assert!(matches!(&entries[2], PlanEntry::Bound(b) if b.media == BoundMedia::Still));
}

#[test]
fn a_non_finite_frame_is_a_plan_error_for_every_value() {
    let table = ResolveTable::new();
    let facts = Facts::default();
    let inv = inventory(vec![], vec![]);
    let input = PlanInput {
        inventory: &inv,
        links: &table,
        namespace: LocatorNamespace::DesktopPath,
        now_ms: 0,
        env: env(24.0),
        facts: &facts,
    };
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(plan_frame(&input, bad), Err(PlanError::NonFiniteFrame));
    }
    assert_eq!(plan_frame(&input, 0.0), Ok(vec![]));
}

#[test]
fn catalog_offset_extremes_are_exact_in_the_kernels() {
    const MAX: i64 = 9_007_199_254_740_991;
    let facts = facts_video(10.0);
    // 5 - (2^53 - 1) clamps to the first frame; 5 + (2^53 - 1) to the last
    assert_eq!(
        media_of(&plan_embedded(&video_inv(MAX), &facts, 24.0, 5.0)),
        single(0.0)
    );
    assert_eq!(
        media_of(&plan_embedded(&video_inv(-MAX), &facts, 24.0, 5.0)),
        single(9.0)
    );
    // an audio track placed 2^53 - 1 frames late is still in the future: a delayed start
    let facts = facts_audio(audio_facts());
    let StartOutcome::Play(s) = audio_of(&plan_embedded(&audio_inv(MAX), &facts, 24.0, 48.0))
    else {
        panic!("expected a delayed start");
    };
    assert!(!s.starts_now && s.delay_s > 3.7e14 && s.offset_s == 0.0 && s.duration_s == 10.0);
}

#[test]
fn the_plan_neither_changes_nor_retains_the_pinned_inventory() {
    let bytes = Arc::new(vec![9_u8, 8, 7]);
    let source =
        AssetSource::admitted_embedded("video/mp4".to_string(), Arc::clone(&bytes), 1 << 16)
            .expect("source");
    let record = Arc::new(
        AssetRecord::new(
            AssetId::new("vid".to_string()).unwrap(),
            "v".to_string(),
            AssetKind::Video,
            source,
        )
        .expect("record"),
    );
    let inv = inventory(vec![record], vec![place("pv", "vid", drawing("lv"), 3)]);
    let before = inv.clone();
    let held = Arc::strong_count(&bytes);
    let facts = facts_video(10.0);
    let first = plan_embedded(&inv, &facts, 24.0, 5.0);
    let second = plan_embedded(&inv, &facts, 24.0, 5.0);
    assert_eq!(first, second, "a plan is a pure function of its inputs");
    assert_eq!(inv, before, "the pinned inventory is untouched");
    assert_eq!(
        Arc::strong_count(&bytes),
        held,
        "no encoded allocation is cloned into a plan"
    );
    assert_eq!(
        inv.placements()[0]
            .offset_frames()
            .as_i64()
            .expect("original integer fixture"),
        3
    );
}

#[test]
fn linked_audio_is_refused_at_the_gate_before_any_facts_are_read() {
    let inv = inventory(
        vec![super::tests::linked(AssetKind::Audio, "aud", "/m/a.wav")],
        vec![place("pa", "aud", track("ta"), 0)],
    );
    let entries = plan_embedded(&inv, &Facts::default(), 24.0, 5.0);
    assert_eq!(
        entries,
        vec![PlanEntry::Refused(RefusedBinding {
            placement_id: "pa".to_string(),
            asset_id: aid("aud"),
            why: Refusal::Link(LinkGateError::UnsupportedKind(AssetKind::Audio)),
        })]
    );
}

#[test]
fn kernel_source_reads_no_clock_or_io() {
    let src = include_str!("../src/object_media_link_plan.rs");
    let code = src.split("#[cfg(test)]").next().expect("code before tests");
    for banned in [
        "Instant",
        "SystemTime",
        "std::time",
        "std::fs",
        "std::io",
        "std::thread",
        "std::env",
        "unwrap(",
        "expect(",
        "panic!",
    ] {
        assert!(!code.contains(banned), "plan source mentions {banned}");
    }
}
