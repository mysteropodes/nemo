//! Identity, lifetime and refusal controls of the retained text layout. The
//! expected values are typed here or pinned from the baseline oracle in the
//! sibling file; none is computed by the code under test.
use super::fonts::{BundledFontBytes, FontPin, FontUnavailable, RetainedFont, RetainedFontPort};
use super::tests::{
    assert_matches_case, build_case, case, record_json, record_of, RecordedPort, HOLDER, SNAPSHOT,
};
use super::{RetainedLayoutError, RetainedTextLayout};
use crate::object_codec::decode_project;
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::{
    FontAdvancePort, TextLayoutError,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::pin::GlyphPin;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::object_text_layout_kernels::text_glyph_contours::{
    GlyphContourError, OutlineCommand,
};
use crate::object_document::object_text_layout_kernels::text_glyph_source::oracle::{
    recorded_faces, RecordedFace,
};
use crate::object_document::object_text_layout_kernels::text_glyph_source::{
    GlyphSourceError, GlyphSourceOpener, GlyphSourcePort,
};
use crate::revision::ObjectSnapshot;
use serde_json::json;
use std::sync::Arc;

fn hello() -> TextBlockRecord {
    record_of(&case("hello_roboto_100"))
}

fn with_text(text: &str) -> TextBlockRecord {
    let mut json = record_json(&case("hello_roboto_100"));
    json["text"] = json!(text);
    TextBlockRecord::decode(&json.to_string()).expect("valid record")
}

#[test]
fn identity_is_deterministic_and_keyed_by_snapshot_holder_and_face() {
    let port = RecordedPort::new();
    let a = RetainedTextLayout::build(SNAPSHOT, HOLDER, &hello(), &port).unwrap();
    let b = RetainedTextLayout::build(SNAPSHOT, HOLDER, &hello(), &port).unwrap();
    assert!(!a.same_value_as(&b), "every build is a new value");
    assert!(a.key().matches(b.key()));
    assert_eq!(a.glyphs(), b.glyphs());
    assert_eq!(a.layout(), b.layout());
    let other_snapshot =
        RetainedTextLayout::build("native-object:doc-2:7", HOLDER, &hello(), &port).unwrap();
    assert!(!a.key().matches(other_snapshot.key()));
    assert_eq!(a.glyphs(), other_snapshot.glyphs());
    let other_holder =
        RetainedTextLayout::build(SNAPSHOT, "scene-root/layer-1/text-2", &hello(), &port).unwrap();
    assert!(!a.key().matches(other_holder.key()));
    assert_eq!(a.key().snapshot_id(), SNAPSHOT);
    assert_eq!(a.key().holder_id(), HOLDER);
    assert_eq!(
        a.key().requested_font(),
        &FontRef::from_legacy_key("Roboto-Regular").unwrap()
    );
}

#[test]
fn a_one_character_edit_never_rekeys_the_glyphs_before_it() {
    let port = RecordedPort::new();
    let before = RetainedTextLayout::build(SNAPSHOT, HOLDER, &hello(), &port).unwrap();
    let replaced = RetainedTextLayout::build(SNAPSHOT, HOLDER, &with_text("Helpo"), &port).unwrap();
    let inserted =
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &with_text("Hxello"), &port).unwrap();
    assert!(!before.same_value_as(&replaced));
    for earlier in 0..3 {
        assert_eq!(
            before.glyph(earlier),
            replaced.glyph(earlier),
            "ordinal {earlier}"
        );
        assert_eq!(before.pins()[earlier].key(), replaced.pins()[earlier].key());
    }
    assert_ne!(
        before.glyph(3).unwrap().unit,
        replaced.glyph(3).unwrap().unit
    );
    assert_eq!(before.glyph(0), inserted.glyph(0));
    assert_eq!(inserted.glyph(1).unwrap().unit, u16::from(b'x'));
    assert_eq!(inserted.pins().len(), 6);
    assert_eq!(before.pins()[0].key(), format!("{HOLDER}/glyph/0"));
    let pin = GlyphPin::parse(&inserted.pins()[2].key()).unwrap();
    assert_eq!(
        pin.resolve(inserted.layout()).unwrap().unit,
        u16::from(b'e')
    );
}

#[test]
fn a_font_asset_is_resolved_through_the_port_and_pinned_by_reference() {
    let port = RecordedPort::new().with_asset("font-A", "Roboto-Regular");
    let mut json = record_json(&case("hello_roboto_100"));
    json["font"] = json!({"kind": "asset", "assetId": "font-A"});
    let record = TextBlockRecord::decode(&json.to_string()).unwrap();
    let layout = RetainedTextLayout::build(SNAPSHOT, HOLDER, &record, &port).unwrap();
    assert_matches_case(&case("hello_roboto_100"), &layout);
    assert!(matches!(layout.key().requested_font(), FontRef::Asset(id) if id.as_str() == "font-A"));
    assert!(matches!(layout.key().face().reference(), FontRef::Asset(_)));
    assert!(Arc::ptr_eq(
        layout.key().face().allocation(),
        &port.allocations["Roboto-Regular"]
    ));
    // An unknown asset id is a typed unavailability.
    json["font"] = json!({"kind": "asset", "assetId": "font-B"});
    let unknown = TextBlockRecord::decode(&json.to_string()).unwrap();
    assert_eq!(
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &unknown, &port).unwrap_err(),
        RetainedLayoutError::FontUnavailable(FontUnavailable::Missing)
    );
}

#[test]
fn the_pin_lives_exactly_as_long_as_the_last_clone() {
    let port = RecordedPort::new();
    let allocation = Arc::clone(&port.allocations["Roboto-Regular"]);
    let base = Arc::strong_count(&allocation);
    let layout = RetainedTextLayout::build(SNAPSHOT, HOLDER, &hello(), &port).unwrap();
    // Faces opened during the build are gone; only the key's pin remains.
    assert_eq!(Arc::strong_count(&allocation), base + 1);
    let alias = layout.clone();
    assert_eq!(layout.handle_count(), 2);
    assert!(layout.same_value_as(&alias));
    assert_eq!(
        Arc::strong_count(&allocation),
        base + 1,
        "an alias adds no pin"
    );
    drop(layout);
    assert_eq!(alias.handle_count(), 1);
    assert_eq!(Arc::strong_count(&allocation), base + 1);
    drop(alias);
    assert_eq!(
        Arc::strong_count(&allocation),
        base,
        "the last clone releases the pin"
    );
}

#[test]
fn replacing_the_font_asset_is_a_new_layout_and_the_old_one_stays_valid() {
    let mut port = RecordedPort::new().with_asset("font-A", "Roboto-Regular");
    let mut json = record_json(&case("hello_roboto_100"));
    json["font"] = json!({"kind": "asset", "assetId": "font-A"});
    let record = TextBlockRecord::decode(&json.to_string()).unwrap();
    let old = RetainedTextLayout::build(SNAPSHOT, HOLDER, &record, &port).unwrap();
    let old_allocation = Arc::clone(old.key().face().allocation());
    let fresh = port.replace_allocation("Roboto-Regular");
    let new = RetainedTextLayout::build(SNAPSHOT, HOLDER, &record, &port).unwrap();
    assert!(
        !old.key().matches(new.key()),
        "same asset id, different retained allocation"
    );
    assert!(!old.key().face().same_allocation_as(new.key().face()));
    assert!(Arc::ptr_eq(new.key().face().allocation(), &fresh));
    assert_eq!(old.glyphs(), new.glyphs(), "equal bytes lay out equally");
    // The old value still reads its own allocation until it is released.
    let before = Arc::strong_count(&old_allocation);
    drop(new);
    assert_eq!(Arc::strong_count(&old_allocation), before);
    assert_matches_case(&case("hello_roboto_100"), &old);
    drop(old);
    assert_eq!(Arc::strong_count(&old_allocation), before - 1);
}

#[test]
fn snapshot_and_holder_identities_are_validated_before_any_output() {
    let port = RecordedPort::new();
    for bad in ["", "   ", "a\0b"] {
        assert_eq!(
            RetainedTextLayout::build(bad, HOLDER, &hello(), &port).unwrap_err(),
            RetainedLayoutError::InvalidSnapshotId
        );
    }
    assert_eq!(
        RetainedTextLayout::build(&"s".repeat(256), HOLDER, &hello(), &port).unwrap_err(),
        RetainedLayoutError::InvalidSnapshotId
    );
    assert_eq!(
        RetainedTextLayout::build(SNAPSHOT, "", &hello(), &port).unwrap_err(),
        RetainedLayoutError::Layout(TextLayoutError::InvalidHolderId)
    );
}

/// A face whose outline is tampered with: a refusal or a NaN coordinate.
#[derive(Clone)]
struct Tamper {
    inner: RecordedFace,
    nan: bool,
}

impl FontAdvancePort for Tamper {
    fn units_per_em(&self) -> f64 {
        self.inner.units_per_em()
    }
    fn advance_units(&self, unit: u16) -> Option<f64> {
        self.inner.advance_units(unit)
    }
}

impl GlyphSourcePort for Tamper {
    fn as_advance(&self) -> &dyn FontAdvancePort {
        self
    }
    fn outline(
        &self,
        unit: u16,
        x: f64,
        y: f64,
        size: f64,
    ) -> Result<Vec<OutlineCommand>, GlyphSourceError> {
        if unit != u16::from(b'e') {
            return self.inner.outline(unit, x, y, size);
        }
        if self.nan {
            Ok(vec![OutlineCommand::Move {
                x: f64::NAN,
                y: 0.0,
            }])
        } else {
            Err(GlyphSourceError::MalformedGlyph(7))
        }
    }
}

struct TamperPort {
    face: Tamper,
}

impl RetainedFontPort for TamperPort {
    fn retained_font(&self, font: &FontRef) -> Result<RetainedFont, FontUnavailable> {
        Ok(RetainedFont {
            pin: FontPin::new(font.clone(), Arc::new(vec![1])),
            source: Box::new(self.face.clone()),
        })
    }
}

#[test]
fn a_glyph_the_face_cannot_outline_refuses_the_whole_layout() {
    let inner = recorded_faces().remove("Roboto-Regular").unwrap();
    let refusal = TamperPort {
        face: Tamper {
            inner: inner.clone(),
            nan: false,
        },
    };
    assert_eq!(
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &hello(), &refusal).unwrap_err(),
        RetainedLayoutError::Glyph {
            ordinal: 1,
            error: GlyphSourceError::MalformedGlyph(7)
        }
    );
    let poisoned = TamperPort {
        face: Tamper { inner, nan: true },
    };
    assert_eq!(
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &hello(), &poisoned).unwrap_err(),
        RetainedLayoutError::Contour {
            ordinal: 1,
            error: GlyphContourError::NonFiniteCoordinate { command: 0 }
        }
    );
}

/// Opens the oracle faces from allocations whose bytes are their font key.
struct RecordedOpener;

impl GlyphSourceOpener for RecordedOpener {
    fn open(&self, encoded: &Arc<Vec<u8>>) -> Result<Box<dyn GlyphSourcePort>, GlyphSourceError> {
        let key = String::from_utf8(encoded.to_vec()).map_err(|_| GlyphSourceError::Unreadable)?;
        recorded_faces()
            .remove(&key)
            .map(|face| Box::new(face) as Box<dyn GlyphSourcePort>)
            .ok_or(GlyphSourceError::Unreadable)
    }
}

impl BundledFontBytes for RecordedPort {
    fn bundled_bytes(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.allocations.get(key).cloned()
    }
}

const DOCUMENT: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[]}"#;

#[test]
fn a_snapshot_supplies_its_id_and_a_bare_drawing_has_no_font_catalog() {
    let snapshot =
        ObjectSnapshot::new("inst", decode_project(DOCUMENT.as_bytes()).unwrap()).unwrap();
    let port = RecordedPort::new();
    let layout = RetainedTextLayout::build_for_snapshot(
        &snapshot,
        HOLDER,
        &hello(),
        Some(&port),
        &RecordedOpener,
    )
    .unwrap();
    assert_eq!(layout.key().snapshot_id(), snapshot.snapshot_id());
    assert_matches_case(&case("hello_roboto_100"), &layout);
    // Without a bundled provider the same record is a typed unavailability...
    assert_eq!(
        RetainedTextLayout::build_for_snapshot(&snapshot, HOLDER, &hello(), None, &RecordedOpener)
            .unwrap_err(),
        RetainedLayoutError::FontUnavailable(FontUnavailable::BundledNotProvided)
    );
    // ...and a font asset of a bare drawing has no catalog to resolve in.
    let mut json = record_json(&case("hello_roboto_100"));
    json["font"] = json!({"kind": "asset", "assetId": "font-A"});
    let asset = TextBlockRecord::decode(&json.to_string()).unwrap();
    assert_eq!(
        RetainedTextLayout::build_for_snapshot(
            &snapshot,
            HOLDER,
            &asset,
            Some(&port),
            &RecordedOpener
        )
        .unwrap_err(),
        RetainedLayoutError::FontUnavailable(FontUnavailable::NoInventory)
    );
}

#[test]
fn the_bundled_weight_rules_follow_the_baseline_font_table() {
    let port = RecordedPort::new();
    // Bold on a bundled family that has a Bold face resolves it...
    let bold = build_case(&case("hello_world_bold_italic"), &port);
    assert!(
        matches!(bold.key().face().reference(), FontRef::Bundled(f) if f.key() == "Roboto-Bold")
    );
    assert_eq!(bold.layout().resolved_font_key, "Roboto-Bold");
    // ...Manrope has no Bold, so the key as given stays...
    let manrope = build_case(&case("manrope_bold_falls_back"), &port);
    assert!(
        matches!(manrope.key().face().reference(), FontRef::Bundled(f) if f.key() == "Manrope-Regular")
    );
    // ...and a Bold key with the bold flag off resolves the Regular face.
    let toggled = build_case(&case("bold_key_regular_toggle"), &port);
    assert!(
        matches!(toggled.key().face().reference(), FontRef::Bundled(f) if f.key() == "Roboto-Regular")
    );
    assert!(
        matches!(toggled.key().requested_font(), FontRef::Bundled(f) if f.key() == "Roboto-Bold")
    );
}
