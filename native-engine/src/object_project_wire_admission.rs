//! coherent managed project wire admission; no host activation.
use crate::object_codec;
use crate::object_snapshot::validate_png_in_domain;
use crate::revision::{shared_image_resource_budget, ImageResourceBudget, ObjectSnapshot};

impl ObjectSnapshot {
    /// Private managed wire admission; no host route or attach/replace command.
    #[cfg(all(feature = "history", feature = "project_media_codec"))]
    pub(crate) fn from_managed_project_wire(
        instance: String,
        bytes: &[u8],
        limits: object_codec::ProjectWireLimits,
    ) -> Result<Self, object_codec::ProjectWireError> {
        Self::managed_project_wire(
            instance,
            bytes,
            limits,
            Self::reserve_encoded,
            |p| p.allocate().map_err(|_| "encoded allocation unavailable"),
            |p, a| p.bind(a).map_err(|_| "encoded binding unavailable"),
            Self::from_admitted_project,
        )
    }
    #[cfg(all(feature = "history", feature = "project_media_codec"))]
    pub(super) fn managed_project_wire<R>(
        instance: String,
        bytes: &[u8],
        limits: object_codec::ProjectWireLimits,
        reserve: impl FnMut(usize) -> Result<R, &'static str>,
        allocate: impl FnMut(&mut R) -> Result<Vec<u8>, &'static str>,
        bind: impl FnMut(&mut R, &std::sync::Arc<Vec<u8>>) -> Result<(), &'static str>,
        identity: impl FnOnce(String, object_codec::ProjectEnvelope) -> Result<Self, &'static str>,
    ) -> Result<Self, object_codec::ProjectWireError> {
        Self::managed_project_wire_using(
            instance,
            bytes,
            limits,
            reserve,
            allocate,
            bind,
            identity,
            |source, png| validate_png_in_domain(source, png, shared_image_resource_budget()),
        )
    }
    // Test-only domain selection; production entry always uses the shared domain.
    #[cfg(all(test, feature = "history", feature = "project_media_codec"))]
    pub(crate) fn from_managed_project_wire_cpu_test(
        instance: String,
        bytes: &[u8],
        limits: object_codec::ProjectWireLimits,
        cpu: &ImageResourceBudget,
    ) -> Result<Self, object_codec::ProjectWireError> {
        Self::managed_project_wire_using(
            instance,
            bytes,
            limits,
            Self::reserve_encoded,
            |p| p.allocate().map_err(|_| "encoded allocation unavailable"),
            |p, a| p.bind(a).map_err(|_| "encoded binding unavailable"),
            Self::from_admitted_project,
            |source, png| validate_png_in_domain(source, png, cpu),
        )
    }
    #[cfg(all(feature = "history", feature = "project_media_codec"))]
    pub(super) fn managed_project_wire_using<R>(
        instance: String,
        bytes: &[u8],
        limits: object_codec::ProjectWireLimits,
        mut reserve: impl FnMut(usize) -> Result<R, &'static str>,
        mut allocate: impl FnMut(&mut R) -> Result<Vec<u8>, &'static str>,
        mut bind: impl FnMut(&mut R, &std::sync::Arc<Vec<u8>>) -> Result<(), &'static str>,
        identity: impl FnOnce(String, object_codec::ProjectEnvelope) -> Result<Self, &'static str>,
        validate: impl FnMut(
            &[u8],
            object_codec::ImageAssetLimits,
        ) -> Result<(), object_codec::ProjectWireError>,
    ) -> Result<Self, object_codec::ProjectWireError> {
        use object_codec::ProjectWireError;
        if instance.is_empty() {
            return Err(ProjectWireError::Invalid);
        }
        // Prepare validates all structure and payload lengths without allocating
        // binary storage. All permits exist before the first allocator callback.
        let plan = object_codec::prepare_project_wire(bytes, limits)?;
        let lengths = plan.encoded_lengths();
        // Declared before decoded project: model/buffers die before unbound
        // permits on every return or unwind, including partial decoding/binding.
        let mut permits = Vec::new();
        permits
            .try_reserve_exact(lengths.len())
            .map_err(|_| ProjectWireError::Budget)?;
        for length in lengths {
            permits.push(reserve(length).map_err(|_| ProjectWireError::Budget)?);
        }
        let mut next = 0;
        let decoded = plan.decode(
            |length| {
                let permit = permits.get_mut(next).ok_or(ProjectWireError::Invalid)?;
                next += 1;
                let bytes = allocate(permit).map_err(|_| ProjectWireError::Budget)?;
                if bytes.len() != length || bytes.capacity() != length {
                    return Err(ProjectWireError::Budget);
                }
                Ok(bytes)
            },
            validate,
        )?;
        if next != permits.len() || decoded.allocations().len() != permits.len() {
            drop(decoded);
            return Err(ProjectWireError::Invalid);
        }
        let binding = permits
            .iter_mut()
            .zip(decoded.allocations())
            .try_for_each(|(permit, allocation)| bind(permit, allocation));
        if binding.is_err() {
            drop(decoded); // Drop all bytes before releasing unbound permits.
            return Err(ProjectWireError::Budget);
        }
        let project = decoded.into_project();
        identity(instance, project).map_err(|_| ProjectWireError::Invalid)
    }
}

#[cfg(test)]
#[path = "../tests/project_asset_wire.rs"]
mod asset_tests;
