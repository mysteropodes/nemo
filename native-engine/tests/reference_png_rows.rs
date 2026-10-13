use super::*;
use crate::object_codec::decode_png_pixels_with_workspace;

#[test]
fn actual_charged_row_overlaps_rgba_and_releases_while_pixel_aliases_remain() {
    let snapshot = tests::workspace_fixture();
    let budget = ImageResourceBudget::isolated(24).unwrap();
    let pixels = decode_using_buffers(
        &snapshot,
        tests::limits(),
        &budget,
        |charge| {
            assert_eq!(budget.used(), Ok(24), "row plus current RGBA");
            charge.allocate()
        },
        || Ok(Box::new(PngWorkspace::new())),
        |n| {
            assert_eq!(n, 12);
            budget
                .allocate_bytes(n)
                .map_err(ReferenceResourceError::Budget)
        },
        |pixels| Ok(ReferencePixels(Arc::new(pixels))),
    )
    .unwrap();
    assert_eq!(budget.used(), Ok(12));
    assert_eq!(
        pixels.rgba8(),
        [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
    );
    let alias = pixels.clone();
    drop(pixels);
    assert_eq!(budget.used(), Ok(12));
    drop(alias);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn row_refusal_bad_length_and_allocator_error_stop_before_pixel_write() {
    let source = tests::workspace_png();
    for length in [0, 11, 13] {
        let result = decode_png_pixels_with_buffers::<ReferenceResourceError, _, _>(
            &source,
            tests::limits(),
            || Ok(Box::new(PngWorkspace::new())),
            |_| Ok(vec![0; length]),
            |_| panic!("bad row must refuse before pixel write"),
        );
        assert!(matches!(
            result,
            Err(ReferenceResourceError::Decode(ImageAssetError::Allocation))
        ));
    }
    let budget = ImageResourceBudget::isolated(8).unwrap();
    let refused = decode_png_pixels_with_buffers::<ReferenceResourceError, _, _>(
        &source,
        tests::limits(),
        || Ok(Box::new(PngWorkspace::new())),
        |n| {
            budget
                .allocate_bytes(n)
                .map_err(ReferenceResourceError::Budget)
        },
        |_| unreachable!(),
    );
    assert!(matches!(
        refused,
        Err(ReferenceResourceError::Budget(ImageResourceError::Capacity))
    ));
    let failed = decode_png_pixels_with_buffers::<ReferenceResourceError, _, Vec<u8>>(
        &source,
        tests::limits(),
        || Ok(Box::new(PngWorkspace::new())),
        |_| {
            Err(ReferenceResourceError::Budget(
                ImageResourceError::Allocation,
            ))
        },
        |_| unreachable!(),
    );
    assert!(failed.is_err());
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn charged_row_and_pixels_clean_up_on_error_unwind_and_late_crc() {
    let budget = ImageResourceBudget::isolated(24).unwrap();
    for stage in 0..3 {
        let mut source = tests::workspace_png();
        if stage == 2 {
            *source.last_mut().unwrap() ^= 1;
        }
        let mut pixel_charge = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decode_png_pixels_with_buffers::<ReferenceResourceError, _, _>(
                &source,
                tests::limits(),
                || Ok(Box::new(PngWorkspace::new())),
                |n| {
                    budget
                        .allocate_bytes(n)
                        .map_err(ReferenceResourceError::Budget)
                },
                |n| {
                    pixel_charge = Some(budget.reserve(n).map_err(ReferenceResourceError::Budget)?);
                    assert_eq!(budget.used(), Ok(24));
                    match stage {
                        0 => Err(ReferenceResourceError::Budget(
                            ImageResourceError::Allocation,
                        )),
                        1 => panic!("pixel callback unwind with live row"),
                        _ => pixel_charge
                            .as_mut()
                            .unwrap()
                            .allocate()
                            .map_err(ReferenceResourceError::Budget),
                    }
                },
            )
        }));
        if stage == 1 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        assert_eq!(
            budget.used(),
            Ok(12),
            "row gone, outer pixel token still held"
        );
        drop(pixel_charge);
        assert_eq!(budget.used(), Ok(0));
    }
}
#[test]
fn retained_pixels_and_managed_wire_validation_compete_in_one_cpu_domain() {
    let pin = tests::workspace_fixture();
    let budget =
        ImageResourceBudget::isolated(std::mem::size_of::<crate::object_codec::PngWorkspace>())
            .unwrap();
    let pixels = crate::read_queries::reference_image_resources::decode_managed(
        &pin,
        tests::limits(),
        &budget,
        || Ok(crate::object_codec::PngWorkspace::new()),
        |c| c.allocate(),
    )
    .unwrap();
    let bytes = serde_json::to_vec(&crate::object_codec::encoded_wire_test::wire()).unwrap();
    assert_eq!(
        ObjectSnapshot::from_managed_project_wire_cpu_test(
            "wire".into(),
            &bytes,
            crate::object_codec::encoded_wire_test::limits(),
            &budget
        )
        .unwrap_err(),
        crate::object_codec::ProjectWireError::Budget
    );
    assert_eq!(budget.used(), Ok(12));
    drop(pixels);
    let snapshot = ObjectSnapshot::from_managed_project_wire_cpu_test(
        "wire".into(),
        &bytes,
        crate::object_codec::encoded_wire_test::limits(),
        &budget,
    )
    .unwrap();
    assert!(snapshot.project().is_some());
    assert_eq!(
        budget.used(),
        Ok(0),
        "wire retains encoded source only, never RGBA"
    );
}
#[test]
fn actual_workspace_competes_with_prior_pixels_and_dies_before_current_pixels() {
    let snapshot = tests::workspace_fixture();
    let cost = std::mem::size_of::<PngWorkspace>();
    let budget = ImageResourceBudget::isolated(cost).unwrap();
    let pixels = decode_managed(
        &snapshot,
        tests::limits(),
        &budget,
        || {
            assert_eq!(budget.used(), Ok(cost));
            Ok(PngWorkspace::new())
        },
        |charge| {
            assert_eq!(
                budget.used(),
                Ok(24),
                "strict workspace gone; row plus RGBA remain"
            );
            charge.allocate()
        },
    )
    .unwrap();
    assert_eq!(budget.used(), Ok(12));
    assert_eq!(
        decode_managed(
            &snapshot,
            tests::limits(),
            &budget,
            || panic!("prior pixels must refuse workspace before initialization"),
            |_| panic!("no pixel allocation on workspace refusal")
        )
        .unwrap_err(),
        ReferenceResourceError::Budget(ImageResourceError::Capacity)
    );
    let alias = pixels.clone();
    drop(pixels);
    assert_eq!(budget.used(), Ok(12));
    drop(alias);
    let next = decode_managed(
        &snapshot,
        tests::limits(),
        &budget,
        || Ok(PngWorkspace::new()),
        |c| c.allocate(),
    )
    .unwrap();
    assert_eq!(
        next.rgba8(),
        [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
    );
    drop(next);
    assert_eq!(budget.used(), Ok(0));
}
#[test]
fn workspace_error_unwind_and_malformed_stream_free_actual_box_and_charge() {
    use crate::reference_image_asset::measure_allocations;
    let layout = std::alloc::Layout::new::<PngWorkspace>();
    let budget = ImageResourceBudget::isolated(layout.size()).unwrap();
    let mut source = tests::workspace_png();
    source[61] ^= 1;
    let crc = tests::crc(&source[37..62]);
    source[62..66].copy_from_slice(&crc);
    for stage in 0..3 {
        let (result, calls) = measure_allocations(Some(layout), || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                decode_png_pixels_with_workspace::<ReferenceResourceError, _>(
                    &source,
                    tests::limits(),
                    || {
                        charged_workspace(&budget, || {
                            assert_eq!(budget.used(), Ok(layout.size()));
                            match stage {
                                0 => Err(ImageResourceError::Allocation),
                                1 => panic!("workspace initializer unwind"),
                                _ => Ok(PngWorkspace::new()),
                            }
                        })
                        .map_err(ReferenceResourceError::Budget)
                    },
                    |_| panic!("no RGBA after failed workspace verification"),
                )
            }))
        });
        assert_eq!(calls, [1, 1, 0]);
        if stage == 1 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        assert_eq!(budget.used(), Ok(0));
    }
}
#[test]
fn concurrent_managed_producers_cannot_overbook_workspace_storage() {
    let snapshot = tests::workspace_fixture();
    let budget = ImageResourceBudget::isolated(std::mem::size_of::<PngWorkspace>()).unwrap();
    let ready = Arc::new(std::sync::Barrier::new(2));
    let release = Arc::new(std::sync::Barrier::new(2));
    let (b, r, exit, pin) = (
        budget.clone(),
        ready.clone(),
        release.clone(),
        snapshot.clone(),
    );
    let worker = std::thread::spawn(move || {
        decode_managed(
            &pin,
            tests::limits(),
            &b,
            || {
                r.wait();
                exit.wait();
                Ok(PngWorkspace::new())
            },
            |c| c.allocate(),
        )
    });
    ready.wait();
    let refused = decode_managed(
        &snapshot,
        tests::limits(),
        &budget,
        || panic!("concurrent workspace must refuse"),
        |_| unreachable!(),
    );
    release.wait();
    let pixels = worker.join().unwrap().unwrap();
    assert_eq!(
        refused.unwrap_err(),
        ReferenceResourceError::Budget(ImageResourceError::Capacity)
    );
    assert_eq!(budget.used(), Ok(12));
    drop(pixels);
    assert_eq!(budget.used(), Ok(0));
}
