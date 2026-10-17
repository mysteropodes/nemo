//! Oracle-driven tests of the text block record (G38a / L06-F). Every expected
//! value below is either read from the baseline oracle rows (the REAL
//! `buildVectorTextGroup` / `serP` / `desP` of fbe4b8a3) or is a literal pinned
//! next to the row it comes from.
use super::oracle::{admitted_cases, baseline_glyphs, case, cases, layout_of, native_glyphs, rows};
use super::pin::{pins_of, GlyphPin};
use super::{TextBlockRecord, TextRecordError};
use serde_json::{json, Value};

const MEMBERS: [&str; 16] = [
    "text",
    "font",
    "size",
    "color",
    "align",
    "fixedWidth",
    "fixedHeight",
    "bold",
    "italic",
    "underline",
    "strike",
    "letterSpacing",
    "wordSpacing",
    "lineHeightMult",
    "textCase",
    "anchorTopLeft",
];

fn record_of(row: &Value) -> TextBlockRecord {
    TextBlockRecord::capture_root_data(&row["rootData"]).expect("admitted root")
}

/// Top-level member names of a canonical record, in the order they are written
/// (a parsed `Value` map is sorted, so the order is read back from the text).
fn keys_of(text: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(text).expect("record json");
    let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
    keys.sort_by_key(|key| text.find(&format!("\"{key}\":")).expect("member in text"));
    keys
}

#[test]
fn every_oracle_root_captures_to_the_expected_canonical_record() {
    let admitted = admitted_cases();
    assert_eq!(admitted.len(), 32);
    for row in admitted {
        let name = row["name"].as_str().expect("name");
        let record = record_of(&row);
        let expected = row["record"].as_str().expect("record text");
        assert_eq!(record.encode().unwrap(), expected, "{name}");
        let decoded = TextBlockRecord::decode(expected).expect(name);
        assert_eq!(decoded, record, "{name}: decode(encode(capture))");
        assert_eq!(decoded.encode().unwrap(), expected, "{name}: byte stable");
    }
}

#[test]
fn record_members_are_exactly_the_baseline_root_metadata() {
    let keys = rows("KEYS");
    assert_eq!(keys.len(), 1);
    let root_keys: Vec<&str> = keys[0]["rootKeys"]
        .as_array()
        .expect("rootKeys")
        .iter()
        .map(|k| k.as_str().expect("key"))
        .collect();
    // What buildVectorTextGroup really assigned, in order (pinned literal).
    assert_eq!(
        root_keys,
        [
            "isText",
            "isTextRoot",
            "text",
            "vectorFont",
            "size",
            "color",
            "align",
            "fixedWidth",
            "fixedHeight",
            "textOverflow",
            "bold",
            "italic",
            "underline",
            "strike",
            "letterSpacing",
            "wordSpacing",
            "lineHeightMult",
            "textCase",
            "anchorTopLeft"
        ]
    );
    // Type tags and derived output are not members; the font key is renamed.
    let expected: Vec<&str> = root_keys
        .iter()
        .filter(|k| !["isText", "isTextRoot", "textOverflow"].contains(*k))
        .map(|k| if *k == "vectorFont" { "font" } else { *k })
        .collect();
    assert_eq!(expected, MEMBERS);
    // A freshly built root writes every member, so its record has them all, in order.
    let full = record_of(&case("default_plain")).encode().unwrap();
    assert_eq!(keys_of(&full), MEMBERS);
    // The per-glyph keys never belong to the record.
    let glyph_keys: Vec<&str> = keys[0]["glyphKeys"]
        .as_array()
        .expect("glyphKeys")
        .iter()
        .map(|k| k.as_str().expect("key"))
        .collect();
    for glyph_key in &glyph_keys {
        assert!(!full.contains(glyph_key), "{glyph_key}");
    }
    assert!(!full.contains("textOverflow"));
}

#[test]
fn refused_roots_get_their_pinned_typed_refusal() {
    let expected: [(&str, TextRecordError); 3] = [
        ("align_justify_refused", TextRecordError::Align),
        (
            "align_undefined_refused",
            TextRecordError::RootField("align"),
        ),
        ("case_unknown_refused", TextRecordError::TextCase),
    ];
    let refused: Vec<Value> = cases()
        .into_iter()
        .filter(|r| r["refused"] == true)
        .collect();
    assert_eq!(refused.len(), expected.len());
    for (name, error) in expected {
        let row = case(name);
        assert_eq!(row["refused"], true, "{name}");
        assert_eq!(
            TextBlockRecord::capture_root_data(&row["rootData"]).expect_err(name),
            error,
            "{name}"
        );
    }
}

#[test]
fn texts_the_baseline_never_builds_a_root_for_are_refused() {
    let expected = [
        ("empty_text_no_root", TextRecordError::EmptyText),
        ("blank_text_no_root", TextRecordError::NoVisibleUnit),
    ];
    for (name, error) in expected {
        let row = case(name);
        assert_eq!(row["noRoot"], true, "{name}");
        assert!(row["rootData"].is_null(), "{name}");
        let text = row["args"]["text"].as_str().expect("text");
        let record = json!({
            "text": text,
            "font": {"kind": "bundled", "fontKey": "Roboto-Regular"},
            "size": 48, "color": "#112233", "align": "left",
            "anchorTopLeft": {"x": 10, "y": 20}
        });
        assert_eq!(
            TextBlockRecord::decode(&record.to_string()).expect_err(name),
            error,
            "{name}"
        );
    }
}

#[test]
fn the_record_lays_out_exactly_as_the_baseline_built_the_glyphs() {
    for row in admitted_cases() {
        let name = row["name"].as_str().expect("name");
        let layout = layout_of(&row, "blk");
        let baseline = &row["layout"];
        assert_eq!(
            layout.width,
            baseline["width"].as_f64().unwrap(),
            "{name}: width"
        );
        assert_eq!(
            layout.height,
            baseline["height"].as_f64().unwrap(),
            "{name}: height"
        );
        // textOverflow is derived: the kernel's flag equals the one the baseline stored.
        assert_eq!(layout.overflow, baseline["overflow"], "{name}: overflow");
        assert_eq!(
            layout.overflow, row["rootData"]["textOverflow"],
            "{name}: root flag"
        );
        assert_eq!(
            native_glyphs(&layout),
            baseline_glyphs(&row),
            "{name}: glyphs"
        );
    }
}

#[test]
fn glyph_pins_name_the_baseline_char_indices() {
    for row in admitted_cases() {
        let name = row["name"].as_str().expect("name");
        let layout = layout_of(&row, "blk");
        let pins = pins_of(&layout);
        let glyphs = baseline_glyphs(&row);
        assert_eq!(pins.len(), glyphs.len(), "{name}");
        for (pin, glyph) in pins.iter().zip(glyphs.iter()) {
            // glyph[4] is the baseline charIndex of the real Path data.
            assert_eq!(
                pin.key(),
                format!("blk/glyph/{}", glyph[4] as usize),
                "{name}"
            );
            let placed = pin.resolve(&layout).expect(name);
            assert_eq!(f64::from(placed.unit), glyph[0], "{name}");
            assert_eq!(placed.word_index as f64, glyph[5], "{name}");
            assert_eq!(placed.line_index as f64, glyph[6], "{name}");
            assert_eq!(GlyphPin::parse(&pin.key()).unwrap(), *pin, "{name}");
        }
    }
}

#[test]
fn baseline_persistence_collapses_what_the_record_keeps() {
    // serP writes only truthy members, never fixedHeight or textOverflow; desP
    // always rebuilds lineHeightMult. The record keeps presence as authored.
    let all = [
        "isTextRoot",
        "text",
        "vectorFont",
        "textSize",
        "textColor",
        "textAlign",
        "textFixedWidth",
        "anchorTopLeft",
        "bold",
        "italic",
        "underline",
        "strike",
        "letterSpacing",
        "wordSpacing",
        "lineHeightMult",
        "textCase",
    ];
    let mut seen: Vec<String> = Vec::new();
    for row in admitted_cases() {
        for key in row["persisted"].as_object().expect("persisted").keys() {
            assert!(all.contains(&key.as_str()), "unexpected persisted {key}");
            if !seen.contains(key) {
                seen.push(key.clone());
            }
        }
        assert!(row["persisted"].get("fixedHeight").is_none());
        assert!(row["persisted"].get("textFixedHeight").is_none());
        assert!(row["persisted"].get("textOverflow").is_none());
        assert_eq!(row["restored"]["lineHeightMult"].is_number(), true);
    }
    for key in all {
        assert!(seen.iter().any(|s| s == key), "oracle never persists {key}");
    }

    let area = case("area_overflow");
    assert_eq!(area["rootData"]["fixedHeight"], 60);
    assert_eq!(area["rootData"]["textOverflow"], true);
    assert!(record_of(&area)
        .encode()
        .unwrap()
        .contains(r#""fixedHeight":60,"#));

    let zero = case("spacing_explicit_zero");
    assert_eq!(zero["rootData"]["letterSpacing"], 0);
    assert!(zero["persisted"].get("letterSpacing").is_none());
    assert!(zero["restored"].get("letterSpacing").is_none());
    let encoded = record_of(&zero).encode().unwrap();
    assert!(encoded.contains(r#""letterSpacing":0,"wordSpacing":0,"#));

    let default_mult = case("default_plain");
    assert_eq!(default_mult["rootData"]["lineHeightMult"], 1.25);
    assert!(default_mult["persisted"].get("lineHeightMult").is_none());
    assert_eq!(default_mult["restored"]["lineHeightMult"], 1.25);
    let record = record_of(&default_mult);
    assert_eq!(record.line_height_mult_authored(), Some(1.25));
    assert_eq!(record.fixed_width_authored(), Some(None));
}

#[test]
fn zero_boxes_are_stored_as_explicit_null_by_the_baseline_build() {
    for name in [
        "box_width_zero_stored_null",
        "box_width_undefined_stored_null",
    ] {
        let row = case(name);
        assert!(row["rootData"]["fixedWidth"].is_null(), "{name}");
        let record = record_of(&row);
        assert_eq!(record.fixed_width_authored(), Some(None), "{name}");
        assert!(
            row["record"]
                .as_str()
                .unwrap()
                .contains(r#""fixedWidth":null"#),
            "{name}"
        );
    }
    let zero_height = case("area_height_zero_stored_null");
    assert!(zero_height["rootData"]["fixedHeight"].is_null());
    assert_eq!(record_of(&zero_height).fixed_height_authored(), Some(None));
    assert_eq!(
        record_of(&zero_height).fixed_width_authored(),
        Some(Some(40.0))
    );
}

#[test]
fn oracle_has_not_been_hollowed_out() {
    assert_eq!(rows("FACE").len(), 8);
    assert_eq!(rows("FONTS").len(), 1);
    assert_eq!(rows("KEYS").len(), 1);
    let all = cases();
    assert_eq!(all.len(), 37);
    assert_eq!(all.iter().filter(|r| r["noRoot"] == true).count(), 2);
    assert_eq!(all.iter().filter(|r| r["refused"] == true).count(), 3);
    let admitted = admitted_cases();
    let fonts: std::collections::BTreeSet<&str> = admitted
        .iter()
        .map(|r| r["rootData"]["vectorFont"].as_str().unwrap())
        .collect();
    assert_eq!(fonts.len(), 5);
    assert_eq!(
        admitted
            .iter()
            .filter(|r| r["layout"]["overflow"] == true)
            .count(),
        2
    );
    assert!(admitted.iter().any(|r| r["rootData"]["bold"] == true));
    assert!(admitted
        .iter()
        .any(|r| r["rootData"]["fixedWidth"].is_number()));
    assert!(admitted
        .iter()
        .any(|r| r["rootData"]["fixedWidth"].is_null()));
    let glyphs: usize = admitted.iter().map(|r| baseline_glyphs(r).len()).sum();
    assert_eq!(glyphs, 311);
}
