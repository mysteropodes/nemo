//! Codec error adaptation for the existing property-holder admission.
use super::{
    ImageMeshStore, ObjectCodecError, ObjectCodecErrorKind, ObjectLayer, ObjectRecord, Presence,
    PropertyHolder,
};
pub(crate) fn validate_property_holders(
    holders: &Presence<Vec<PropertyHolder>>,
    layers: &[ObjectLayer],
    records: &[ObjectRecord],
    plan: Option<&crate::object_document::OrderPlan>,
    tables: Option<&crate::object_document::ComponentTables>,
    items: Option<&crate::object_document::ItemRecords>,
    meshes: Option<&ImageMeshStore>,
) -> Result<(), ObjectCodecError> {
    use crate::object_document::PropertyAdmissionError;
    PropertyHolder::validate_with_items(holders, layers, records, plan, tables, items, meshes)
        .map_err(|error| {
            let (kind, message) = match error {
                PropertyAdmissionError::Invalid(message) => {
                    (ObjectCodecErrorKind::Invalid, message)
                }
                PropertyAdmissionError::Unsupported(message) => {
                    (ObjectCodecErrorKind::Unsupported, message)
                }
                PropertyAdmissionError::DuplicateId(message) => {
                    (ObjectCodecErrorKind::DuplicateId, message)
                }
            };
            ObjectCodecError::new(kind, message)
        })
}
