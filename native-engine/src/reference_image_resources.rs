//! Actual immutable snapshot -> retained CPU pixels. No cache or GPU acceptance.
use crate::object_codec::{
    decode_png_pixels_with_workspace, DecodedPngPixels, ImageAssetError, ImageAssetLimits,
    PngWorkspace, ProjectReferenceState,
};
use crate::object_snapshot::charged_png_workspace as charged_workspace;
use crate::revision::{
    shared_image_resource_budget, ImageResourceBudget, ImageResourceCharge, ImageResourceError,
    ObjectSnapshot,
};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceResourceError {
    BareDrawing,
    NoMedia,
    Budget(ImageResourceError),
    Decode(ImageAssetError),
}
impl From<ImageAssetError> for ReferenceResourceError {
    fn from(error: ImageAssetError) -> Self {
        Self::Decode(error)
    }
}

/// Fields drop in declaration order: pixels BEFORE quota, then the original pins.
#[derive(Debug)]
struct RetainedPixels {
    pixels: DecodedPngPixels,
    _charge: ImageResourceCharge,
    snapshot: ObjectSnapshot,
    encoded: Arc<Vec<u8>>,
}
/// Aliases own the same allocation AND charge. No detached Vec/Arc<Vec> accessor.
#[derive(Debug, Clone)]
pub(crate) struct ReferencePixels(Arc<RetainedPixels>);
impl ReferencePixels {
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        self.0.pixels.dimensions()
    }
    pub(crate) fn row_stride(&self) -> usize {
        self.0.pixels.row_stride()
    }
    pub(crate) fn rgba8(&self) -> &[u8] {
        self.0.pixels.rgba8()
    }
    pub(crate) fn encoded_png(&self) -> &[u8] {
        &self.0.encoded
    }
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.0.snapshot
    }
}

pub(crate) fn decode_snapshot_reference(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_managed(
        snapshot,
        limits,
        shared_image_resource_budget(),
        || Ok(PngWorkspace::new()),
        |charge| charge.allocate(),
    )
}

fn decode_managed(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    initialize: impl FnOnce() -> Result<PngWorkspace, ImageResourceError>,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_using_workspace(
        snapshot,
        limits,
        budget,
        allocate,
        || charged_workspace(budget, initialize).map_err(ReferenceResourceError::Budget),
        |pixels| Ok(ReferencePixels(Arc::new(pixels))),
    )
}

// Explicit test-only uncharged workspace factory preserves the RGBA-only controls.
#[cfg(test)]
fn decode_with_budget(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_with_factory(snapshot, limits, budget, allocate, |pixels| {
        Ok(ReferencePixels(Arc::new(pixels)))
    })
}
#[cfg(test)]
fn decode_with_factory(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
    retain: impl FnOnce(RetainedPixels) -> Result<ReferencePixels, ReferenceResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    decode_using_workspace(
        snapshot,
        limits,
        budget,
        allocate,
        || Ok(Box::new(PngWorkspace::new())),
        retain,
    )
}
fn decode_using_workspace<W: AsMut<PngWorkspace>>(
    snapshot: &ObjectSnapshot,
    limits: ImageAssetLimits,
    budget: &ImageResourceBudget,
    allocate: impl FnOnce(&mut ImageResourceCharge) -> Result<Vec<u8>, ImageResourceError>,
    workspace: impl FnOnce() -> Result<W, ReferenceResourceError>,
    retain: impl FnOnce(RetainedPixels) -> Result<ReferencePixels, ReferenceResourceError>,
) -> Result<ReferencePixels, ReferenceResourceError> {
    let project = snapshot
        .project()
        .ok_or(ReferenceResourceError::BareDrawing)?;
    let ProjectReferenceState::StillImage(image) = project.reference_state() else {
        return Err(ReferenceResourceError::NoMedia);
    };
    // Pin the original encoded allocation and immutable document before decoding.
    let snapshot = snapshot.clone();
    let encoded = Arc::clone(image.encoded_allocation());
    // Declared before the decoder's Vec: errors/unwind destroy bytes before charge.
    let mut charge = None;
    let pixels = decode_png_pixels_with_workspace(&encoded, limits, workspace, |cost| {
        charge = Some(
            budget
                .reserve(cost)
                .map_err(ReferenceResourceError::Budget)?,
        );
        allocate(charge.as_mut().expect("reserved before allocation"))
            .map_err(ReferenceResourceError::Budget)
    })?;
    // No fallible work after moving pixels and charge into their lifetime carrier.
    retain(RetainedPixels {
        pixels,
        _charge: charge.expect("positive admitted PNG has a pixel allocation"),
        snapshot,
        encoded,
    })
}

#[cfg(test)]
#[path = "../tests/reference_image_resources.rs"]
mod tests;

// Keep original tiny-quota tests intact; these exercise the production factory.
#[cfg(test)]
mod workspace_tests {
    use super::*;
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
                assert_eq!(budget.used(), Ok(12), "workspace gone before RGBA");
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
}
