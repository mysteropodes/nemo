use crate::object_codec::{
    decode_png_pixels, encoded_wire_test, ImageAssetError, ImageAssetLimits,
};
use crate::read_queries::reference_image_resources::{
    decode_snapshot_reference, decode_with_budget, decode_with_factory, ReferencePixels,
    ReferenceResourceError,
};
use crate::revision::{ImageResourceBudget, ImageResourceError, ObjectSnapshot};
use base64::Engine;
use std::cell::Cell;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

// Frozen original corpus, independent manual RGBA expectations below.
const RGBA: &str = "89504e470d0a1a0a0000000d49484452000000030000000108060000001be014b40000001549444154789c63105432feef129ad650de319301001b41047d056017fe0000000049454e44ae426082";
const RGB: &str = "89504e470d0a1a0a0000000d4948445200000002000000020802000000fdd49a730000001649444154789c63e051b2389162c4c0cec1c9c5cd0300124401fe0da78d180000000049454e44ae426082";
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
pub(super) fn limits() -> ImageAssetLimits {
    ImageAssetLimits::new(1024, 64, 4096, 4, 4).unwrap()
}
fn snapshot(hex: &str) -> ObjectSnapshot {
    let mut wire = encoded_wire_test::wire();
    wire["assets"][0]["data"] = base64::prelude::BASE64_STANDARD.encode(bytes(hex)).into();
    ObjectSnapshot::from_managed_project_wire(
        "pixels".into(),
        &serde_json::to_vec(&wire).unwrap(),
        encoded_wire_test::limits(),
    )
    .unwrap()
}
fn decode(snapshot: &ObjectSnapshot, budget: &ImageResourceBudget) -> ReferencePixels {
    decode_with_budget(snapshot, limits(), budget, |c| c.allocate()).unwrap()
}
pub(super) fn workspace_fixture() -> ObjectSnapshot {
    snapshot(RGBA)
}
pub(super) fn workspace_png() -> Vec<u8> {
    bytes(RGBA)
}

#[test]
fn actual_managed_snapshots_have_literal_rgb_and_straight_rgba_pixels() {
    for (hex, dimensions, expected) in [
        (
            RGBA,
            (3, 1),
            vec![17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0],
        ),
        (
            RGB,
            (2, 2),
            vec![
                12, 34, 56, 255, 200, 100, 50, 255, 7, 8, 9, 255, 10, 11, 12, 255,
            ],
        ),
    ] {
        let snapshot = snapshot(hex);
        let budget = ImageResourceBudget::isolated(64).unwrap();
        let original = snapshot.project().unwrap().reference_state();
        let crate::object_codec::ProjectReferenceState::StillImage(image) = original else {
            panic!()
        };
        let source = image.encoded_allocation();
        let weak = Arc::downgrade(source);
        let pixels = decode(&snapshot, &budget);
        assert_eq!(pixels.dimensions(), dimensions);
        assert_eq!(pixels.row_stride(), dimensions.0 as usize * 4);
        assert_eq!(pixels.rgba8(), expected);
        assert_eq!(pixels.0.pixels.capacity(), expected.len());
        assert_eq!(pixels.encoded_png(), bytes(hex));
        assert!(Arc::ptr_eq(&pixels.0.encoded, source));
        assert!(std::ptr::eq(
            pixels.snapshot().project().unwrap(),
            snapshot.project().unwrap()
        ));
        assert_eq!(pixels.snapshot().snapshot_id(), snapshot.snapshot_id());
        drop(image);
        drop(snapshot);
        assert!(weak.upgrade().is_some());
        assert_eq!(budget.used(), Ok(expected.len()));
        drop(pixels);
        assert_eq!(budget.used(), Ok(0));
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn aliases_share_one_charge_but_distinct_decodes_compete() {
    let snapshot = snapshot(RGBA);
    let budget = ImageResourceBudget::isolated(24).unwrap();
    let pixels = decode(&snapshot, &budget);
    let alias = pixels.clone();
    assert!(std::ptr::eq(
        pixels.rgba8().as_ptr(),
        alias.rgba8().as_ptr()
    ));
    assert_eq!(budget.used(), Ok(12));
    let second = decode(&snapshot, &budget);
    assert_ne!(pixels.rgba8().as_ptr(), second.rgba8().as_ptr());
    assert_eq!(budget.used(), Ok(24));
    let called = Cell::new(false);
    assert_eq!(
        decode_with_budget(&snapshot, limits(), &budget, |_| {
            called.set(true);
            panic!("quota refusal must precede RGBA allocator")
        })
        .unwrap_err(),
        ReferenceResourceError::Budget(ImageResourceError::Capacity)
    );
    assert!(!called.get());
    drop(pixels);
    assert_eq!(budget.used(), Ok(24));
    drop(alias);
    assert_eq!(budget.used(), Ok(12));
    drop(second);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn original_history_snapshot_stays_pinned_across_edit_undo_and_owner_drop() {
    let managed = snapshot(RGB);
    let mut owner = crate::history::NativeObjectHistory::from_admitted_project(
        "history",
        managed.project().unwrap().clone(),
    )
    .unwrap();
    let original = owner.acquire_snapshot(0).unwrap();
    let budget = ImageResourceBudget::isolated(16).unwrap();
    let pixels = decode(&original, &budget);
    let prepared = owner
        .prepare_reference_edit(crate::commands::ReferenceEdit::Remove)
        .unwrap();
    assert_eq!(owner.commit_reference_edit(prepared), Ok(1));
    assert_eq!(
        decode_with_budget(
            &owner.acquire_snapshot(1).unwrap(),
            limits(),
            &budget,
            |_| panic!("NoMedia must refuse before allocation")
        )
        .unwrap_err(),
        ReferenceResourceError::NoMedia
    );
    assert_eq!(owner.undo(1), Ok(2));
    assert_eq!(owner.redo(2), Ok(3));
    drop(owner);
    drop(original);
    drop(managed);
    assert_eq!(pixels.snapshot().content_revision(), 0);
    assert_eq!(
        pixels.rgba8(),
        [12, 34, 56, 255, 200, 100, 50, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    assert_eq!(budget.used(), Ok(16));
    drop(pixels);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn pixel_storage_drops_before_charge_on_success_failure_and_unwind() {
    let snapshot = snapshot(RGBA);
    let budget = ImageResourceBudget::isolated(12).unwrap();
    let dropped = Arc::new(AtomicUsize::new(0));
    for stage in 0..3 {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decode_with_factory(
                &snapshot,
                limits(),
                &budget,
                |c| c.allocate(),
                |mut retained| {
                    let (budget, dropped) = (budget.clone(), dropped.clone());
                    retained.pixels.observe_release(move || {
                        // This hook runs after the real Vec field's deallocation.
                        assert_eq!(budget.used(), Ok(12));
                        dropped.fetch_add(1, Ordering::SeqCst);
                    });
                    match stage {
                        0 => Ok(ReferencePixels(Arc::new(retained))),
                        1 => Err(ReferenceResourceError::Budget(
                            ImageResourceError::Allocation,
                        )),
                        _ => panic!("carrier construction unwind"),
                    }
                },
            )
        }));
        if stage == 0 {
            let pixels = result.unwrap().unwrap();
            let alias = pixels.clone();
            drop(pixels);
            assert_eq!(dropped.load(Ordering::SeqCst), 0);
            drop(alias);
        } else if stage == 1 {
            assert!(result.unwrap().is_err());
        } else {
            assert!(result.is_err());
        }
        assert_eq!(dropped.load(Ordering::SeqCst), stage + 1);
        assert_eq!(budget.used(), Ok(0));
    }
}

#[test]
fn allocator_failure_wrong_buffer_and_unwind_never_escape_a_charge() {
    let snapshot = snapshot(RGBA);
    let budget = ImageResourceBudget::isolated(12).unwrap();
    for stage in 0..3 {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            decode_with_budget(&snapshot, limits(), &budget, |_| {
                assert_eq!(budget.used(), Ok(12));
                match stage {
                    0 => Err(ImageResourceError::Allocation),
                    1 => Ok(vec![]),
                    _ => panic!("allocator unwind"),
                }
            })
        }));
        if stage == 2 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        assert_eq!(budget.used(), Ok(0));
    }
}

pub(super) fn crc(input: &[u8]) -> [u8; 4] {
    let mut value = !0u32;
    for &byte in input {
        value ^= u32::from(byte);
        for _ in 0..8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ 0xedb88320
            } else {
                value >> 1
            };
        }
    }
    (!value).to_be_bytes()
}
#[test]
fn malformed_and_late_checksums_drop_pixels_and_then_release_the_outer_charge() {
    let good = bytes(RGBA);
    // Corrupt IEND CRC: failure happens after decoded rows/allocation.
    let mut late = good.clone();
    *late.last_mut().unwrap() ^= 1;
    // Corrupt zlib Adler while keeping the IDAT chunk CRC valid.
    let mut adler = good.clone();
    adler[61] ^= 1;
    let check = crc(&adler[37..62]);
    adler[62..66].copy_from_slice(&check);
    let budget = ImageResourceBudget::isolated(12).unwrap();
    let mut late_calls = 0;
    for source in [late, adler, good[..good.len() - 1].to_vec(), vec![0; 12]] {
        let mut charge = None;
        let called = Cell::new(false);
        let result = decode_png_pixels::<ReferenceResourceError>(&source, limits(), |n| {
            called.set(true);
            charge = Some(budget.reserve(n).map_err(ReferenceResourceError::Budget)?);
            charge
                .as_mut()
                .unwrap()
                .allocate()
                .map_err(ReferenceResourceError::Budget)
        });
        assert!(matches!(
            result,
            Err(ReferenceResourceError::Decode(ImageAssetError::Malformed))
        ));
        if called.get() {
            late_calls += 1;
            assert_eq!(budget.used(), Ok(12));
        }
        drop(result);
        drop(charge);
        assert_eq!(budget.used(), Ok(0));
    }
    assert!(
        late_calls > 0,
        "a real post-allocation decoder failure must be covered"
    );
}

#[test]
fn typed_bare_drawing_and_limits_refusal_plus_shared_production_entry() {
    let snapshot = snapshot(RGBA);
    let bare = ObjectSnapshot::from_admitted("bare".into(), snapshot.document().clone()).unwrap();
    let budget = ImageResourceBudget::isolated(12).unwrap();
    assert_eq!(
        decode_with_budget(&bare, limits(), &budget, |_| unreachable!()).unwrap_err(),
        ReferenceResourceError::BareDrawing
    );
    let small = ImageAssetLimits::new(1024, 11, 4096, 4, 4).unwrap();
    assert_eq!(
        decode_with_budget(&snapshot, small, &budget, |_| unreachable!()).unwrap_err(),
        ReferenceResourceError::Decode(ImageAssetError::DecodedBudget)
    );
    assert_eq!(budget.used(), Ok(0));
    let (pixels, calls) = crate::reference_image_asset::measure_allocations(
        Some(std::alloc::Layout::new::<crate::object_codec::PngWorkspace>()),
        || decode_snapshot_reference(&snapshot, limits()).unwrap(),
    );
    assert_eq!(
        calls,
        [1, 1, 0],
        "production owns and frees exactly one workspace"
    );
    assert_eq!(
        pixels.rgba8(),
        [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
    );
}
