//! Admission of the one stored bitmap-anchor extension, never a pixel owner.
use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::bitmap_brush::{self as value, BitmapBrushValueError};
pub(crate) use crate::object_document::bitmap_brush::{
    rgba, trim_holder_consumed, PROCEDURAL_TIPS,
};
use crate::object_document::{BitmapBrushFields, ObjectRecord};

fn codec_error(error: BitmapBrushValueError) -> ObjectCodecError {
    match error {
        BitmapBrushValueError::Invalid(message) => {
            ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message)
        }
        BitmapBrushValueError::Unsupported(message) => {
            ObjectCodecError::new(ObjectCodecErrorKind::Unsupported, message)
        }
    }
}
pub(crate) fn validate_fields(fields: &BitmapBrushFields) -> Result<(), ObjectCodecError> {
    value::validate_fields(fields).map_err(codec_error)
}
pub(crate) fn validate(record: &ObjectRecord) -> Result<(), ObjectCodecError> {
    value::validate_record(record).map_err(codec_error)
}
#[cfg(test)]
#[path = "../tests/object_bitmap_brush_codec.rs"]
mod tests;
