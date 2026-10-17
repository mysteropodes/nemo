//! L08-J record tests: literal expectations from the executed baseline oracle (fbe4b8a3) and hand-derived
//! admission cases. Real codec, real documents; no stand-in for the owner under test.
use super::oracle_rows::{
    document_text, document_text_v2, layer_text, raw_import_inputs, rows, CASES, ORACLE,
};
use super::*;
use crate::object_codec::{
    decode_project, decode_storage_project, encode_project, encode_storage_project,
    ObjectCodecErrorKind,
};
use serde_json::{json, Value};

#[test]
fn oracle_rows_are_well_formed() {
    let meta = rows("meta");
    assert_eq!(meta.len(), 1);
    assert_eq!(
        meta[0]["baseline"],
        "fbe4b8a33e6d5a229f014154df2ba21988a9fabc"
    );
    for (kind, count) in [
        ("blend-vocab", 1),
        ("blend-write", 6),
        ("import", 26),
        ("freeze", 8),
        ("menu", 13),
        ("remap", 5),
        ("create", 2),
        ("effect-entry", 4),
        ("export-order", 1),
    ] {
        assert_eq!(rows(kind).len(), count, "{kind}");
    }
    assert_eq!(ORACLE.lines().count(), 67);
}

#[test]
fn blend_and_matte_vocabularies_are_the_baseline_literals() {
    let row = rows("blend-vocab").remove(0);
    let names: Vec<&str> = row["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 16);
    assert_eq!(names, BLEND_MODE_NAMES.to_vec());
    assert_eq!(
        MATTE_MODE_NAMES,
        ["alpha", "alphaInverted", "luma", "lumaInverted"]
    );
    assert_eq!(FOLDER_LAYER_COLOR, "#ffb020");
}

#[test]
fn blend_write_rule_matches_every_baseline_row() {
    for row in rows("blend-write") {
        let mode = row["mode"].as_str().unwrap();
        assert_eq!(blend_write(mode), row["stored"].as_str(), "{mode:?}");
    }
    assert_eq!(blend_write("normal"), None);
    assert_eq!(blend_write("Normal"), Some("Normal"));
}

#[derive(Clone, Copy)]
enum Disposition {
    /// The baseline keeps every authored member: native accepts and re-encodes it byte for byte.
    Same,
    /// The baseline drops, rewrites or (legacy marker / provider) cannot represent it: native refuses.
    Refused(ObjectCodecErrorKind),
    /// The baseline drops an explicit false; native stores it.
    ExplicitFalse,
    /// The baseline fills in a default (`effects: []`, `folderCollapsed: false`); native keeps absent absent.
    AbsentStaysAbsent,
}

fn disposition(id: &str) -> Disposition {
    use Disposition::*;
    use ObjectCodecErrorKind::{Invalid, Unsupported};
    match id {
        "blend-multiply"
        | "blend-explicit-normal"
        | "blend-unknown-name"
        | "matte-mode-alpha"
        | "matte-mode-none-explicit"
        | "matte-source-uid"
        | "mattes-more-two"
        | "effects-two"
        | "effects-empty"
        | "effect-layer-true"
        | "folder-layer-collapsed-true"
        | "effects-from-uid" => Same,
        "blend-number-dropped"
        | "matte-mode-number-dropped"
        | "matte-source-number-dropped"
        | "matte-source-empty-string-kept"
        | "mattes-more-filtered"
        | "mattes-more-empty-dropped"
        | "mattes-more-all-bad-dropped"
        | "effects-null-becomes-empty"
        | "collapsed-without-folder-dropped"
        | "effects-from-empty-dropped" => Refused(Invalid),
        "blend-keys-retained" => Refused(Unsupported),
        "effect-layer-false-dropped" => ExplicitFalse,
        "effects-absent-becomes-empty" | "folder-layer-defaults-collapsed-false" => {
            AbsentStaysAbsent
        }
        other => panic!("oracle row {other} has no pinned native disposition"),
    }
}

#[test]
fn every_baseline_import_row_has_its_pinned_native_disposition() {
    let raws = raw_import_inputs();
    let imports = rows("import");
    assert_eq!(raws.len(), imports.len());
    for ((id, raw), row) in raws.iter().zip(imports.iter()) {
        assert_eq!(row["id"], id.as_str());
        let input = row["input"].as_object().unwrap();
        let kept = &row["kept"];
        let text = document_text(&[layer_text("L0", raw)]);
        let decoded = decode_project(text.as_bytes());
        match disposition(id) {
            Disposition::Same => {
                let doc = decoded.unwrap_or_else(|error| panic!("{id}: {error}"));
                assert_eq!(encode_project(&doc).unwrap(), text.into_bytes(), "{id}");
                for (key, value) in input {
                    assert_eq!(&kept[key], value, "{id}: baseline keeps {key}");
                }
            }
            Disposition::Refused(kind) => {
                let error = decoded.unwrap_err();
                assert_eq!(error.kind(), kind, "{id}: {error}");
                if id != "matte-source-empty-string-kept" && id != "blend-keys-retained" {
                    assert!(
                        input
                            .iter()
                            .any(|(key, value)| kept.get(key) != Some(value)),
                        "{id}: the baseline must lose or rewrite what native refuses"
                    );
                }
            }
            Disposition::ExplicitFalse => {
                let doc = decoded.unwrap();
                assert_eq!(doc.layers[0].is_effect_layer.value(), Some(&false));
                assert!(!doc.layers[0].is_effect_layer());
                assert_eq!(encode_project(&doc).unwrap(), text.into_bytes());
                assert!(kept.get("isEffectLayer").is_none(), "baseline drops false");
            }
            Disposition::AbsentStaysAbsent => {
                let doc = decoded.unwrap();
                assert_eq!(encode_project(&doc).unwrap(), text.into_bytes(), "{id}");
                if id == "effects-absent-becomes-empty" {
                    assert_eq!(doc.layers[0].effects(), None);
                    assert_eq!(kept["effects"], json!([]));
                } else {
                    assert!(doc.layers[0].folder_collapsed.missing());
                    assert!(!doc.layers[0].folder_collapsed());
                    assert_eq!(kept["folderCollapsed"], json!(false));
                }
            }
        }
    }
    // The empty-string source the baseline keeps is its legacy "layer above" marker: the freeze row says so.
    let freeze = rows("freeze");
    let marker = freeze
        .iter()
        .find(|row| row["id"] == "empty-string-source-is-frozen")
        .unwrap();
    assert_eq!(marker["sources"], json!(["b", null]));
}

#[test]
fn absent_default_null_and_explicit_values_stay_distinct_through_the_codec() {
    let text = document_text(&[
        layer_text("A", "{}"),
        layer_text(
            "B",
            r#"{"blendMode":"normal","matteMode":"none","isEffectLayer":false,"effects":[{"type":"blur","enabled":true,"p1":0,"p2":null}]}"#,
        ),
    ]);
    let doc = decode_project(text.as_bytes()).unwrap();
    let (a, b) = (&doc.layers[0], &doc.layers[1]);
    assert_eq!((a.blend_mode(), b.blend_mode()), (None, Some("normal")));
    assert_eq!((a.matte_mode(), b.matte_mode()), (None, Some("none")));
    assert!(a.is_effect_layer.missing() && !b.is_effect_layer.missing());
    assert_eq!(a.effects(), None);
    let effect = &b.effects().unwrap()[0];
    assert!(effect.p3.missing());
    assert_eq!(effect.p2.value(), Some(&None));
    assert!(effect.p1.value().and_then(Option::as_ref).is_some());
    assert_eq!(
        (effect.param(0), effect.param(1), effect.param(2)),
        (Some(0.0), None, None)
    );
    assert_eq!(encode_project(&doc).unwrap(), text.into_bytes());
}

#[test]
fn member_order_is_the_baseline_export_order_and_roundtrips_byte_exact() {
    let layer = r#"{"layerUid":"L0","blendMode":"multiply","matteMode":"alpha","matteSourceLayerUid":"L1","mattesMore":[{"uid":"L2","mode":"luma"}],"isEffectLayer":false,"isFolderLayer":true,"folderCollapsed":true,"effects":[{"type":"blur","enabled":false,"p1":4}],"effectsFrom":"L2"}"#;
    let text = document_text(&[
        layer.to_string(),
        layer_text("L1", "{}"),
        layer_text("L2", "{}"),
    ]);
    let doc = decode_project(text.as_bytes()).unwrap();
    assert_eq!(encode_project(&doc).unwrap(), text.clone().into_bytes());
    let order = rows("export-order").remove(0);
    let mut last = 0;
    for key in order["keys"].as_array().unwrap() {
        let at = layer
            .find(&format!("\"{}\":", key.as_str().unwrap()))
            .unwrap();
        assert!(at > last, "{key} out of baseline order");
        last = at;
    }
    let layer0 = &doc.layers[0];
    assert_eq!(layer0.compositing_reference_uids(), ["L1", "L2", "L2"]);
    assert_eq!(
        layer0.mattes_more(),
        [MatteRef {
            uid: "L2".into(),
            mode: "luma".into()
        }]
    );
    assert!(layer0.matte_on() && layer0.is_folder_layer() && layer0.folder_collapsed());
}

#[test]
fn effect_entries_roundtrip_the_shapes_addeffect_pushes() {
    for row in rows("effect-entry") {
        let json = row["json"].as_str().unwrap();
        let entry: LayerEffect = serde_json::from_str(json).unwrap();
        // serde_json prints 1e21 where JSON.stringify prints 1e+21: same number, other spelling.
        assert_eq!(
            serde_json::to_string(&entry).unwrap(),
            json.replace("e+", "e"),
            "{}",
            row["id"]
        );
    }
}

fn kind_of(layer_members: &str) -> Result<(), ObjectCodecErrorKind> {
    let text = document_text(&[format!("{{\"layerUid\":\"L0\",{layer_members}}}")]);
    decode_project(text.as_bytes())
        .map(|_| ())
        .map_err(|error| error.kind())
}

#[test]
fn admission_refusals_have_typed_kinds_and_nothing_is_rewritten() {
    use ObjectCodecErrorKind::{Invalid, Unsupported};
    for (members, expected) in [
        (r#""matteSourceLayerUid":"""#, Invalid),
        (r#""effectsFrom":"""#, Invalid),
        (r#""mattesMore":[]"#, Invalid),
        (r#""mattesMore":[{"uid":"","mode":"alpha"}]"#, Invalid),
        (r#""isEffectLayer":true,"isFolderLayer":true"#, Invalid),
        (r#""folderCollapsed":false"#, Invalid),
        (r#""isFolderLayer":false,"folderCollapsed":false"#, Invalid),
        (r#""blendMode":null"#, Invalid),
        (r#""matteMode":null"#, Invalid),
        (r#""effects":null"#, Invalid),
        (r#""isEffectLayer":null"#, Invalid),
        (r#""blendMode":"a","blendMode":"b""#, Invalid),
        (r#""mattesMore":[["L1","alpha"]]"#, Invalid),
        (r#""effects":[{"type":"blur"}]"#, Invalid),
        (r#""effects":[{"enabled":true}]"#, Invalid),
        (r#""effects":[{"type":"blur","enabled":1}]"#, Invalid),
        (
            r#""effects":[{"type":"blur","enabled":true,"p1":"4"}]"#,
            Invalid,
        ),
        (r#""effects":[["blur",true]]"#, Invalid),
        (
            r#""effects":[{"type":"blur","enabled":true,"keys":{}}]"#,
            Unsupported,
        ),
        (
            r#""effects":[{"type":"blur","enabled":true,"p9":1}]"#,
            Unsupported,
        ),
        (
            r#""mattesMore":[{"uid":"L1","mode":"alpha","extra":1}]"#,
            Unsupported,
        ),
        (
            r#""blendKeys":[{"frame":0,"mode":"multiply"}]"#,
            Unsupported,
        ),
        (r#""isCanvasBackground":true"#, Unsupported),
    ] {
        assert_eq!(kind_of(members), Err(expected), "{members}");
    }
    // Admitted alongside: unknown NAMES are stored as authored; dangling and self references are admitted.
    for members in [
        r#""blendMode":"Multiply""#,
        r#""blendMode":"""#,
        r#""matteMode":"foo","matteSourceLayerUid":"nowhere""#,
        r#""matteMode":"alpha","matteSourceLayerUid":"L0""#,
        r#""effectsFrom":"L0""#,
        r#""effects":[{"type":"custom:twirl","enabled":false}]"#,
    ] {
        assert_eq!(kind_of(members), Ok(()), "{members}");
    }
}

#[test]
fn effect_and_folder_layers_own_no_objects() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","layerUid":"L0","strokeId":"ink",
        "frameScope":{"kind":"authored","frame":0}});
    let document = |members: Value, painted_layer: &str| {
        let mut record = record.clone();
        record["target"]["layerUid"] = json!(painted_layer);
        let mut layer = json!({"layerUid":"L0"});
        for (key, value) in members.as_object().unwrap() {
            layer[key] = value.clone();
        }
        json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":8,
            "layers":[layer,{"layerUid":"L1"}],"objects":[record]})
    };
    let decode = |value: &Value| decode_project(value.to_string().as_bytes());
    assert!(decode(&document(json!({}), "L0")).is_ok());
    for members in [json!({"isEffectLayer":true}), json!({"isFolderLayer":true})] {
        let error = decode(&document(members.clone(), "L0")).unwrap_err();
        assert_eq!(error.kind(), ObjectCodecErrorKind::Invalid);
        assert_eq!(
            error.message(),
            LayerCompositingError::KindLayerHasObjects.message()
        );
        assert!(
            decode(&document(members, "L1")).is_ok(),
            "objects of ANOTHER layer are fine"
        );
    }
    // `false` flags are not kind layers.
    assert!(decode(&document(
        json!({"isEffectLayer":false,"isFolderLayer":false}),
        "L0"
    ))
    .is_ok());
}

#[test]
fn v2_documents_share_the_same_admission_and_roundtrip() {
    let layers = [
        (
            "L0",
            layer_text(
                "L0",
                r#"{"blendMode":"screen","matteMode":"lumaInverted","matteSourceLayerUid":"L1"}"#,
            ),
        ),
        (
            "L1",
            layer_text(
                "L1",
                r#"{"isFolderLayer":true,"folderCollapsed":false,"effects":[]}"#,
            ),
        ),
        (
            "L2",
            layer_text(
                "L2",
                r#"{"isEffectLayer":true,"effects":[{"type":"vignette","enabled":true,"p1":0.5}],"effectsFrom":"L1"}"#,
            ),
        ),
    ];
    let text = document_text_v2(&layers);
    let doc = decode_storage_project(text.as_bytes()).unwrap();
    let encoded = encode_storage_project(&doc).unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&encoded).unwrap(),
        serde_json::from_str::<Value>(&text).unwrap()
    );
    assert_eq!(doc.layers[2].effects().unwrap().len(), 1);
    for bad in [
        r#"{"folderCollapsed":true}"#,
        r#"{"isEffectLayer":true,"isFolderLayer":true}"#,
        r#"{"mattesMore":[]}"#,
    ] {
        let broken = [("L0", layer_text("L0", bad))];
        assert!(
            decode_storage_project(document_text_v2(&broken).as_bytes()).is_err(),
            "{bad}"
        );
    }
}
