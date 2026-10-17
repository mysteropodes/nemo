//! Negative controls and guards for the widget layer record (L03-K, F19): single-flaw rejection
//! tables against an admitted twin, typed codec kinds, the unresolved-binding rule, the existing
//! layer rules the record must not paper over, and oracle non-vacuity.

use super::oracle_rows::{
    control_rows, holders_for, import_rows, layer, make_rows, v1, v2, ORACLE,
};
use super::{AxisBinding, AxisUnresolved, WidgetAdmissionError, WidgetBindings, WidgetRecord};
use crate::object_codec::{
    decode_ordered_project, decode_project, encode_project, ObjectCodecErrorKind,
};
use crate::object_document::{Presence, PropertyHolder};

const Y_TEXT: &str = r#"{"key":"xc_b","min":-100,"max":100,"rest":0,"label":"Y"}"#;
const TWIN: &str = r#"{"kind":"joystick","pos":[960,540],"size":[160,160],"screenSpace":false,"x":{"key":"xc_a","min":-100,"max":100,"rest":0,"label":"X"},"y":{"key":"xc_b","min":-100,"max":100,"rest":0,"label":"Y"}}"#;
const SLIDER: &str = r#"{"kind":"slider","pos":[960,540],"size":[200,26],"screenSpace":false,"x":{"key":"xc_a","min":0,"max":100,"rest":0,"label":"Value"}}"#;
const CONTROLS: &str = r#"[{"key":"xc_a","name":"X","type":"number","default":[0]},{"key":"xc_b","name":"Y","type":"number","default":[0]}]"#;
const STATICS: &str = r#"{"xc_a":[0],"xc_b":[0]}"#;

fn accepts(text: &str) -> bool {
    serde_json::from_str::<WidgetRecord>(text)
        .map(|record| record.validate().is_ok())
        .unwrap_or(false)
}

fn v1_kind(widget: &str) -> Result<(), ObjectCodecErrorKind> {
    decode_project(v1(&[layer("W", widget)], &[], None).as_bytes())
        .map(|_| ())
        .map_err(|error| error.kind())
}

fn v2_kind(widget: &str) -> Result<(), ObjectCodecErrorKind> {
    decode_ordered_project(v2(&[layer("W", widget)], &["W"], &[], None).as_bytes())
        .map(|_| ())
        .map_err(|error| error.kind())
}

#[test]
fn the_admitted_twins_pass_so_each_rejection_is_its_single_flaw() {
    assert!(accepts(TWIN));
    assert!(accepts(SLIDER));
    assert_eq!(v1_kind(TWIN), Ok(()));
    assert_eq!(v2_kind(TWIN), Ok(()));
    assert_eq!(v1_kind(SLIDER), Ok(()));
    assert!(
        TWIN.contains(Y_TEXT),
        "the y axis text used by the tables is the twin's own"
    );
}

/// (what is wrong, the twin text to edit, edit from, edit to, expected codec kind).
fn flaws() -> Vec<(
    &'static str,
    &'static str,
    String,
    String,
    ObjectCodecErrorKind,
)> {
    use ObjectCodecErrorKind::{Invalid, Parse, Unsupported};
    let swap = |from: &str, to: &str| (from.to_string(), to.to_string());
    let rows: Vec<(
        &'static str,
        &'static str,
        (String, String),
        ObjectCodecErrorKind,
    )> = vec![
        (
            "unknown kind",
            TWIN,
            swap(r#""kind":"joystick""#, r#""kind":"dial""#),
            Invalid,
        ),
        (
            "kind wrong case",
            TWIN,
            swap(r#""kind":"joystick""#, r#""kind":"Joystick""#),
            Invalid,
        ),
        (
            "kind not a string",
            TWIN,
            swap(r#""kind":"joystick""#, r#""kind":3"#),
            Invalid,
        ),
        (
            "joystick without y",
            TWIN,
            swap(&format!(",\"y\":{Y_TEXT}"), ""),
            Invalid,
        ),
        ("y null", TWIN, swap(Y_TEXT, "null"), Invalid),
        (
            "slider with y",
            SLIDER,
            swap(
                r#""label":"Value"}"#,
                &format!(r#""label":"Value"}},"y":{Y_TEXT}"#),
            ),
            Invalid,
        ),
        (
            "unknown widget member",
            TWIN,
            swap(r#""screenSpace":false"#, r#""screenSpace":false,"extra":1"#),
            Unsupported,
        ),
        (
            "unknown axis member",
            TWIN,
            swap(r#""label":"X""#, r#""label":"X","curve":"ease""#),
            Unsupported,
        ),
        (
            "key is a number",
            TWIN,
            swap(r#""key":"xc_a""#, r#""key":7"#),
            Invalid,
        ),
        (
            "key is null",
            TWIN,
            swap(r#""key":"xc_a""#, r#""key":null"#),
            Invalid,
        ),
        ("key missing", TWIN, swap(r#""key":"xc_a","#, ""), Invalid),
        (
            "min is a string",
            TWIN,
            swap(
                r#""min":-100,"max":100,"rest":0,"label":"X""#,
                r#""min":"0","max":100,"rest":0,"label":"X""#,
            ),
            Invalid,
        ),
        (
            "max missing",
            TWIN,
            swap(
                r#""max":100,"rest":0,"label":"X""#,
                r#""rest":0,"label":"X""#,
            ),
            Invalid,
        ),
        (
            "rest is a string",
            TWIN,
            swap(r#""rest":0,"label":"X""#, r#""rest":"0","label":"X""#),
            Invalid,
        ),
        (
            "label is a number",
            TWIN,
            swap(r#""label":"X""#, r#""label":5"#),
            Invalid,
        ),
        // serde_json reports a surplus element of a fixed-size array as TrailingCharacters (a
        // Syntax-class error, de.rs end_seq), which the codec maps to Parse; the short array
        // below is an invalid_length Data error and maps to Invalid.
        (
            "pos has three numbers",
            TWIN,
            swap("[960,540]", "[960,540,0]"),
            Parse,
        ),
        (
            "pos has one number",
            TWIN,
            swap("[960,540]", "[960]"),
            Invalid,
        ),
        (
            "pos is an object",
            TWIN,
            swap("[960,540]", r#"{"x":960,"y":540}"#),
            Invalid,
        ),
        ("size is a scalar", TWIN, swap("[160,160]", "160"), Invalid),
        (
            "screenSpace is null",
            TWIN,
            swap(r#""screenSpace":false"#, r#""screenSpace":null"#),
            Invalid,
        ),
        (
            "screenSpace is a string",
            TWIN,
            swap(r#""screenSpace":false"#, r#""screenSpace":"no""#),
            Invalid,
        ),
        (
            "x missing",
            TWIN,
            swap(
                r#""x":{"key":"xc_a","min":-100,"max":100,"rest":0,"label":"X"},"#,
                "",
            ),
            Invalid,
        ),
        (
            "positional x",
            TWIN,
            swap(
                r#""x":{"key":"xc_a","min":-100,"max":100,"rest":0,"label":"X"}"#,
                r#""x":["xc_a",-100,100,0,"X"]"#,
            ),
            Invalid,
        ),
    ];
    rows.into_iter()
        .map(|(name, twin, (from, to), kind)| (name, twin, from, to, kind))
        .collect()
}

#[test]
fn single_flaw_widgets_are_rejected_with_the_typed_codec_kind_on_both_wires() {
    for (name, twin, from, to, kind) in flaws() {
        assert!(
            twin.contains(&from),
            "{name}: the edit anchor must exist in the twin"
        );
        let flawed = twin.replacen(&from, &to, 1);
        assert_ne!(flawed, twin, "{name}: the edit must change the twin");
        assert!(!accepts(&flawed), "{name}: record level");
        assert_eq!(v1_kind(&flawed), Err(kind), "{name}: v1");
        assert_eq!(v2_kind(&flawed), Err(kind), "{name}: v2");
    }
}

#[test]
fn axis_count_rules_come_from_the_validator_not_the_decoder() {
    // Both shapes decode (the members are individually legal); only validate() refuses them.
    let joystick_without_y = TWIN.replacen(&format!(",\"y\":{Y_TEXT}"), "", 1);
    let record: WidgetRecord = serde_json::from_str(&joystick_without_y).unwrap();
    assert_eq!(
        record.validate(),
        Err(WidgetAdmissionError::JoystickNeedsYAxis)
    );
    let slider_with_y = SLIDER.replacen(
        r#""label":"Value"}"#,
        &format!(r#""label":"Value"}},"y":{Y_TEXT}"#),
        1,
    );
    let record: WidgetRecord = serde_json::from_str(&slider_with_y).unwrap();
    assert_eq!(record.validate(), Err(WidgetAdmissionError::SliderHasYAxis));
    assert!(WidgetAdmissionError::SliderHasYAxis
        .message()
        .contains("slider"));
}

#[test]
fn non_finite_numbers_cannot_enter_the_wire_or_a_number() {
    let huge = TWIN.replacen("[960,540]", "[1e999,540]", 1);
    assert!(serde_json::from_str::<WidgetRecord>(&huge).is_err());
    assert!(serde_json::Number::from_f64(f64::NAN).is_none());
    assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());
}

#[test]
fn encoding_an_inadmissible_widget_is_refused_without_bytes() {
    let text = v1(
        &[layer("W", TWIN)],
        &[],
        Some(&holders_for("W", CONTROLS, STATICS)),
    );
    let mut joystick = decode_project(text.as_bytes()).expect("decode");
    if let Presence::Present(widget) = &mut joystick.layers[0].widget {
        widget.y = Presence::Missing;
    } else {
        panic!("the twin carries a widget");
    }
    let refused = encode_project(&joystick).expect_err("a joystick needs its y axis");
    assert_eq!(refused.kind(), ObjectCodecErrorKind::Invalid);
    let mut slider = decode_project(v1(&[layer("W", SLIDER)], &[], None).as_bytes()).unwrap();
    if let Presence::Present(widget) = &mut slider.layers[0].widget {
        widget.y = Presence::Present(serde_json::from_str(Y_TEXT).unwrap());
    }
    assert_eq!(
        encode_project(&slider)
            .expect_err("a slider has no y")
            .kind(),
        ObjectCodecErrorKind::Invalid
    );
}

#[test]
fn an_unresolved_axis_key_is_admitted_flagged_and_never_rewritten() {
    // No holder at all: both keys name nothing, the document still decodes and re-encodes byte-identically.
    let text = v1(&[layer("W", TWIN)], &[], None);
    let document = decode_project(text.as_bytes()).expect("admitted");
    let missing = AxisBinding::Unresolved(AxisUnresolved::NoSuchControl);
    let want = WidgetBindings {
        x: missing,
        y: Some(missing),
    };
    assert_eq!(document.widget_bindings("W"), Some(want));
    assert_eq!(
        document.widget_bindings("W"),
        Some(want),
        "evaluation is pure"
    );
    assert_eq!(encode_project(&document).unwrap(), text.as_bytes());
    // A control of another type stays admitted too and is flagged differently.
    let angle = r#"[{"key":"xc_a","name":"X","type":"angle","default":[0]},{"key":"xc_b","name":"Y","type":"number","default":[0]}]"#;
    let text = v1(
        &[layer("W", TWIN)],
        &[],
        Some(&holders_for("W", angle, "{}")),
    );
    let document = decode_project(text.as_bytes()).expect("admitted");
    let flagged = WidgetBindings {
        x: AxisBinding::Unresolved(AxisUnresolved::NotNumberControl),
        y: Some(AxisBinding::Resolved),
    };
    assert_eq!(document.widget_bindings("W"), Some(flagged));
    assert_eq!(encode_project(&document).unwrap(), text.as_bytes());
    // Missing layer and widget-less layer have no bindings at all.
    assert_eq!(document.widget_bindings("nope"), None);
    let plain = v1(&["{\"layerUid\":\"P\"}".to_string()], &[], None);
    assert_eq!(
        decode_project(plain.as_bytes())
            .unwrap()
            .widget_bindings("P"),
        None
    );
}

#[test]
fn only_layer_level_holders_of_the_same_layer_resolve_an_axis() {
    let widget: WidgetRecord = serde_json::from_str(TWIN).unwrap();
    let holder = |target: &str| -> Vec<PropertyHolder> {
        serde_json::from_str(&format!(
            "[{{\"target\":{target},\"controls\":{CONTROLS},\"properties\":[]}}]"
        ))
        .unwrap()
    };
    let own = holder(r#"{"contextId":"scene-root","layerUid":"W"}"#);
    assert_eq!(widget.bindings("W", &own).x, AxisBinding::Resolved);
    let stroke = holder(r#"{"contextId":"scene-root","layerUid":"W","strokeId":"s1"}"#);
    assert_eq!(
        widget.bindings("W", &stroke).x,
        AxisBinding::Unresolved(AxisUnresolved::NoSuchControl),
        "an element-level holder is not the layer's control table"
    );
    let other = holder(r#"{"contextId":"scene-root","layerUid":"V"}"#);
    assert_eq!(
        widget.bindings("W", &other).x,
        AxisBinding::Unresolved(AxisUnresolved::NoSuchControl)
    );
}

#[test]
fn a_duplicated_layer_resolves_only_with_its_own_copy_of_the_holder() {
    // plan_duplicate copies the layer record (widget included) under a fresh uid; the controls live
    // in the holder, so the copy is unresolved until that holder is copied too (obligation for L02).
    let own = holders_for("W", CONTROLS, STATICS);
    let layers = [layer("W", TWIN), layer("W2", TWIN)];
    let without = v1(&layers, &[], Some(&own));
    let document = decode_project(without.as_bytes()).expect("admitted");
    let unresolved = AxisBinding::Unresolved(AxisUnresolved::NoSuchControl);
    assert_eq!(
        document.widget_bindings("W2"),
        Some(WidgetBindings {
            x: unresolved,
            y: Some(unresolved)
        })
    );
    assert_eq!(
        document.widget_bindings("W").map(|b| b.x),
        Some(AxisBinding::Resolved)
    );
    let copy = holders_for("W2", CONTROLS, STATICS);
    let both = format!("[{},{}]", &own[1..own.len() - 1], &copy[1..copy.len() - 1]);
    let with = v1(&layers, &[], Some(&both));
    let document = decode_project(with.as_bytes()).expect("admitted");
    let resolved = WidgetBindings {
        x: AxisBinding::Resolved,
        y: Some(AxisBinding::Resolved),
    };
    assert_eq!(document.widget_bindings("W2"), Some(resolved));
}

#[test]
fn locking_and_parenting_a_widget_layer_stay_refused_by_the_existing_layer_rule() {
    // Baseline widget layers can be locked (the gesture escape hatch) and parented. F06 admits
    // locked layers (Drawing Fill enforces its own lock guard); parenting stays refused for EVERY
    // layer until a transform-parent consumer lands, and this record does not paper over that.
    let locked = format!("{{\"layerUid\":\"W\",\"locked\":true,\"widget\":{TWIN}}}");
    decode_project(v1(&[locked], &[], None).as_bytes())
        .expect("F06: a locked widget layer is admitted");
    let parented = format!("{{\"layerUid\":\"W\",\"parentLayerUid\":\"P\",\"widget\":{TWIN}}}");
    let layers = [parented, "{\"layerUid\":\"P\"}".to_string()];
    let error = decode_project(v1(&layers, &[], None).as_bytes()).expect_err("parented");
    assert_eq!(error.kind(), ObjectCodecErrorKind::Unsupported);
    // an unlocked widget layer with explicit false members is fine
    let plain =
        format!("{{\"layerUid\":\"W\",\"visible\":true,\"locked\":false,\"widget\":{TWIN}}}");
    assert!(decode_project(v1(&[plain], &[], None).as_bytes()).is_ok());
}

#[test]
fn the_widget_member_is_optional_and_null_is_not_a_widget() {
    let plain = v1(&["{\"layerUid\":\"L\"}".to_string()], &[], None);
    let document = decode_project(plain.as_bytes()).expect("plain");
    assert!(document.layers()[0].widget().is_none());
    let bytes = encode_project(&document).unwrap();
    assert_eq!(bytes, plain.as_bytes());
    assert!(!String::from_utf8(bytes).unwrap().contains("widget"));
    let null = v1(
        &["{\"layerUid\":\"L\",\"widget\":null}".to_string()],
        &[],
        None,
    );
    assert!(decode_project(null.as_bytes()).is_err());
}

#[test]
fn the_oracle_is_not_vacuous() {
    let makes = make_rows();
    assert!(makes
        .iter()
        .any(|r| r.widget.starts_with("{\"kind\":\"slider\"")));
    assert!(makes.iter().any(|r| r.widget.contains("\"y\":{")));
    let imports = import_rows();
    assert!(imports.iter().filter(|r| accepts(&r.widget)).count() >= 20);
    assert!(imports.iter().filter(|r| !accepts(&r.widget)).count() >= 19);
    assert!(imports.iter().any(|r| r.widget.contains("\"rest\":null")));
    assert!(imports.iter().any(|r| !r.widget.contains("\"rest\"")));
    assert!(imports.iter().any(|r| !r.widget.contains("\"pos\"")));
    assert!(imports.iter().any(|r| r.widget.contains("\"pos\":null")));
    assert!(imports
        .iter()
        .any(|r| !r.widget.contains("\"screenSpace\"")));
    let controls = control_rows();
    let types: Vec<Option<&str>> = controls.iter().map(|r| r.found_type.as_deref()).collect();
    assert!(
        types.contains(&Some("number")) && types.contains(&Some("angle")) && types.contains(&None)
    );
    assert!(ORACLE.lines().all(|line| line.split('\t').count() >= 4));
}
