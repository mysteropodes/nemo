use super::*;
use serde_json::{json, Value};
impl BitmapBrushFields {
    pub(crate) fn test_value() -> Value {
        json!({"bitmapBrushSpec":{"tip":"hard","size":8,"spacing":15,"scatter":0,"opacity":1,
            "color":"#0000ffff","pressure":true,"seed":17},"brushGroupId":"ink group:雪",
            "preTextureStroke":null,"bitmapPressureProfile":[[4,4,0.2],[40,4,2.5]]})
    }
    pub(crate) fn test_fields() -> Self {
        serde_json::from_value(Self::test_value()).unwrap()
    }
    /// Real source triangle, not a replacement contour supplied by a test.
    pub(crate) fn test_drawing() -> Value {
        let point = |x, y| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
        json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":12,
            "layers":[{"layerUid":"L"}],"objects":[{"schemaVersion":1,"family":"editable-vector-path",
                "target":{"contextId":"scene-root","layerUid":"L","strokeId":"anchor","frameScope":{"kind":"authored","frame":0}},
                "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
                    "segments":[point(4,4),point(40,4),point(4,40)]},
                "fill":{"kind":"solid","r":0,"g":1,"b":0,"a":1},
                "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":8,"strokeCap":"round","strokeJoin":"round",
                    "miterLimit":10,"dashOffset":0,"opacity":1},"vectorExt":{"bitmapBrush":Self::test_value()}}],
            "items":{"raster":[{"target":{"contextId":"scene-root","layerUid":"L","strokeId":"companion",
                "frameScope":{"kind":"authored","frame":0}},"item":{"assetId":"ink","x":16,"y":16,"width":8,"height":8,
                "opacity":1,"isBitmapBrush":true,"isBrushTextureCopy":true,"brushGroupId":"ink group:雪"}}]},
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],
                    "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                        {"layerUid":"L","strokeId":"anchor","naturalPosition":0,"order":{}},
                        {"layerUid":"L","strokeId":"companion","naturalPosition":1,"order":{}}]}]}]}})
    }
}
#[test]
fn bitmap_fields_roundtrip_original_numbers_profile_order_and_remembered_null() {
    let wire = BitmapBrushFields::test_value();
    let fields: BitmapBrushFields = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&fields).unwrap(), wire);
    assert_eq!(fields.pre_texture_stroke, Presence::Present(None));
    assert!(fields.pre_texture_opacity.missing());
    assert_eq!(
        fields.bitmap_pressure_profile.value().unwrap()[0][2].as_f64(),
        Some(0.2)
    );
    assert_eq!(
        fields.bitmap_pressure_profile.value().unwrap()[1][2].as_f64(),
        Some(2.5)
    );
}
#[test]
fn missing_empty_and_explicit_values_remain_distinct_without_companion_authority() {
    let mut wire = BitmapBrushFields::test_value();
    wire.as_object_mut()
        .unwrap()
        .remove("bitmapPressureProfile");
    let missing: BitmapBrushFields = serde_json::from_value(wire.clone()).unwrap();
    assert!(missing.bitmap_pressure_profile.missing());
    wire["bitmapPressureProfile"] = json!([]);
    let empty: BitmapBrushFields = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(empty.bitmap_pressure_profile.value().unwrap().len(), 0);
    assert_ne!(missing, empty);
    wire["preTextureOpacity"] = json!(0);
    let zero: BitmapBrushFields = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(zero.pre_texture_opacity.value().unwrap().as_i64(), Some(0));
    wire["assetId"] = json!("second authority");
    assert!(serde_json::from_value::<BitmapBrushFields>(wire).is_err());
}
#[test]
fn original_byte_maps_reject_duplicate_nested_spec_and_array_substitutions() {
    let source = BitmapBrushFields::test_value().to_string();
    for token in ["\"size\":8", "\"preTextureStroke\":null", "\"seed\":17"] {
        assert!(source.contains(token));
        assert!(serde_json::from_str::<BitmapBrushFields>(&source.replacen(
            token,
            &format!("{token},{token}"),
            1
        ))
        .is_err());
    }
    for wire in [
        json!([]),
        json!({"bitmapBrushSpec":[],"brushGroupId":"g"}),
        json!({"bitmapBrushSpec":BitmapBrushFields::test_value()["bitmapBrushSpec"],"brushGroupId":"g","bitmapPressureProfile":[[0,0,1,2]]}),
    ] {
        assert!(serde_json::from_value::<BitmapBrushFields>(wire).is_err());
    }
}
