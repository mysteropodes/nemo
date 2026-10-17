//! Oracle conformance of the raster item value (G01 / L06-C): every row of the
//! baseline oracle must be reproduced exactly by the native readings.
use super::geometry::{capture_geometry, engine_image_rect};
use super::oracle::{id, inventory, row, rows};
use super::{RasterItem, RasterSourceForm};
use crate::object_document::AssetKind;
use serde_json::Value;

fn encoded(item: &RasterItem) -> Value {
    serde_json::from_str(&item.encode().expect("encode")).expect("encoded JSON")
}

#[test]
fn every_oracle_row_engine_rect_matches_raster_image_rect() {
    let all = rows();
    for r in &all {
        let got = engine_image_rect(&r.surface).expect("finite row");
        assert_eq!(got, r.engine, "engine rect of row {}", r.name);
    }
}

#[test]
fn every_oracle_row_capture_matches_ser_r() {
    for r in &rows() {
        let item = RasterItem::capture(id("asset-1"), &r.surface, &r.tags).expect("capture");
        let enc = encoded(&item);
        let mut have: Vec<&str> = enc
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut want: Vec<&str> = r
            .ser
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        want.push("assetId");
        have.sort_unstable();
        want.sort_unstable();
        assert_eq!(have, want, "member set of row {}", r.name);
        for key in ["x", "y", "width", "height", "opacity"] {
            assert_eq!(
                enc[key].as_f64(),
                r.ser[key].as_f64(),
                "{key} of row {}",
                r.name
            );
        }
        assert_eq!(
            enc["rotation"].as_f64(),
            r.ser["rotation"].as_f64(),
            "rotation of {}",
            r.name
        );
        for key in [
            "isBitmapBrush",
            "brushGroupId",
            "isBrushTextureCopy",
            "meshId",
        ] {
            assert_eq!(enc[key], r.ser[key], "{key} of row {}", r.name);
        }
        assert_eq!(enc["assetId"], "asset-1");
    }
}

#[test]
fn captured_geometry_is_serialized_under_the_baseline_dict_names() {
    // The baseline dict (members x, y, width, height, opacity, rotation, tags) plus an assetId is
    // itself a valid record: it decodes to the same values capture produced.
    for r in &rows() {
        let item = RasterItem::capture(id("asset-1"), &r.surface, &r.tags).expect("capture");
        let mut dict = r.ser.clone();
        dict["assetId"] = Value::from("asset-1");
        let decoded = RasterItem::decode(&dict.to_string()).expect("baseline dict decodes");
        assert_eq!(decoded.center(), item.center(), "centre of {}", r.name);
        assert_eq!(decoded.size(), item.size(), "size of {}", r.name);
        assert_eq!(
            decoded.rotation_authored(),
            item.rotation_authored(),
            "rotation of {}",
            r.name
        );
        assert_eq!(
            decoded.opacity_authored(),
            item.opacity_authored(),
            "opacity of {}",
            r.name
        );
        assert_eq!(
            decoded.is_bitmap_brush(),
            item.is_bitmap_brush(),
            "{}",
            r.name
        );
        assert_eq!(
            decoded.brush_group_id(),
            item.brush_group_id(),
            "{}",
            r.name
        );
        assert_eq!(
            decoded.is_brush_texture_copy(),
            item.is_brush_texture_copy(),
            "{}",
            r.name
        );
        assert_eq!(decoded.mesh_id(), item.mesh_id(), "{}", r.name);
    }
}

#[test]
fn loaded_rows_stored_centre_rect_equals_the_engine_rect() {
    let mut checked = 0;
    for r in rows().iter().filter(|r| r.cross_check) {
        let item = RasterItem::capture(id("asset-1"), &r.surface, &r.tags).expect("capture");
        let shown = item.display_rect();
        // Paper builds `bounds` through its own matrix path, so an unrotated origin may differ by
        // rounding; a missed centre shift or a swapped extent is orders of magnitude larger.
        for (got, want, what) in [
            (shown.x, r.engine.x, "x"),
            (shown.y, r.engine.y, "y"),
            (shown.width, r.engine.width, "width"),
            (shown.height, r.engine.height, "height"),
        ] {
            assert!(
                (got - want).abs() < 1e-9,
                "{what} of row {}: {got} vs {want}",
                r.name
            );
        }
        assert_eq!(
            shown.rotation, r.engine.rotation,
            "rotation of row {}",
            r.name
        );
        checked += 1;
    }
    assert!(checked >= 25, "cross-check rows: {checked}");
}

#[test]
fn source_form_follows_the_inventory_asset_source() {
    for r in &rows() {
        let item = RasterItem::capture(id("asset-1"), &r.surface, &r.tags).expect("capture");
        let (same, other) = match r.form {
            RasterSourceForm::Linked => (RasterSourceForm::Linked, RasterSourceForm::Embedded),
            RasterSourceForm::Embedded => (RasterSourceForm::Embedded, RasterSourceForm::Linked),
        };
        let held = inventory("asset-1", AssetKind::Image, same);
        assert_eq!(item.source_form(&held), Ok(r.form), "row {}", r.name);
        let swapped = inventory("asset-1", AssetKind::Image, other);
        assert_ne!(item.source_form(&swapped), Ok(r.form), "row {}", r.name);
    }
}

#[test]
fn oracle_has_not_been_hollowed_out() {
    let all = rows();
    assert_eq!(all.len(), 36);
    let rotated = all.iter().filter(|r| r.engine.rotation != 0.0).count();
    assert!(rotated >= 14, "rotated rows {rotated}");
    assert!(all.iter().filter(|r| r.surface.pending.is_some()).count() >= 3);
    assert!(all.iter().filter(|r| !r.surface.loaded).count() >= 3);
    assert!(
        all.iter()
            .filter(|r| r.form == RasterSourceForm::Linked)
            .count()
            >= 3
    );
    assert!(
        all.iter()
            .filter(|r| r.ser.get("rotation").is_none())
            .count()
            >= 10
    );
    assert!(all.iter().filter(|r| r.ser.get("meshId").is_some()).count() >= 2);
    // A live Paper mirror flip is a 180 degree rotation plus a negative scale, not a flag.
    let mirror = row(&all, "mirror-x-unrotated");
    assert_eq!(
        (mirror.surface.matrix_rotation, mirror.surface.scaling_y),
        (Some(-180.0), -1.0)
    );
    // Generator drift guards: literal rows.
    let plain = row(&all, "plain-64x32");
    assert_eq!(
        (
            plain.engine.x,
            plain.engine.y,
            plain.engine.width,
            plain.engine.height,
            plain.engine.rotation
        ),
        (168.0, 134.0, 64.0, 32.0, 0.0)
    );
    assert_eq!(plain.ser["x"].as_f64(), Some(200.0));
    let r90 = row(&all, "rot-90");
    assert_eq!(
        (
            r90.engine.x,
            r90.engine.y,
            r90.engine.width,
            r90.engine.height,
            r90.engine.rotation
        ),
        (150.0, 125.0, 100.0, 50.0, 90.0)
    );
    assert_eq!(r90.ser["rotation"].as_f64(), Some(90.0));
}

#[test]
fn pending_geometry_wins_only_while_unloaded() {
    let all = rows();
    let unloaded = row(&all, "pending-rotated");
    let g = capture_geometry(&unloaded.surface).unwrap();
    assert_eq!(
        (g.x, g.y, g.width, g.height, g.rotation),
        (300.0, 200.0, 80.0, 40.0, Some(25.0))
    );
    let loaded = row(&all, "pending-ignored-when-loaded");
    let g = capture_geometry(&loaded.surface).unwrap();
    assert_eq!((g.x, g.y, g.width, g.height), (200.0, 150.0, 64.0, 32.0));
    assert_eq!(g.rotation, loaded.surface.matrix_rotation);
}
