//! Strict record codec of the raster item value (G01 / L06-C): round trips,
//! absent / default / explicit, typed refusals, the inventory reference.
use super::geometry::{PendingGeometry, RasterItemError, RasterSurface, SurfaceBounds};
use super::oracle::{id, inventory};
use super::{RasterItem, RasterSourceForm, RasterTags};
use crate::object_document::AssetKind;

const MINIMAL: &str = r#"{"assetId":"a1","x":200,"y":150,"width":64,"height":32}"#;
const FULL: &str = r#"{"assetId":"a1","x":200,"y":150.5,"width":64,"height":32.25,"opacity":0.5,"rotation":30,"isBitmapBrush":true,"brushGroupId":"bg_1","isBrushTextureCopy":false,"meshId":"im_1"}"#;

fn refused(text: &str) -> RasterItemError {
    RasterItem::decode(text).expect_err(text)
}

fn surface(width: f64, height: f64) -> RasterSurface {
    RasterSurface {
        matrix_rotation: Some(0.0),
        scaling_x: 1.0,
        scaling_y: 1.0,
        width,
        height,
        position_x: 10.0,
        position_y: 20.0,
        bounds: SurfaceBounds {
            x: 10.0 - width / 2.0,
            y: 20.0 - height / 2.0,
            width,
            height,
        },
        loaded: true,
        pending: None,
        opacity: None,
    }
}

#[test]
fn canonical_text_round_trips_byte_for_byte() {
    for text in [MINIMAL, FULL] {
        let item = RasterItem::decode(text).expect(text);
        assert_eq!(item.encode().unwrap(), text);
        let again = RasterItem::decode(&item.encode().unwrap()).unwrap();
        assert_eq!(again, item);
    }
    let via_serde: RasterItem = serde_json::from_str(FULL).unwrap();
    assert_eq!(via_serde, RasterItem::decode(FULL).unwrap());
}

#[test]
fn absent_default_and_explicit_stay_distinct() {
    let bare = RasterItem::decode(MINIMAL).unwrap();
    assert_eq!((bare.opacity_authored(), bare.opacity()), (None, 1.0));
    assert_eq!((bare.rotation_authored(), bare.rotation()), (None, 0.0));
    assert_eq!(bare.is_bitmap_brush(), None);
    assert_eq!(bare.brush_group_id(), None);
    assert_eq!(bare.mesh_id(), None);
    let explicit = RasterItem::decode(
        r#"{"assetId":"a1","x":200,"y":150,"width":64,"height":32,"opacity":1,"rotation":0,"isBitmapBrush":false,"isBrushTextureCopy":false}"#,
    )
    .unwrap();
    assert_eq!(
        (explicit.opacity_authored(), explicit.opacity()),
        (Some(1.0), 1.0)
    );
    assert_eq!(
        (explicit.rotation_authored(), explicit.rotation()),
        (Some(0.0), 0.0)
    );
    assert_eq!(explicit.is_bitmap_brush(), Some(false));
    assert_eq!(explicit.is_brush_texture_copy(), Some(false));
    assert_ne!(bare, explicit);
    // Nothing is invented on the way out: explicit stays explicit, absent stays absent.
    assert_eq!(bare.encode().unwrap(), MINIMAL);
    assert!(explicit
        .encode()
        .unwrap()
        .contains(r#""opacity":1,"rotation":0,"isBitmapBrush":false"#));
    assert!(!bare.encode().unwrap().contains("opacity"));
}

#[test]
fn passthrough_tags_survive_verbatim_and_uninterpreted() {
    let item = RasterItem::decode(FULL).unwrap();
    assert_eq!(item.is_bitmap_brush(), Some(true));
    assert_eq!(item.brush_group_id(), Some("bg_1"));
    assert_eq!(item.is_brush_texture_copy(), Some(false));
    assert_eq!(item.mesh_id(), Some("im_1"));
    // A tag changes nothing about geometry or the asset reference.
    let plain = RasterItem::decode(
        r#"{"assetId":"a1","x":200,"y":150.5,"width":64,"height":32.25,"opacity":0.5,"rotation":30}"#,
    )
    .unwrap();
    assert_eq!(plain.display_rect(), item.display_rect());
    assert_eq!(plain.asset_id(), item.asset_id());
}

#[test]
fn number_spelling_class_is_preserved() {
    let int = RasterItem::decode(r#"{"assetId":"a","x":1,"y":2,"width":64,"height":32}"#).unwrap();
    let float =
        RasterItem::decode(r#"{"assetId":"a","x":1,"y":2,"width":64.0,"height":32}"#).unwrap();
    assert_ne!(int, float);
    assert!(float.encode().unwrap().contains(r#""width":64.0"#));
    assert!(int.encode().unwrap().contains(r#""width":64,"#));
    assert_eq!(int.display_rect(), float.display_rect());
}

#[test]
fn structure_refusals_are_typed_malformed() {
    for text in [
        "",
        "[]",
        r#"["a1",1,2,3,4]"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":4,"extra":1}"#,
        r#"{"assetId":"a1","x":1,"x":2,"y":2,"width":3,"height":4}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":4,"opacity":0.5,"opacity":0.5}"#,
        r#"{"assetId":"a1","x":"1","y":2,"width":3,"height":4}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":4,"opacity":null}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":4,"rotation":null}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":4,"meshId":null}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":4,"isBitmapBrush":1}"#,
        r#"{"assetId":7,"x":1,"y":2,"width":3,"height":4}"#,
        r#"{"assetId":"a1","x":1e999,"y":2,"width":3,"height":4}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":4} trailing"#,
    ] {
        assert_eq!(refused(text), RasterItemError::Malformed, "{text}");
    }
}

#[test]
fn bytes_urls_locators_and_other_records_members_are_not_accepted() {
    for member in [
        r#""src":"data:image/png;base64,AAAA""#,
        r#""linked":true"#,
        r#""linkedPath":"/media/a.png""#,
        r#""linkedHandleId":"h1""#,
        r#""isRaster":true"#,
        r#""groupId":"g1""#,
        r#""isText":true"#,
    ] {
        let text = format!(r#"{{"assetId":"a1","x":1,"y":2,"width":3,"height":4,{member}}}"#);
        assert_eq!(refused(&text), RasterItemError::Malformed, "{member}");
    }
    let captured =
        RasterItem::capture(id("a1"), &surface(8.0, 8.0), &RasterTags::default()).unwrap();
    let text = captured.encode().unwrap();
    for forbidden in ["src", "data:", "linked", "locator", "isRaster"] {
        assert!(!text.contains(forbidden), "{forbidden} in {text}");
    }
}

#[test]
fn value_refusals_are_typed() {
    let with = |tail: &str| format!(r#"{{"assetId":"a1","x":1,"y":2,"width":3,"height":4{tail}}}"#);
    let bad_extent = [
        r#"{"assetId":"a1","x":1,"y":2,"width":0,"height":4}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":0}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":-3,"height":4}"#,
        r#"{"assetId":"a1","x":1,"y":2,"width":3,"height":-0.5}"#,
    ];
    for text in bad_extent {
        assert_eq!(refused(text), RasterItemError::Extent, "{text}");
    }
    for tail in [r#","opacity":1.5"#, r#","opacity":-0.1"#, r#","opacity":2"#] {
        assert_eq!(refused(&with(tail)), RasterItemError::Opacity, "{tail}");
    }
    for tail in [r#","opacity":0"#, r#","opacity":1"#, r#","opacity":0.25"#] {
        RasterItem::decode(&with(tail)).expect(tail);
    }
    let long = "m".repeat(256);
    for tail in [
        r#","brushGroupId":"""#.to_string(),
        r#","meshId":"""#.to_string(),
        format!(r#","meshId":"{long}""#),
        format!(r#","brushGroupId":"{long}""#),
        r#","meshId":"a\u0000b""#.to_string(),
    ] {
        assert_eq!(refused(&with(&tail)), RasterItemError::Tag, "{tail}");
    }
    RasterItem::decode(&with(&format!(r#","meshId":"{}""#, "m".repeat(255)))).unwrap();
    for text in [
        r#"{"assetId":"","x":1,"y":2,"width":3,"height":4}"#.to_string(),
        r#"{"assetId":"   ","x":1,"y":2,"width":3,"height":4}"#.to_string(),
        format!(
            r#"{{"assetId":"{}","x":1,"y":2,"width":3,"height":4}}"#,
            "a".repeat(256)
        ),
        r#"{"assetId":"a\u0000","x":1,"y":2,"width":3,"height":4}"#.to_string(),
    ] {
        assert_eq!(refused(&text), RasterItemError::AssetId, "{text}");
    }
}

#[test]
fn capture_refuses_non_finite_and_empty_extents_before_any_record() {
    let id1 = || id("a1");
    let tags = RasterTags::default();
    let mut s = surface(8.0, 8.0);
    s.position_x = f64::NAN;
    assert_eq!(
        RasterItem::capture(id1(), &s, &tags),
        Err(RasterItemError::NonFinite)
    );
    let mut s = surface(8.0, 8.0);
    s.matrix_rotation = Some(f64::INFINITY);
    assert_eq!(
        RasterItem::capture(id1(), &s, &tags),
        Err(RasterItemError::NonFinite)
    );
    let mut s = surface(8.0, 8.0);
    s.matrix_rotation = Some(f64::NAN);
    assert_eq!(
        RasterItem::capture(id1(), &s, &tags),
        Err(RasterItemError::NonFinite)
    );
    let mut s = surface(8.0, 8.0);
    s.opacity = Some(f64::NAN);
    assert_eq!(
        RasterItem::capture(id1(), &s, &tags),
        Err(RasterItemError::NonFinite)
    );
    let mut s = surface(8.0, 8.0);
    s.opacity = Some(1.01);
    assert_eq!(
        RasterItem::capture(id1(), &s, &tags),
        Err(RasterItemError::Opacity)
    );
    let mut s = surface(8.0, 8.0);
    s.matrix_rotation = Some(30.0);
    s.scaling_x = f64::MAX;
    s.width = f64::MAX;
    assert_eq!(
        RasterItem::capture(id1(), &s, &tags),
        Err(RasterItemError::NonFinite)
    );
    assert_eq!(
        RasterItem::capture(id1(), &surface(0.0, 8.0), &tags),
        Err(RasterItemError::Extent)
    );
    let mut s = surface(8.0, 8.0);
    s.loaded = false;
    s.pending = Some(PendingGeometry {
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: -4.0,
        rotation: 0.0,
    });
    assert_eq!(
        RasterItem::capture(id1(), &s, &tags),
        Err(RasterItemError::Extent)
    );
    let too_long = RasterTags {
        mesh_id: "m".repeat(256),
        ..RasterTags::default()
    };
    assert_eq!(
        RasterItem::capture(id1(), &surface(8.0, 8.0), &too_long),
        Err(RasterItemError::Tag)
    );
}

#[test]
fn the_reference_resolves_only_to_an_image_in_the_inventory() {
    let item = RasterItem::decode(MINIMAL).unwrap();
    for form in [RasterSourceForm::Embedded, RasterSourceForm::Linked] {
        assert_eq!(
            item.source_form(&inventory("a1", AssetKind::Image, form)),
            Ok(form)
        );
        assert_eq!(
            item.source_form(&inventory("other", AssetKind::Image, form)),
            Err(RasterItemError::MissingAsset)
        );
        for kind in [AssetKind::Video, AssetKind::Audio, AssetKind::Font] {
            assert_eq!(
                item.source_form(&inventory("a1", kind, form)),
                Err(RasterItemError::NotImage)
            );
        }
    }
}

#[test]
fn error_text_is_distinct_per_variant() {
    let all = [
        RasterItemError::Malformed,
        RasterItemError::AssetId,
        RasterItemError::NonFinite,
        RasterItemError::Extent,
        RasterItemError::Opacity,
        RasterItemError::Tag,
        RasterItemError::MissingAsset,
        RasterItemError::NotImage,
    ];
    let mut texts: Vec<String> = all.iter().map(ToString::to_string).collect();
    texts.sort();
    texts.dedup();
    assert_eq!(texts.len(), all.len());
}
