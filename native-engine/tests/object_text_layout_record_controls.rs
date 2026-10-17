//! Negative and mutation controls of the text block record (G38a / L06-F).
//! Each control builds a FLAWED copy of what the record guarantees and shows
//! that the oracle comparison, the strict decoder or a named equality rejects it.
use super::oracle::{baseline_glyphs, case, catalog, native_glyphs};
use super::{TextBlockRecord, TextRecordError};
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::TextCase;
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

fn expected_text(name: &str) -> String {
    case(name)["record"]
        .as_str()
        .expect("record text")
        .to_string()
}

fn expected_value(name: &str) -> Value {
    serde_json::from_str(&expected_text(name)).expect("record json")
}

#[test]
fn renaming_a_member_is_refused_and_would_fail_the_oracle() {
    let canonical = expected_text("default_plain");
    assert!(TextBlockRecord::decode(&canonical).is_ok());
    for member in MEMBERS {
        let renamed = canonical.replace(&format!("\"{member}\":"), &format!("\"{member}_\":"));
        assert_ne!(
            renamed, canonical,
            "{member} is spelled in the canonical text"
        );
        assert_eq!(
            TextBlockRecord::decode(&renamed).unwrap_err(),
            TextRecordError::Malformed,
            "{member}"
        );
    }
    // The same flaw on the ENCODE side: a record that wrote a snake_case member would
    // differ from the baseline-derived canonical text.
    let record = TextBlockRecord::capture_root_data(&case("default_plain")["rootData"]).unwrap();
    let mut value = serde_json::to_value(&record).unwrap();
    let anchor = value
        .as_object_mut()
        .unwrap()
        .remove("anchorTopLeft")
        .unwrap();
    value["anchor_top_left"] = anchor;
    assert_ne!(value, expected_value("default_plain"));
}

#[test]
fn dropping_anchor_top_left_is_refused_on_decode_and_capture() {
    let mut value = expected_value("anchor_negative_fraction");
    assert_eq!(value["anchorTopLeft"], json!({"x": -12.75, "y": 104.5}));
    value.as_object_mut().unwrap().remove("anchorTopLeft");
    assert_eq!(
        TextBlockRecord::decode(&value.to_string()).unwrap_err(),
        TextRecordError::Malformed
    );
    let mut root = case("anchor_negative_fraction")["rootData"].clone();
    root.as_object_mut().unwrap().remove("anchorTopLeft");
    assert_eq!(
        TextBlockRecord::capture_root_data(&root).unwrap_err(),
        TextRecordError::RootField("anchorTopLeft")
    );
    // Without the anchor the glyphs would be placed from (0, 0): a different layout.
    let row = case("anchor_negative_fraction");
    let record = TextBlockRecord::capture_root_data(&row["rootData"]).unwrap();
    let catalog = catalog();
    let mut request = record.layout_request("blk");
    assert_eq!(
        native_glyphs(&layout_text(&request, Some(&catalog)).unwrap()),
        baseline_glyphs(&row)
    );
    request.top_left = [0.0, 0.0];
    assert_ne!(
        native_glyphs(&layout_text(&request, Some(&catalog)).unwrap()),
        baseline_glyphs(&row)
    );
}

#[test]
fn storing_text_overflow_is_refused_on_decode_and_never_captured() {
    let mut value = expected_value("area_overflow");
    assert_eq!(value["fixedHeight"], 60);
    value["textOverflow"] = json!(true);
    assert_ne!(value.to_string(), expected_text("area_overflow"));
    assert_eq!(
        TextBlockRecord::decode(&value.to_string()).unwrap_err(),
        TextRecordError::Malformed
    );
    // The baseline root did store textOverflow (true here); the record drops it.
    let row = case("area_overflow");
    assert_eq!(row["rootData"]["textOverflow"], true);
    let record = TextBlockRecord::capture_root_data(&row["rootData"]).unwrap();
    assert!(!record.encode().unwrap().contains("textOverflow"));
    let mut cleared = row["rootData"].clone();
    cleared["textOverflow"] = json!(false);
    assert_eq!(
        TextBlockRecord::capture_root_data(&cleared).unwrap(),
        record
    );
}

#[test]
fn collapsing_flaws_change_the_decoded_value() {
    // What the baseline serP does (truthy-only, defaults dropped) would lose presence.
    let explicit = TextBlockRecord::decode(&expected_text("spacing_explicit_zero")).unwrap();
    let text = explicit.encode().unwrap();
    for (piece, what) in [
        (r#""fixedWidth":null,"#, "null box collapsed to absent"),
        (r#""letterSpacing":0,"#, "explicit zero spacing dropped"),
        (r#""wordSpacing":0,"#, "explicit zero word spacing dropped"),
        (
            r#""lineHeightMult":1.25,"#,
            "explicit default multiplier dropped",
        ),
        (r#""bold":false,"#, "explicit false flag dropped"),
        (r#""textCase":"none","#, "explicit none case dropped"),
    ] {
        assert!(text.contains(piece), "{what}: the canonical text has it");
        let flawed = TextBlockRecord::decode(&text.replace(piece, "")).unwrap();
        assert_ne!(flawed, explicit, "{what}");
    }
    // And the opposite flaw: inventing a default the author never wrote.
    let bare = TextBlockRecord::decode(
        &expected_text("default_plain").replace(r#""lineHeightMult":1.25,"#, ""),
    )
    .unwrap();
    assert_eq!(bare.line_height_mult_authored(), None);
    let invented = TextBlockRecord::decode(&bare.encode().unwrap().replace(
        r#""textCase":"none","#,
        r#""lineHeightMult":1.25,"textCase":"none","#,
    ))
    .unwrap();
    assert_ne!(invented, bare);
}

#[test]
fn a_record_mapping_that_ignores_a_member_diverges_from_the_baseline() {
    // Each flaw rewrites one member of an otherwise honest record to its "ignored" value.
    let table: [(&str, &str, &str); 7] = [
        ("bold_on_regular_key", "bold", "false"),
        ("area_overflow", "fixedHeight", "null"),
        ("spacing_negative_and_word", "letterSpacing", "0"),
        ("line_height_2", "lineHeightMult", "1.25"),
        ("case_upper_sharp_s", "textCase", "\"none\""),
        ("box_width_center", "align", "\"left\""),
        ("box_width_wrap_inter", "fixedWidth", "null"),
    ];
    for (name, member, ignored) in table {
        let row = case(name);
        let mut root = row["rootData"].clone();
        let was = root[member].clone();
        root[member] = serde_json::from_str(ignored).unwrap();
        assert_ne!(root[member], was, "{name}: the flaw changes {member}");
        let flawed_record = TextBlockRecord::capture_root_data(&root).unwrap();
        let honest_record = TextBlockRecord::capture_root_data(&row["rootData"]).unwrap();
        assert_ne!(flawed_record, honest_record, "{name}");
        let catalog = catalog();
        let honest = native_glyphs(
            &layout_text(&honest_record.layout_request("blk"), Some(&catalog)).unwrap(),
        );
        let flawed = native_glyphs(
            &layout_text(&flawed_record.layout_request("blk"), Some(&catalog)).unwrap(),
        );
        assert_eq!(honest, baseline_glyphs(&row), "{name}: honest");
        assert_ne!(
            flawed, honest,
            "{name}: ignoring {member} changes the glyphs"
        );
    }
}

#[test]
fn a_request_mapping_that_drops_style_diverges_from_the_baseline() {
    let catalog = catalog();
    let request_of = |name: &str| {
        let row = case(name);
        let record = TextBlockRecord::capture_root_data(&row["rootData"]).unwrap();
        (row, record)
    };

    let (row, record) = request_of("bold_on_regular_key");
    let mut request = record.layout_request("blk");
    assert!(request.style.bold);
    assert_eq!(
        native_glyphs(&layout_text(&request, Some(&catalog)).unwrap()),
        baseline_glyphs(&row)
    );
    request.style.bold = false;
    assert_ne!(
        native_glyphs(&layout_text(&request, Some(&catalog)).unwrap()),
        baseline_glyphs(&row)
    );

    let (row, record) = request_of("case_upper_sharp_s");
    let mut request = record.layout_request("blk");
    assert_eq!(request.style.text_case, TextCase::Upper);
    request.style.text_case = TextCase::Unchanged;
    assert_ne!(
        native_glyphs(&layout_text(&request, Some(&catalog)).unwrap()),
        baseline_glyphs(&row)
    );

    let (row, record) = request_of("area_overflow");
    let mut request = record.layout_request("blk");
    assert_eq!(request.fixed_height, Some(60.0));
    let honest = layout_text(&request, Some(&catalog)).unwrap();
    assert!(honest.overflow);
    assert_eq!(native_glyphs(&honest), baseline_glyphs(&row));
    request.fixed_height = None;
    let flawed = layout_text(&request, Some(&catalog)).unwrap();
    assert!(
        !flawed.overflow,
        "dropping fixedHeight loses the derived overflow flag"
    );
    assert_ne!(native_glyphs(&flawed), native_glyphs(&honest));
}
