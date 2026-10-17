//! Charged retained brush bitmap and its two-phase publication handle (L07-D / G06).
//!
//! A `BrushBitmap` is an `Arc` over the pixel storage AND the `ImageResourceCharge`
//! that paid for it, declared pixels first, so the quota becomes reusable only
//! after the storage is gone and exactly when the LAST alias drops (the store's
//! own entry, a scene or a lease). `PendingBrush` is the not-yet-visible half:
//! dropping it (cancel, any failure, a refused commit) releases the charge and
//! the id mark, so nothing partial is ever published.

use crate::revision::{ImageResourceCharge, ImageResourceError};
use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, Mutex};

/// Namespace of the scheduler-declared resource id (L07-C merges these).
pub(crate) const RESOURCE_ID_PREFIX: &str = "bitmap-brush:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BrushStoreError {
    /// Empty id (the baseline caller skips a falsy engine id).
    InvalidId,
    InvalidDimensions,
    CostOverflow,
    PixelLength {
        expected: usize,
        actual: usize,
    },
    /// Limit / Capacity / Allocation / Unavailable from the shared domain.
    Budget(ImageResourceError),
    /// A live pending publication already holds a charge for this id.
    PendingConflict,
    ForeignPending,
    /// `commit` before `fill`.
    Incomplete,
    Unavailable,
}

/// Fields drop in declaration order: pixels BEFORE the quota.
struct RetainedBrush {
    pixels: Vec<u8>,
    _charge: ImageResourceCharge,
    id: String,
    width: u32,
    height: u32,
    version: u64,
}

/// Aliases own the same allocation AND charge; no detached pixel accessor.
#[derive(Clone)]
pub(crate) struct BrushBitmap(Arc<RetainedBrush>);
impl BrushBitmap {
    pub(crate) fn id(&self) -> &str {
        &self.0.id
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }
    pub(crate) fn rgba8(&self) -> &[u8] {
        &self.0.pixels
    }
    /// Charged bytes (`w*h*4`).
    pub(crate) fn cost(&self) -> usize {
        self.0.pixels.len()
    }
    pub(crate) fn version(&self) -> u64 {
        self.0.version
    }
    pub(crate) fn same_allocation_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Live aliases, the store's own entry included.
    pub(crate) fn holders(&self) -> usize {
        Arc::strong_count(&self.0)
    }
    /// The `(resource id, version)` pair a scheduler key declares for this bitmap.
    pub(crate) fn declared_resource(&self) -> (String, String) {
        (
            format!("{RESOURCE_ID_PREFIX}{}", self.0.id),
            self.0.version.to_string(),
        )
    }
}
impl fmt::Debug for BrushBitmap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrushBitmap")
            .field("id", &self.0.id)
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .field("version", &self.0.version)
            .finish()
    }
}

/// Ids with a live pending publication, shared by a store and its handles.
pub(crate) type Marks = Arc<Mutex<BTreeSet<String>>>;
pub(crate) fn release_mark(marks: &Mutex<BTreeSet<String>>, id: &str) {
    // A poisoned registry keeps the mark: fail closed, never reopen the id.
    if let Ok(mut marks) = marks.lock() {
        marks.remove(id);
    }
}

/// A reserved, allocated, not yet visible publication.
pub(crate) struct PendingBrush {
    /// Pixels drop before the charge.
    parts: Option<(Vec<u8>, ImageResourceCharge)>,
    id: String,
    width: u32,
    height: u32,
    cost: usize,
    filled: bool,
    marks: Marks,
}
impl PendingBrush {
    pub(crate) fn new(
        parts: (Vec<u8>, ImageResourceCharge),
        id: &str,
        dimensions: (u32, u32),
        marks: &Marks,
    ) -> Self {
        let cost = parts.0.len();
        Self {
            parts: Some(parts),
            id: id.to_string(),
            width: dimensions.0,
            height: dimensions.1,
            cost,
            filled: false,
            marks: Arc::clone(marks),
        }
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    pub(crate) fn cost(&self) -> usize {
        self.cost
    }
    pub(crate) fn belongs_to(&self, marks: &Marks) -> bool {
        Arc::ptr_eq(&self.marks, marks)
    }
    pub(crate) fn fill(&mut self, rgba: &[u8]) -> Result<(), BrushStoreError> {
        if rgba.len() != self.cost {
            return Err(BrushStoreError::PixelLength {
                expected: self.cost,
                actual: rgba.len(),
            });
        }
        let Some((pixels, _)) = self.parts.as_mut() else {
            return Err(BrushStoreError::Unavailable);
        };
        pixels.copy_from_slice(rgba);
        self.filled = true;
        Ok(())
    }
    pub(crate) fn cancel(self) {}
    /// Moves the charged pixels into a published bitmap of `version`.
    pub(crate) fn into_bitmap(mut self, version: u64) -> Result<BrushBitmap, BrushStoreError> {
        if !self.filled {
            return Err(BrushStoreError::Incomplete);
        }
        let Some((pixels, charge)) = self.parts.take() else {
            return Err(BrushStoreError::Unavailable);
        };
        Ok(BrushBitmap(Arc::new(RetainedBrush {
            pixels,
            _charge: charge,
            id: self.id.clone(),
            width: self.width,
            height: self.height,
            version,
        })))
    }
}
impl fmt::Debug for PendingBrush {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingBrush")
            .field("id", &self.id)
            .field("cost", &self.cost)
            .field("filled", &self.filled)
            .finish()
    }
}
impl Drop for PendingBrush {
    fn drop(&mut self) {
        release_mark(&self.marks, &self.id);
    }
}
