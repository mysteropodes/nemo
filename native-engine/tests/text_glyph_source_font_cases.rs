//! End to end with the real parser (feature `font_asset_admission`): the same
//! CASE rows of the real `buildVectorTextGroup`, laid out and outlined from the
//! actual bundled TTF bytes through `InventoryFonts` + `TrueTypeOpener`.
use super::tests::FONTS;
use super::TrueTypeOpener;
use crate::object_document::object_text_layout_kernels::object_text_layout_retained::fonts::{
    BundledFontBytes, FontUnavailable, InventoryFonts,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_retained::tests::{
    assert_matches_case, case, record_of, HOLDER, SNAPSHOT,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_retained::{
    RetainedLayoutError, RetainedTextLayout,
};
use crate::object_document::object_text_layout_kernels::text_glyph_source::oracle::rows;
use std::collections::HashMap;
use std::sync::Arc;

/// One retained allocation per bundled face, as a host that embeds them would hold.
struct RealBundled(HashMap<&'static str, Arc<Vec<u8>>>);

impl RealBundled {
    fn new() -> Self {
        Self(
            FONTS
                .iter()
                .map(|(key, bytes)| (*key, Arc::new(bytes.to_vec())))
                .collect(),
        )
    }
}

impl BundledFontBytes for RealBundled {
    fn bundled_bytes(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.0.get(key).cloned()
    }
}

#[test]
fn every_case_matches_the_real_baseline_with_the_real_parser() {
    let bundled = RealBundled::new();
    let provider = InventoryFonts {
        inventory: None,
        bundled: Some(&bundled),
        opener: &TrueTypeOpener,
    };
    let mut paths = 0;
    for row in rows("CASE") {
        let layout =
            RetainedTextLayout::build(SNAPSHOT, HOLDER, &record_of(&row), &provider).unwrap();
        paths += assert_matches_case(&row, &layout);
    }
    assert_eq!(paths, 265);
}

#[test]
fn hello_roboto_100_from_the_real_bytes_is_the_acceptance_layout() {
    let bundled = RealBundled::new();
    let provider = InventoryFonts {
        inventory: None,
        bundled: Some(&bundled),
        opener: &TrueTypeOpener,
    };
    let row = case("hello_roboto_100");
    let layout = RetainedTextLayout::build(SNAPSHOT, HOLDER, &record_of(&row), &provider).unwrap();
    let origins: Vec<f64> = layout.glyphs().iter().map(|g| g.origin_x).collect();
    assert_eq!(
        origins,
        vec![
            0.0,
            71.337890625,
            124.365234375,
            148.681640625,
            172.998046875
        ]
    );
    assert_eq!(
        (layout.layout().width, layout.layout().height),
        (230.029296875, 125.0)
    );
    assert!(Arc::ptr_eq(
        layout.key().face().allocation(),
        &bundled.0["Roboto-Regular"]
    ));
}

#[test]
fn without_the_retained_bytes_nothing_is_laid_out() {
    let provider = InventoryFonts {
        inventory: None,
        bundled: None,
        opener: &TrueTypeOpener,
    };
    let row = case("hello_roboto_100");
    assert_eq!(
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &record_of(&row), &provider).unwrap_err(),
        RetainedLayoutError::FontUnavailable(FontUnavailable::BundledNotProvided)
    );
}
