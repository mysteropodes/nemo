//! Negative and mutation controls of the component evaluation join (L04-K). Each control edits
//! ONE input of a set that otherwise reproduces the baseline and shows that the oracle comparison
//! sees the edit, with the exact number of rows that must disagree (counted from the baseline
//! arithmetic in the comment of each test). A comparison that passed these edits would be blind.

use super::support::{
    diff, disagreements, entry_of, inside_triples, selector, set, Prepared, SCENE,
};
use super::{evaluate_packet, ComponentFrameError};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentJoinError;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    PinField, ScenePin,
};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::prepare_object_frame;
use serde_json::{json, Number, Value};

fn instance_mut<'a>(document: &'a mut Value, uid: &str) -> &'a mut Value {
    document["componentTables"]["components"]["instances"]
        .as_array_mut()
        .expect("instances")
        .iter_mut()
        .find(|instance| instance["layerUid"] == uid)
        .unwrap_or_else(|| panic!("no instance on {uid}"))
}

fn component_mut<'a>(document: &'a mut Value, symbol: &str) -> &'a mut Value {
    document["componentTables"]["components"]["components"]
        .as_array_mut()
        .expect("components")
        .iter_mut()
        .find(|component| component["symbolId"] == symbol)
        .unwrap_or_else(|| panic!("no component {symbol}"))
}

fn only(found: &[String], uid: &str) -> bool {
    found.iter().all(|message| message.starts_with(uid))
}

#[test]
fn the_unmutated_documents_reproduce_every_row() {
    for name in ["base", "playback", "overrides", "remap"] {
        assert_eq!(
            disagreements(&Prepared::of(name)),
            Vec::<String>::new(),
            "{name}"
        );
    }
}

#[test]
fn removing_the_held_component_frame_key_disagrees_on_22_of_24_hold_rows() {
    // Held at 3 the layer shows inner frame 3 everywhere; without the key it loops from 0, which
    // equals 3 only at outer frames 3 and 15.
    let mut mutated = set("base");
    instance_mut(&mut mutated.document, "ly_comp_hold")
        .as_object_mut()
        .unwrap()
        .remove("frameKeys");
    let found = disagreements(&Prepared::from_set(mutated));
    assert_eq!(found.len(), 22, "{found:?}");
    assert!(only(&found, "ly_comp_hold"));
}

#[test]
fn a_changed_speed_or_placement_disagrees_on_exactly_the_shifted_rows() {
    // ly_comp_speed 2x -> 1x: 2f % 12 equals f % 12 only at f = 0 and 12 (22 rows differ).
    let mut slower = set("base");
    instance_mut(&mut slower.document, "ly_comp_speed")["symSpeed"] = json!(1);
    let found = disagreements(&Prepared::from_set(slower));
    assert_eq!(found.len(), 22, "{found:?}");
    assert!(only(&found, "ly_comp_speed"));
    // ly_comp_loop placed at 4 -> 0: only f = 0 (inner 0 both ways) agrees (23 rows differ).
    let mut moved = set("base");
    instance_mut(&mut moved.document, "ly_comp_loop")["symPlacedAt"] = json!(0);
    let found = disagreements(&Prepared::from_set(moved));
    assert_eq!(found.len(), 23, "{found:?}");
    assert!(only(&found, "ly_comp_loop"));
}

#[test]
fn a_swapped_canvas_changes_the_component_camera_geometry() {
    let mut mutated = set("camera");
    assert_eq!(mutated.canvas, Some((640.0, 360.0)));
    mutated.canvas = Some((360.0, 640.0));
    let found = disagreements(&Prepared::from_set(mutated));
    assert!(!found.is_empty());
    assert!(
        found.iter().all(|message| message.contains("geometry")),
        "{found:?}"
    );
}

#[test]
fn a_one_ulp_geometry_a_fill_and_an_inner_frame_change_are_each_detected() {
    let prepared = Prepared::of("placed");
    let scene = prepared.scene(0);
    let entry = entry_of(&scene, "ly_comp_loop");
    let row = prepared
        .set
        .expects
        .iter()
        .find(|row| row.uid == "ly_comp_loop" && row.frame == 0)
        .expect("row");
    assert_eq!(diff(entry, &row.want), None);
    let x = row.want["shapes"][0]["segments"][0]["point"][0]
        .as_f64()
        .unwrap();
    let mut nudged = row.want.clone();
    nudged["shapes"][0]["segments"][0]["point"][0] =
        Value::Number(Number::from_f64(f64::from_bits(x.to_bits() + 1)).unwrap());
    assert_eq!(
        diff(entry, &nudged),
        Some("geometry of s_comp_a_0".to_owned())
    );
    let mut recolored = row.want.clone();
    recolored["shapes"][0]["fill"] = json!("#ff9501");
    assert_eq!(
        diff(entry, &recolored),
        Some("fill of s_comp_a_0".to_owned())
    );
    let mut shifted = row.want.clone();
    shifted["inner"] = json!(1);
    assert_eq!(diff(entry, &shifted), Some("inner frame 0 != 1".to_owned()));
    assert_eq!(
        diff(entry, &json!({ "blank": true })),
        Some("baseline is blank, native is not".to_owned())
    );
}

#[test]
fn a_value_that_does_not_hide_changes_only_that_instance() {
    // ly_comp_ping reads 10 (below 50: stroke B hidden); 50 keeps B, so every ping row that has
    // a B stroke now has two shapes where the baseline has one.
    let mut mutated = set("overrides");
    mutated
        .values
        .insert(("ly_comp_ping".to_owned(), "ep_vis".to_owned()), json!(50));
    let found = disagreements(&Prepared::from_set(mutated));
    assert!(!found.is_empty());
    assert!(only(&found, "ly_comp_ping"), "{found:?}");
}

#[test]
fn a_hidden_inner_layer_leaves_the_entered_view() {
    let mut mutated = set("variants");
    let component = component_mut(&mut mutated.document, "sym_comp");
    assert_eq!(component["layers"][1]["layerUid"], "ly_comp_ib");
    component["layers"][1]["visible"] = json!(false);
    let prepared = Prepared::from_set(mutated);
    let mut differing = 0;
    for row in prepared
        .set
        .inside
        .iter()
        .filter(|row| row.symbol == "sym_comp")
    {
        let got = inside_triples(&prepared.query("sym_comp", row.frame).expect("inside"));
        if got != row.want {
            differing += 1;
            assert!(got.iter().all(|(layer, _, _)| layer == "ly_comp_ia"));
        }
    }
    // Stroke B exists at every one of the 12 frames of the component.
    assert_eq!(differing, 12);
}

#[test]
fn a_pin_taken_from_another_snapshot_of_the_same_document_text_is_refused() {
    let (mine, theirs) = (Prepared::of("base"), Prepared::of("base"));
    let sel_mine = selector(&mine.snapshot, SCENE, FrameScopeKind::Authored, 2);
    let sel_theirs = selector(&theirs.snapshot, SCENE, FrameScopeKind::Authored, 2);
    let packet = prepare_object_frame(&mine.snapshot, &sel_mine).expect("packet");
    let foreign =
        ScenePin::of_packet(&prepare_object_frame(&theirs.snapshot, &sel_theirs).expect("packet"));
    let got = evaluate_packet(
        &packet,
        &foreign,
        None,
        &mut mine.ports(),
        &mut mine.runtime(),
    );
    assert_eq!(
        got.err(),
        Some(ComponentFrameError::Join(ComponentJoinError::WrongPin {
            producer: None,
            field: PinField::Document,
        }))
    );
}
