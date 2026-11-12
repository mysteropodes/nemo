//! Shadow Brush (#2361): baseline `serP` writes a Shadow Brush stroke's `channelTag` (only ever `'shadow'`) and the
//! swatch that drew it (`shadowSwatchId`) when truthy. Native stores them in `vectorExt.shadow` exactly as serialized,
//! round-trips them through both codecs, and refuses what serP can never write.
use super::{brush, decode, decoded, kind_of, put, record_doc};
use crate::object_codec::{encode_project, ObjectCodecErrorKind};
use serde_json::{json, Value};

fn tagged(shadow: Value) -> Value {
    put(brush(), "/vectorExt", json!({"shadow":shadow}))
}
fn saved(record: Value) -> Value {
    let bytes = encode_project(&decoded(record)).unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["objects"][0].clone()
}

#[test]
fn a_shadow_stroke_round_trips_its_channel_and_swatch_exactly_as_serp_writes_them() {
    for shadow in [
        json!({"channelTag":"shadow","shadowSwatchId":"sh3"}),
        json!({"channelTag":"shadow"}),
    ] {
        let record = tagged(shadow.clone());
        assert_eq!(saved(record.clone())["vectorExt"]["shadow"], shadow);
        assert_eq!(
            saved(saved(record)),
            saved(tagged(shadow.clone())),
            "{shadow}"
        );
    }
}

#[test]
fn channel_members_serp_never_writes_are_refused() {
    for shadow in [
        json!({}),
        json!({"channelTag":"fill"}),
        json!({"channelTag":""}),
        json!({"shadowSwatchId":"sh1"}),
        json!({"channelTag":"shadow","shadowSwatchId":""}),
        json!({"channelTag":"shadow","shadowSwatchId":"x".repeat(257)}),
        json!({"channelTag":"shadow","shadowSwatchId":null}),
        json!({"channelTag":"shadow","extra":1}),
    ] {
        let kind = kind_of(&record_doc(vec![tagged(shadow.clone())]));
        assert!(
            matches!(
                kind,
                ObjectCodecErrorKind::Invalid | ObjectCodecErrorKind::Unsupported
            ),
            "{shadow}: {kind:?}"
        );
    }
    assert!(
        decode(&record_doc(vec![tagged(json!({"channelTag":"shadow"}))])).is_ok(),
        "control"
    );
}
