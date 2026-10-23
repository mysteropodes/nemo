//! Oracle tests of the component evaluation join (L04-K; I15, D33): the baseline results of the
//! verbatim `getEffectiveStrokes` against the output of `evaluate_component_frame`, reached through a
//! real `ObjectSnapshot` whose document carries the component table (L04-I), the admission of the
//! context (L04-J), the frame packet and the component producer join (L07-K). Every expected value is
//! a baseline row or a literal typed from the baseline arithmetic, never read back from the code under
//! test. Row format and provenance: `support`.

use super::support::{
    channels, diff, disagreements, entry_of, hex_channels, inside_triples, set, Prepared, ORACLE,
    SCENE,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentOutcome;
use std::collections::BTreeMap;

fn inner_of(prepared: &Prepared, uid: &str, frame: u32) -> Option<f64> {
    match &entry_of(&prepared.scene(frame), uid).outcome {
        ComponentOutcome::Shapes(placed) => Some(placed.inner_frame.raw()),
        ComponentOutcome::Blank => None,
        other => panic!("{uid} @{frame}: {other:?}"),
    }
}

fn ids_of(prepared: &Prepared, uid: &str, frame: u32) -> Vec<String> {
    match &entry_of(&prepared.scene(frame), uid).outcome {
        ComponentOutcome::Shapes(placed) => placed
            .shapes
            .iter()
            .map(|shape| shape.target.stroke_id().to_owned())
            .collect(),
        other => panic!("{uid} @{frame}: {other:?}"),
    }
}

/// Row counts per kind for one set of the fixture.
fn counts(name: &str) -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    let mut inside = false;
    for line in ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols[0] == "S" {
            inside = cols[1] == name;
        } else if inside {
            *found.entry(cols[0].to_owned()).or_insert(0) += 1;
        }
    }
    found
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.starts_with(
        "# L04-K object_component_evaluate oracle: component frame through the document path. Baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"
    ));
    // (set, X rows, I rows): outer frames x placing layers, entered-component frames.
    for (name, x, i) in [
        ("base", 168, 0),
        ("playback", 120, 0),
        ("placed", 72, 0),
        ("camera", 48, 0),
        ("variants", 96, 48),
        ("overrides", 36, 0),
        ("remap", 48, 0),
    ] {
        let found = counts(name);
        assert_eq!(found.get("X").copied(), Some(x), "{name} X rows");
        assert_eq!(found.get("I").copied().unwrap_or(0), i, "{name} I rows");
        assert_eq!(found.get("G").copied(), Some(1), "{name} document");
    }
    assert_eq!(counts("overrides").get("V").copied(), Some(6));
    assert_eq!(counts("remap").get("R").copied(), Some(48));
    assert_eq!(counts("camera").get("P").copied(), Some(1));
    assert_eq!(counts("camera").get("K").copied(), Some(1));
}

#[test]
fn the_base_fixture_is_reproduced_for_every_layer_and_frame() {
    let prepared = Prepared::of("base");
    assert_eq!(prepared.set.expects.len(), 168);
    assert_eq!(disagreements(&prepared), Vec::<String>::new());
}

#[test]
fn loop_once_pingpong_and_single_with_speed_and_placement_follow_the_baseline() {
    let prepared = Prepared::of("playback");
    for uid in ["pb_loop", "pb_once", "pb_ping", "pb_single"] {
        let rows = prepared
            .set
            .expects
            .iter()
            .filter(|row| row.uid == uid)
            .count();
        assert_eq!(rows, 24, "{uid}");
    }
    assert_eq!(disagreements(&prepared), Vec::<String>::new());
    // Literals typed from the baseline arithmetic of resolveSymbolFrameIdx at three outer frames
    // (component of 12 frames): loop 1.5x from 3 -> floor(1.5 * (f - 3)) % 12; once 0.5x from 2,
    // trimIn 1, trimOut 8 -> min(8, 1 + floor(0.5 * (f - 2))); pingpong 2x from 5 over a 22-frame
    // cycle; single holds symSingleFrame 7 whatever the speed and placement.
    for (uid, at6, at13, at20) in [
        ("pb_loop", 4.0, 3.0, 1.0),
        ("pb_once", 3.0, 6.0, 8.0),
        ("pb_ping", 2.0, 6.0, 8.0),
        ("pb_single", 7.0, 7.0, 7.0),
    ] {
        assert_eq!(inner_of(&prepared, uid, 6), Some(at6), "{uid} @6");
        assert_eq!(inner_of(&prepared, uid, 13), Some(at13), "{uid} @13");
        assert_eq!(inner_of(&prepared, uid, 20), Some(at20), "{uid} @20");
    }
}

#[test]
fn a_held_component_frame_a_blank_key_and_the_next_plain_key_end_to_end() {
    let prepared = Prepared::of("playback");
    // pb_keys: loop at speed 1 placed at 2; keys at 6 (componentFrame 4), 10 (blankOverride),
    // 14 (a plain key: decides nothing, playback resumes by mode) and 18 (componentFrame 9).
    let expect: [(std::ops::RangeInclusive<u32>, Option<f64>); 5] = [
        (6..=9, Some(4.0)),
        (10..=13, None),
        (18..=23, Some(9.0)),
        (0..=2, Some(0.0)),
        (14..=14, Some(0.0)),
    ];
    for (frames, inner) in expect {
        for frame in frames {
            assert_eq!(inner_of(&prepared, "pb_keys", frame), inner, "@{frame}");
        }
    }
    for (frame, inner) in [
        (3, 1.0),
        (4, 2.0),
        (5, 3.0),
        (15, 1.0),
        (16, 2.0),
        (17, 3.0),
    ] {
        assert_eq!(
            inner_of(&prepared, "pb_keys", frame),
            Some(inner),
            "@{frame}"
        );
    }
    // A held blank is a reported Blank outcome with its slot, not a missing producer.
    assert_eq!(
        entry_of(&prepared.scene(11), "pb_keys").outcome,
        ComponentOutcome::Blank
    );
    assert_eq!(
        ids_of(&prepared, "pb_keys", 7),
        ["s_comp_a_4".to_owned(), "s_comp_b_0".to_owned()]
    );
    assert_eq!(disagreements(&prepared), Vec::<String>::new());
}

#[test]
fn symmatrix_handles_and_the_component_camera_come_through_the_document() {
    for name in ["placed", "camera"] {
        let prepared = Prepared::of(name);
        assert!(!prepared.set.expects.is_empty());
        assert_eq!(disagreements(&prepared), Vec::<String>::new(), "{name}");
    }
    let camera = Prepared::of("camera");
    assert_eq!(camera.set.canvas, Some((640.0, 360.0)));
    // The camera sampled at the inner frame moves the geometry: the same instance without camera
    // keys is not what the baseline returned.
    let mut without = Prepared::of("camera");
    without.set.cameras.clear();
    assert!(!disagreements(&without).is_empty());
}

#[test]
fn exposed_overrides_on_two_instances_of_one_component_stay_per_instance() {
    let prepared = Prepared::of("overrides");
    assert_eq!(disagreements(&prepared), Vec::<String>::new());
    let scene = prepared.scene(0);
    let shapes = |uid: &str| match &entry_of(&scene, uid).outcome {
        ComponentOutcome::Shapes(placed) => placed.shapes.clone(),
        other => panic!("{uid}: {other:?}"),
    };
    let (looped, ping, speed) = (
        shapes("ly_comp_loop"),
        shapes("ly_comp_ping"),
        shapes("ly_comp_speed"),
    );
    // Visibility 100 / 10 / 50: only the second is below 50 and loses stroke B.
    assert_eq!((looped.len(), ping.len(), speed.len()), (2, 1, 2));
    assert_eq!(channels(&looped[0].paint), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(channels(&ping[0].paint), [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(
        channels(&speed[0].paint),
        [17.0 / 255.0, 34.0 / 255.0, 51.0 / 255.0, 1.0]
    );
    // The stroke no property addresses keeps the stored fill (#5856d6) on every instance.
    for other in [&looped[1], &speed[1]] {
        assert_eq!(channels(&other.paint), hex_channels("#5856d6"));
    }
    // One availability ask per instance; the reads follow the records of each instance (stroke A
    // then stroke B), one value per addressed stroke.
    let mut ports = prepared.ports();
    let mut runtime = prepared.runtime();
    prepared
        .query_with(SCENE, 0, &mut ports, &mut runtime)
        .expect("evaluates");
    assert_eq!(runtime.availability_asks, 3);
    let reads: Vec<(&str, &str)> = runtime
        .reads
        .iter()
        .map(|(uid, key, _)| (uid.as_str(), key.as_str()))
        .collect();
    assert_eq!(
        reads,
        [
            ("ly_comp_loop", "ep_fill"),
            ("ly_comp_loop", "ep_vis"),
            ("ly_comp_ping", "ep_fill"),
            ("ly_comp_ping", "ep_vis"),
            ("ly_comp_speed", "ep_fill"),
            ("ly_comp_speed", "ep_vis"),
        ]
    );
}

#[test]
fn time_remap_names_the_frame_and_a_decided_layer_never_asks_the_sampler() {
    let prepared = Prepared::of("remap");
    assert_eq!(disagreements(&prepared), Vec::<String>::new());
    // Sampler rows: frame * 0.75 - 2 below frame 20, null from 20. Typed from that arithmetic:
    // -2 and -0.5 clamp to 0, 2.5 rounds up to 3, 4 stays, 12.25 clamps to 11; null plays by
    // mode (loop placed at 4: (f - 4) % 12).
    for (frame, inner) in [
        (0, 0.0),
        (2, 0.0),
        (6, 3.0),
        (8, 4.0),
        (19, 11.0),
        (20, 4.0),
        (21, 5.0),
    ] {
        assert_eq!(
            inner_of(&prepared, "ly_comp_loop", frame),
            Some(inner),
            "@{frame}"
        );
    }
    // The held componentFrame 3 of ly_comp_hold decides before Time Remap is consulted.
    for frame in [0, 6, 22] {
        assert_eq!(
            inner_of(&prepared, "ly_comp_hold", frame),
            Some(3.0),
            "@{frame}"
        );
    }
    let mut ports = prepared.ports();
    let mut runtime = prepared.runtime();
    prepared
        .query_with(SCENE, 9, &mut ports, &mut runtime)
        .expect("evaluates");
    assert_eq!(ports.remap_asks, [("ly_comp_loop".to_owned(), 9)]);
}

#[test]
fn an_entered_component_shows_the_baseline_plain_layer_frames() {
    let prepared = Prepared::of("variants");
    assert_eq!(prepared.set.inside.len(), 48);
    for row in &prepared.set.inside {
        let frame = prepared.query(&row.symbol, row.frame).expect("inside");
        assert_eq!(
            inside_triples(&frame),
            row.want,
            "{} @{}",
            row.symbol,
            row.frame
        );
    }
    // Hand-typed anchors: the hidden layer of sym_hidden contributes nothing, and the late
    // keyframe of sym_late appears at its own frame 3 and stays until the component ends.
    let hidden = prepared.query("sym_hidden", 0).expect("inside");
    assert_eq!(
        inside_triples(&hidden),
        [("ly_comp_ia".to_owned(), 0, "s_comp_a_0".to_owned())]
    );
    let late = prepared.query("sym_late", 8).expect("inside");
    assert!(inside_triples(&late).contains(&("ly_comp_ib".to_owned(), 3, "s_comp_b_3".to_owned())));
}

#[test]
fn placed_shapes_and_the_entered_view_share_stable_target_identities() {
    let prepared = Prepared::of("variants");
    let mut compared = 0;
    for frame in 0..24 {
        let scene = prepared.scene(frame);
        for slot in scene.slots() {
            let ComponentOutcome::Shapes(placed) = &slot.entry().outcome else {
                continue;
            };
            let symbol = slot.entry().symbol_id.as_str();
            let inner = placed.inner_frame.raw() as u32;
            let outer: Vec<_> = placed
                .shapes
                .iter()
                .map(|shape| shape.target.clone())
                .collect();
            let entered = match prepared.query(symbol, inner).expect("inside") {
                super::ComponentFrame::Inside(inside) => inside,
                other => panic!("{other:?}"),
            };
            assert_eq!(entered.symbol_id, symbol);
            assert_eq!(entered.frame, inner);
            let inside: Vec<_> = entered.records.iter().map(|r| r.target().clone()).collect();
            assert_eq!(
                outer,
                inside,
                "{symbol} placed on {} @{frame}",
                slot.entry().layer_uid
            );
            compared += outer.len();
        }
    }
    assert_eq!(compared, 165, "shapes of the 96 variants rows");
    // The baseline itself agrees: what a placing layer shows (X rows) is what the entered
    // component shows at the resolved inner frame (I rows), for every layer and frame.
    let probe = set("variants");
    let symbol_of = [
        ("ly_v_comp", "sym_comp"),
        ("ly_v_hidden", "sym_hidden"),
        ("ly_v_late", "sym_late"),
        ("ly_v_interp", "sym_interp"),
    ];
    let mut agreed = 0;
    for row in &probe.expects {
        let Some(inner) = row.want["inner"].as_u64() else {
            continue;
        };
        let symbol = symbol_of.iter().find(|(uid, _)| *uid == row.uid).unwrap().1;
        let shown: Vec<(String, u32, String)> = row.want["shapes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                (
                    s["layer"].as_str().unwrap().to_owned(),
                    u32::try_from(s["frame"].as_u64().unwrap()).unwrap(),
                    s["strokeId"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let entered = probe
            .inside
            .iter()
            .find(|i| i.symbol == symbol && u64::from(i.frame) == inner)
            .unwrap();
        assert_eq!(shown, entered.want, "{} @{}", row.uid, row.frame);
        agreed += 1;
    }
    assert_eq!(agreed, 96);
}
