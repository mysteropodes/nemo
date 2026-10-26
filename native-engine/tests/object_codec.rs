//! nemo-test-harness: test-codec => object_codec_tests
//! N25A frozen record oracle exercised through real native persistence admission.
use crate::object_codec::{decode_project, encode_project, ObjectCodecErrorKind};
use serde_json::{json, Value};

#[path = "object_order_codec.rs"]
mod order_tests;

fn fixture() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    json!({"format":"nemo.native-object-document", "formatVersion":1, "totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}], "objects":cases["records"]})
}
fn decode(
    value: &Value,
) -> Result<crate::object_document::ObjectDocument, crate::object_codec::ObjectCodecError> {
    decode_project(&serde_json::to_vec(value).unwrap())
}

#[test]
fn exact_records_survive_persistence_and_disk_reopen_without_id_or_geometry_changes() {
    let source = fixture();
    let doc = decode(&source).unwrap();
    assert_eq!(doc.total_frames(), 21);
    assert_eq!(doc.layers()[1].layer_uid(), "0");
    assert_eq!(doc.layers()[1].name(), None);
    assert_eq!(doc.objects()[0].target().stroke_id(), "ink/path_A");
    assert_eq!(doc.objects()[1].target().stroke_id(), "0001");
    let bytes = encode_project(&doc).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), source);
    let path = std::env::temp_dir().join(format!(
        "nemo-n25b-object-{}-{}.json",
        std::process::id(),
        doc.objects().len()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let reopened = decode_project(&std::fs::read(&path).unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(reopened, doc);
    assert_eq!(encode_project(&reopened).unwrap(), bytes);
}

fn named_layers() -> Value {
    let baseline: Value = serde_json::from_str(include_str!(
        "../../tests/animation/fixtures/curve-workflow.json"
    ))
    .unwrap();
    assert_eq!(baseline["version"], 13);
    assert_eq!(baseline["layers"][0]["layerUid"], "r08_curve_layer");
    assert_eq!(baseline["layers"][0]["name"], "R08 rectangle");
    let mut value = fixture();
    value["layers"][0] = json!({"layerUid":"r08_curve_layer","name":"R08 rectangle"});
    value["objects"][0]["target"]["layerUid"] = json!("r08_curve_layer");
    value
}

fn named_layer_order_plan() -> Value {
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[
            {"layerUid":"r08_curve_layer","naturalPosition":0,"order":{}},
            {"layerUid":"0","naturalPosition":1,"order":{}}],
        "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
            {"layerUid":"r08_curve_layer","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"})
}

#[test]
fn stored_names_preserve_exact_strings_absence_and_explicit_order_migration() {
    use crate::object_codec::{
        decode_ordered_project, encode_ordered_project, migrate_ordered_project,
    };
    let plan = named_layer_order_plan();
    for name in ["R08 rectangle", "", "  Calque 雪 🟦\n"] {
        let mut input = named_layers();
        input["layers"][0]["name"] = json!(name);
        let document = decode(&input).unwrap();
        assert_eq!(document.layers()[0].name(), Some(name));
        assert_eq!(document.layers()[1].name(), None);
        assert_eq!(
            serde_json::from_slice::<Value>(&encode_project(&document).unwrap()).unwrap(),
            input
        );
        let ordered =
            migrate_ordered_project(&document, &serde_json::to_vec(&plan).unwrap()).unwrap();
        let bytes = encode_ordered_project(&ordered).unwrap();
        let reopened = decode_ordered_project(&bytes).unwrap();
        assert_eq!(reopened.layers()[0].name(), Some(name));
        assert_eq!(reopened.layers()[1].name(), None);
        assert_eq!(reopened.layers()[0].layer_uid(), "r08_curve_layer");
        assert_eq!(encode_ordered_project(&reopened).unwrap(), bytes);
        assert_eq!(
            document,
            decode(&input).unwrap(),
            "migration must not mutate its source"
        );
    }
}

#[test]
fn invalid_names_and_other_unadmitted_layer_fields_refuse_atomically() {
    use crate::object_codec::{
        decode_ordered_project, encode_ordered_project, migrate_ordered_project,
    };
    let named = decode(&named_layers()).unwrap();
    let ordered = migrate_ordered_project(
        &named,
        &serde_json::to_vec(&named_layer_order_plan()).unwrap(),
    )
    .unwrap();
    let ordered_bytes = encode_ordered_project(&ordered).unwrap();
    assert!(
        decode_ordered_project(&ordered_bytes).is_ok(),
        "v2 negative starts from admitted data"
    );
    for name in [
        Value::Null,
        json!(true),
        json!(3),
        json!([]),
        json!({"name":"nested"}),
    ] {
        let mut input = named_layers();
        input["layers"][0]["name"] = name.clone();
        assert_eq!(
            decode(&input).unwrap_err().kind(),
            ObjectCodecErrorKind::Invalid
        );
        let mut input: Value = serde_json::from_slice(&ordered_bytes).unwrap();
        input["layers"][0]["name"] = name;
        assert_eq!(
            decode_ordered_project(&serde_json::to_vec(&input).unwrap())
                .unwrap_err()
                .kind(),
            ObjectCodecErrorKind::Invalid
        );
    }
    let input = named_layers();
    let bytes = serde_json::to_string(&input).unwrap().replace(
        "\"name\":\"R08 rectangle\"",
        "\"name\":\"R08 rectangle\",\"name\":\"forged\"",
    );
    assert_eq!(
        decode_project(bytes.as_bytes()).unwrap_err().kind(),
        ObjectCodecErrorKind::Invalid
    );
    let duplicate = String::from_utf8(ordered_bytes.clone()).unwrap().replace(
        "\"name\":\"R08 rectangle\"",
        "\"name\":\"R08 rectangle\",\"name\":\"forged\"",
    );
    assert_eq!(
        decode_ordered_project(duplicate.as_bytes())
            .unwrap_err()
            .kind(),
        ObjectCodecErrorKind::Invalid
    );
    for field in ["privateExtra"] {
        let mut input = named_layers();
        input["layers"][0][field] = json!(true);
        assert_eq!(
            decode(&input).unwrap_err().kind(),
            ObjectCodecErrorKind::Unsupported
        );
        let mut input: Value = serde_json::from_slice(&ordered_bytes).unwrap();
        input["layers"][0][field] = json!(true);
        assert_eq!(
            decode_ordered_project(&serde_json::to_vec(&input).unwrap())
                .unwrap_err()
                .kind(),
            ObjectCodecErrorKind::Unsupported
        );
    }
    let mut input = named_layers();
    input["layers"][0] = json!(["r08_curve_layer", "R08 rectangle"]);
    assert_eq!(
        decode(&input).unwrap_err().kind(),
        ObjectCodecErrorKind::Invalid
    );
}

#[test]
fn layer_visibility_is_strict_lossless_and_keeps_all_stored_records() {
    use crate::object_codec::{
        decode_ordered_project, encode_ordered_project, migrate_ordered_project,
    };
    for visible in [None, Some(true), Some(false)] {
        let mut input = named_layers();
        if let Some(value) = visible {
            input["layers"][0]["visible"] = json!(value);
        }
        let document = decode(&input).unwrap();
        assert_eq!(document.layers()[0].visible(), visible.unwrap_or(true));
        assert_eq!(document.layers()[1].visible(), true);
        assert_eq!(document.objects().len(), 2);
        assert_eq!(
            serde_json::from_slice::<Value>(&encode_project(&document).unwrap()).unwrap(),
            input
        );
        let ordered = migrate_ordered_project(
            &document,
            &serde_json::to_vec(&named_layer_order_plan()).unwrap(),
        )
        .unwrap();
        let bytes = encode_ordered_project(&ordered).unwrap();
        let reopened = decode_ordered_project(&bytes).unwrap();
        assert_eq!(reopened.layers()[0].visible(), visible.unwrap_or(true));
        assert_eq!(reopened.objects().len(), 2);
        assert_eq!(encode_ordered_project(&reopened).unwrap(), bytes);
    }
    for value in [Value::Null, json!(0), json!("false"), json!([]), json!({})] {
        let mut input = named_layers();
        input["layers"][0]["visible"] = value;
        assert_eq!(
            decode(&input).unwrap_err().kind(),
            ObjectCodecErrorKind::Invalid
        );
    }
    let mut input = named_layers();
    input["layers"][0]["visible"] = json!(false);
    let bytes = serde_json::to_string(&input)
        .unwrap()
        .replace("\"visible\":false", "\"visible\":false,\"visible\":true");
    assert_eq!(
        decode_project(bytes.as_bytes()).unwrap_err().kind(),
        ObjectCodecErrorKind::Invalid
    );
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[test]
fn hidden_layer_project_revisions_save_reload_and_fill_history_keep_visibility() {
    use crate::history::NativeObjectHistory;
    let mut drawing = named_layers();
    drawing["layers"][0]["visible"] = json!(false);
    let input = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24.0},"drawing":drawing,"viewportReference":null,"assets":[]});
    let mut owner =
        NativeObjectHistory::from_project_file("visibility", &serde_json::to_vec(&input).unwrap())
            .unwrap();
    let zero = owner.acquire_snapshot(0).unwrap();
    let before = owner.serialize_project_file(0, zero.snapshot_id()).unwrap();
    let mut payload = cases_for_visibility()["command"]["request"]["payload"].clone();
    payload["stableTarget"]["layerUid"] = json!("r08_curve_layer");
    let prepared = owner
        .prepare_fill_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(prepared), Ok(1));
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(owner.redo(2), Ok(3));
    for revision in [0, 1, 2, 3] {
        let snapshot = owner.acquire_snapshot(revision).unwrap();
        assert!(!snapshot.document().layers()[0].visible());
        let bytes = owner
            .serialize_project_file(revision, snapshot.snapshot_id())
            .unwrap();
        let reopened = NativeObjectHistory::from_project_file("reopened", &bytes).unwrap();
        assert!(!reopened.acquire_snapshot(0).unwrap().document().layers()[0].visible());
        assert_eq!(
            reopened.project_file_info().unwrap()["layers"][0]["visible"],
            false
        );
    }
    assert_eq!(
        owner.serialize_project_file(0, zero.snapshot_id()).unwrap(),
        before
    );
}
#[cfg(all(feature = "history", feature = "project_media_codec"))]
fn cases_for_visibility() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}

#[test]
fn scoped_keys_are_not_cross_frame_or_scope_identity() {
    let mut value = fixture();
    let original = value["objects"][0].clone();
    let mut other_frame = original.clone();
    other_frame["target"]["frameScope"]["frame"] = json!(8);
    let mut reference = original.clone();
    reference["target"]["frameScope"]["kind"] = json!("reference");
    value["objects"]
        .as_array_mut()
        .unwrap()
        .extend([other_frame, reference]);
    let doc = decode(&value).unwrap();
    assert_eq!(doc.objects().len(), 4);
    assert_eq!(serde_json::to_value(doc).unwrap(), value);
    value["objects"].as_array_mut().unwrap().push(original);
    assert_eq!(
        decode(&value).unwrap_err().kind(),
        ObjectCodecErrorKind::DuplicateId
    );
}

#[test]
fn unresolved_references_and_duplicate_layers_reject_whole_document() {
    for (pointer, replacement) in [
        ("/objects/1/target/layerUid", json!("missing")),
        ("/objects/1/target/frameScope/frame", json!(21)),
        ("/objects/1/target/frameScope/frame", json!(-1)),
        ("/objects/1/target/strokeId", json!("")),
        ("/objects/1/target/contextId", json!("nested-context")),
        ("/totalFrames", json!(0)),
        ("/layers/1/layerUid", json!("legacy layer:alpha")),
    ] {
        let mut value = fixture();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(decode(&value).is_err(), "{pointer}");
    }
    let mut empty = fixture();
    empty["objects"] = json!([]);
    assert!(
        decode(&empty).is_ok(),
        "an identified empty frame is a supported document"
    );
}

#[test]
fn duplicate_raw_json_members_are_rejected_at_every_struct_level() {
    let source = serde_json::to_string(&fixture()).unwrap();
    for (needle, replacement) in [
        (
            "\"formatVersion\":1",
            "\"formatVersion\":1,\"formatVersion\":1",
        ),
        (
            "\"layerUid\":\"0\"",
            "\"layerUid\":\"0\",\"layerUid\":\"0\"",
        ),
        (
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
        ),
        (
            "\"strokeId\":\"ink/path_A\"",
            "\"strokeId\":\"ink/path_A\",\"strokeId\":\"ink/path_A\"",
        ),
        ("\"closed\":true", "\"closed\":true,\"closed\":true"),
        ("\"x\":10.125", "\"x\":10.125,\"x\":10.125"),
        ("\"a\":0.75", "\"a\":0.75,\"a\":0.75"),
        (
            "\"handleIn\":",
            "\"handleIn\":{\"x\":0,\"y\":0},\"handleIn\":",
        ),
        ("\"frame\":7", "\"frame\":7,\"frame\":7"),
    ] {
        assert!(
            source.contains(needle),
            "control must alter the frozen fixture"
        );
        let duplicate = source.replacen(needle, replacement, 1);
        assert!(decode_project(duplicate.as_bytes()).is_err(), "{needle}");
    }
}

#[test]
fn unsupported_and_malformed_records_never_get_silently_dropped() {
    for (pointer, replacement) in [
        ("/formatVersion", json!(2)),
        ("/objects/1/schemaVersion", json!(2)),
        ("/objects/1/family", json!("group")),
        ("/objects/1/geometry", Value::Null),
        ("/objects/1/geometry/closed", json!(false)),
        ("/objects/1/geometry/handleSpace", json!("absolute")),
        ("/objects/1/fill/kind", json!("gradient")),
        ("/objects/1/fill/a", json!(1.1)),
        ("/objects/1/target/layerUid", json!(0)),
        ("/objects/1/target/frameScope/kind", json!("all-frames")),
    ] {
        let mut value = fixture();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(decode(&value).is_err(), "{pointer}");
    }
    let mut unknown = fixture();
    unknown["objects"][1]["stroke"] = json!({});
    assert_eq!(
        decode(&unknown).unwrap_err().kind(),
        ObjectCodecErrorKind::Unsupported
    );
    for count in [0, 1, 2, 256, 257] {
        let mut value = fixture();
        value["objects"][0]["geometry"]["segments"] =
            json!(vec![
                value["objects"][0]["geometry"]["segments"][0].clone();
                count
            ]);
        assert_eq!(
            decode(&value).is_ok(),
            (2..=256).contains(&count),
            "{count}"
        );
    }
    let source = serde_json::to_string(&fixture()).unwrap();
    assert!(decode_project(source.replacen("10.125", "1e400", 1).as_bytes()).is_err());
}

#[test]
fn active_opacity_codec_remains_closed_to_objects() {
    assert!(crate::codec::decode_project(&serde_json::to_vec(&fixture()).unwrap()).is_err());
    let mut opacity: Value =
        serde_json::from_slice(include_bytes!("fixtures/opacity-v2/project.json")).unwrap();
    opacity["objects"] = fixture()["objects"].clone();
    assert!(crate::codec::decode_project(&serde_json::to_vec(&opacity).unwrap()).is_err());
    assert!(decode_project(include_bytes!("fixtures/opacity-v2/project.json")).is_err());
}

#[test]
fn positional_arrays_cannot_substitute_for_any_schema_object() {
    for (pointer, keys) in [
        (
            "",
            vec![
                "format",
                "formatVersion",
                "totalFrames",
                "layers",
                "objects",
            ],
        ),
        ("/layers/0", vec!["layerUid"]),
        (
            "/objects/0",
            vec!["schemaVersion", "family", "target", "geometry", "fill"],
        ),
        (
            "/objects/0/target",
            vec!["contextId", "frameScope", "layerUid", "strokeId"],
        ),
        ("/objects/0/target/frameScope", vec!["kind", "frame"]),
        (
            "/objects/0/geometry",
            vec![
                "kind",
                "closed",
                "coordinateSpace",
                "handleSpace",
                "segments",
            ],
        ),
        (
            "/objects/0/geometry/segments/0",
            vec!["point", "handleIn", "handleOut"],
        ),
        ("/objects/0/geometry/segments/0/point", vec!["x", "y"]),
        ("/objects/0/geometry/segments/0/handleIn", vec!["x", "y"]),
        ("/objects/0/geometry/segments/0/handleOut", vec!["x", "y"]),
        ("/objects/0/fill", vec!["kind", "r", "g", "b", "a"]),
    ] {
        let mut value = fixture();
        let object = value.pointer(pointer).unwrap();
        let positional = Value::Array(keys.iter().map(|key| object[*key].clone()).collect());
        *value.pointer_mut(pointer).unwrap() = positional;
        assert!(decode(&value).is_err(), "positional array at {pointer}");
    }
    assert!(decode_project(br#"["nemo.native-object-document",1,21,[["layer"]],[]]"#).is_err());
}

#[test]
fn frame_scope_kind_is_a_literal_string_and_never_a_tagged_enum_object() {
    for invalid in [
        json!({"authored":null}),
        json!({"reference":null}),
        json!(["authored"]),
        Value::Null,
        json!(7),
        json!(true),
        json!("other"),
    ] {
        let mut value = fixture();
        value["objects"][0]["target"]["frameScope"]["kind"] = invalid.clone();
        assert!(decode(&value).is_err(), "kind {invalid}");
    }
}
