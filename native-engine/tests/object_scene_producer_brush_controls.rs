//! Negative and mutation controls for the bitmap-brush scene producer (L07-J; G17).
//! Same real owners as the behaviour tests: packet, charged store, join registry, stamp plan.

use super::rows::{
    anchor, document, ink_of, join, payloads, publish, rect, run, snapshot, source, store, used,
    window, with_trim, world, PlanBaker, FRAME, RED,
};
use super::{
    BrushProduceError, BrushProduction, BrushStrokeScene, BrushStrokeSource, BrushUnavailable,
    InkSource,
};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::{
    BrushBitmapStore, BrushStoreError,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::SlotError;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::TrimSlots;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinField, PinnedScene, ProducerEntry, ProducerOutput, SceneJoinError, SceneOutcome,
    ScenePin, UnavailableCause,
};
use crate::object_frame_packet::ObjectFramePacket;
use crate::revision::ObjectSnapshot;

const BLUE: [u8; 4] = [0, 0, 255, 255];
const ANCHORS: [(&str, &str); 3] = [("L0", "a"), ("L1", "b"), ("L2", "c")];

fn three_anchor_world() -> (ObjectSnapshot, ObjectFramePacket) {
    let anchors: Vec<_> = ANCHORS
        .iter()
        .map(|&(layer, stroke)| anchor(layer, stroke, Some(RED)))
        .collect();
    world(FRAME, &anchors)
}

fn three_sources() -> Vec<BrushStrokeSource> {
    ANCHORS
        .iter()
        .map(|(layer, stroke)| {
            let group = format!("g-{stroke}");
            let ink = format!("ink-{stroke}");
            source(layer, stroke, &group, true, &ink, rect(0.0, 0.0, 4.0, 4.0))
        })
        .collect()
}

fn published_store() -> BrushBitmapStore {
    let mut store = store(1 << 20);
    for (_, stroke) in ANCHORS {
        publish(&mut store, &format!("ink-{stroke}"), 2, 2, BLUE);
    }
    store
}

fn sequence(scene: &PinnedScene<BrushStrokeScene>) -> Vec<(String, String, u64)> {
    payloads(scene)
        .iter()
        .map(|p| {
            (
                p.layer_uid.clone(),
                p.stroke_id.clone(),
                ink_of(p).bitmap.version(),
            )
        })
        .collect()
}

fn produce_with(pk: &ObjectFramePacket, sources: &[BrushStrokeSource]) -> BrushProduction {
    let mut store = published_store();
    let mut slots = TrimSlots::new();
    let mut baker = PlanBaker::new([255, 0, 0, 255]);
    run(pk, sources, &mut store, &mut slots, &mut baker).unwrap()
}

#[test]
fn source_order_never_decides_the_scene_order() {
    let (_snap, pk) = three_anchor_world();
    let forward = three_sources();
    let mut reversed = three_sources();
    reversed.reverse();
    let mut rotated = three_sources();
    rotated.rotate_left(1);
    let expect = [("L0", "a", 1), ("L1", "b", 2), ("L2", "c", 3)]
        .map(|(l, s, v)| (l.to_owned(), s.to_owned(), v));
    for sources in [&forward, &reversed, &rotated] {
        let scene = join(&pk, produce_with(&pk, sources)).unwrap();
        assert_eq!(sequence(&scene), expect);
    }
}

/// A record-level producer next to the brush producer: a payload type the join is generic over.
#[derive(Debug)]
enum Part {
    Brush(String),
    Plain(String),
}

fn lifted(production: BrushProduction) -> ProducerOutput<Part> {
    let ProducerOutput {
        producer,
        pin,
        entries,
        resources,
    } = production.output;
    let entries = entries
        .into_iter()
        .map(|entry| match entry {
            ProducerEntry::Item {
                source_object_index,
                payload,
            } => ProducerEntry::Item {
                source_object_index,
                payload: Part::Brush(payload.layer_uid),
            },
            ProducerEntry::Unavailable {
                source_object_index,
                reason,
            } => ProducerEntry::Unavailable {
                source_object_index,
                reason,
            },
        })
        .collect();
    ProducerOutput {
        producer,
        pin,
        entries,
        resources,
    }
}

fn rest(pk: &ObjectFramePacket, skip: &[usize]) -> ProducerOutput<Part> {
    let mut output = ProducerOutput::new("rest", ScenePin::of_packet(pk));
    for record in pk.records() {
        if !skip.contains(&record.source_object_index()) {
            let layer = record.record().target().layer_uid().to_owned();
            output = output.with_item(record.source_object_index(), Part::Plain(layer));
        }
    }
    output
}

#[test]
fn a_record_producer_must_leave_the_claimed_brush_records_alone() {
    let (_snap, pk) = three_anchor_world();
    // Only L0 and L2 are brush strokes; L1 stays a plain record.
    let sources = [three_sources().remove(0), three_sources().remove(2)];
    let production = produce_with(&pk, &sources);
    let claimed = production.claimed_indices();
    assert_eq!(claimed.len(), 2);
    let pin = ScenePin::of_packet(&pk);
    let rest_only = rest(&pk, &claimed);
    let scene = join_scene(&pk, &pin, vec![rest_only, lifted(production)]).unwrap();
    let kinds: Vec<String> = scene
        .entries()
        .iter()
        .map(|entry| match entry.outcome() {
            SceneOutcome::Item {
                payload: Part::Brush(layer),
                ..
            } => format!("brush {layer}"),
            SceneOutcome::Item {
                payload: Part::Plain(layer),
                ..
            } => format!("plain {layer}"),
            SceneOutcome::Unavailable(cause) => format!("unavailable {cause:?}"),
        })
        .collect();
    // Interleaved by the packet plan, not by producer: brush L0, plain L1, brush L2.
    assert_eq!(kinds, ["brush L0", "plain L1", "brush L2"]);
    // Mutation: a record producer that claims every record double-claims the brush ones.
    let production = produce_with(&pk, &sources);
    let greedy = rest(&pk, &[]);
    let got = join_scene(&pk, &pin, vec![lifted(production), greedy]);
    assert!(
        matches!(got, Err(SceneJoinError::DuplicateClaim { .. })),
        "{got:?}"
    );
}

#[test]
fn structural_source_errors_refuse_before_any_lease_or_bake() {
    let (_snap, pk) = three_anchor_world();
    let mut store = published_store();
    let mut slots = TrimSlots::new();
    let mut baker = PlanBaker::new([255, 0, 0, 255]);
    let mut duplicate_target = three_sources();
    duplicate_target.push(duplicate_target[0].clone());
    duplicate_target[3].group_id = "other".to_owned();
    let mut duplicate_group = three_sources();
    duplicate_group[1].group_id = duplicate_group[0].group_id.clone();
    let mut empty_group = three_sources();
    empty_group[2].group_id.clear();
    let mut empty_brush = three_sources();
    empty_brush[1].ink = InkSource::Resident(super::InkRef {
        brush_id: String::new(),
        rect: rect(0.0, 0.0, 1.0, 1.0),
    });
    let cases = [
        (
            duplicate_target,
            BrushProduceError::DuplicateTarget {
                layer_uid: "L0".into(),
                stroke_id: "a".into(),
            },
        ),
        (
            duplicate_group,
            BrushProduceError::DuplicateGroup {
                group_id: "g-a".into(),
            },
        ),
        (empty_group, BrushProduceError::InvalidSource { index: 2 }),
        (empty_brush, BrushProduceError::InvalidSource { index: 1 }),
    ];
    for (sources, expected) in cases {
        let got = run(&pk, &sources, &mut store, &mut slots, &mut baker);
        assert_eq!(got.err(), Some(expected));
    }
    // Nothing was leased, baked or charged: every ink has only the store's own alias.
    let one = store.lease("ink-a").unwrap();
    assert_eq!(
        (one.holders(), baker.trims, baker.bakes, slots.len()),
        (2, 0, 0, 0)
    );
    assert_eq!(used(&store), 3 * 16);
}

#[test]
fn a_baker_that_disagrees_with_the_stamp_plan_cannot_publish_and_the_slot_retries() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 2, 2, BLUE);
    let trimmed = || {
        with_trim(
            source("L0", "a", "g", true, "ink-a", rect(0.0, 0.0, 4.0, 4.0)),
            Some(window(0.0, 20.0)),
        )
    };
    let mut slots = TrimSlots::new();
    let used_before = used(&store);
    // Mutation: a canvas one pixel too wide, and a baker that refuses outright.
    for (skew, refuse, reason) in [
        (
            true,
            None,
            "trim-bake-refused: trim-bake-disagrees-with-plan",
        ),
        (false, Some("no canvas"), "trim-bake-refused: no canvas"),
    ] {
        let mut baker = PlanBaker::new([255, 0, 0, 255]);
        baker.skew = skew;
        baker.refuse = refuse;
        let production = run(&pk, &[trimmed()], &mut store, &mut slots, &mut baker).unwrap();
        let scene = join(&pk, production).unwrap();
        match scene.entries()[0].outcome() {
            SceneOutcome::Unavailable(UnavailableCause::Reported {
                producer,
                reason: got,
            }) => {
                assert_eq!((producer.as_str(), got.as_str()), ("brush", reason));
            }
            other => panic!("a disagreeing baker must be unavailable: {other:?}"),
        }
        assert_eq!(
            (
                baker.bakes,
                used(&store),
                slots.key_of("11:brush-scene/10:scene-root/2:L0/1:a")
            ),
            (1, used_before, None)
        );
    }
    // The slot kept no key, so a good baker is asked again and publishes.
    let mut good = PlanBaker::new([255, 0, 0, 255]);
    let scene = join(
        &pk,
        run(&pk, &[trimmed()], &mut store, &mut slots, &mut good).unwrap(),
    )
    .unwrap();
    assert_eq!((good.trims, good.bakes), (1, 1));
    assert_eq!(ink_of(payloads(&scene)[0]).bitmap.dimensions(), (71, 31));
}

#[test]
fn a_re_bake_that_cannot_fit_beside_the_previous_version_is_typed_over_budget() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    // 8868 = ink-a (64) + the 71x31 slot bitmap (8804); the next bake (69x31 = 8556) cannot join it.
    let mut store = store(9000);
    publish(&mut store, "ink-a", 4, 4, BLUE);
    let trimmed = |end| {
        with_trim(
            source("L0", "a", "g", true, "ink-a", rect(0.0, 0.0, 4.0, 4.0)),
            Some(window(0.0, end)),
        )
    };
    let mut slots = TrimSlots::new();
    let mut baker = PlanBaker::new([255, 0, 0, 255]);
    let first = join(
        &pk,
        run(&pk, &[trimmed(20.0)], &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    drop(first);
    assert_eq!(used(&store), 8868);
    let mut again = PlanBaker::new([255, 0, 0, 255]);
    let second = join(
        &pk,
        run(&pk, &[trimmed(19.0)], &mut store, &mut slots, &mut again).unwrap(),
    )
    .unwrap();
    match second.entries()[0].outcome() {
        SceneOutcome::Unavailable(cause) => {
            assert!(
                format!("{cause:?}").contains("trim-over-budget"),
                "{cause:?}"
            );
        }
        SceneOutcome::Item { .. } => panic!("an unaffordable bake must be unavailable"),
    }
    assert_eq!(
        used(&store),
        8868,
        "a refused publication changes no charge"
    );
    assert!(second.declared_resource_versions().is_empty());
    // The previous window is still the slot's key and still hits.
    let mut idle = PlanBaker::new([255, 0, 0, 255]);
    let hit = join(
        &pk,
        run(&pk, &[trimmed(20.0)], &mut store, &mut slots, &mut idle).unwrap(),
    )
    .unwrap();
    assert_eq!(
        (
            idle.trims,
            idle.bakes,
            ink_of(payloads(&hit)[0]).bitmap.version()
        ),
        (0, 0, 2)
    );
}

#[test]
fn a_production_for_another_instance_or_frame_is_refused_by_the_join() {
    let anchors = [anchor("L0", "a", Some(RED))];
    let (_snap, pk) = world(FRAME, &anchors);
    let other_snap = snapshot(&document(FRAME, &anchors), "other-instance");
    let other = super::rows::packet(&other_snap, FRAME);
    let sources = [three_sources().remove(0)];
    let production = produce_with(&other, &sources);
    let got = join_scene(&pk, &ScenePin::of_packet(&pk), vec![production.output]);
    assert!(
        matches!(
            got,
            Err(SceneJoinError::WrongPin {
                producer: Some(_),
                field: PinField::Instance
            })
        ),
        "{got:?}"
    );
    // A frame-8 production never joins a frame-7 packet.
    let (_s8, pk8) = world(8, &anchors);
    let production = produce_with(&pk8, &sources);
    let got = join_scene(&pk, &ScenePin::of_packet(&pk), vec![production.output]);
    assert!(
        matches!(
            got,
            Err(SceneJoinError::WrongPin {
                producer: Some(_),
                ..
            })
        ),
        "{got:?}"
    );
}

#[test]
fn a_scene_lease_pins_its_ink_against_eviction_until_the_scene_is_dropped() {
    let (_snap, pk) = three_anchor_world();
    let mut store = published_store();
    let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new([255, 0, 0, 255]));
    let sources = [three_sources().remove(0)];
    let scene = join(
        &pk,
        run(&pk, &sources, &mut store, &mut slots, &mut baker).unwrap(),
    )
    .unwrap();
    // Soft budget of one byte: every unpinned resident is a candidate of the next end_build.
    store.set_budget_bytes(1.0);
    let mut accept = |_: &[String]| true;
    store.begin_build();
    let dropped = store.end_build(Some(&mut accept));
    assert_eq!(dropped, ["ink-b".to_owned(), "ink-c".to_owned()]);
    assert!(store.contains("ink-a"));
    assert_eq!(
        &ink_of(payloads(&scene)[0]).bitmap.rgba8()[..4],
        BLUE.as_slice()
    );
    drop(scene);
    store.begin_build();
    assert_eq!(store.end_build(Some(&mut accept)), ["ink-a".to_owned()]);
}

/// The reason the registry carries for the only entry of a one-anchor production.
fn reason_of(scene: &PinnedScene<BrushStrokeScene>) -> String {
    match scene.entries()[0].outcome() {
        SceneOutcome::Unavailable(UnavailableCause::Reported { reason, .. }) => reason.clone(),
        other => panic!("expected a reported unavailability: {other:?}"),
    }
}

#[test]
fn a_non_finite_opacity_or_trim_window_is_a_typed_unavailable_that_bakes_nothing() {
    let (_snap, pk) = world(FRAME, &[anchor("L0", "a", Some(RED))]);
    let mut store = store(1 << 20);
    publish(&mut store, "ink-a", 2, 2, BLUE);
    let base = || source("L0", "a", "g", true, "ink-a", rect(0.0, 0.0, 4.0, 4.0));
    let mut bad_ink = base();
    bad_ink.ink_opacity = f64::INFINITY;
    let cases = [
        (bad_ink, "ink-opacity-not-finite"),
        (
            with_trim(base(), Some(window(f64::NAN, 20.0))),
            "trim-window-not-finite",
        ),
        (
            with_trim(base(), Some(window(0.0, f64::INFINITY))),
            "trim-window-not-finite",
        ),
    ];
    for (src, expected) in cases {
        let (mut slots, mut baker) = (TrimSlots::new(), PlanBaker::new([255, 0, 0, 255]));
        let scene = join(
            &pk,
            run(&pk, &[src], &mut store, &mut slots, &mut baker).unwrap(),
        )
        .unwrap();
        assert_eq!(reason_of(&scene), expected);
        assert_eq!((baker.bakes, slots.len()), (0, 0));
    }
}

#[test]
fn a_store_refusal_other_than_capacity_has_its_own_typed_code() {
    let refused = BrushUnavailable::Trim(SlotError::Store(BrushStoreError::InvalidId));
    assert_eq!(refused.code(), "trim-store-refused");
    assert_eq!(refused.reason(), "trim-store-refused");
    let pending = BrushUnavailable::Trim(SlotError::Store(BrushStoreError::PendingConflict));
    assert_eq!(pending.code(), "trim-store-refused");
}
