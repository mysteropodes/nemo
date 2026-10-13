//! Controlled image payload bytes only; decoder scratch and GPU storage are separate gates.
use std::sync::{Arc, Mutex, OnceLock};

const MAX_BYTES: usize = 384 * 1024 * 1024;
static SHARED: OnceLock<ImageResourceBudget> = OnceLock::new();
pub(crate) fn shared_image_resource_budget() -> &'static ImageResourceBudget {
    SHARED.get_or_init(|| ImageResourceBudget::new(MAX_BYTES).expect("fixed image payload quota"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageResourceError {
    Limit,
    Capacity,
    Allocation,
    Unavailable,
}

#[derive(Debug, Clone)]
pub(crate) struct ImageResourceBudget {
    limit: usize,
    used: Arc<Mutex<usize>>,
}
impl ImageResourceBudget {
    fn new(limit: usize) -> Result<Self, ImageResourceError> {
        if limit == 0 || limit > MAX_BYTES {
            return Err(ImageResourceError::Limit);
        }
        Ok(Self {
            limit,
            used: Arc::new(Mutex::new(0)),
        })
    }
    pub(crate) fn reserve(&self, cost: usize) -> Result<ImageResourceCharge, ImageResourceError> {
        if cost == 0 || cost > self.limit {
            return Err(ImageResourceError::Capacity);
        }
        let mut used = self
            .used
            .lock()
            .map_err(|_| ImageResourceError::Unavailable)?;
        *used = used
            .checked_add(cost)
            .filter(|n| *n <= self.limit)
            .ok_or(ImageResourceError::Capacity)?;
        Ok(ImageResourceCharge {
            budget: self.clone(),
            cost,
            allocated: false,
        })
    }
    #[cfg(test)]
    pub(crate) fn isolated(limit: usize) -> Result<Self, ImageResourceError> {
        Self::new(limit)
    }
    #[cfg(test)]
    pub(crate) fn used(&self) -> Result<usize, ImageResourceError> {
        self.used
            .lock()
            .map(|n| *n)
            .map_err(|_| ImageResourceError::Unavailable)
    }
}

/// Not cloneable: move into the opaque carrier AFTER its pixel field. Never detach it.
#[derive(Debug)]
pub(crate) struct ImageResourceCharge {
    budget: ImageResourceBudget,
    cost: usize,
    allocated: bool,
}
impl ImageResourceCharge {
    pub(crate) fn allocate(&mut self) -> Result<Vec<u8>, ImageResourceError> {
        self.allocate_after(|| Ok(()))
    }
    fn allocate_after(
        &mut self,
        before: impl FnOnce() -> Result<(), ImageResourceError>,
    ) -> Result<Vec<u8>, ImageResourceError> {
        if self.allocated || self.budget.used.is_poisoned() {
            return Err(ImageResourceError::Unavailable);
        }
        let layout =
            std::alloc::Layout::array::<u8>(self.cost).map_err(|_| ImageResourceError::Capacity)?;
        self.allocated = true;
        before()?;
        // SAFETY: positive checked u8 layout has alignment 1 and size <= isize::MAX.
        // The global allocator initializes all cost bytes. Null is never adopted.
        // Immediately transfer unique ownership to Vec using the identical global
        // allocation/deallocation layout, len and capacity; no fallible work between.
        let pointer = unsafe { std::alloc::alloc_zeroed(layout) };
        if pointer.is_null() {
            return Err(ImageResourceError::Allocation);
        }
        Ok(unsafe { Vec::from_raw_parts(pointer, self.cost, self.cost) })
    }
}
impl Drop for ImageResourceCharge {
    fn drop(&mut self) {
        // Poison/corruption fails closed. Do not panic or make damaged quota reusable.
        if let Ok(mut used) = self.budget.used.lock() {
            if let Some(remaining) = used.checked_sub(self.cost) {
                *used = remaining;
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/image_resource_budget.rs"]
mod tests;
