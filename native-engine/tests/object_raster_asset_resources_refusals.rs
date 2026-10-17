//! L06-E refusals: typed pre-decode errors, decoder failures, quota refusals and
//! allocator failures. Every case ends with the private budget at zero and no
//! handle. Cancellation and the baseline contrast are in the `cancel` module.
use super::oracle::{
    arc_pixels, budget, embedded, inventory, linked, one_image, png, project,
    project_without_assets, request, retain_in, rows, snapshot, unhex, workspace_bytes,
};
use super::{resolve_in, retain_core, AssetPixelsError, Request};
use crate::object_codec::{ImageAssetError, ImageAssetLimits, PngWorkspace};
use crate::object_document::AssetKind;
use crate::revision::{ImageResourceBudget, ImageResourceError, ObjectSnapshot};
use std::cell::Cell;
use std::sync::Arc;

#[test]
fn every_refusal_before_the_decoder_is_a_distinct_typed_error_with_no_work_and_no_charge() {
    let plain = snapshot(vec![
        embedded(
            "ok",
            "image/png",
            AssetKind::Image,
            Arc::new(png("rgba-3x1")),
        ),
        linked(
            "remote",
            AssetKind::Image,
            "https://example.invalid/never-opened.png",
        ),
        linked("remote-audio", AssetKind::Audio, "file:///never-opened.wav"),
    ]);
    let bare = ObjectSnapshot::from_admitted("bare".into(), plain.document().clone()).unwrap();
    let no_catalog =
        ObjectSnapshot::from_admitted_project("nocat".into(), project_without_assets()).unwrap();
    let budget = budget();
    let long = "x".repeat(256);
    let cases: Vec<(&ObjectSnapshot, &str, AssetPixelsError)> = vec![
        (&bare, "ok", AssetPixelsError::BareDrawing),
        (&no_catalog, "ok", AssetPixelsError::Missing),
        (&plain, "absent", AssetPixelsError::Missing),
        (&plain, "", AssetPixelsError::Missing),
        (&plain, "a\0b", AssetPixelsError::Missing),
        (&plain, long.as_str(), AssetPixelsError::Missing),
        (&plain, "remote", AssetPixelsError::Unavailable),
        (&plain, "remote-audio", AssetPixelsError::WrongKind),
    ];
    let ((), calls) = crate::reference_image_asset::measure_allocations(
        Some(std::alloc::Layout::new::<PngWorkspace>()),
        || {
            for (snapshot, id, expected) in &cases {
                let result = retain_in(&budget, snapshot, id, &[]);
                assert_eq!(result.unwrap_err(), *expected, "{id:?}");
            }
        },
    );
    assert_eq!(calls, [0, 0, 0], "no refusal reaches the decoder workspace");
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn resolution_refuses_non_image_kinds_and_embedded_types_without_a_decoder() {
    let fine = Arc::new(png("rgba-3x1"));
    let inventory = inventory(vec![
        embedded("fine", "image/png", AssetKind::Image, Arc::clone(&fine)),
        embedded(
            "jpeg",
            "image/jpeg",
            AssetKind::Image,
            Arc::new(vec![0xff, 0xd8, 0xff, 0xe0]),
        ),
        embedded(
            "upper",
            "Image/PNG",
            AssetKind::Image,
            Arc::new(png("rgba-3x1")),
        ),
        embedded(
            "wav",
            "audio/wav",
            AssetKind::Audio,
            Arc::new(vec![1, 2, 3, 4]),
        ),
        embedded(
            "mislabelled",
            "image/png",
            AssetKind::Audio,
            Arc::new(png("rgba-3x1")),
        ),
        linked("remote", AssetKind::Image, "opaque-locator"),
        linked("remote-video", AssetKind::Video, "opaque-locator"),
    ]);
    assert!(Arc::ptr_eq(&resolve_in(&inventory, "fine").unwrap(), &fine));
    for (id, expected) in [
        ("jpeg", AssetPixelsError::DecoderUnavailable),
        ("upper", AssetPixelsError::DecoderUnavailable), // exact type, as the owner compares it
        ("wav", AssetPixelsError::WrongKind),
        ("mislabelled", AssetPixelsError::WrongKind), // the kind decides, not the media type
        ("remote", AssetPixelsError::Unavailable),
        ("remote-video", AssetPixelsError::WrongKind),
        ("missing", AssetPixelsError::Missing),
    ] {
        assert_eq!(resolve_in(&inventory, id).unwrap_err(), expected, "{id}");
    }
}

#[test]
fn the_owner_does_not_admit_embedded_non_png_so_those_refusals_are_defensive_until_it_does() {
    for record in [
        embedded(
            "jpeg",
            "image/jpeg",
            AssetKind::Image,
            Arc::new(vec![0xff, 0xd8, 0xff, 0xe0]),
        ),
        embedded(
            "wav",
            "audio/wav",
            AssetKind::Audio,
            Arc::new(vec![1, 2, 3, 4]),
        ),
    ] {
        let admitted = ObjectSnapshot::from_admitted_project("x".into(), project(vec![record]));
        assert!(admitted.is_err(), "project_asset_admission.rs refuses it");
    }
}

#[test]
fn a_linked_image_is_unavailable_even_when_its_locator_names_a_readable_png() {
    let path = std::env::temp_dir().join(format!("nemo-l06e-readable-{}.png", std::process::id()));
    std::fs::write(&path, png("rgba-3x1")).unwrap();
    let locator = path.to_str().unwrap().to_string();
    let snapshot = snapshot(vec![linked("remote", AssetKind::Image, &locator)]);
    let result = retain_in(&budget(), &snapshot, "remote", &[]);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(result.unwrap_err(), AssetPixelsError::Unavailable);
}

#[test]
fn corrupt_unsupported_and_zero_sized_inputs_are_typed_decode_errors_that_release_every_charge() {
    let failing: Vec<_> = rows("png")
        .into_iter()
        .filter(|row| row["expect"] != "ok")
        .collect();
    assert_eq!(failing.len(), 5);
    for row in failing {
        let name = row["name"].as_str().unwrap();
        let snapshot = snapshot(vec![embedded(
            "a",
            "image/png",
            AssetKind::Image,
            Arc::new(unhex(row["hex"].as_str().unwrap())),
        )]);
        let budget = budget();
        let allocated = Cell::new(false);
        let result = retain_core(
            &request(&snapshot, "a", &[]),
            &budget,
            |charge| {
                allocated.set(true);
                charge.allocate()
            },
            arc_pixels,
        );
        match row["expect"].as_str().unwrap() {
            "Malformed" => assert_eq!(
                result.unwrap_err(),
                AssetPixelsError::Decode(ImageAssetError::Malformed),
                "{name}"
            ),
            "Unsupported" => assert_eq!(
                result.unwrap_err(),
                AssetPixelsError::Decode(ImageAssetError::Unsupported),
                "{name}"
            ),
            "any" => assert!(matches!(result, Err(AssetPixelsError::Decode(_))), "{name}"),
            other => panic!("unknown expectation {other}"),
        }
        if row["late"] == true {
            assert!(
                allocated.get(),
                "{name}: the failure follows the pixel allocation"
            );
        }
        assert_eq!(budget.used(), Ok(0), "{name}");
    }
}

#[test]
fn decoder_limits_map_to_distinct_decode_errors_and_oversize_leaves_the_budget_untouched() {
    let run = |name: &str, bounds: ImageAssetLimits| {
        let snapshot = one_image(name);
        let budget = budget();
        let bounded = Request {
            limits: bounds,
            ..request(&snapshot, "a", &[])
        };
        let result = retain_core(&bounded, &budget, |charge| charge.allocate(), arc_pixels);
        assert_eq!(budget.used(), Ok(0), "{name}");
        result.unwrap_err()
    };
    let wide = ImageAssetLimits::new(1 << 20, 1 << 20, 4 << 20, 4, 4).unwrap();
    let small_rgba = ImageAssetLimits::new(1024, 11, 4096, 4, 4).unwrap();
    let small_source = ImageAssetLimits::new(10, 1024, 4096, 4, 4).unwrap();
    assert_eq!(
        run("rgba-5x1", wide),
        AssetPixelsError::Decode(ImageAssetError::DimensionBudget)
    );
    assert_eq!(
        run("rgba-3x1", small_rgba),
        AssetPixelsError::Decode(ImageAssetError::DecodedBudget)
    );
    assert_eq!(
        run("rgba-3x1", small_source),
        AssetPixelsError::Decode(ImageAssetError::EncodedBudget)
    );
}

#[test]
fn quota_refusals_return_budget_errors_with_zero_retained_bytes_and_no_allocator_call() {
    let workspace = workspace_bytes();
    let small = one_image("rgba-3x1");
    let called = Cell::new(false);
    let tight = ImageResourceBudget::isolated(workspace - 1).unwrap();
    let result = retain_core(
        &request(&small, "a", &[]),
        &tight,
        |charge| {
            called.set(true);
            charge.allocate()
        },
        arc_pixels,
    );
    assert_eq!(
        result.unwrap_err(),
        AssetPixelsError::Budget(ImageResourceError::Capacity)
    );
    assert!(!called.get());
    assert_eq!(tight.used(), Ok(0));
    // Exactly the workspace is enough for a 12-byte image: pixels are all that stays charged.
    let exact = ImageResourceBudget::isolated(workspace).unwrap();
    let kept = retain_in(&exact, &small, "a", &[]).unwrap();
    assert_eq!(exact.used(), Ok(12));
    drop(kept);
    assert_eq!(exact.used(), Ok(0));
    // The workspace fits but 262144 RGBA bytes do not: refused at the pixel reservation.
    assert!(workspace + 1024 < 262_144);
    let big = one_image("rgba-256x256-flat");
    let pressured = ImageResourceBudget::isolated(workspace + 1024).unwrap();
    let result = retain_core(
        &request(&big, "a", &[]),
        &pressured,
        |charge| {
            called.set(true);
            charge.allocate()
        },
        arc_pixels,
    );
    assert_eq!(
        result.unwrap_err(),
        AssetPixelsError::Budget(ImageResourceError::Capacity)
    );
    assert!(
        !called.get(),
        "the quota refusal precedes the pixel allocator"
    );
    assert_eq!(pressured.used(), Ok(0));
}

#[test]
fn allocator_failure_wrong_buffer_and_unwind_never_escape_a_charge() {
    let snapshot = one_image("rgba-3x1");
    let budget = budget();
    for stage in 0..3 {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            retain_core(
                &request(&snapshot, "a", &[]),
                &budget,
                |_| {
                    // The 12-byte pixel charge is held (the row buffer is charged too).
                    assert!(budget.used().unwrap() >= 12);
                    match stage {
                        0 => Err(ImageResourceError::Allocation),
                        1 => Ok(vec![]),
                        _ => panic!("allocator unwind"),
                    }
                },
                arc_pixels,
            )
        }));
        match stage {
            0 => assert_eq!(
                result.unwrap().unwrap_err(),
                AssetPixelsError::Budget(ImageResourceError::Allocation)
            ),
            1 => assert_eq!(
                result.unwrap().unwrap_err(),
                AssetPixelsError::Decode(ImageAssetError::Allocation)
            ),
            _ => assert!(result.is_err()),
        }
        assert_eq!(budget.used(), Ok(0));
    }
}
