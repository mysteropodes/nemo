//! Read-only property sampling facade; no second owner or publication authority.
use crate::object_codec::PropertyHolderTarget;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
impl ObjectSnapshot {
    /// Raw values from this exact retained snapshot. Expressions are not evaluated.
    pub fn sample_property_raw(
        &self,
        target: &PropertyHolderTarget,
        property: &str,
        frame: f64,
    ) -> Result<Vec<f64>, DispatchErrorCode> {
        #[cfg(feature = "evaluation")]
        {
            use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
            sample_property_raw(self, target, property, frame).map_err(|error| match error {
                PropertySampleError::InvalidFrame => DispatchErrorCode::InvalidRequest,
                PropertySampleError::MissingTarget => DispatchErrorCode::NotFound,
                PropertySampleError::UnsupportedProperty | PropertySampleError::Unavailable => {
                    DispatchErrorCode::Unavailable
                }
            })
        }
        #[cfg(not(feature = "evaluation"))]
        {
            let _ = (target, property, frame);
            Err(DispatchErrorCode::Unavailable)
        }
    }
}
