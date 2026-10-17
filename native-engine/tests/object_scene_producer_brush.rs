//! Behaviour tests for the bitmap-brush scene producer (L07-J; G17).
//!
//! Everything runs on the real owners: a real v2 document, snapshot and `ObjectFramePacket`
//! (`rows`), the real charged `BrushBitmapStore` over an isolated budget, the real
//! `join_scene` registry and the real L07-E stamp plan. Only the trim math and the pixel
//! baker (providers that do not exist natively yet) are test providers, and the baker
//! paints exactly the plan it is given.

use super::rows::{
    anchor, ink_of, join, payloads, publish, rect, run, source, store, used, window, with_trim,
    world, PlanBaker, FRAME, RED,
};
use super::{InkState, PRODUCER};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::TrimSlots;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    SceneOutcome, UnavailableCause,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::object_document::SolidFill;
use std::collections::BTreeMap;

const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

fn resources(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn each_brush_anchor_is_one_claim_with_anchor_ink_and_a_declared_resource() {
    let (snap, pk) = world(
        FRAME,
        &[anchor("L0", "a", Some(RED)), anchor("L1", "b", None)],
    );
    let before = snap.document().clone();
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    publish(&mut store, "ink-b", 2, 2, GREEN);
    let sources = [
        source("L0", "a", "g-a", true, "ink-a", rect(8.0, 8.0, 16.0, 16.0)),
        source("L1", "b", "g-b", false, "ink-b", rect(30.0, 0.0, 8.0, 8.0)),
    ];
    let mut slots = TrimSlots::new();
    let mut baker = PlanBaker::new([255, 0, 0, 255]);
    let production = run(&pk, &sources, &mut store, &mut slots, &mut baker).unwrap();
    assert_eq!(production.unmatched, Vec::<(String, String)>::new());
    assert_eq!(production.claimed_indices().len(), 2);
    let scene = join(&pk, production).unwrap();
    assert!(scene.is_complete());
    assert_eq!(scene.entries().len(), 2);
    let producers: Vec<&str> = scene
        .entries()
        .iter()
        .map(|entry| match entry.outcome() {
            SceneOutcome::Item { producer, .. } => producer.as_str(),
            SceneOutcome::Unavailable(_) => "unavailable",
        })
        .collect();
    assert_eq!(producers, [PRODUCER, PRODUCER]);
    // Back to front by the packet plan: L0 then L1 (document layers are stored reversed).
    let parts = payloads(&scene);
    assert_eq!(
        parts
            .iter()
            .map(|p| (
                p.layer_uid.as_str(),
                p.stroke_id.as_str(),
                p.group_id.as_str()
            ))
            .collect::<Vec<_>>(),
        [("L0", "a", "g-a"), ("L1", "b", "g-b")]
    );
    // Every payload agrees with its immutable source record: target, fill, stroke, opacity.
    let solid = |paint: Option<&SolidFill>| paint.map(|p| SceneSolid::from_fill(p).unwrap());
    for (entry, part) in scene.entries().iter().zip(&parts) {
        let record = &snap.document().objects()[entry.source_object_index()];
        let vector = record.vector().expect("vector body");
        assert_eq!(record.target().layer_uid(), part.layer_uid);
        assert_eq!(record.target().stroke_id(), part.stroke_id);
        assert_eq!(part.anchor.fill, solid(record.solid_fill()));
        assert_eq!(part.anchor.stroke, solid(vector.solid_stroke()));
        assert_eq!(part.anchor.opacity, vector.opacity.as_f64().unwrap());
    }
    // Anchor a keeps its fill and has no stroke; anchor b has no fill, keeps its stroke and is
    // invisible (opacity 0): all of it read from the record.
    let fill = parts[0].anchor.fill.as_ref().expect("fill kept");
    let channels: Vec<f64> = fill
        .channels()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect();
    assert_eq!(channels, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        (parts[0].anchor.stroke.as_ref(), parts[0].anchor.opacity),
        (None, 1.0)
    );
    assert!(parts[1].anchor.fill.is_none() && parts[1].anchor.stroke.is_some());
    assert_eq!(parts[1].anchor.opacity, 0.0);
    // ONE companion per anchor: the leased bitmap, its rectangle and opacity.
    let (ink_a, ink_b) = (ink_of(parts[0]), ink_of(parts[1]));
    assert_eq!(
        (
            ink_a.bitmap.id(),
            ink_a.bitmap.dimensions(),
            ink_a.bitmap.version()
        ),
        ("ink-a", (4, 4), 1)
    );
    assert_eq!(
        (
            ink_b.bitmap.id(),
            ink_b.bitmap.dimensions(),
            ink_b.bitmap.version()
        ),
        ("ink-b", (2, 2), 2)
    );
    assert_eq!(&ink_a.bitmap.rgba8()[..4], BLUE.as_slice());
    assert_eq!(
        (ink_a.rect, ink_a.opacity),
        (rect(8.0, 8.0, 16.0, 16.0), 1.0)
    );
    // Leased for the scene: the store's alias plus the payload's, and declared for the key.
    assert_eq!(ink_a.bitmap.holders(), 2);
    assert_eq!(
        scene.declared_resource_versions(),
        &resources(&[("bitmap-brush:ink-a", "1"), ("bitmap-brush:ink-b", "2")])
    );
    assert_eq!(
        snap.document(),
        &before,
        "the producer never edits the document"
    );
}

#[test]
fn unavailable_ink_and_unmatched_sources_are_typed_and_never_dropped() {
    let mut hidden = anchor("L4", "e", Some(RED));
    hidden.visible = false;
    let (_snap, pk) = world(
        FRAME,
        &[
            anchor("L0", "a", Some(RED)),
            anchor("L1", "b", Some(RED)),
            anchor("L2", "c", Some(RED)),
            anchor("L3", "d", Some(RED)),
            hidden,
            anchor("L5", "f", Some(RED)),
        ],
    );
    let mut store = store(1 << 20);
    publish(&mut store, "ink-d", 2, 2, BLUE);
    let mut refused = source("L1", "b", "g-b", true, "ink-b", rect(0.0, 0.0, 1.0, 1.0));
    refused.ink = super::InkSource::Refused("missing-asset");
    let mut nan_opacity = source("L2", "c", "g-c", true, "ink-d", rect(0.0, 0.0, 1.0, 1.0));
    nan_opacity.ink_opacity = f64::NAN;
    let sources = [
        // The bitmap was never published (its asset was missing or over budget).
        source("L0", "a", "g-a", true, "ink-a", rect(0.0, 0.0, 1.0, 1.0)),
        refused,
        nan_opacity,
        source("L3", "d", "g-d", true, "ink-d", rect(0.0, 0.0, 1.0, 1.0)),
        // Names a record on an invisible layer: not selected, reported, not lost.
        source("L4", "e", "g-e", true, "ink-d", rect(0.0, 0.0, 1.0, 1.0)),
    ];
    let mut slots = TrimSlots::new();
    let mut baker = PlanBaker::new([255, 0, 0, 255]);
    let production = run(&pk, &sources, &mut store, &mut slots, &mut baker).unwrap();
    assert_eq!(production.unmatched, [("L4".to_owned(), "e".to_owned())]);
    let scene = join(&pk, production).unwrap();
    let reasons: Vec<String> = scene
        .entries()
        .iter()
        .map(|entry| match entry.outcome() {
            SceneOutcome::Item { .. } => "item".to_owned(),
            SceneOutcome::Unavailable(UnavailableCause::Reported { producer, reason }) => {
                format!("{producer}/{reason}")
            }
            SceneOutcome::Unavailable(UnavailableCause::Unclaimed) => "unclaimed".to_owned(),
        })
        .collect();
    assert_eq!(
        reasons,
        [
            "brush/ink-missing: 'ink-a'",
            "brush/ink-refused: missing-asset",
            "brush/ink-opacity-not-finite",
            "item",
            "unclaimed",
        ]
    );
    assert!(!scene.is_complete());
    assert_eq!(scene.unavailable().count(), 4);
    // Only the drawable ink declared a resource.
    assert_eq!(
        scene.declared_resource_versions(),
        &resources(&[("bitmap-brush:ink-d", "1")])
    );
}

#[test]
fn regenerate_publishes_a_new_version_while_the_earlier_scene_keeps_its_lease() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let mut store = store(1 << 16);
    publish(&mut store, "g1", 4, 4, BLUE);
    let sources = [source(
        "L0",
        "a",
        "g1",
        true,
        "g1",
        rect(0.0, 0.0, 8.0, 8.0),
    )];
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new([255, 0, 0, 255]));
    let first = join(
        &pk,
        run(&pk, &sources, &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    assert_eq!(used(&store), 64);
    // Regenerate: the same group re-stamped under the same id.
    publish(&mut store, "g1", 4, 4, GREEN);
    let second = join(
        &pk,
        run(&pk, &sources, &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    let (old, new) = (ink_of(payloads(&first)[0]), ink_of(payloads(&second)[0]));
    assert_eq!((old.bitmap.version(), new.bitmap.version()), (1, 2));
    assert_eq!(&old.bitmap.rgba8()[..4], BLUE.as_slice());
    assert_eq!(&new.bitmap.rgba8()[..4], GREEN.as_slice());
    assert!(!store.is_current(&old.bitmap) && store.is_current(&new.bitmap));
    // The scheduler key sees the new version; the older scene still carries the old one.
    assert_eq!(
        first.declared_resource_versions(),
        &resources(&[("bitmap-brush:g1", "1")])
    );
    assert_eq!(
        second.declared_resource_versions(),
        &resources(&[("bitmap-brush:g1", "2")])
    );
    // Both versions stay charged while a scene holds them, and the old one frees with it.
    assert_eq!(used(&store), 128);
    drop(first);
    assert_eq!(used(&store), 64);
    drop(second);
}

#[test]
fn tween_frames_share_one_group_identity_and_pin_their_own_frame_and_resource() {
    let (_s7, pk7) = world(7, &[anchor("L0", "a", Some(RED))]);
    let (_s8, pk8) = world(8, &[anchor("L0", "a", Some(RED))]);
    let mut store = store(1 << 16);
    publish(&mut store, "tw@7", 2, 2, BLUE);
    publish(&mut store, "tw@8", 2, 2, GREEN);
    let at = |id: &str| [source("L0", "a", "tw", true, id, rect(0.0, 0.0, 4.0, 4.0))];
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new([255, 0, 0, 255]));
    let p7 = run(&pk7, &at("tw@7"), &mut store, &mut slots, &mut baker).unwrap();
    let p8 = run(&pk8, &at("tw@8"), &mut store, &mut slots, &mut baker).unwrap();
    assert_eq!((p7.output.pin.frame, p8.output.pin.frame), (7, 8));
    let s7 = join(&pk7, p7).unwrap();
    let s8 = join(&pk8, p8).unwrap();
    let (a, b) = (payloads(&s7)[0], payloads(&s8)[0]);
    assert_eq!((a.group_id.as_str(), b.group_id.as_str()), ("tw", "tw"));
    assert_eq!(
        s7.declared_resource_versions(),
        &resources(&[("bitmap-brush:tw@7", "1")])
    );
    assert_eq!(
        s8.declared_resource_versions(),
        &resources(&[("bitmap-brush:tw@8", "2")])
    );
    assert_eq!(&ink_of(a).bitmap.rgba8()[..4], BLUE.as_slice());
    assert_eq!(&ink_of(b).bitmap.rgba8()[..4], GREEN.as_slice());
}

#[test]
fn a_trim_window_re_bakes_one_slot_per_anchor_checked_against_the_stamp_plan() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let trimmed = |w| {
        with_trim(
            source("L0", "a", "g", true, "ink-a", rect(8.0, 8.0, 16.0, 16.0)),
            w,
        )
    };
    let mut slots = TrimSlots::new();
    let slot_resource = "bitmap-brush:bmbtrim:11:brush-scene/10:scene-root/2:L0/1:a";

    // Window 0..20 bakes the line (10,10)-(30,10): pad 7.75, 35.5 x 15.5 world, SCALE 2.
    let mut baker = PlanBaker::new([255, 0, 0, 255]);
    let s1 = join(
        &pk,
        run(
            &pk,
            &[trimmed(Some(window(0.0, 20.0)))],
            &mut store,
            &mut slots,
            &mut baker,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!((baker.trims, baker.bakes), (1, 1));
    let ink1 = ink_of(payloads(&s1)[0]);
    assert_eq!(
        (
            ink1.bitmap.id(),
            ink1.bitmap.dimensions(),
            ink1.bitmap.version()
        ),
        ("bmbtrim:11:brush-scene/10:scene-root/2:L0/1:a", (71, 31), 2)
    );
    assert_eq!(ink1.rect, rect(2.25, 2.25, 35.5, 15.5));
    assert_eq!(
        s1.declared_resource_versions(),
        &resources(&[(slot_resource, "2")])
    );

    // The same window again reuses the slot: no trim, no bake, same version.
    let mut idle = PlanBaker::new([255, 0, 0, 255]);
    let s2 = join(
        &pk,
        run(
            &pk,
            &[trimmed(Some(window(0.0, 20.0)))],
            &mut store,
            &mut slots,
            &mut idle,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (
            idle.trims,
            idle.bakes,
            ink_of(payloads(&s2)[0]).bitmap.version()
        ),
        (0, 0, 2)
    );

    // A new window replaces it: published first, the old version released with its scene.
    let mut next = PlanBaker::new([0, 255, 0, 255]);
    let before = used(&store);
    let s3 = join(
        &pk,
        run(
            &pk,
            &[trimmed(Some(window(0.0, 30.0)))],
            &mut store,
            &mut slots,
            &mut next,
        )
        .unwrap(),
    )
    .unwrap();
    let ink3 = ink_of(payloads(&s3)[0]);
    assert_eq!(
        (ink3.bitmap.version(), ink3.bitmap.dimensions()),
        (3, (91, 31))
    );
    assert_eq!(
        used(&store),
        before + 91 * 31 * 4,
        "the held old version stays charged"
    );
    assert!(!store.is_current(&ink1.bitmap) && store.is_current(&ink3.bitmap));
    assert_eq!(slots.len(), 1, "one slot per anchor");
    drop((s1, s2));
    assert_eq!(
        used(&store),
        64 + 91 * 31 * 4,
        "the old version frees with its last holder"
    );

    // A collapsed window draws the anchor and no ink, and declares no slot resource.
    let mut idle = PlanBaker::new([255, 0, 0, 255]);
    let s4 = join(
        &pk,
        run(
            &pk,
            &[trimmed(Some(window(5.0, 5.0)))],
            &mut store,
            &mut slots,
            &mut idle,
        )
        .unwrap(),
    )
    .unwrap();
    let collapsed = payloads(&s4)[0];
    assert!(matches!(collapsed.ink, InkState::Collapsed));
    assert!(collapsed.anchor.fill.is_some());
    assert!(s4.declared_resource_versions().is_empty());

    // No window leaves the slot alone and draws the untrimmed ink again.
    let s5 = join(
        &pk,
        run(&pk, &[trimmed(None)], &mut store, &mut slots, &mut idle).unwrap(),
    )
    .unwrap();
    assert_eq!(
        s5.declared_resource_versions(),
        &resources(&[("bitmap-brush:ink-a", "1")])
    );
    assert_eq!(ink_of(payloads(&s5)[0]).rect, rect(8.0, 8.0, 16.0, 16.0));
    assert_eq!(
        idle.trims, 1,
        "only the collapsed window asked the trim math"
    );
}
