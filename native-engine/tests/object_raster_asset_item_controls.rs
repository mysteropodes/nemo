//! Negative and mutation controls of the raster item value (G01 / L06-C).
//!
//! Each control is a standalone copy of the baseline readings with exactly one
//! flaw. The un-flawed copy must reproduce every oracle row (so the copy is a
//! faithful stand-in); every flawed copy must disagree with the baseline oracle
//! on a NAMED row. The literal pins are worked by hand and do not read the
//! oracle.
use super::geometry::{
    capture_geometry, engine_image_rect, ImageRect, RasterSurface, SurfaceBounds,
};
use super::oracle::{id, row, rows, Row};
use super::{RasterItem, RasterTags};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flaw {
    None,
    SwapExtent,
    NoCentreShift,
    RotationTwice,
    SignedScaling,
    BoundsAlways,
}

/// `rasterImageRect` with one optional flaw, written independently of the native code.
fn engine_variant(s: &RasterSurface, flaw: Flaw) -> ImageRect {
    let rot = s.matrix_rotation.unwrap_or(0.0);
    if rot == 0.0 || flaw == Flaw::BoundsAlways {
        return ImageRect {
            x: s.bounds.x,
            y: s.bounds.y,
            width: s.bounds.width,
            height: s.bounds.height,
            rotation: 0.0,
        };
    }
    let (mut sw, mut sh) = if flaw == Flaw::SignedScaling {
        (s.scaling_x * s.width, s.scaling_y * s.height)
    } else {
        (s.scaling_x.abs() * s.width, s.scaling_y.abs() * s.height)
    };
    if flaw == Flaw::SwapExtent {
        std::mem::swap(&mut sw, &mut sh);
    }
    let (dx, dy) = if flaw == Flaw::NoCentreShift {
        (0.0, 0.0)
    } else {
        (sw / 2.0, sh / 2.0)
    };
    ImageRect {
        x: s.position_x - dx,
        y: s.position_y - dy,
        width: sw,
        height: sh,
        rotation: if flaw == Flaw::RotationTwice {
            rot * 2.0
        } else {
            rot
        },
    }
}

fn mismatching(all: &[Row], flaw: Flaw) -> Vec<&str> {
    all.iter()
        .filter(|r| engine_variant(&r.surface, flaw) != r.engine)
        .map(|r| r.name.as_str())
        .collect()
}

#[test]
fn the_unflawed_copy_reproduces_every_row() {
    let all = rows();
    assert!(mismatching(&all, Flaw::None).is_empty());
}

#[test]
fn engine_rect_flaws_each_fail_a_named_oracle_row() {
    let all = rows();
    for (flaw, must_fail) in [
        (Flaw::SwapExtent, "rot-30"),
        (Flaw::NoCentreShift, "rot-90"),
        (Flaw::RotationTwice, "rot-30"),
        (Flaw::SignedScaling, "stub-rotated-negative-scale"),
        (Flaw::BoundsAlways, "rot-30-scaled-2x0.5"),
    ] {
        let bad = mismatching(&all, flaw);
        assert!(
            bad.contains(&must_fail),
            "{flaw:?} must fail row {must_fail}, failed {bad:?}"
        );
    }
}

/// The stored-dict reading of `serR` with one optional flaw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CaptureFlaw {
    None,
    PendingIgnored,
    RotationAlwaysEmitted,
    BoundsWhenRotated,
    TopLeftOrigin,
}

fn capture_variant(r: &Row, flaw: CaptureFlaw) -> (f64, f64, f64, f64, Option<f64>) {
    let s = &r.surface;
    if let (false, Some(p), true) = (s.loaded, s.pending, flaw != CaptureFlaw::PendingIgnored) {
        return (
            p.x,
            p.y,
            p.width,
            p.height,
            if p.rotation != 0.0 {
                Some(p.rotation)
            } else {
                None
            },
        );
    }
    let rot = s.matrix_rotation.unwrap_or(0.0);
    let rotated = rot != 0.0 && flaw != CaptureFlaw::BoundsWhenRotated;
    let width = if rotated {
        s.scaling_x.abs() * s.width
    } else {
        s.bounds.width
    };
    let height = if rotated {
        s.scaling_y.abs() * s.height
    } else {
        s.bounds.height
    };
    let (x, y) = if flaw == CaptureFlaw::TopLeftOrigin {
        (s.bounds.x, s.bounds.y)
    } else {
        (s.position_x, s.position_y)
    };
    let rotation = if rot != 0.0 || flaw == CaptureFlaw::RotationAlwaysEmitted {
        Some(rot)
    } else {
        None
    };
    (x, y, width, height, rotation)
}

fn capture_matches(r: &Row, flaw: CaptureFlaw) -> bool {
    let got = capture_variant(r, flaw);
    let want = (
        r.ser["x"].as_f64().unwrap(),
        r.ser["y"].as_f64().unwrap(),
        r.ser["width"].as_f64().unwrap(),
        r.ser["height"].as_f64().unwrap(),
        r.ser.get("rotation").and_then(|v| v.as_f64()),
    );
    got == want
}

#[test]
fn capture_flaws_each_fail_a_named_oracle_row() {
    let all = rows();
    assert!(all.iter().all(|r| capture_matches(r, CaptureFlaw::None)));
    for (flaw, must_fail) in [
        (CaptureFlaw::PendingIgnored, "pending-rotated"),
        (CaptureFlaw::RotationAlwaysEmitted, "plain-64x32"),
        (CaptureFlaw::BoundsWhenRotated, "rot-45-mirror-x"),
        (CaptureFlaw::TopLeftOrigin, "plain-64x32"),
    ] {
        let r = row(&all, must_fail);
        assert!(
            !capture_matches(r, flaw),
            "{flaw:?} must fail row {must_fail}"
        );
    }
}

#[test]
fn the_native_readings_agree_with_the_unflawed_copies_on_every_row() {
    for r in &rows() {
        assert_eq!(
            engine_image_rect(&r.surface).unwrap(),
            engine_variant(&r.surface, Flaw::None)
        );
        let g = capture_geometry(&r.surface).unwrap();
        assert_eq!(
            (g.x, g.y, g.width, g.height, g.rotation),
            capture_variant(r, CaptureFlaw::None)
        );
    }
}

fn hand(rotation: Option<f64>, sx: f64, sy: f64) -> RasterSurface {
    RasterSurface {
        matrix_rotation: rotation,
        scaling_x: sx,
        scaling_y: sy,
        width: 10.0,
        height: 20.0,
        position_x: 100.0,
        position_y: 50.0,
        bounds: SurfaceBounds {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        },
        loaded: true,
        pending: None,
        opacity: None,
    }
}

#[test]
fn literal_pins_worked_by_hand() {
    // Rotated: natural size times |scaling|, centred on position, rotation reported separately.
    let r = engine_image_rect(&hand(Some(90.0), 2.0, -3.0)).unwrap();
    assert_eq!(
        r,
        ImageRect {
            x: 90.0,
            y: 20.0,
            width: 20.0,
            height: 60.0,
            rotation: 90.0
        }
    );
    // Unrotated (or no matrix): the bounds, whatever position or scaling say.
    for rotation in [Some(0.0), Some(-0.0), None] {
        let r = engine_image_rect(&hand(rotation, 2.0, -3.0)).unwrap();
        assert_eq!(
            r,
            ImageRect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
                rotation: 0.0
            }
        );
    }
    // Stored dict: centre, display size, rotation only when non-zero.
    let g = capture_geometry(&hand(Some(90.0), 2.0, -3.0)).unwrap();
    assert_eq!(
        (g.x, g.y, g.width, g.height, g.rotation),
        (100.0, 50.0, 20.0, 60.0, Some(90.0))
    );
    let g = capture_geometry(&hand(Some(0.0), 2.0, -3.0)).unwrap();
    assert_eq!(
        (g.x, g.y, g.width, g.height, g.rotation),
        (100.0, 50.0, 3.0, 4.0, None)
    );
    // The record's own rect is the rotated branch for any rotation, including absent (= 0).
    let item = RasterItem::capture(
        id("a1"),
        &hand(Some(90.0), 2.0, -3.0),
        &RasterTags::default(),
    )
    .unwrap();
    assert_eq!(
        item.display_rect(),
        ImageRect {
            x: 90.0,
            y: 20.0,
            width: 20.0,
            height: 60.0,
            rotation: 90.0
        }
    );
    assert_eq!(item.opacity_authored(), Some(1.0));
}

#[test]
fn truthiness_of_passthrough_tags_matches_the_baseline() {
    let s = hand(Some(0.0), 1.0, 1.0);
    let off = RasterItem::capture(id("a1"), &s, &RasterTags::default()).unwrap();
    assert_eq!(
        (
            off.is_bitmap_brush(),
            off.brush_group_id(),
            off.is_brush_texture_copy(),
            off.mesh_id()
        ),
        (None, None, None, None)
    );
    let on = RasterTags {
        is_bitmap_brush: true,
        brush_group_id: "bg".into(),
        is_brush_texture_copy: true,
        mesh_id: "im".into(),
    };
    let item = RasterItem::capture(id("a1"), &s, &on).unwrap();
    assert_eq!(
        (
            item.is_bitmap_brush(),
            item.brush_group_id(),
            item.is_brush_texture_copy(),
            item.mesh_id()
        ),
        (Some(true), Some("bg"), Some(true), Some("im"))
    );
}
