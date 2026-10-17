//! The admitted-font path through a real `AssetInventory` (feature
//! `font_asset_wire`, the provider's own constructor of the admitted-font
//! proof): the retained bytes of a Font asset become a pinned, read face.
use super::fonts::{FontUnavailable, InventoryFonts};
use super::tests::{assert_matches_case, case, record_json, HOLDER, SNAPSHOT};
use super::{RetainedLayoutError, RetainedTextLayout};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::object_text_layout_kernels::text_glyph_source::font::TrueTypeOpener;
use crate::object_document::object_text_layout_kernels::text_glyph_source::GlyphSourceError;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource};
use serde_json::json;
use std::sync::Arc;

const ROBOTO: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");

fn inventory(allocation: &Arc<Vec<u8>>) -> AssetInventory {
    let source =
        AssetSource::admitted_font("font/ttf", Arc::clone(allocation), allocation.len() + 1024)
            .unwrap();
    let record = AssetRecord::new(
        AssetId::new("native-font-1".to_string()).unwrap(),
        "Roboto-Regular".to_string(),
        AssetKind::Font,
        source,
    )
    .unwrap();
    AssetInventory::new(vec![Arc::new(record)], Vec::new(), 4, 0).unwrap()
}

fn asset_record() -> TextBlockRecord {
    let mut json = record_json(&case("hello_roboto_100"));
    json["font"] = json!({"kind": "asset", "assetId": "native-font-1"});
    TextBlockRecord::decode(&json.to_string()).unwrap()
}

#[test]
fn an_admitted_font_asset_is_read_and_pinned_by_its_allocation() {
    let allocation = Arc::new(ROBOTO.to_vec());
    let catalog = inventory(&allocation);
    let provider = InventoryFonts {
        inventory: Some(&catalog),
        bundled: None,
        opener: &TrueTypeOpener,
    };
    let layout = RetainedTextLayout::build(SNAPSHOT, HOLDER, &asset_record(), &provider).unwrap();
    assert_matches_case(&case("hello_roboto_100"), &layout);
    assert!(Arc::ptr_eq(layout.key().face().allocation(), &allocation));
    // The test, the inventory record and the layout's pin.
    assert_eq!(Arc::strong_count(&allocation), 3);
    drop(layout);
    assert_eq!(Arc::strong_count(&allocation), 2);
}

#[test]
fn replacing_the_asset_bytes_is_a_new_key_even_when_the_bytes_are_equal() {
    let first = Arc::new(ROBOTO.to_vec());
    let second = Arc::new(ROBOTO.to_vec());
    let (a, b) = (inventory(&first), inventory(&second));
    let build = |catalog: &AssetInventory| {
        let provider = InventoryFonts {
            inventory: Some(catalog),
            bundled: None,
            opener: &TrueTypeOpener,
        };
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &asset_record(), &provider).unwrap()
    };
    let (old, new) = (build(&a), build(&b));
    assert!(!old.key().matches(new.key()));
    assert_eq!(old.glyphs(), new.glyphs());
    assert!(old.key().matches(build(&a).key()));
}

#[test]
fn admitted_bytes_the_glyph_source_cannot_read_are_font_unavailable() {
    let garbage = Arc::new(vec![1_u8, 2, 3, 4]);
    let catalog = inventory(&garbage);
    let provider = InventoryFonts {
        inventory: Some(&catalog),
        bundled: None,
        opener: &TrueTypeOpener,
    };
    assert_eq!(
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &asset_record(), &provider).unwrap_err(),
        RetainedLayoutError::FontUnavailable(FontUnavailable::Unreadable(
            GlyphSourceError::Unreadable
        ))
    );
}
