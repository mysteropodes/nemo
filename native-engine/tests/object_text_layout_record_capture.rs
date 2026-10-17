//! Capture of the text block record from baseline root data (G38a / L06-F):
//! which members are read, what keeps its presence, and the typed refusals.
use super::font::FontRefError;
use super::oracle::{case, MINIMAL};
use super::{TextBlockRecord, TextRecordError};
use serde_json::{json, Value};

fn root() -> Value {
    case("default_plain")["rootData"].clone()
}

#[test]
fn capture_reads_only_the_text_members_and_keeps_presence() {
    let plain = TextBlockRecord::capture_root_data(&root()).unwrap();
    // The shared data bag and derived output do not change the record.
    let mut noisy = root();
    noisy["groupId"] = json!("vtxt1");
    noisy["charIndex"] = json!(0);
    noisy["isVectorText"] = json!(true);
    noisy["textOverflow"] = json!("not even a bool");
    assert_eq!(TextBlockRecord::capture_root_data(&noisy).unwrap(), plain);
    // Only the required members: everything else stays absent, nothing is invented.
    let required = json!({
        "isTextRoot": true, "text": "Hi", "vectorFont": "Roboto-Regular", "size": 48,
        "color": "#112233", "align": "left", "anchorTopLeft": {"x": 10, "y": 20}
    });
    let record = TextBlockRecord::capture_root_data(&required).unwrap();
    assert_eq!(record.encode().unwrap(), MINIMAL);
    // The dropdown spelling of the font maps through the same table.
    let mut dropdown = required.clone();
    dropdown["vectorFont"] = json!("vector:Roboto-Regular");
    assert_eq!(
        TextBlockRecord::capture_root_data(&dropdown).unwrap(),
        record
    );
}

#[test]
fn capture_refusals_are_typed() {
    let not_root = [
        json!([]),
        json!(null),
        json!("x"),
        json!({}),
        {
            let mut v = root();
            v["isTextRoot"] = json!(false);
            v
        },
        {
            let mut v = root();
            v["isTextRoot"] = json!("true");
            v
        },
        {
            let mut v = root();
            v.as_object_mut().unwrap().remove("isTextRoot");
            v
        },
    ];
    for value in not_root {
        assert_eq!(
            TextBlockRecord::capture_root_data(&value).unwrap_err(),
            TextRecordError::NotATextRoot,
            "{value}"
        );
    }
    let wrong: [(&'static str, Option<Value>); 12] = [
        ("text", None),
        ("text", Some(json!(1))),
        ("vectorFont", None),
        ("size", Some(json!("48"))),
        ("color", Some(json!(null))),
        ("align", None),
        ("fixedWidth", Some(json!("300"))),
        ("fixedHeight", Some(json!(false))),
        ("bold", Some(json!(null))),
        ("letterSpacing", Some(json!("0"))),
        ("textCase", Some(json!(1))),
        ("anchorTopLeft", None),
    ];
    for (member, value) in wrong {
        let mut v = root();
        match value {
            Some(bad) => v[member] = bad,
            None => {
                v.as_object_mut().unwrap().remove(member);
            }
        }
        assert_eq!(
            TextBlockRecord::capture_root_data(&v).unwrap_err(),
            TextRecordError::RootField(member),
            "{member}"
        );
    }
    let mut anchor = root();
    anchor["anchorTopLeft"] = json!({"x": "1", "y": 2});
    assert_eq!(
        TextBlockRecord::capture_root_data(&anchor).unwrap_err(),
        TextRecordError::RootField("anchorTopLeft")
    );
    for key in [
        "Google:Lato-Regular",
        "NoSuchFont",
        "Roboto",
        "vector:vector:Roboto-Regular",
        "roboto-regular",
        "",
    ] {
        let mut v = root();
        v["vectorFont"] = json!(key);
        assert_eq!(
            TextBlockRecord::capture_root_data(&v).unwrap_err(),
            TextRecordError::Font(FontRefError::UnknownKey),
            "{key}"
        );
    }
}
