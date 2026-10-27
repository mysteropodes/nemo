//! A Fill Brush shape: baseline `serP` writes `isFillShape: true` (only when truthy) on the shapes the Fill Brush
//! commits. Native stores it in `vectorExt.isFillShape`, round-trips it through both codecs, and refuses what serP
//! can never write.
use super::{brush, decode, decoded, kind_of, put, record_doc};
use crate::object_codec::{encode_project, ObjectCodecErrorKind};
use serde_json::{json, Value};

fn tagged(flag: Value) -> Value {
    put(brush(), "/vectorExt", json!({ "isFillShape": flag }))
}

#[test]
fn a_fill_brush_shape_round_trips_its_flag_and_refuses_what_serp_never_writes() {
    let bytes = encode_project(&decoded(tagged(json!(true)))).unwrap();
    let saved = serde_json::from_slice::<Value>(&bytes).unwrap()["objects"][0].clone();
    assert_eq!(saved["vectorExt"], json!({"isFillShape": true}));
    for flag in [json!(false), json!("true"), json!(null), json!(1)] {
        let kind = kind_of(&record_doc(vec![tagged(flag.clone())]));
        assert!(
            matches!(
                kind,
                ObjectCodecErrorKind::Invalid | ObjectCodecErrorKind::Unsupported
            ),
            "{flag}: {kind:?}"
        );
    }
    assert!(
        decode(&record_doc(vec![tagged(json!(true))])).is_ok(),
        "control"
    );
}
