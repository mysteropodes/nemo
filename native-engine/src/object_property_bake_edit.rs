//! Seal a native-produced replacement in the existing property transaction.
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::object_document::{
    AuthoredOrder, Presence, PropertyHolder, PropertyHolderTarget, PropertyRecord,
};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;

impl ObjectFillOwner {
    pub(crate) fn prepare_baked_order(
        origin: ObjectSnapshot,
        target: &PropertyHolderTarget,
        replacement: AuthoredOrder,
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        let mut document = origin.document().clone();
        let old = document
            .property_order_mut(target)
            .ok_or(DispatchErrorCode::NotFound)?;
        let changed = *old != replacement;
        *old = replacement;
        ObjectSnapshot::validate_drawing(&document).map_err(|_| DispatchErrorCode::Unavailable)?;
        Ok(PreparedPropertyEdit {
            origin,
            document,
            changed,
        })
    }
    pub(crate) fn prepare_baked_property(
        origin: ObjectSnapshot,
        target: &PropertyHolderTarget,
        replacement: PropertyRecord,
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        let mut document = origin.document().clone();
        let mut holders = document
            .property_holders
            .value()
            .cloned()
            .ok_or(DispatchErrorCode::NotFound)?;
        let holder = holders
            .iter_mut()
            .find(|h| h.target() == target)
            .ok_or(DispatchErrorCode::NotFound)?;
        let old = holder
            .properties
            .iter_mut()
            .find(|p| p.property == replacement.property)
            .ok_or(DispatchErrorCode::NotFound)?;
        let changed = *old != replacement;
        *old = replacement;
        PropertyHolder::validate_with_items(
            &Presence::Present(holders.clone()),
            &document.layers,
            &document.objects,
            document.composition_order.value(),
            document.component_tables(),
            document.items.value(),
            document.image_meshes.value(),
        )
        .map_err(|_| DispatchErrorCode::Unavailable)?;
        document.property_holders = Presence::Present(holders);
        Ok(PreparedPropertyEdit {
            origin,
            document,
            changed,
        })
    }
}

impl ObjectFillOwner {
    /// Seal only the timeout flips produced by an already-retired native read.
    /// The expected drawing lets that exact caller prove a later replay source.
    pub(crate) fn prepare_expression_disables(
        origin: ObjectSnapshot,
        targets: &[(PropertyHolderTarget, String)],
    ) -> Result<(PreparedPropertyEdit, crate::object_document::ObjectDocument), DispatchErrorCode>
    {
        let mut document = origin.document().clone();
        let mut seen = Vec::new();
        for (target, property) in targets {
            if seen.iter().any(|entry| *entry == (target, property)) {
                continue;
            }
            seen.push((target, property));
            let expression = if property == "order" {
                &mut document
                    .property_order_mut(target)
                    .ok_or(DispatchErrorCode::NotFound)?
                    .expression
            } else {
                let Presence::Present(holders) = &mut document.property_holders else {
                    return Err(DispatchErrorCode::NotFound);
                };
                &mut holders
                    .iter_mut()
                    .find(|holder| holder.target() == target)
                    .ok_or(DispatchErrorCode::NotFound)?
                    .properties
                    .iter_mut()
                    .find(|record| record.property() == property.as_str())
                    .ok_or(DispatchErrorCode::NotFound)?
                    .expression
            };
            let Presence::Present(expression) = expression else {
                return Err(DispatchErrorCode::NotFound);
            };
            if !expression.enabled {
                return Err(DispatchErrorCode::StaleRevision);
            }
            expression.enabled = false;
        }
        ObjectSnapshot::validate_drawing(&document).map_err(|_| DispatchErrorCode::Unavailable)?;
        let changed = &document != origin.document();
        let expected = document.clone();
        Ok((
            PreparedPropertyEdit {
                origin,
                document,
                changed,
            },
            expected,
        ))
    }
}
