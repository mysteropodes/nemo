//! Oracle-backed and literal tests for the widget layer record (L03-K, F19).
//!
//! The oracle rows come from executing the baseline JS (`rig-widget.js`, `motion.js`, `timeline.js`,
//! `transplant.js`, `app.js`, `engine-bridge.js` at fbe4b8a3) and are parsed in
//! `object_widget_record_rows.rs`; rejection tables and guards are in
//! `object_widget_record_controls.rs`. Literals below are hand-derived from the baseline source,
//! never produced by the record under test.

use super::oracle_rows::{
    control_rows, holders_for, import_row, import_rows, layer, make_rows, object_on, stack_rows,
    v1, v2, ORACLE,
};
use super::{
    validate_widget_layers, AxisBinding, AxisUnresolved, WidgetAdmissionError, WidgetAxis,
    WidgetBindings, WidgetLayerKind, WidgetRecord, WidgetRest,
};
use crate::object_codec::{
    decode_ordered_project, decode_project, decode_storage_project, encode_project,
    encode_storage_project, migrate_ordered_project, ObjectCodecErrorKind,
};
use crate::object_document::{ObjectLayer, Presence, PropertyHolder};
use serde_json::Value;

fn widget(text: &str) -> WidgetRecord {
    serde_json::from_str(text).expect("widget decodes")
}

fn s(text: &str) -> String {
    text.to_string()
}

fn reencode(record: &WidgetRecord) -> String {
    serde_json::to_string(record).expect("widget encodes")
}

fn native_accepts(text: &str) -> bool {
    serde_json::from_str::<WidgetRecord>(text)
        .map(|record| record.validate().is_ok())
        .unwrap_or(false)
}

fn holders(controls: Option<&str>) -> Vec<PropertyHolder> {
    match controls {
        None => Vec::new(),
        Some(list) => serde_json::from_str(&format!(
            "[{{\"target\":{{\"contextId\":\"scene-root\",\"layerUid\":\"W\"}},\"controls\":{list},\"properties\":[]}}]"
        ))
        .expect("holders decode"),
    }
}

/// Native rejections among the `imp` rows: every one is an input the baseline import deep-copies
/// (or, for the last four, does not accept either).
const NATIVE_REJECTS: [&str; 19] = [
    "joystick_without_y",
    "slider_with_y",
    "null_y",
    "unknown_kind",
    "kind_wrong_case",
    "missing_kind",
    "x_key_not_string",
    "x_key_null",
    "missing_x",
    "unknown_member",
    "unknown_axis_member",
    "min_not_number",
    "missing_max",
    "pos_wrong_length",
    "size_not_array",
    "not_object_five",
    "not_object_string",
    "not_object_null",
    "not_object_array",
];

/// Inputs the baseline import ACCEPTS and the native record refuses (the documented differences).
const NATIVE_NARROWER: [&str; 12] = [
    "joystick_without_y",
    "kind_wrong_case",
    "min_not_number",
    "missing_kind",
    "missing_max",
    "null_y",
    "pos_wrong_length",
    "size_not_array",
    "slider_with_y",
    "unknown_axis_member",
    "unknown_kind",
    "unknown_member",
];

#[test]
fn oracle_row_counts_are_pinned() {
    assert_eq!(make_rows().len(), 6);
    assert_eq!(import_rows().len(), 39);
    assert_eq!(control_rows().len(), 13);
    assert_eq!(stack_rows().len(), 3);
    assert_eq!(ORACLE.lines().count(), 61);
}

#[test]
fn baseline_created_widgets_decode_validate_and_reencode_byte_identically() {
    for row in make_rows() {
        let record = widget(&row.widget);
        assert_eq!(record.validate(), Ok(()), "{}", row.name);
        assert_eq!(reencode(&record), row.widget, "{}", row.name);
        assert!(row.flag, "{}: addWidgetLayer sets isWidgetLayer", row.name);
        assert_eq!(row.color, "\"#ffb86c\"", "{}: ld.color", row.name);
    }
}

#[test]
fn baseline_created_joystick_has_the_assigned_literal_shape() {
    let rows = make_rows();
    let row = rows.iter().find(|r| r.name == "joystick").unwrap();
    assert_eq!(row.canvas, (1920, 1080));
    let record = widget(&row.widget);
    assert_eq!(record.kind, WidgetLayerKind::Joystick);
    // pos is the canvas centre, size the joystick default, screenSpace explicitly false.
    assert_eq!(record.pos_value(), Some([960.0, 540.0]));
    assert_eq!(record.size_value(), Some([160.0, 160.0]));
    assert_eq!(record.screen_space, Presence::Present(false));
    assert!(!record.screen_space_value());
    assert_eq!(
        (record.x.min_value(), record.x.max_value()),
        (-100.0, 100.0)
    );
    assert_eq!(record.x.rest_state(), WidgetRest::Value(0.0));
    assert_eq!(record.x.label, Presence::Present(Some("X".to_string())));
    let y = record.y_axis().expect("a joystick has a y axis");
    assert_eq!((y.min_value(), y.max_value()), (-100.0, 100.0));
    assert_eq!(y.label, Presence::Present(Some("Y".to_string())));
    assert!(record.x.key.starts_with("xc_") && y.key.starts_with("xc_"));
    assert_ne!(record.x.key, y.key);
    // makeAxis seeds each control's static value at rest (0), one entry per axis key.
    assert_eq!(
        row.statics,
        format!("{{\"{}\":[0],\"{}\":[0]}}", record.x.key, y.key)
    );
}

#[test]
fn baseline_created_slider_and_canvas_centres_have_the_assigned_literals() {
    let rows = make_rows();
    let pick = |name: &str| widget(&rows.iter().find(|r| r.name == name).unwrap().widget);
    let slider = pick("slider");
    assert_eq!(slider.kind, WidgetLayerKind::Slider);
    assert_eq!(slider.size_value(), Some([200.0, 26.0]));
    assert!(slider.y_axis().is_none());
    assert_eq!((slider.x.min_value(), slider.x.max_value()), (0.0, 100.0));
    assert_eq!(slider.x.label, Presence::Present(Some("Value".to_string())));
    assert_eq!(pick("joystick_720p").pos_value(), Some([640.0, 360.0]));
    assert_eq!(pick("slider_odd_canvas").pos_value(), Some([500.5, 281.5]));
    // addWidgetLayer coerces any unknown or missing kind to a joystick.
    assert_eq!(
        pick("unknown_kind_falls_to_joystick").kind,
        WidgetLayerKind::Joystick
    );
    assert_eq!(
        pick("missing_kind_falls_to_joystick").kind,
        WidgetLayerKind::Joystick
    );
}

#[test]
fn baseline_created_layers_pass_the_real_codec_byte_for_byte_and_resolve_their_controls() {
    for row in make_rows() {
        let record = widget(&row.widget);
        let text = v1(
            &[layer("W", &row.widget)],
            &[],
            Some(&holders_for("W", &row.controls, &row.statics)),
        );
        let document = decode_project(text.as_bytes()).expect(&row.name);
        assert_eq!(
            encode_project(&document).expect(&row.name),
            text.as_bytes(),
            "{}: byte-identical document",
            row.name
        );
        let want = WidgetBindings {
            x: AxisBinding::Resolved,
            y: record.y_axis().map(|_| AxisBinding::Resolved),
        };
        assert_eq!(document.widget_bindings("W"), Some(want), "{}", row.name);
        let kinds = document.layers()[0].widget().map(|w| w.kind);
        assert_eq!(kinds, Some(record.kind), "{}", row.name);
    }
}

#[test]
fn v2_storage_keeps_the_widget_through_decode_encode_ordered_and_migration() {
    let rows = make_rows();
    let row = rows.iter().find(|r| r.name == "joystick").unwrap();
    let layers = [layer("W", &row.widget)];
    let holders_text = holders_for("W", &row.controls, &row.statics);
    let text = v2(&layers, &["W"], &[], Some(&holders_text));
    let document = decode_storage_project(text.as_bytes()).expect("storage decode");
    assert_eq!(document.format_version(), 2);
    assert_eq!(
        document.widget_bindings("W").map(|b| b.x),
        Some(AxisBinding::Resolved)
    );
    let bytes = encode_storage_project(&document).expect("storage encode");
    let member = |bytes: &[u8]| -> Value {
        let value: Value = serde_json::from_slice(bytes).unwrap();
        value["layers"][0]["widget"].clone()
    };
    let want: Value = serde_json::from_str(&row.widget).unwrap();
    assert_eq!(member(&bytes), want);
    assert_eq!(decode_storage_project(&bytes).expect("reopen"), document);
    let ordered = decode_ordered_project(text.as_bytes()).expect("ordered decode");
    assert_eq!(
        ordered.layers()[0].widget().map(reencode),
        Some(row.widget.clone())
    );
    let v1_text = v1(&layers, &[], Some(&holders_text));
    let from_v1 = decode_project(v1_text.as_bytes()).expect("v1");
    let plan_text = super::oracle_rows::plan(&["W"]);
    let migrated = migrate_ordered_project(&from_v1, plan_text.as_bytes()).expect("migrate");
    assert_eq!(
        migrated.layers()[0].widget().map(reencode),
        Some(row.widget.clone())
    );
}

#[test]
fn authored_states_stay_distinct_on_the_native_wire() {
    let minimal = r#"{"kind":"slider","x":{"key":"k","min":0,"max":1}}"#;
    let nulls = r#"{"kind":"slider","pos":null,"size":null,"screenSpace":false,"x":{"key":"k","min":0,"max":1,"rest":null,"label":null}}"#;
    let explicit = r#"{"kind":"slider","pos":[1,2],"size":[3,4],"screenSpace":true,"x":{"key":"k","min":0,"max":1,"rest":0.25,"label":"L"}}"#;
    let (a, b, c) = (widget(minimal), widget(nulls), widget(explicit));
    // absent: nothing is invented on re-encode
    assert_eq!(reencode(&a), minimal);
    assert!(a.pos.missing() && a.size.missing() && a.screen_space.missing());
    assert!(a.x.rest.missing() && a.x.label.missing());
    assert_eq!((a.pos_value(), a.size_value()), (None, None));
    assert_eq!(a.x.rest_state(), WidgetRest::Absent);
    // null: present-but-empty, still not the same record
    assert_eq!(reencode(&b), nulls);
    assert_eq!(b.pos, Presence::Present(None));
    assert_eq!(b.screen_space, Presence::Present(false));
    assert_eq!((b.pos_value(), b.size_value()), (None, None));
    assert_eq!(b.x.rest_state(), WidgetRest::Null);
    // explicit values
    assert_eq!(reencode(&c), explicit);
    assert_eq!(c.pos_value(), Some([1.0, 2.0]));
    assert_eq!(c.size_value(), Some([3.0, 4.0]));
    assert!(c.screen_space_value());
    assert_eq!(c.x.rest_state(), WidgetRest::Value(0.25));
    assert!(a != b && b != c && a != c);
}

#[test]
fn numbers_keep_their_authored_spelling() {
    let text =
        r#"{"kind":"slider","pos":[1.0,2],"x":{"key":"k","min":0.5,"max":1.0,"rest":-0.25}}"#;
    assert_eq!(reencode(&widget(text)), text);
    let integer = widget(r#"{"kind":"slider","x":{"key":"k","min":0,"max":1}}"#);
    let float = widget(r#"{"kind":"slider","x":{"key":"k","min":0.0,"max":1.0}}"#);
    assert_ne!(
        integer, float,
        "integer and float spellings are different records"
    );
    assert_eq!(integer.x.min_value(), float.x.min_value());
}

#[test]
fn oracle_import_rows_decide_the_native_verdict_and_the_documented_narrowing() {
    let rows = import_rows();
    for name in NATIVE_REJECTS {
        import_row(&rows, name);
    }
    let mut narrower = Vec::new();
    for row in &rows {
        let expect = !NATIVE_REJECTS.contains(&row.name.as_str());
        assert_eq!(native_accepts(&row.widget), expect, "{}", row.name);
        if row.import && !expect {
            narrower.push(row.name.as_str());
        }
        if expect {
            // never wider than the baseline, and the duplicate deep copy IS the re-encode
            assert!(
                row.import && row.widget_of && row.transplant,
                "{}",
                row.name
            );
            assert_eq!(
                row.copy.as_deref(),
                Some(row.widget.as_str()),
                "{}",
                row.name
            );
            assert_eq!(reencode(&widget(&row.widget)), row.widget, "{}", row.name);
        }
    }
    narrower.sort_unstable();
    assert_eq!(narrower, NATIVE_NARROWER);
    // the three kind faults are inputs the baseline itself does not treat as a widget
    let not_widgets: Vec<&str> = rows
        .iter()
        .filter(|r| r.import && !r.widget_of)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(
        not_widgets,
        ["unknown_kind", "kind_wrong_case", "missing_kind"]
    );
}

#[test]
fn control_rows_decide_display_labels_and_bindings() {
    for row in control_rows() {
        let axis: WidgetAxis = serde_json::from_str(&row.axis).expect(&row.name);
        let list = holders(row.controls.as_deref());
        assert_eq!(axis.display_label("W", &list), row.label, "{}", row.name);
        let record = widget(&format!("{{\"kind\":\"slider\",\"x\":{}}}", row.axis));
        let want = if axis.key.is_empty() {
            AxisBinding::Unresolved(AxisUnresolved::EmptyKey)
        } else {
            match row.found_type.as_deref() {
                Some("number") => AxisBinding::Resolved,
                Some(_) => AxisBinding::Unresolved(AxisUnresolved::NotNumberControl),
                None => AxisBinding::Unresolved(AxisUnresolved::NoSuchControl),
            }
        };
        assert_eq!(record.bindings("W", &list).x, want, "{}", row.name);
        // another layer's controls never reach this widget
        let other = record.bindings("OTHER", &list).x;
        assert_eq!(
            other,
            if axis.key.is_empty() {
                AxisBinding::Unresolved(AxisUnresolved::EmptyKey)
            } else {
                AxisBinding::Unresolved(AxisUnresolved::NoSuchControl)
            },
            "{}",
            row.name
        );
    }
}

#[test]
fn control_rows_pin_literal_labels_and_bindings() {
    let rows = control_rows();
    let pick = |name: &str| rows.iter().find(|r| r.name == name).unwrap();
    let state = |name: &str| {
        let row = pick(name);
        let record = widget(&format!("{{\"kind\":\"slider\",\"x\":{}}}", row.axis));
        let list = holders(row.controls.as_deref());
        (
            record.bindings("W", &list).x,
            record.x.display_label("W", &list),
        )
    };
    let missing = AxisBinding::Unresolved(AxisUnresolved::NoSuchControl);
    assert_eq!(
        state("number_control_found"),
        (AxisBinding::Resolved, s("Turn"))
    );
    assert_eq!(
        state("live_name_beats_stored_label"),
        (AxisBinding::Resolved, s("Renamed"))
    );
    assert_eq!(
        state("empty_name_falls_to_label"),
        (AxisBinding::Resolved, s("Old"))
    );
    assert_eq!(
        state("whitespace_name_is_truthy"),
        (AxisBinding::Resolved, s(" "))
    );
    assert_eq!(state("control_missing_uses_label"), (missing, s("Old")));
    assert_eq!(state("control_missing_no_label"), (missing, String::new()));
    assert_eq!(state("null_label_no_control"), (missing, String::new()));
    assert_eq!(state("no_controls_at_all"), (missing, s("Old")));
    assert_eq!(
        state("empty_key_never_looks_up"),
        (
            AxisBinding::Unresolved(AxisUnresolved::EmptyKey),
            s("Stored")
        )
    );
    let not_number = AxisBinding::Unresolved(AxisUnresolved::NotNumberControl);
    assert_eq!(state("angle_control_is_found"), (not_number, s("Spin")));
    assert_eq!(state("checkbox_control_is_found"), (not_number, s("On")));
    // the first control with the key decides, as controlByKey does
    assert_eq!(state("first_duplicate_key_wins"), (not_number, s("One")));
    assert_eq!(
        state("unicode_name"),
        (AxisBinding::Resolved, s("T\u{ea}te"))
    );
}

#[test]
fn a_widget_layer_renders_nothing_so_it_may_not_own_objects() {
    for row in stack_rows() {
        let has_widget = row.flag == Some(true);
        assert_eq!(row.stack_empty, has_widget, "{}", row.name);
        assert_eq!(row.scene_empty, has_widget, "{}", row.name);
        let mut layer_value = ObjectLayer::identified("W".to_string());
        if has_widget {
            layer_value.widget = Presence::Present(widget(
                r#"{"kind":"slider","x":{"key":"k","min":0,"max":1}}"#,
            ));
        }
        let objects: Vec<crate::object_document::ObjectRecord> =
            vec![serde_json::from_str(&object_on("W")).expect("object record decodes")];
        let verdict = validate_widget_layers(&[layer_value.clone()], &objects);
        assert_eq!(
            verdict.is_err(),
            row.stack_empty,
            "{}: objects are refused exactly where the baseline renders nothing",
            row.name
        );
        if has_widget {
            assert_eq!(verdict, Err(WidgetAdmissionError::WidgetLayerHasObjects));
        }
        assert_eq!(validate_widget_layers(&[layer_value], &[]), Ok(()));
    }
}

#[test]
fn documents_refuse_objects_on_a_widget_layer_on_both_wires_with_typed_kinds() {
    let slider = r#"{"kind":"slider","x":{"key":"k","min":0,"max":1}}"#;
    let objects = [object_on("W")];
    let kind_v1 = decode_project(v1(&[layer("W", slider)], &objects, None).as_bytes())
        .expect_err("v1 refused")
        .kind();
    assert_eq!(kind_v1, ObjectCodecErrorKind::Invalid);
    let kind_v2 =
        decode_ordered_project(v2(&[layer("W", slider)], &["W"], &objects, None).as_bytes())
            .expect_err("v2 refused")
            .kind();
    assert_eq!(kind_v2, ObjectCodecErrorKind::Invalid);
    // the same object on an ordinary layer is accepted, so the refusal is the widget's alone
    let plain = v1(&["{\"layerUid\":\"W\"}".to_string()], &objects, None);
    assert!(decode_project(plain.as_bytes()).is_ok());
}
