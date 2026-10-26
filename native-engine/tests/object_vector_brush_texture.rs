//! G16 (#2361): the baseline `serP` brush-texture members (tests/native-object-vector-fixtures/
//! brush-texture-oracle.json, fbe4b8a3 run verbatim) are stored in `vectorExt.brushTexture` exactly as
//! serialized, round-trip through both native codecs, and refuse what serP can never write.
use super::{brush, decode, decoded, kind_of, put, record_doc};
use crate::object_codec::{encode_project, ObjectCodecErrorKind};
use serde_json::{json, Value};

fn tagged(tags: Value) -> Value {
    put(brush(), "/vectorExt", json!({"brushTexture":tags}))
}
fn saved(record: Value) -> Value {
    let bytes = encode_project(&decoded(record)).unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["objects"][0].clone()
}

#[test]
fn anchor_dab_and_lone_group_members_round_trip_exactly_as_serp_writes_them() {
    for tags in [
        // anchor-no-fill / anchor-opacity-zero: preTextureOpacity whenever defined, including 0.
        json!({"brushGroupId":"bg1","brushTexturePreset":"charcoal-smooth","preTextureOpacity":1}),
        json!({"brushGroupId":"g","brushTexturePreset":"p","preTextureOpacity":0}),
        // anchor-fill: the hidden stroke verbatim (#rrggbbaa) or an explicit null.
        json!({"brushGroupId":"bg2","brushTexturePreset":"ink-dry","preTextureStroke":"#112233ff"}),
        json!({"brushGroupId":"bg3","brushTexturePreset":"ink-dry","preTextureStroke":null}),
        // dab, and a group id alone (serP keeps it without a preset or copy flag).
        json!({"brushGroupId":"bg1","isBrushTextureCopy":true}),
        json!({"brushGroupId":"lone"}),
    ] {
        let record = tagged(tags.clone());
        assert_eq!(saved(record.clone())["vectorExt"]["brushTexture"], tags);
        assert_eq!(saved(saved(record)), saved(tagged(tags.clone())), "{tags}");
    }
}

#[test]
fn members_serp_never_writes_and_a_record_that_is_anchor_and_dab_are_refused() {
    for tags in [
        json!({}),
        json!({"brushGroupId":""}),
        json!({"brushTexturePreset":""}),
        json!({"isBrushTextureCopy":false}),
        json!({"brushGroupId":"g","isBrushTextureCopy":true,"brushTexturePreset":"p"}),
        json!({"brushGroupId":"a\u{0}b"}),
        json!({"brushGroupId":"x".repeat(256)}),
        json!({"preTextureStroke":"red"}),
        json!({"preTextureStroke":"#12345"}),
        json!({"preTextureOpacity":"1"}),
        json!({"brushGroupId":"g","unknown":1}),
        json!({"brushGroupId":null}),
    ] {
        let kind = kind_of(&record_doc(vec![tagged(tags.clone())]));
        assert!(matches!(kind, ObjectCodecErrorKind::Invalid | ObjectCodecErrorKind::Unsupported), "{tags}: {kind:?}");
    }
    assert!(decode(&record_doc(vec![tagged(json!({"brushGroupId":"g"}))])).is_ok(), "control");
}
