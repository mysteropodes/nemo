//! Tests of the font reference (G38a / L06-F): the bundled table against the
//! REAL `VECTOR_FONTS` of the baseline, the legacy key mapping and the wire form.
use super::super::oracle::{admitted_cases, rows};
use super::{BundledFont, FontRef, FontRefError, BUNDLED_FONT_KEYS};
use crate::object_document::AssetId;

fn asset(id: &str) -> FontRef {
    FontRef::Asset(AssetId::new(id.to_string()).expect("asset id"))
}

#[test]
fn the_bundled_table_is_the_baseline_vector_fonts_in_order() {
    let fonts = rows("FONTS");
    assert_eq!(fonts.len(), 1);
    let baseline: Vec<&str> = fonts[0]
        .as_array()
        .expect("FONTS row")
        .iter()
        .map(|k| k.as_str().expect("key"))
        .collect();
    assert_eq!(baseline, BUNDLED_FONT_KEYS);
    assert_eq!(
        BUNDLED_FONT_KEYS,
        [
            "Roboto-Regular",
            "Roboto-Bold",
            "DejaVuSans-Regular",
            "DejaVuSans-Bold",
            "DejaVuSerif-Regular",
            "DejaVuSerif-Bold",
            "Manrope-Regular",
            "Inter-Regular"
        ]
    );
    // The stub faces of the oracle cover exactly the table.
    let faces: Vec<String> = rows("FACE")
        .iter()
        .map(|f| f["key"].as_str().expect("key").to_string())
        .collect();
    assert_eq!(faces, BUNDLED_FONT_KEYS);
}

#[test]
fn every_font_the_baseline_stored_is_in_the_table() {
    for row in admitted_cases() {
        let key = row["rootData"]["vectorFont"].as_str().expect("vectorFont");
        let font = BundledFont::from_key(key).unwrap_or_else(|| panic!("{key}"));
        assert_eq!(font.key(), key);
        assert_eq!(FontRef::from_legacy_key(key).unwrap().layout_key(), key);
    }
}

#[test]
fn bundled_lookup_is_exact() {
    for key in BUNDLED_FONT_KEYS {
        assert_eq!(BundledFont::from_key(key).map(BundledFont::key), Some(key));
    }
    for key in [
        "",
        "Roboto",
        "roboto-regular",
        "Roboto-Regular ",
        " Roboto-Regular",
        "vector:Roboto-Regular",
        "Inter-Bold",
        "Google:Lato-Regular",
    ] {
        assert_eq!(BundledFont::from_key(key), None, "{key:?}");
    }
}

#[test]
fn legacy_keys_map_through_the_table_and_everything_else_is_refused() {
    for key in BUNDLED_FONT_KEYS {
        let bare = FontRef::from_legacy_key(key).unwrap();
        let prefixed = FontRef::from_legacy_key(&format!("vector:{key}")).unwrap();
        assert_eq!(bare, prefixed);
        assert_eq!(bare.layout_key(), key);
    }
    for key in [
        "",
        "vector:",
        "Roboto",
        "vector:vector:Roboto-Regular",
        "Vector:Roboto-Regular",
        "Google:Lato-Regular",
        "vector:Google:Lato-Regular",
        "NoSuchFont",
        "Inter-Bold",
    ] {
        assert_eq!(
            FontRef::from_legacy_key(key),
            Err(FontRefError::UnknownKey),
            "{key:?}"
        );
    }
}

#[test]
fn wire_form_is_tagged_and_round_trips() {
    let bundled = FontRef::from_legacy_key("Roboto-Bold").unwrap();
    let text = serde_json::to_string(&bundled).unwrap();
    assert_eq!(text, r#"{"kind":"bundled","fontKey":"Roboto-Bold"}"#);
    assert_eq!(serde_json::from_str::<FontRef>(&text).unwrap(), bundled);
    let font = asset("font-asset-1");
    let text = serde_json::to_string(&font).unwrap();
    assert_eq!(text, r#"{"kind":"asset","assetId":"font-asset-1"}"#);
    assert_eq!(serde_json::from_str::<FontRef>(&text).unwrap(), font);
    assert_eq!(font.layout_key(), "font-asset-1");
    assert_ne!(bundled, font);
}

#[test]
fn wire_refusals_name_the_variant_or_the_structure() {
    let typed = [
        (r#"{"kind":"bundled","fontKey":"Nope"}"#, "UnknownKey"),
        (r#"{"kind":"font","fontKey":"Roboto-Bold"}"#, "Kind"),
        (r#"{"kind":"bundled"}"#, "Members"),
        (r#"{"kind":"asset","fontKey":"Roboto-Bold"}"#, "Members"),
        (r#"{"kind":"asset","assetId":" "}"#, "AssetId"),
    ];
    for (text, variant) in typed {
        let message = serde_json::from_str::<FontRef>(text)
            .unwrap_err()
            .to_string();
        assert!(message.contains(variant), "{text}: {message}");
    }
    for text in [
        "",
        "null",
        r#""Roboto-Bold""#,
        r#"["bundled","Roboto-Bold"]"#,
        r#"{"kind":"bundled","fontKey":"Roboto-Bold","extra":1}"#,
        r#"{"kind":"bundled","kind":"bundled","fontKey":"Roboto-Bold"}"#,
        r#"{"kind":"bundled","fontKey":null}"#,
        r#"{"kind":"asset","assetId":3}"#,
    ] {
        assert!(serde_json::from_str::<FontRef>(text).is_err(), "{text}");
    }
}
