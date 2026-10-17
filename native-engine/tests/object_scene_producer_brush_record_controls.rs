//! Record-authority and port controls for the bitmap-brush scene producer (L07-J; G17).
//! Same real owners as the behaviour tests: packet, charged store, join registry, stamp plan.
//!
//! * the anchor's paint is the record's: a source that remembers a different camouflage than
//!   the record shows is a typed unavailable entry, never a second copy of the paint;
//! * the frame's retire port really reaches the store when a trim re-bake needs room;
//! * the trim slot is resolved before the untrimmed companion is looked at;
//! * the trimmed centerline is the PINNED record's geometry: the provider receives exactly the
//!   record's segments, an edited record under the same window re-bakes, and a record whose
//!   geometry cannot be read is a typed unavailability.

use super::rows::{
    anchor, ink_of, join, payloads, publish, rect, run, source, store, trimmed_line, used, window,
    with_trim, world, Anchor, PlanBaker, FRAME, RED,
};
use super::{
    produce, BrushContext, BrushProduceError, BrushProduction, BrushStrokeScene, BrushStrokeSource,
    BrushUnavailable, InkSource,
};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmapStore;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_anchor::Remembered;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::{
    TrimAnchor, TrimProvider, TrimSlots,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    PinnedScene, SceneOutcome, UnavailableCause,
};
use crate::evaluation::property_follow_path_sample::SegmentIn;
use crate::object_frame_packet::ObjectFramePacket;

const BLUE: [u8; 4] = [0, 0, 255, 255];
const SLOT_ID: &str = "bmbtrim:11:brush-scene/10:scene-root/2:L0/1:a";

/// The reason the registry carries for the only entry of a one-anchor production.
fn reason_of(scene: &PinnedScene<BrushStrokeScene>) -> String {
    match scene.entries()[0].outcome() {
        SceneOutcome::Unavailable(UnavailableCause::Reported { reason, .. }) => reason.clone(),
        other => panic!("expected a reported unavailability: {other:?}"),
    }
}

/// One anchor record plus its source, produced against a store holding `ink-a`.
fn one(record: Anchor, src: BrushStrokeSource) -> PinnedScene<BrushStrokeScene> {
    let (_snap, pk) = world(FRAME, &[record]);
    let mut store = store(1 << 16);
    publish(&mut store, "ink-a", 2, 2, BLUE);
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new([255, 0, 0, 255]));
    join(
        &pk,
        run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap()
}

fn brush(has_fill: bool) -> BrushStrokeSource {
    source("L0", "a", "g", has_fill, "ink-a", rect(0.0, 0.0, 4.0, 4.0))
}

#[test]
fn a_record_the_source_contradicts_is_a_typed_unavailable_entry_and_draws_nothing() {
    let prefix = "anchor-camouflage-disagrees-with-record: ";
    // A filled record that still has a stroke.
    let mut keeps_stroke = anchor("L0", "a", Some(RED));
    keeps_stroke.vector_stroke = Some(RED);
    // A filled record whose stroke the source never remembered.
    let mut forgotten_stroke = brush(true);
    forgotten_stroke.restore.pre_stroke = Remembered::Unset;
    // An unfilled record that is visible.
    let mut visible = anchor("L0", "a", None);
    visible.vector_opacity = 0.5;
    // An unfilled invisible record whose opacity the source never remembered.
    let mut forgotten_opacity = brush(false);
    forgotten_opacity.restore.pre_opacity = Remembered::Unset;
    let cases = [
        (keeps_stroke, brush(true), "filled-anchor-keeps-a-stroke"),
        (
            anchor("L0", "a", Some(RED)),
            forgotten_stroke,
            "filled-anchor-without-remembered-stroke",
        ),
        (visible, brush(false), "unfilled-anchor-is-visible"),
        (
            anchor("L0", "a", None),
            forgotten_opacity,
            "unfilled-anchor-without-remembered-opacity",
        ),
    ];
    for (record, src, why) in cases {
        let scene = one(record, src);
        assert_eq!(reason_of(&scene), format!("{prefix}{why}"));
        assert!(scene.declared_resource_versions().is_empty(), "{why}");
    }
}

#[test]
fn the_drawn_paint_follows_the_record_not_the_source() {
    // A filled anchor whose record opacity is 0.25: the payload draws exactly that.
    let mut faded = anchor("L0", "a", Some(RED));
    faded.vector_opacity = 0.25;
    let scene = one(faded, brush(true));
    let parts = payloads(&scene);
    assert_eq!(parts[0].anchor.opacity, 0.25);
    assert!(parts[0].anchor.stroke.is_none());
    // An unfilled anchor whose record keeps a green stroke at opacity 0 draws that stroke.
    let mut green = anchor("L0", "a", None);
    green.vector_stroke = Some([0.0, 1.0, 0.0, 1.0]);
    let scene = one(green, brush(false));
    let stroke = payloads(&scene)[0]
        .anchor
        .stroke
        .clone()
        .expect("kept stroke");
    let channels: Vec<f64> = stroke
        .channels()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect();
    assert_eq!(channels, [0.0, 1.0, 0.0, 1.0]);
}

/// `run` with the frame's retire port installed.
fn run_with_retire(
    pk: &ObjectFramePacket,
    sources: &[BrushStrokeSource],
    store: &mut BrushBitmapStore,
    slots: &mut TrimSlots,
    provider: &mut dyn TrimProvider,
    retire: &mut dyn FnMut(&[String]) -> bool,
) -> Result<BrushProduction, BrushProduceError> {
    store.begin_build();
    let mut ctx = BrushContext {
        store,
        slots,
        provider,
        retire: Some(retire),
    };
    produce(pk, sources, &mut ctx)
}

#[test]
fn a_re_bake_that_needs_room_asks_the_frame_retire_port_and_obeys_its_answer() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let trimmed = || with_trim(brush(true), Some(window(0.0, 20.0)));
    for accept in [true, false] {
        // 8870 holds ink-a (64) beside the 71x31 slot bitmap (8804) but not beside `spare`
        // (16) as well; `spare` is the oldest unpinned resident, so it is the only victim.
        let mut store = store(8870);
        publish(&mut store, "spare", 2, 2, BLUE);
        publish(&mut store, "ink-a", 4, 4, BLUE);
        let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new([255, 0, 0, 255]));
        let mut seen: Vec<String> = Vec::new();
        let mut port = |ids: &[String]| {
            seen.extend_from_slice(ids);
            accept
        };
        let production = run_with_retire(
            &pk,
            &[trimmed()],
            &mut store,
            &mut slots,
            &mut baker,
            &mut port,
        )
        .unwrap();
        let scene = join(&pk, production).unwrap();
        assert_eq!(seen, ["spare".to_owned()], "accept={accept}");
        if accept {
            let ink = ink_of(payloads(&scene)[0]);
            assert_eq!(
                (ink.bitmap.id(), ink.bitmap.dimensions()),
                (SLOT_ID, (71, 31))
            );
            assert_eq!(
                (store.contains("spare"), store.stats().pressure_evictions),
                (false, 1)
            );
            assert_eq!(used(&store), 64 + 8804);
        } else {
            assert_eq!(reason_of(&scene), "trim-over-budget");
            assert_eq!(
                (store.contains("spare"), store.stats().pressure_evictions),
                (true, 0)
            );
            assert_eq!((used(&store), baker.bakes, slots.len()), (80, 1, 0));
        }
    }
}

#[test]
fn an_unpublishable_companion_does_not_stop_an_active_trim_window() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let mut refused = brush(true);
    refused.ink = InkSource::Refused("missing-asset");
    // `ink-a` is never published in this test: a resident companion would be missing.
    let missing = brush(true);
    for (src, cause) in [
        (refused, "ink-refused: missing-asset"),
        (missing, "ink-missing: 'ink-a'"),
    ] {
        let mut store = store(1 << 20);
        let mut slots = TrimSlots::new();
        // An active window draws the re-baked slot image alone, as the baseline trimmed path
        // never reads the untrimmed raster.
        let mut baker = PlanBaker::new([255, 0, 0, 255]);
        let active = with_trim(src.clone(), Some(window(0.0, 20.0)));
        let scene = join(
            &pk,
            run(&pk, &[active], &mut store, &mut slots, &mut baker).unwrap(),
        )
        .unwrap();
        let ink = ink_of(payloads(&scene)[0]);
        assert_eq!(
            (ink.bitmap.id(), ink.bitmap.dimensions()),
            (SLOT_ID, (71, 31))
        );
        // Without a window the untrimmed companion is needed and its cause is reported.
        let idle = with_trim(src.clone(), None);
        let scene = join(
            &pk,
            run(&pk, &[idle], &mut store, &mut slots, &mut baker).unwrap(),
        )
        .unwrap();
        assert_eq!(reason_of(&scene), cause);
        let plain = join(
            &pk,
            run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap(),
        )
        .unwrap();
        assert_eq!(reason_of(&plain), cause);
    }
}

/// The pinned record's centerline as the provider must receive it.
fn record_anchor(end_x: f64) -> TrimAnchor {
    let at = |x: f64| SegmentIn {
        point: [x, 10.0],
        handle_in: Some([0.0, 0.0]),
        handle_out: Some([0.0, 0.0]),
    };
    TrimAnchor {
        segments: vec![at(10.0), at(end_x)],
        closed: false,
    }
}

#[test]
fn the_provider_receives_exactly_the_records_segments_and_trims_inside_them() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new([255, 0, 0, 255]));
    let src = with_trim(brush(true), Some(window(0.0, 20.0)));
    let production = run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap();
    assert!(production.unmatched.is_empty());
    // The record is the open centerline (10, 10) -> (110, 10): that, and nothing a provider
    // held on its own, is what `applyTrimFor` receives.
    assert_eq!(baker.anchors, [record_anchor(110.0)]);
    let line = trimmed_line(&baker.anchors[0], &window(0.0, 20.0)).unwrap();
    let ends: Vec<[f64; 2]> = line.segments.iter().map(|s| s.point).collect();
    assert_eq!(ends, [[10.0, 10.0], [30.0, 10.0]]);
    // Every trimmed point lies on the record's segment, and the baked world rectangle is the
    // stamp plan of exactly that line.
    assert!(ends
        .iter()
        .all(|p| p[1] == 10.0 && (10.0..=110.0).contains(&p[0])));
    let scene = join(&pk, production).unwrap();
    assert_eq!(
        ink_of(payloads(&scene)[0]).rect,
        rect(2.25, 2.25, 35.5, 15.5)
    );
}

/// One trimmed production; the slot image's `(version, canvas size)`.
fn trimmed_ink(
    pk: &ObjectFramePacket,
    store: &mut BrushBitmapStore,
    slots: &mut TrimSlots,
    baker: &mut PlanBaker,
) -> (u64, (u32, u32)) {
    let src = with_trim(brush(true), Some(window(0.0, 20.0)));
    let scene = join(pk, run(pk, &[src], store, slots, baker).unwrap()).unwrap();
    let ink = ink_of(payloads(&scene)[0]);
    (ink.bitmap.version(), ink.bitmap.dimensions())
}

#[test]
fn the_same_window_over_an_edited_record_re_bakes_and_a_repeat_hits() {
    let mut edited = anchor("L0", "a", Some(RED));
    edited.end_x = 60.0;
    let (_s1, pk1) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let (_s2, pk2) = world(FRAME, &[edited]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let mut slots = TrimSlots::new();
    let mut baker = PlanBaker::new([255, 0, 0, 255]);
    let first = trimmed_ink(&pk1, &mut store, &mut slots, &mut baker);
    assert_eq!((baker.trims, baker.bakes, first.1), (1, 1, (71, 31)));
    // The window key is unchanged, but the record is another packet's geometry: re-bake.
    let second = trimmed_ink(&pk2, &mut store, &mut slots, &mut baker);
    assert_eq!((baker.trims, baker.bakes), (2, 2));
    assert!(
        second.0 > first.0 && second.1 != first.1,
        "{first:?} {second:?}"
    );
    assert_eq!(baker.anchors[1], record_anchor(60.0));
    // A repeat over the edited record is a hit again.
    let mut idle = PlanBaker::new([255, 0, 0, 255]);
    assert_eq!(trimmed_ink(&pk2, &mut store, &mut slots, &mut idle), second);
    assert_eq!((idle.trims, idle.bakes, slots.len()), (0, 0, 1));
}

#[test]
fn a_record_whose_centerline_cannot_be_read_is_a_typed_conversion_error() {
    let (snap, _pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let record = snap.document().objects()[0].clone();
    assert_eq!(TrimAnchor::from_record(&record), Ok(record_anchor(110.0)));
    for (field, reason) in [
        ("kind", "not-a-document-world-relative-cubic-path"),
        (
            "coordinate_space",
            "not-a-document-world-relative-cubic-path",
        ),
        ("handle_space", "not-a-document-world-relative-cubic-path"),
    ] {
        let mut bad = record.clone();
        match field {
            "kind" => bad.geometry.kind = "polyline".to_owned(),
            "coordinate_space" => bad.geometry.coordinate_space = "layer-local".to_owned(),
            _ => bad.geometry.handle_space = "absolute".to_owned(),
        }
        assert_eq!(TrimAnchor::from_record(&bad), Err(reason), "{field}");
    }
    // The registry text of an unreadable centerline (the codec admits only readable ones, so
    // this mapping is reachable through the conversion alone).
    let why = BrushUnavailable::AnchorGeometry("coordinate-not-finite");
    assert_eq!(why.code(), "anchor-geometry-unrepresentable");
    assert_eq!(
        why.reason(),
        "anchor-geometry-unrepresentable: coordinate-not-finite"
    );
}
