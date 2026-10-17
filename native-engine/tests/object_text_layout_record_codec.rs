//! Strict record codec of the text block (G38a / L06-F): round trips,
//! absent / null / default / explicit and the mapping onto the L06-A layout
//! request. Refusals and capture from root data are in the sibling test files.
use super::oracle::{catalog, minimal, with, FULL, MINIMAL};
use super::TextBlockRecord;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    TextAlign, TextCase, TextLayoutError, TextStyle,
};
use serde_json::{json, Value};

#[test]
fn canonical_text_round_trips_byte_for_byte() {
    for text in [MINIMAL, FULL] {
        let record = TextBlockRecord::decode(text).expect(text);
        assert_eq!(record.encode().unwrap(), text);
        assert_eq!(
            TextBlockRecord::decode(&record.encode().unwrap()).unwrap(),
            record
        );
    }
    let via_serde: TextBlockRecord = serde_json::from_str(FULL).unwrap();
    assert_eq!(via_serde, TextBlockRecord::decode(FULL).unwrap());
    assert_eq!(serde_json::to_string(&via_serde).unwrap(), FULL);
}

#[test]
fn absent_null_default_and_explicit_stay_distinct() {
    let bare = TextBlockRecord::decode(MINIMAL).unwrap();
    assert_eq!(bare.fixed_width_authored(), None);
    assert_eq!(bare.fixed_height_authored(), None);
    assert_eq!(bare.letter_spacing_authored(), None);
    assert_eq!(bare.word_spacing_authored(), None);
    assert_eq!(bare.line_height_mult_authored(), None);
    // Defaults exist only at evaluation: the multiplier stays None for the kernel.
    assert_eq!(
        bare.style(),
        TextStyle {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            line_height_mult: None,
            text_case: TextCase::Unchanged,
        }
    );
    assert_eq!(bare.encode().unwrap(), MINIMAL);

    let mut value = minimal();
    for (member, spelled) in [
        ("fixedWidth", Value::Null),
        ("fixedHeight", Value::Null),
        ("bold", json!(false)),
        ("italic", json!(false)),
        ("underline", json!(false)),
        ("strike", json!(false)),
        ("letterSpacing", json!(0)),
        ("wordSpacing", json!(0)),
        ("lineHeightMult", json!(1.25)),
        ("textCase", json!("none")),
    ] {
        value[member] = spelled;
    }
    let explicit = TextBlockRecord::decode(&value.to_string()).unwrap();
    assert_ne!(bare, explicit);
    assert_eq!(explicit.fixed_width_authored(), Some(None));
    assert_eq!(explicit.fixed_height_authored(), Some(None));
    assert_eq!(explicit.letter_spacing_authored(), Some(0.0));
    assert_eq!(explicit.word_spacing_authored(), Some(0.0));
    assert_eq!(explicit.line_height_mult_authored(), Some(1.25));
    let text = explicit.encode().unwrap();
    for piece in [
        r#""fixedWidth":null,"fixedHeight":null,"#,
        r#""bold":false,"italic":false,"underline":false,"strike":false,"#,
        r#""letterSpacing":0,"wordSpacing":0,"lineHeightMult":1.25,"textCase":"none","#,
    ] {
        assert!(text.contains(piece), "{piece}");
    }
    // The three states of a box extent are three different values.
    let absent = TextBlockRecord::decode(&with("size", json!(48))).unwrap();
    let null = TextBlockRecord::decode(&with("fixedWidth", Value::Null)).unwrap();
    let set = TextBlockRecord::decode(&with("fixedWidth", json!(300))).unwrap();
    assert_ne!(absent, null);
    assert_ne!(null, set);
    assert_ne!(absent, set);
    assert_eq!(
        (
            absent.fixed_width_authored(),
            null.fixed_width_authored(),
            set.fixed_width_authored()
        ),
        (None, Some(None), Some(Some(300.0)))
    );
}

#[test]
fn number_spelling_class_is_preserved() {
    let int = TextBlockRecord::decode(&with("size", json!(17))).unwrap();
    let float = TextBlockRecord::decode(&with("size", json!(17.0))).unwrap();
    assert_ne!(int, float);
    assert!(int.encode().unwrap().contains(r#""size":17,"#));
    assert!(float.encode().unwrap().contains(r#""size":17.0,"#));
    assert_eq!(int.size(), float.size());
}

#[test]
fn layout_request_carries_every_member() {
    let bare = TextBlockRecord::decode(MINIMAL).unwrap();
    let request = bare.layout_request("blk");
    assert_eq!(request.holder_id, "blk");
    assert_eq!(request.text, "Hi");
    assert_eq!(request.font_key, "Roboto-Regular");
    assert_eq!(request.size, 48.0);
    assert_eq!(request.align, TextAlign::Left);
    assert_eq!((request.fixed_width, request.fixed_height), (None, None));
    assert_eq!(request.top_left, [10.0, 20.0]);
    assert_eq!(request.style, bare.style());

    let full = TextBlockRecord::decode(FULL).unwrap();
    let request = full.layout_request("blk2");
    assert_eq!(request.holder_id, "blk2");
    assert_eq!(request.text, "Hello\nworld");
    assert_eq!(request.font_key, "font-1");
    assert_eq!(request.size, 17.5);
    assert_eq!(request.align, TextAlign::Center);
    // fixedHeight is an explicit null: no box, exactly like an absent one.
    assert_eq!(
        (request.fixed_width, request.fixed_height),
        (Some(300.0), None)
    );
    assert_eq!(request.top_left, [-12.75, 104.5]);
    assert_eq!(
        request.style,
        TextStyle {
            bold: true,
            italic: false,
            underline: true,
            strike: false,
            letter_spacing: -2.5,
            word_spacing: 0.0,
            line_height_mult: Some(1.25),
            text_case: TextCase::Upper,
        }
    );
    assert_eq!(full.color(), "#ff00aa80");
    assert_eq!(full.text(), "Hello\nworld");
    assert_eq!(full.anchor(), [-12.75, 104.5]);
}

#[test]
fn a_missing_font_is_a_typed_unavailability_never_a_fallback() {
    let bundled = TextBlockRecord::decode(MINIMAL).unwrap();
    assert_eq!(
        layout_text(&bundled.layout_request("blk"), None).unwrap_err(),
        TextLayoutError::FontUnavailable
    );
    // An asset font the catalog does not retain names the asset, and nothing else is tried.
    let asset = TextBlockRecord::decode(FULL).unwrap();
    let catalog = catalog();
    assert_eq!(
        layout_text(&asset.layout_request("blk"), Some(&catalog)).unwrap_err(),
        TextLayoutError::FontFaceUnavailable("font-1".to_string())
    );
}
