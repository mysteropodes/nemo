//! Hand-written literal cases of the media-needs join. Every expected frame,
//! weight and refusal below is a literal derived from the baseline rule
//! `clamp(frame - offset, 0, frameCount - 1)` / the injected remap value, not
//! read back from the code under test.
use super::tests::{
    aid, drawing, embedded, font, input, inventory, linked, place, track, video_facts, Facts,
    NanAt, Remap,
};
use super::{needs_for_frame, preflight, MediaNeed, NeedsRefusal, PreflightFailure};
use crate::object_document::media_timing::object_media_link_join::{
    MissingLink, MissingLinkReason,
};
use crate::object_document::media_timing::object_media_link_plan::{
    plan_frame, AudioFacts, BoundMedia, PlanEntry, Refusal, RefusedBinding, Unavailable,
    UnavailableBinding, VideoFacts,
};
use crate::object_document::media_timing::object_media_link_state::{
    LinkedRef, ResolveOutcome, ResolveTable,
};
use crate::object_document::media_timing::object_media_timing_audio::TrackVolume;
use crate::object_document::media_timing::object_media_timing_video::VideoTimingRefusal;
use crate::object_document::{AssetInventory, AssetKind};

fn video_inventory(offset: i64) -> AssetInventory {
    inventory(
        vec![embedded(AssetKind::Video, "vid")],
        vec![place("pv", "vid", drawing("lv"), offset)],
    )
}

fn single(frame: u32) -> MediaNeed {
    MediaNeed::Single {
        placement_id: "pv".into(),
        asset_id: aid("vid"),
        frame,
    }
}

fn needs(inv: &AssetInventory, facts: &Facts, frame: u32) -> Result<Vec<MediaNeed>, NeedsRefusal> {
    needs_for_frame(&input(inv, &ResolveTable::new(), facts), frame)
}

fn facts_with(frame_count: f64, remap: Option<f64>) -> Facts {
    let mut facts = Facts::default();
    facts
        .video
        .insert("pv".into(), video_facts(frame_count, remap.is_some()));
    if let Some(value) = remap {
        facts
            .remap
            .insert("pv".into(), Box::new(Remap(Some(value))));
    }
    facts
}

#[test]
fn an_exported_frame_uses_the_kernel_source_frame_of_its_timeline_frame() {
    // offset 3, 20 source frames: timeline frame N shows source clamp(N - 3, 0, 19).
    let inv = video_inventory(3);
    let facts = facts_with(20.0, None);
    for (timeline, source) in [(4, 1), (5, 2), (6, 3), (0, 0), (2, 0), (3, 0), (100, 19)] {
        assert_eq!(
            needs(&inv, &facts, timeline),
            Ok(vec![single(source)]),
            "timeline frame {timeline}"
        );
    }
    // A negative offset shifts the other way: frame 2 shows source 2 + 3 = 5.
    assert_eq!(needs(&video_inventory(-3), &facts, 2), Ok(vec![single(5)]));
}

#[test]
fn a_time_remap_value_overrides_the_offset_and_a_null_remap_falls_back_to_it() {
    let inv = video_inventory(3);
    let remapped = facts_with(20.0, Some(7.0));
    for timeline in [4, 5, 40] {
        assert_eq!(needs(&inv, &remapped, timeline), Ok(vec![single(7)]));
    }
    // The remap is present but its sampler answers null: the offset path applies.
    let mut null = Facts::default();
    null.video.insert("pv".into(), video_facts(20.0, true));
    null.remap.insert("pv".into(), Box::new(Remap(None)));
    assert_eq!(needs(&inv, &null, 6), Ok(vec![single(3)]));
}

#[test]
fn a_blend_pair_needs_both_frames_and_a_fractional_partner_is_refused() {
    let inv = video_inventory(0);
    let mut facts = Facts::default();
    facts.video.insert(
        "pv".into(),
        VideoFacts {
            frame_count: 10.0,
            time_remap: true,
            interp_mode: Some("blend".into()),
            frame_blend: false,
        },
    );
    facts.remap.insert("pv".into(), Box::new(Remap(Some(3.5))));
    let pair = needs(&inv, &facts, 1).unwrap();
    assert_eq!(
        pair,
        vec![MediaNeed::Blend {
            placement_id: "pv".into(),
            asset_id: aid("vid"),
            from: 3,
            to: 4,
            weight: 0.5,
        }]
    );
    assert_eq!(pair[0].decode_frames(), vec![3, 4]);
    // frameCount 2.5: last = 1.5, so the clamped exact frame 1.5 pairs 1 with 1.5.
    facts.video.insert(
        "pv".into(),
        VideoFacts {
            frame_count: 2.5,
            time_remap: true,
            interp_mode: Some("blend".into()),
            frame_blend: false,
        },
    );
    assert_eq!(
        needs(&inv, &facts, 1),
        Err(NeedsRefusal::InvalidSourceFrame {
            placement_id: "pv".into(),
            frame: 1.5
        })
    );
}

#[test]
fn decode_frames_lists_only_what_a_decoder_must_supply() {
    let still = MediaNeed::Still {
        placement_id: "pi".into(),
        asset_id: aid("img"),
    };
    assert_eq!(still.decode_frames(), Vec::<u32>::new());
    assert_eq!(single(9).decode_frames(), vec![9]);
    let blend = |from, to| MediaNeed::Blend {
        placement_id: "pv".into(),
        asset_id: aid("vid"),
        from,
        to,
        weight: 0.25,
    };
    assert_eq!(blend(2, 3).decode_frames(), vec![2, 3]);
    assert_eq!(blend(4, 4).decode_frames(), vec![4]);
    assert_eq!(
        (still.placement_id(), still.asset_id()),
        ("pi", &aid("img"))
    );
}

#[test]
fn needs_follow_the_placement_order_and_fonts_and_stills_are_handled() {
    let inv = inventory(
        vec![
            embedded(AssetKind::Image, "img"),
            embedded(AssetKind::Video, "vid"),
            embedded(AssetKind::Font, "fnt"),
        ],
        vec![
            place("pi", "img", drawing("li"), 0),
            place("pf", "fnt", font("lt"), 0),
            place("pv", "vid", drawing("lv"), 3),
        ],
    );
    let facts = facts_with(20.0, None);
    assert_eq!(
        needs(&inv, &facts, 5),
        Ok(vec![
            MediaNeed::Still {
                placement_id: "pi".into(),
                asset_id: aid("img")
            },
            single(2),
        ])
    );
}

fn resolved_table(path: &'static str) -> ResolveTable {
    let reference = LinkedRef {
        linked_path: Some(path),
        linked_handle_id: None,
    };
    let mut table = ResolveTable::new();
    table.begin(&reference, 0);
    table.finish(&mut [], &reference, ResolveOutcome::Resolved, 0);
    table
}

#[test]
fn a_missing_link_outranks_every_other_refusal_and_never_becomes_a_frame() {
    // p2 is an embedded video with no facts (Unavailable); pv is a linked, unresolved video.
    let inv = inventory(
        vec![
            embedded(AssetKind::Video, "v2"),
            linked(AssetKind::Video, "vid", "/media/clip.mp4"),
        ],
        vec![
            place("p2", "v2", drawing("l2"), 0),
            place("pv", "vid", drawing("lv"), 0),
        ],
    );
    let facts = Facts::default();
    let unresolved = ResolveTable::new();
    assert_eq!(
        needs_for_frame(&input(&inv, &unresolved, &facts), 0),
        Err(NeedsRefusal::MissingLink(MissingLink {
            asset_id: aid("vid"),
            placement_id: "pv".into(),
            reason: MissingLinkReason::NotResolved,
        }))
    );
    // Once the link resolves, the earlier placement's missing fact is what refuses.
    let resolved = resolved_table("/media/clip.mp4");
    assert_eq!(
        needs_for_frame(&input(&inv, &resolved, &facts), 0),
        Err(NeedsRefusal::Unavailable(UnavailableBinding {
            placement_id: "p2".into(),
            asset_id: aid("v2"),
            what: Unavailable::VideoFacts,
        }))
    );
}

#[test]
fn a_kernel_refusal_is_typed_for_its_placement_and_never_a_default_frame() {
    let inv = video_inventory(0);
    let mut facts = Facts::default();
    facts.video.insert("pv".into(), video_facts(10.0, true));
    facts.remap.insert("pv".into(), Box::new(NanAt(2.0)));
    assert_eq!(needs(&inv, &facts, 1), Ok(vec![single(1)]));
    assert_eq!(
        needs(&inv, &facts, 2),
        Err(NeedsRefusal::Refused(RefusedBinding {
            placement_id: "pv".into(),
            asset_id: aid("vid"),
            why: Refusal::Video(VideoTimingRefusal::NonFiniteTimeRemap),
        }))
    );
}

#[test]
fn preflight_reports_the_first_frame_that_cannot_be_exported() {
    let inv = video_inventory(0);
    let mut facts = Facts::default();
    facts.video.insert("pv".into(), video_facts(10.0, true));
    facts.remap.insert("pv".into(), Box::new(NanAt(4.0)));
    let table = ResolveTable::new();
    let plan_input = input(&inv, &table, &facts);
    assert_eq!(preflight(&plan_input, 0..=3), Ok(()));
    let failure = preflight(&plan_input, 0..=9).unwrap_err();
    assert_eq!(failure.frame, 4);
    assert!(matches!(
        failure,
        PreflightFailure {
            refusal: NeedsRefusal::Refused(_),
            ..
        }
    ));
}

#[test]
fn preflight_reaches_the_last_representable_frame_without_overflow() {
    // Frames past the clip clamp to its last frame (19), so the top of the u32 range is
    // exportable and the inclusive range ends exactly at u32::MAX.
    let inv = video_inventory(0);
    let facts = facts_with(20.0, None);
    let table = ResolveTable::new();
    let plan_input = input(&inv, &table, &facts);
    assert_eq!(preflight(&plan_input, u32::MAX - 1..=u32::MAX), Ok(()));
    assert_eq!(needs_for_frame(&plan_input, u32::MAX), Ok(vec![single(19)]));
}

#[test]
fn audio_placements_never_enter_a_frame_export() {
    // No audio facts: the plan reports Unavailable for the track; the export ignores it.
    let inv = inventory(
        vec![
            embedded(AssetKind::Video, "vid"),
            embedded(AssetKind::Audio, "aud"),
        ],
        vec![
            place("pv", "vid", drawing("lv"), 0),
            place("pa", "aud", track("tk"), 0),
        ],
    );
    let mut facts = facts_with(20.0, None);
    let table = ResolveTable::new();
    let planned = plan_frame(&input(&inv, &table, &facts), 5.0).unwrap();
    assert!(matches!(&planned[1], PlanEntry::Unavailable(_)));
    assert_eq!(needs(&inv, &facts, 5), Ok(vec![single(5)]));
    // A bound audio track contributes no pixels either.
    facts.audio.insert(
        "pa".into(),
        AudioFacts {
            muted: false,
            buffer_duration: Some(10.0),
            trim_start: 0.0,
            trim_end: None,
            volume: TrackVolume::Flat(1.0),
        },
    );
    let planned = plan_frame(&input(&inv, &table, &facts), 5.0).unwrap();
    assert!(matches!(
        &planned[1],
        PlanEntry::Bound(binding) if matches!(binding.media, BoundMedia::Audio(_))
    ));
    assert_eq!(needs(&inv, &facts, 5), Ok(vec![single(5)]));
    // A linked audio track is a gate refusal in the plan; the export still ignores it.
    let linked_audio = inventory(
        vec![
            embedded(AssetKind::Video, "vid"),
            linked(AssetKind::Audio, "aud", "/media/a.wav"),
        ],
        vec![
            place("pv", "vid", drawing("lv"), 0),
            place("pa", "aud", track("tk"), 0),
        ],
    );
    let planned = plan_frame(&input(&linked_audio, &table, &facts), 5.0).unwrap();
    assert!(matches!(&planned[1], PlanEntry::Refused(_)));
    assert_eq!(needs(&linked_audio, &facts, 5), Ok(vec![single(5)]));
}

#[test]
fn a_linked_image_is_gated_like_a_linked_video() {
    let inv = inventory(
        vec![linked(AssetKind::Image, "img", "/media/a.png")],
        vec![place("pi", "img", drawing("li"), 0)],
    );
    let facts = Facts::default();
    assert_eq!(
        needs(&inv, &facts, 0),
        Err(NeedsRefusal::MissingLink(MissingLink {
            asset_id: aid("img"),
            placement_id: "pi".into(),
            reason: MissingLinkReason::NotResolved,
        }))
    );
    let table = resolved_table("/media/a.png");
    assert_eq!(
        needs_for_frame(&input(&inv, &table, &facts), 0),
        Ok(vec![MediaNeed::Still {
            placement_id: "pi".into(),
            asset_id: aid("img")
        }])
    );
}
