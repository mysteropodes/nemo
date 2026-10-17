//! Typed refusals of the strict text block decoder (G38a / L06-F): structure,
//! text, values and the font object, each with the exact error it must give.
use super::font::FontRefError;
use super::oracle::{minimal, with, MINIMAL};
use super::{TextBlockRecord, TextRecordError};
use serde_json::{json, Value};

fn refused(text: &str) -> TextRecordError {
    TextBlockRecord::decode(text).expect_err(text)
}

#[test]
fn structure_refusals_are_typed_malformed() {
    let mut cases: Vec<String> = vec![
        String::new(),
        "[]".into(),
        "null".into(),
        format!("{MINIMAL}x"),
        format!("{MINIMAL}{MINIMAL}"),
        r##"["Hi",1]"##.into(),
        MINIMAL.replace(r#""text":"Hi","#, r#""text":"Hi","text":"Ho","#),
        MINIMAL.replace(r#""x":10,"#, r#""x":10,"x":11,"#),
        MINIMAL.replace(r#""size":48,"#, r#""size":48,"size":49,"#),
        with("textOverflow", json!(false)),
        with("isText", json!(true)),
        with("isTextRoot", json!(true)),
        with("vectorFont", json!("Roboto-Regular")),
        with("textSize", json!(48)),
        with("groupId", json!("g1")),
        with("charIndex", json!(0)),
        with("size", json!("48")),
        with("text", json!(5)),
        with("align", json!(1)),
        with("bold", json!(1)),
        with("bold", Value::Null),
        with("italic", Value::Null),
        with("letterSpacing", Value::Null),
        with("wordSpacing", Value::Null),
        with("lineHeightMult", Value::Null),
        with("textCase", Value::Null),
        with("fixedWidth", json!("300")),
        with("fixedHeight", json!(true)),
        with("anchorTopLeft", json!([10, 20])),
        with("anchorTopLeft", json!({"x": 10})),
        with("anchorTopLeft", json!({"x": 10, "y": 20, "z": 0})),
        with("anchorTopLeft", json!({"x": null, "y": 20})),
        with("font", json!("Roboto-Regular")),
        with("font", json!(["bundled", "Roboto-Regular"])),
        // A lone surrogate half cannot be spelled in a Rust string: the JSON decoder refuses it.
        r##"{"text":"\ud800","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}"##.into(),
    ];
    for required in ["text", "font", "size", "color", "align", "anchorTopLeft"] {
        let mut value = minimal();
        value.as_object_mut().unwrap().remove(required);
        cases.push(value.to_string());
    }
    for text in &cases {
        assert_eq!(refused(text), TextRecordError::Malformed, "{text}");
    }
}

#[test]
fn text_refusals_are_typed() {
    let nul = TextBlockRecord::decode(&with("text", json!("a\u{0}b"))).expect_err("nul");
    assert_eq!(nul, TextRecordError::TextContainsNul);
    assert_eq!(
        refused(&with("text", json!(""))),
        TextRecordError::EmptyText
    );
    for blank in [
        " ", "\n", " \t\r\n ", "\u{a0}", "\u{3000}", "\u{feff}", "\u{2028}",
    ] {
        assert_eq!(
            refused(&with("text", json!(blank))),
            TextRecordError::NoVisibleUnit,
            "{blank:?}"
        );
    }
    // A zero-width space is not trimmed by JS `trim()`, so it counts as a unit with a glyph.
    assert!(TextBlockRecord::decode(&with("text", json!("\u{200b}"))).is_ok());
    // Whitespace around ink is fine, and so is a newline-only prefix.
    assert!(TextBlockRecord::decode(&with("text", json!("\n\n a"))).is_ok());
}

#[test]
fn text_length_bound_counts_utf16_units() {
    let at_bound = "a".repeat(65_536);
    let over = "a".repeat(65_537);
    assert!(TextBlockRecord::decode(&with("text", json!(at_bound))).is_ok());
    assert_eq!(
        refused(&with("text", json!(over))),
        TextRecordError::TextTooLong
    );
    // One emoji is two UTF-16 units: 32_768 fit, 32_769 do not.
    let emoji_fit = "\u{1f600}".repeat(32_768);
    let emoji_over = "\u{1f600}".repeat(32_769);
    assert!(TextBlockRecord::decode(&with("text", json!(emoji_fit))).is_ok());
    assert_eq!(
        refused(&with("text", json!(emoji_over))),
        TextRecordError::TextTooLong
    );
}

#[test]
fn value_refusals_are_typed() {
    let table: [(&str, Value, TextRecordError); 20] = [
        ("size", json!(0), TextRecordError::Size),
        ("size", json!(-1), TextRecordError::Size),
        ("size", json!(-0.5), TextRecordError::Size),
        ("color", json!("#fff"), TextRecordError::Color),
        ("color", json!("red"), TextRecordError::Color),
        ("color", json!("112233"), TextRecordError::Color),
        ("color", json!("#12345g"), TextRecordError::Color),
        ("color", json!("#1122334"), TextRecordError::Color),
        ("color", json!(""), TextRecordError::Color),
        ("align", json!("justify"), TextRecordError::Align),
        ("align", json!("Left"), TextRecordError::Align),
        ("align", json!(""), TextRecordError::Align),
        ("fixedWidth", json!(0), TextRecordError::FixedWidth),
        ("fixedWidth", json!(-5), TextRecordError::FixedWidth),
        ("fixedHeight", json!(0), TextRecordError::FixedHeight),
        ("fixedHeight", json!(-0.25), TextRecordError::FixedHeight),
        ("lineHeightMult", json!(0), TextRecordError::LineHeightMult),
        (
            "lineHeightMult",
            json!(-1.25),
            TextRecordError::LineHeightMult,
        ),
        ("textCase", json!("Upper"), TextRecordError::TextCase),
        ("textCase", json!(""), TextRecordError::TextCase),
    ];
    for (member, value, error) in table {
        assert_eq!(
            refused(&with(member, value.clone())),
            error,
            "{member}={value}"
        );
    }
    for ok in [
        ("color", json!("#ABCDEF")),
        ("color", json!("#abcdef12")),
        ("align", json!("right")),
        ("letterSpacing", json!(-400)),
        ("wordSpacing", json!(-0.5)),
        ("textCase", json!("capitalize")),
        ("textCase", json!("lower")),
        ("size", json!(0.001)),
    ] {
        assert!(
            TextBlockRecord::decode(&with(ok.0, ok.1.clone())).is_ok(),
            "{}={}",
            ok.0,
            ok.1
        );
    }
}

#[test]
fn font_object_refusals_are_typed() {
    let table = [
        (
            json!({"kind": "bundled", "fontKey": "Nope-Regular"}),
            FontRefError::UnknownKey,
        ),
        (
            json!({"kind": "bundled", "fontKey": "vector:Roboto-Regular"}),
            FontRefError::UnknownKey,
        ),
        (
            json!({"kind": "bundled", "fontKey": "Google:Lato-Regular"}),
            FontRefError::UnknownKey,
        ),
        (json!({"kind": "system"}), FontRefError::Kind),
        (
            json!({"kind": "Bundled", "fontKey": "Roboto-Regular"}),
            FontRefError::Kind,
        ),
        (json!({"kind": "bundled"}), FontRefError::Members),
        (json!({"kind": "asset"}), FontRefError::Members),
        (
            json!({"kind": "asset", "fontKey": "Roboto-Regular"}),
            FontRefError::Members,
        ),
        (
            json!({"kind": "bundled", "fontKey": "Roboto-Regular", "assetId": "f"}),
            FontRefError::Members,
        ),
        (
            json!({"kind": "asset", "assetId": "f", "fontKey": "Roboto-Regular"}),
            FontRefError::Members,
        ),
        (
            json!({"kind": "asset", "assetId": ""}),
            FontRefError::AssetId,
        ),
        (
            json!({"kind": "asset", "assetId": "   "}),
            FontRefError::AssetId,
        ),
        (
            json!({"kind": "asset", "assetId": "a\u{0}b"}),
            FontRefError::AssetId,
        ),
    ];
    for (font, error) in table {
        assert_eq!(
            refused(&with("font", font.clone())),
            TextRecordError::Font(error),
            "{font}"
        );
    }
    // Unknown members of the font object are structure errors, not font errors.
    let extra = json!({"kind": "bundled", "fontKey": "Roboto-Regular", "label": "x"});
    assert_eq!(refused(&with("font", extra)), TextRecordError::Malformed);
}

#[test]
fn serde_decode_reports_the_typed_refusal_text() {
    let err = serde_json::from_str::<TextBlockRecord>(&with("text", json!(""))).unwrap_err();
    assert!(err.to_string().contains("EmptyText"), "{err}");
    let err = serde_json::from_str::<TextBlockRecord>(&with("size", json!(0))).unwrap_err();
    assert!(err.to_string().contains("Size"), "{err}");
}
