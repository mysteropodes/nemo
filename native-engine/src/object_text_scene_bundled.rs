//! Exact baseline bundled bytes. No new font selection rule, system font or network.
use crate::evaluation::object_text_layout_kernels::{
    object_text_layout_kernel::model::{FontAdvancePort, FontCatalogPort},
    object_text_layout_record::font::BUNDLED_FONT_KEYS,
};
pub(super) struct Keys;
impl FontCatalogPort for Keys {
    fn knows_key(&self, key: &str) -> bool {
        BUNDLED_FONT_KEYS.contains(&key)
    }
    fn face(&self, _: &str) -> Option<&dyn FontAdvancePort> {
        None
    }
}
pub(super) fn bytes(key: &str) -> Option<&'static [u8]> {
    Some(match key {
        "Roboto-Regular" => include_bytes!("../../src/fonts/Roboto-Regular.ttf"),
        "Roboto-Bold" => include_bytes!("../../src/fonts/Roboto-Bold.ttf"),
        "DejaVuSans-Regular" => include_bytes!("../../src/fonts/DejaVuSans.ttf"),
        "DejaVuSans-Bold" => include_bytes!("../../src/fonts/DejaVuSans-Bold.ttf"),
        "DejaVuSerif-Regular" => include_bytes!("../../src/fonts/DejaVuSerif.ttf"),
        "DejaVuSerif-Bold" => include_bytes!("../../src/fonts/DejaVuSerif-Bold.ttf"),
        "Manrope-Regular" => include_bytes!("../../src/fonts/Manrope-Regular.ttf"),
        "Inter-Regular" => include_bytes!("../../src/fonts/Inter-Regular.ttf"),
        _ => return None,
    })
}
