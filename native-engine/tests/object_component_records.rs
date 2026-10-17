//! Oracle tests for the native component records (L04-G, I14/I35).
//!
//! `fixtures/object-component-records/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_oracle.cjs` of the L04-G leaf from baseline fbe4b8a3: the symbol
//! `sym_comp`, seven instance layers and the held/blank frame keys of
//! `tests/fixtures/components/project.json`, the stroke ids of its
//! `expected.json`, three entries returned by the verbatim `expose()` of
//! `domain/component/exposed-properties.js`, and a Time Remap track, a
//! `symMatrix` and a montage-segment layer produced by the verbatim
//! `enableTimeRemap`, `symMatrixSet` and `enterMontageView` literals. The
//! header of the fixture lists the baseline blob ids.
//!
//! Rows are TAB separated, `-` is absent. `Y` component JSON; `L` layerUid and
//! host-layer JSON; `I` layerUid, instance JSON, mode, speed, placedAt, single,
//! trimIn, trimOut, frameKeys (`frame:componentFrame:blank,..`), matrix JSON,
//! Time Remap key count; `K` stroke id; `X` exposed key, label, stroke, field,
//! default. The JSON columns are the native record shape; every expected token
//! beside them was read from the baseline objects, not from the JSON.
use super::*;
use crate::object_document::ObjectLayer;
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-component-records/oracle.txt");

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}
pub(crate) fn hosts() -> Vec<ObjectLayer> {
    rows("L")
        .iter()
        .map(|f| serde_json::from_str(f[2]).expect("host layer"))
        .collect()
}
pub(crate) fn set_json() -> String {
    let instances: Vec<&str> = rows("I").iter().map(|f| f[2]).collect();
    format!(
        "{{\"format\":\"{}\",\"formatVersion\":{},\"components\":[{}],\"instances\":[{}]}}",
        COMPONENT_SET_FORMAT,
        COMPONENT_SET_FORMAT_VERSION,
        rows("Y")[0][1],
        instances.join(",")
    )
}
pub(crate) fn set_value() -> Value {
    serde_json::from_str(&set_json()).expect("set json")
}
pub(crate) fn baseline() -> ComponentSet {
    serde_json::from_str(&set_json()).expect("baseline set")
}
fn tok(token: &str) -> Option<f64> {
    if token == "-" {
        None
    } else {
        Some(token.parse::<f64>().expect("number token"))
    }
}
fn instance_of(layer_uid: &str) -> ComponentInstance {
    let row = rows("I")
        .into_iter()
        .find(|f| f[1] == layer_uid)
        .expect("instance row");
    serde_json::from_str(row[2]).expect("instance")
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains("Baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains("tests/fixtures/components/project.json"));
    assert_eq!(rows("Y").len(), 1);
    assert_eq!(rows("L").len(), 9);
    assert_eq!(rows("I").len(), 9);
    assert_eq!(rows("K").len(), 13);
    assert_eq!(rows("X").len(), 3);
}

#[test]
fn component_decodes_and_re_encodes_to_the_exact_authored_bytes() {
    let json = rows("Y")[0][1];
    let component: ComponentRecord = serde_json::from_str(json).unwrap();
    assert_eq!(serde_json::to_string(&component).unwrap(), json);
    let again: ComponentRecord =
        serde_json::from_str(&serde_json::to_string(&component).unwrap()).unwrap();
    assert_eq!(again, component);
}

#[test]
fn component_fields_match_the_baseline_symbol() {
    let set = baseline();
    let component = set.component("sym_comp").expect("component");
    assert_eq!(component.symbol_id(), "sym_comp");
    assert_eq!(component.total_frames(), 12);
    assert_eq!(component.name, Presence::Present("Comp".to_string()));
    assert_eq!(
        component.fps.value().map(|n| n.to_string()),
        Some("24".into())
    );
    let uids: Vec<&str> = component.layers().iter().map(|l| l.layer_uid()).collect();
    assert_eq!(uids, ["ly_comp_ia", "ly_comp_ib"]);
    assert_eq!(component.layers()[0].name(), Some("Inner A"));
    // Inner A keys a stroke on every one of the 12 frames, Inner B one stroke on frame 0.
    let on_a = component
        .objects()
        .iter()
        .filter(|o| o.target.layer_uid == "ly_comp_ia")
        .count();
    assert_eq!((component.objects().len(), on_a), (13, 12));
    assert!(component
        .objects()
        .iter()
        .all(|o| o.target.context_id == "sym_comp"));
    assert!(set.component("ghost").is_none());
}

#[test]
fn stroke_ids_equal_the_union_of_the_baseline_expectations() {
    let set = baseline();
    let mut have: Vec<&str> = set
        .component("sym_comp")
        .unwrap()
        .objects()
        .iter()
        .map(|o| o.target.stroke_id.as_str())
        .collect();
    have.sort_unstable();
    let want: Vec<&str> = rows("K").iter().map(|f| f[1]).collect();
    assert_eq!(have, want);
}

#[test]
fn exposed_props_equal_what_the_verbatim_expose_returned() {
    let set = baseline();
    let props = set.component("sym_comp").unwrap().exposed_props();
    let want = rows("X");
    assert_eq!(props.len(), want.len());
    for (prop, row) in props.iter().zip(&want) {
        assert!(prop.key.starts_with("ep_"));
        assert_eq!(prop.key, row[1]);
        assert_eq!(prop.label, row[2]);
        assert_eq!(prop.target_stroke_id, row[3]);
        assert_eq!(prop.target_field, row[4]);
        assert_eq!(
            prop.default_value.value().map(|n| n.to_string()),
            Some(row[5].to_string())
        );
    }
    let fields: Vec<&str> = props.iter().map(|p| p.target_field.as_str()).collect();
    assert_eq!(fields, ["opacity", "__visible", "fillColor"]);
}

#[test]
fn component_passes_the_existing_codec_content_validator() {
    let set = baseline();
    let c = set.component("sym_comp").unwrap();
    crate::object_codec::validate_content(c.total_frames(), c.layers(), c.objects(), true)
        .expect("inner layers and strokes satisfy the document validator");
}

#[test]
fn instances_match_the_baseline_layer_fields() {
    for row in rows("I") {
        let instance: ComponentInstance = serde_json::from_str(row[2]).unwrap();
        assert_eq!(
            serde_json::to_string(&instance).unwrap(),
            row[2],
            "{}",
            row[1]
        );
        assert_eq!(instance.layer_uid(), row[1]);
        assert_eq!(instance.context_id(), "scene-root");
        assert_eq!(instance.symbol_id(), "sym_comp");
        assert_eq!(
            instance.play_mode(),
            if row[3] == "-" { None } else { Some(row[3]) }
        );
        assert_eq!(instance.speed(), tok(row[4]), "speed {}", row[1]);
        assert_eq!(instance.placed_at(), tok(row[5]), "placedAt {}", row[1]);
        assert_eq!(instance.single_frame(), tok(row[6]), "single {}", row[1]);
        assert_eq!(instance.trim_in(), tok(row[7]), "trimIn {}", row[1]);
        assert_eq!(instance.trim_out(), tok(row[8]), "trimOut {}", row[1]);
        let keys: Vec<String> = instance
            .frame_keys()
            .iter()
            .map(|k| {
                let cf = k
                    .component_frame()
                    .map_or("-".to_string(), |v| v.to_string());
                let blank = k
                    .blank_override()
                    .map_or("-", |b| if b { "1" } else { "0" });
                format!("{}:{}:{}", k.frame(), cf, blank)
            })
            .collect();
        assert_eq!(
            if keys.is_empty() {
                "-".to_string()
            } else {
                keys.join(",")
            },
            row[9]
        );
        assert_eq!(
            instance.matrix().map(|m| serde_json::to_string(m).unwrap()),
            if row[10] == "-" {
                None
            } else {
                Some(row[10].to_string())
            }
        );
        let remap = instance.time_remap().map(|t| t.keys.len().to_string());
        assert_eq!(
            remap,
            if row[11] == "-" {
                None
            } else {
                Some(row[11].to_string())
            }
        );
    }
}

#[test]
fn held_component_frame_and_blank_keys_come_through_as_authored() {
    let hold = instance_of("ly_comp_hold");
    let key = &hold.frame_keys()[0];
    assert_eq!(
        (key.frame(), key.component_frame(), key.blank_override()),
        (0, Some(3.0), None)
    );
    let blank = instance_of("ly_comp_blank");
    let key = &blank.frame_keys()[0];
    assert_eq!(
        (key.frame(), key.component_frame(), key.blank_override()),
        (12, None, Some(true))
    );
    assert_eq!(blank.frame_keys().len(), 1);
}

#[test]
fn time_remap_track_is_the_verbatim_enable_time_remap_output() {
    let remap = instance_of("ly_comp_remap");
    let track = remap.time_remap().expect("track");
    let frames: Vec<String> = track.keys.iter().map(|k| k.frame.to_string()).collect();
    let values: Vec<String> = track.keys.iter().map(|k| k.v[0].to_string()).collect();
    // inPoint 0 -> internal frame 0, outPoint 23 -> last internal frame (12 - 1).
    assert_eq!(
        (frames, values),
        (vec!["0".into(), "23".into()], vec!["0".into(), "11".into()])
    );
    assert_eq!(remap.matrix().map(<[_]>::len), Some(6));
}

#[test]
fn whole_table_validates_against_the_baseline_host_layers() {
    let set = baseline();
    assert_eq!(set.components.len(), 1);
    assert_eq!(set.instances.len(), 9);
    set.validate(&hosts())
        .expect("baseline table is admissible");
    assert_eq!(serde_json::to_string(&set).unwrap(), set_json());
    assert!(set.instance("scene-root", "ly_comp_speed").is_some());
    assert!(set.instance("sym_comp", "ly_comp_speed").is_none());
}

#[test]
fn absent_null_and_explicit_values_stay_distinct_for_every_optional_field() {
    let explicit = [
        ("symPlayMode", serde_json::json!("pingpong")),
        ("symSpeed", serde_json::json!(3)),
        ("symPlacedAt", serde_json::json!(5)),
        ("symSingleFrame", serde_json::json!(2)),
        ("symTrimIn", serde_json::json!(1)),
        ("symTrimOut", serde_json::json!(8)),
        ("symMatrix", serde_json::json!([1, 0, 0, 1, 0, 0])),
        ("timeRemap", serde_json::json!({"keys": []})),
    ];
    let base: Value = serde_json::from_str(rows("I")[0][2]).unwrap();
    for (field, value) in explicit {
        let mut absent = base.clone();
        absent.as_object_mut().unwrap().remove(field);
        let mut null = base.clone();
        null[field] = Value::Null;
        let mut set = base.clone();
        set[field] = value.clone();
        for (variant, shown) in [(absent, "absent"), (null, "null"), (set, "explicit")] {
            let instance: ComponentInstance = serde_json::from_value(variant.clone()).unwrap();
            let encoded: Value =
                serde_json::from_str(&serde_json::to_string(&instance).unwrap()).unwrap();
            assert_eq!(
                encoded, variant,
                "{field} {shown} survives a round trip unchanged"
            );
            assert_eq!(
                encoded.get(field).is_some(),
                shown != "absent",
                "{field} {shown}"
            );
        }
    }
    // The accessors collapse exactly what the baseline collapses, the records do not.
    let mut null = base.clone();
    null["symPlayMode"] = Value::Null;
    let mut absent = base.clone();
    absent.as_object_mut().unwrap().remove("symPlayMode");
    let (null, absent): (ComponentInstance, ComponentInstance) = (
        serde_json::from_value(null).unwrap(),
        serde_json::from_value(absent).unwrap(),
    );
    assert_eq!((null.play_mode(), absent.play_mode()), (None, None));
    assert_eq!(null.sym_play_mode, Presence::Present(None));
    assert_eq!(absent.sym_play_mode, Presence::Missing);
    let loop_label: ComponentInstance = serde_json::from_str(rows("I")[0][2]).unwrap();
    assert_eq!(
        loop_label.play_mode(),
        Some("loop"),
        "an explicit loop is not absent"
    );
}

#[test]
fn blank_override_false_is_not_absent_and_null_is_not_false() {
    let key = |text: &str| -> FrameKey { serde_json::from_str(text).unwrap() };
    assert_eq!(
        key("{\"frame\":0,\"blankOverride\":false}").blank_override(),
        Some(false)
    );
    assert_eq!(
        key("{\"frame\":0,\"blankOverride\":null}").blank_override(),
        None
    );
    assert_eq!(
        key("{\"frame\":0,\"componentFrame\":0}").component_frame(),
        Some(0.0)
    );
    assert_eq!(
        key("{\"frame\":0,\"componentFrame\":null}").component_frame(),
        None
    );
    let explicit_false = key("{\"frame\":0,\"blankOverride\":false}");
    assert_ne!(explicit_false, key("{\"frame\":0}"));
    assert_eq!(
        serde_json::to_string(&explicit_false).unwrap(),
        "{\"frame\":0,\"blankOverride\":false}"
    );
}

#[test]
fn re_applying_identical_records_is_byte_identical() {
    let set = baseline();
    let first = serde_json::to_vec(&set).unwrap();
    let reapplied: ComponentSet = serde_json::from_slice(&first).unwrap();
    assert_eq!(reapplied, set);
    assert_eq!(serde_json::to_vec(&reapplied).unwrap(), first);
    reapplied.validate(&hosts()).expect("still admissible");
}

#[test]
fn a_component_without_exposed_props_keeps_the_field_absent() {
    let mut value: Value = serde_json::from_str(rows("Y")[0][1]).unwrap();
    value.as_object_mut().unwrap().remove("exposedProps");
    let bare: ComponentRecord = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(bare.exposed_props, Presence::Missing);
    assert!(bare.exposed_props().is_empty());
    assert!(!serde_json::to_string(&bare)
        .unwrap()
        .contains("exposedProps"));
    value["exposedProps"] = serde_json::json!([]);
    let empty: ComponentRecord = serde_json::from_value(value).unwrap();
    assert_eq!(empty.exposed_props, Presence::Present(Vec::new()));
    assert!(serde_json::to_string(&empty)
        .unwrap()
        .contains("\"exposedProps\":[]"));
}
