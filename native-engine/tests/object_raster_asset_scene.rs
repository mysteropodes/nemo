//! Oracle conformance of the raster scene join (G01 / L06-K): the final image item
//! of the real baseline (element, layer and parent stages, opacity product,
//! `rotation || 0`) and the baseline engine's placement affine, for every case,
//! plus the L06-C record reaching the scene.

use super::oracle::{read, Case, Pixels, ASSET, ORACLE, TRIG_TOLERANCE};
use super::{image_placement, prepare_raster_scene_image, RasterSceneImage};
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::request_receipts::sha256;
use std::sync::Arc;

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn close(got: f64, want: f64, trig: bool) -> bool {
    if trig {
        (got - want).abs() <= TRIG_TOLERANCE * got.abs().max(want.abs()).max(1.0)
    } else {
        got == want
    }
}

pub(super) fn prepare(case: &Case) -> RasterSceneImage<Arc<Vec<u8>>> {
    let port = Pixels::new(case.pixels);
    prepare_raster_scene_image(&case.source(), &case.stages(), &port)
        .unwrap_or_else(|e| panic!("{}: {e:?}", case.name))
}

#[test]
fn the_oracle_is_the_pinned_real_baseline_run() {
    assert!(ORACLE.contains("at fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert_eq!(
        hex(sha256(ORACLE.as_bytes())),
        "81d3486c2d56d7b17d075ba0f9ccfb72ad6693c8a1304b644ed8df5c301dcfb1"
    );
    let cases = read();
    assert_eq!(cases.len(), 8);
    assert_eq!(cases.iter().filter(|c| c.trig).count(), 2);
    assert_eq!(cases.iter().filter(|c| c.element.is_some()).count(), 3);
    assert_eq!(cases.iter().filter(|c| c.motion.is_some()).count(), 5);
    assert_eq!(cases.iter().map(|c| c.parents.len()).sum::<usize>(), 5);
    assert_eq!(cases.iter().filter(|c| c.opacity.is_none()).count(), 4);
}

#[test]
fn final_rect_opacity_and_placement_match_the_baseline_for_every_case() {
    for case in read() {
        let image = prepare(&case);
        let rect = image.rect();
        let got_rect = [
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.rotation,
            image.opacity(),
        ];
        for (k, (g, w)) in got_rect.iter().zip(case.want_rect).enumerate() {
            assert!(
                close(*g, w, case.trig),
                "{} rect[{k}] {g} vs {w}",
                case.name
            );
        }
        let place = image.placement().as_coeffs();
        for (k, (g, w)) in place.iter().zip(case.want_place).enumerate() {
            assert!(
                close(*g, w, case.trig),
                "{} place[{k}] {g} vs {w}",
                case.name
            );
        }
        assert_eq!(image.pixel_size(), case.pixels);
        assert_eq!(image.asset_id(), ASSET);
        assert_eq!(image.opacity_f32(), case.want_rect[5] as f32);
    }
}

#[test]
fn exact_cases_are_bit_exact_and_only_rotation_cases_need_the_tolerance() {
    let mut exact = 0;
    for case in read().iter().filter(|c| !c.trig) {
        let image = prepare(case);
        let rect = image.rect();
        assert_eq!(
            [rect.x, rect.y, rect.width, rect.height, rect.rotation],
            [
                case.want_rect[0],
                case.want_rect[1],
                case.want_rect[2],
                case.want_rect[3],
                case.want_rect[4]
            ],
            "{}",
            case.name
        );
        assert_eq!(image.opacity(), case.want_rect[5], "{}", case.name);
        assert_eq!(
            image.placement().as_coeffs(),
            case.want_place,
            "{}",
            case.name
        );
        exact += 1;
    }
    assert_eq!(exact, 6);
}

#[test]
fn a_mirrored_rect_keeps_its_negative_width_and_a_mirrored_placement() {
    let cases = read();
    let case = cases
        .iter()
        .find(|c| c.name == "mirrored_negative_scale")
        .unwrap();
    let image = prepare(case);
    assert_eq!(image.rect().width, -16.0);
    assert_eq!(image.rect().x, 20.0);
    assert_eq!(
        image.placement().as_coeffs(),
        [-2.0, 0.0, 0.0, 2.0, 20.0, 4.0]
    );
}

#[test]
fn an_unrotated_rect_places_with_translate_and_scale_only() {
    let rect = ImageRect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
        rotation: 0.0,
    };
    assert_eq!(
        image_placement(&rect, (200, 100)).as_coeffs(),
        [0.5, 0.0, 0.0, 0.5, 10.0, 20.0]
    );
    // A rotation of 90 degrees about the display centre (60, 45): the unit axes swap.
    let turned = ImageRect {
        rotation: 90.0,
        ..rect
    };
    let c = image_placement(&turned, (200, 100)).as_coeffs();
    let want = [0.0, 0.5, -0.5, 0.0, 85.0, -5.0];
    for (g, w) in c.iter().zip(want) {
        assert!((g - w).abs() < 1e-12, "{c:?}");
    }
}

#[test]
fn the_l06c_record_reaches_the_scene_with_its_display_rect_and_opacity() {
    let item = RasterItem::decode(
        r#"{"assetId":"asset-1","x":200,"y":150.5,"width":64,"height":32.25,"opacity":0.5,"rotation":30}"#,
    )
    .unwrap();
    let display = item.display_rect();
    let source = super::RasterSceneSource {
        asset_id: item.asset_id().as_str().to_owned(),
        rect: ImageRect {
            x: display.x,
            y: display.y,
            width: display.width,
            height: display.height,
            rotation: display.rotation,
        },
        opacity: item.opacity(),
    };
    let port = Pixels::new((16, 8));
    let image = prepare_raster_scene_image(&source, &[], &port).unwrap();
    let rect = image.rect();
    // centre (200, 150.5), size 64 x 32.25, rotation 30, opacity 0.5: all exact binary fractions.
    assert_eq!(
        [rect.x, rect.y, rect.width, rect.height, rect.rotation],
        [168.0, 134.375, 64.0, 32.25, 30.0]
    );
    assert_eq!(image.opacity(), 0.5);
    assert_eq!(image.opacity_f32(), 0.5_f32);
}

/// #2372 A-image-text-flip-skew: the stored mirror and shear are applied about the final rect's centre, in the rect's own frame, after the
/// placement; a plain item keeps its placement exactly.
#[test]
fn a_stored_mirror_and_shear_move_the_placement_about_the_final_centre() {
    use crate::object_document::object_raster_asset_kernels::object_item_local_form::LocalForm;
    use vello::kurbo::Point;
    let item = RasterItem::decode(r#"{"assetId":"asset-1","x":50,"y":30,"width":40,"height":20}"#).unwrap();
    let display = item.display_rect();
    let source = super::RasterSceneSource {
        asset_id: item.asset_id().as_str().to_owned(),
        rect: ImageRect { x: display.x, y: display.y, width: display.width, height: display.height, rotation: 0.0 },
        opacity: 1.0,
    };
    let port = Pixels::new((40, 20));
    let plain = prepare_raster_scene_image(&source, &[], &port).unwrap();
    let same = plain.clone().with_form(&LocalForm::default());
    assert_eq!((same.placement(), same.world_form()), (plain.placement(), None));
    // pixel (0, 0) is the top-left corner (30, 20); mirrored about x = 50 it lands on (70, 20), and the bottom-left follows.
    let mirrored = plain.clone().with_form(&LocalForm { flip_x: true, flip_y: false, skew: 0.0 });
    let at = |image: &RasterSceneImage<Arc<Vec<u8>>>, x: f64, y: f64| image.placement() * Point::new(x, y);
    assert_eq!((at(&plain, 0.0, 0.0), at(&mirrored, 0.0, 0.0), at(&mirrored, 0.0, 20.0)), (Point::new(30.0, 20.0), Point::new(70.0, 20.0), Point::new(70.0, 40.0)));
    // A 45 degree skew along x moves a point by its distance from the centre row: the top row slides left by 10, the bottom right by 10.
    let sheared = plain.with_form(&LocalForm { flip_x: false, flip_y: false, skew: 45.0 });
    let (top, bottom) = (at(&sheared, 0.0, 0.0), at(&sheared, 0.0, 20.0));
    assert!((top.x - 20.0).abs() < 1e-9 && (bottom.x - 40.0).abs() < 1e-9 && top.y == 20.0 && bottom.y == 40.0, "{top:?} {bottom:?}");
}
