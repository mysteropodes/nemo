//! Refusal matrix of the retained-font provider seam: every way a font
//! reference can fail to become retained bytes is its own typed value, and the
//! weight rules never substitute one face for another silently.
use super::super::tests::RecordedPort;
use super::{
    BundledFontBytes, FaceSet, FontUnavailable, InventoryFonts, RetainedFont, RetainedFontPort,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::text_glyph_source::{
    GlyphSourceError, GlyphSourceOpener, GlyphSourcePort,
};
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource};
use std::sync::Arc;

const CAP: usize = 1024;

fn record(id: &str, kind: AssetKind, source: AssetSource) -> Arc<AssetRecord> {
    Arc::new(
        AssetRecord::new(
            AssetId::new(id.to_string()).unwrap(),
            id.to_string(),
            kind,
            source,
        )
        .unwrap(),
    )
}

fn embedded(media_type: &str) -> AssetSource {
    AssetSource::admitted_embedded(media_type.to_string(), Arc::new(vec![1, 2, 3]), CAP).unwrap()
}

fn inventory() -> AssetInventory {
    AssetInventory::new(
        vec![
            // Looks like a font but carries no admitted-font proof.
            record("generic-font", AssetKind::Font, embedded("font/ttf")),
            record(
                "linked-font",
                AssetKind::Font,
                AssetSource::linked("file:///fonts/a.ttf".to_string(), CAP).unwrap(),
            ),
            record("plate", AssetKind::Image, embedded("image/png")),
        ],
        Vec::new(),
        8,
        0,
    )
    .unwrap()
}

/// The opener must never be reached for a reference that is already refused.
struct Unreachable;

impl GlyphSourceOpener for Unreachable {
    fn open(&self, _encoded: &Arc<Vec<u8>>) -> Result<Box<dyn GlyphSourcePort>, GlyphSourceError> {
        panic!("a refused reference must not reach the glyph source");
    }
}

fn asset(id: &str) -> FontRef {
    FontRef::Asset(AssetId::new(id.to_string()).unwrap())
}

fn refusal(inventory: Option<&AssetInventory>, font: &FontRef) -> FontUnavailable {
    let provider = InventoryFonts {
        inventory,
        bundled: None,
        opener: &Unreachable,
    };
    match provider.retained_font(font) {
        Ok(_) => panic!("{font:?} must be unavailable"),
        Err(error) => error,
    }
}

#[test]
fn every_asset_failure_is_a_distinct_typed_unavailability() {
    let catalog = inventory();
    assert_eq!(
        refusal(None, &asset("generic-font")),
        FontUnavailable::NoInventory
    );
    assert_eq!(
        refusal(Some(&catalog), &asset("absent")),
        FontUnavailable::Missing
    );
    assert_eq!(
        refusal(Some(&catalog), &asset("plate")),
        FontUnavailable::WrongKind
    );
    assert_eq!(
        refusal(Some(&catalog), &asset("linked-font")),
        FontUnavailable::Linked
    );
    assert_eq!(
        refusal(Some(&catalog), &asset("generic-font")),
        FontUnavailable::NotAdmitted
    );
}

#[test]
fn a_bundled_face_needs_a_byte_provider_that_has_it() {
    let roboto = FontRef::from_legacy_key("Roboto-Regular").unwrap();
    assert_eq!(refusal(None, &roboto), FontUnavailable::BundledNotProvided);
    struct Empty;
    impl BundledFontBytes for Empty {
        fn bundled_bytes(&self, _key: &str) -> Option<Arc<Vec<u8>>> {
            None
        }
    }
    let provider = InventoryFonts {
        inventory: None,
        bundled: Some(&Empty),
        opener: &Unreachable,
    };
    assert!(matches!(
        provider.retained_font(&roboto),
        Err(FontUnavailable::BundledNotProvided)
    ));
}

#[test]
fn bytes_the_glyph_source_cannot_read_are_unreadable_not_replaced() {
    struct Broken;
    impl GlyphSourceOpener for Broken {
        fn open(
            &self,
            _encoded: &Arc<Vec<u8>>,
        ) -> Result<Box<dyn GlyphSourcePort>, GlyphSourceError> {
            Err(GlyphSourceError::UnsupportedCmap)
        }
    }
    let port = RecordedPort::new();
    let roboto = FontRef::from_legacy_key("Roboto-Regular").unwrap();
    let provider = InventoryFonts {
        inventory: None,
        bundled: Some(&port),
        opener: &Broken,
    };
    assert!(matches!(
        provider.retained_font(&roboto),
        Err(FontUnavailable::Unreadable(
            GlyphSourceError::UnsupportedCmap
        ))
    ));
}

/// The oracle port with one bundled key withdrawn.
struct Without<'a> {
    inner: &'a RecordedPort,
    missing: &'static str,
}

impl RetainedFontPort for Without<'_> {
    fn retained_font(&self, font: &FontRef) -> Result<RetainedFont, FontUnavailable> {
        match font {
            FontRef::Bundled(bundled) if bundled.key() == self.missing => {
                Err(FontUnavailable::BundledNotProvided)
            }
            _ => self.inner.retained_font(font),
        }
    }
}

#[test]
fn the_face_set_loads_exactly_the_faces_the_baseline_weight_rule_can_ask_for() {
    let port = RecordedPort::new();
    let regular = FontRef::from_legacy_key("Roboto-Regular").unwrap();
    let bold_key = FontRef::from_legacy_key("Roboto-Bold").unwrap();
    let manrope = FontRef::from_legacy_key("Manrope-Regular").unwrap();
    // Same weight as given: one face. Other weight: both, in lookup order.
    assert_eq!(FaceSet::load(&port, &regular, false).unwrap().len(), 1);
    let both = FaceSet::load(&port, &regular, true).unwrap();
    assert_eq!(both.len(), 2);
    assert!(both.get("Roboto-Bold").is_some() && both.get("Roboto-Regular").is_some());
    assert_eq!(FaceSet::load(&port, &bold_key, false).unwrap().len(), 2);
    // No Manrope-Bold exists in VECTOR_FONTS: the key as given only.
    let single = FaceSet::load(&port, &manrope, true).unwrap();
    assert_eq!(single.len(), 1);
    assert!(single.get("Manrope-Bold").is_none());
    // An asset reference is never weight-resolved.
    let by_asset = RecordedPort::new().with_asset("font-A", "Roboto-Regular");
    assert_eq!(
        FaceSet::load(&by_asset, &asset("font-A"), true)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_bundled_weight_that_exists_but_is_withdrawn_is_an_error_not_a_fallback() {
    let port = RecordedPort::new();
    let regular = FontRef::from_legacy_key("Roboto-Regular").unwrap();
    let without_bold = Without {
        inner: &port,
        missing: "Roboto-Bold",
    };
    assert_eq!(
        FaceSet::load(&without_bold, &regular, true).err(),
        Some(FontUnavailable::BundledNotProvided)
    );
    // The same withdrawal does not matter when Bold is not asked for.
    assert_eq!(
        FaceSet::load(&without_bold, &regular, false).unwrap().len(),
        1
    );
    let without_given = Without {
        inner: &port,
        missing: "Roboto-Regular",
    };
    assert_eq!(
        FaceSet::load(&without_given, &regular, false).err(),
        Some(FontUnavailable::BundledNotProvided)
    );
}
