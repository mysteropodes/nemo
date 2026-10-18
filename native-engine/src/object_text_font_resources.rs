//! Charged immutable bundled font bytes in the existing read/resource layer.
//! The shared CPU payload quota is reused; no new budget, cache or font selector.
use crate::revision::{
    shared_image_resource_budget, ImageResourceBudget, ImageResourceCharge, ImageResourceError,
};
use std::sync::Arc;
#[derive(Debug)]
struct Storage {
    bytes: Arc<Vec<u8>>,
    _charge: ImageResourceCharge,
}
#[derive(Debug, Clone)]
pub(crate) struct BundledFontAllocation(Arc<Storage>);
impl BundledFontAllocation {
    pub(crate) fn retain(bytes: &'static [u8]) -> Result<Self, ImageResourceError> {
        Self::retain_using(bytes, shared_image_resource_budget())
    }
    fn retain_using(
        bytes: &[u8],
        budget: &ImageResourceBudget,
    ) -> Result<Self, ImageResourceError> {
        let mut charge = budget.reserve(bytes.len())?;
        let mut allocation = charge.allocate()?;
        allocation.copy_from_slice(bytes);
        Ok(Self(Arc::new(Storage {
            bytes: Arc::new(allocation),
            _charge: charge,
        })))
    }
    pub(crate) fn allocation(&self) -> &Arc<Vec<u8>> {
        &self.0.bytes
    }
}
pub(crate) fn font_version(bytes: &[u8]) -> String {
    crate::request_receipts::sha256(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn font_payload_budget_refusal_is_atomic_and_alias_retains_one_charge() {
        let tiny = ImageResourceBudget::isolated(1).unwrap();
        assert!(BundledFontAllocation::retain_using(&[1, 2], &tiny).is_err());
        assert_eq!(tiny.used().unwrap(), 0);
        let budget = ImageResourceBudget::isolated(2).unwrap();
        let held = BundledFontAllocation::retain_using(&[1, 2], &budget).unwrap();
        let alias = held.clone();
        assert_eq!(budget.used().unwrap(), 2);
        assert!(Arc::ptr_eq(held.allocation(), alias.allocation()));
        drop(held);
        assert_eq!(budget.used().unwrap(), 2);
        drop(alias);
        assert_eq!(budget.used().unwrap(), 0);
    }
}
