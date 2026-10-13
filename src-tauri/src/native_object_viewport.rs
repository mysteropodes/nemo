//! One original object GPU in the existing main-window viewport slot.
use crate::native_application_contract::{host_error, HostResult};
use crate::native_application_viewport::{RetainedViewport, MAIN_VIEWPORT};
use crate::native_viewport::{NativeViewport, NativeViewportConsumer};
use native_engine::{compositor::Compositor, desktop_viewport::ViewportMapping};
use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
};

// Surface/viewport precedes GPU in field-drop order; no global GPU mutex.
pub(super) enum MainViewport<V = NativeViewport> {
    External(V),
    Object {
        viewport: V,
        compositor: Compositor,
        generation: u64,
    },
}
impl<V> Deref for MainViewport<V> {
    type Target = V;
    fn deref(&self) -> &V {
        match self {
            Self::External(viewport) | Self::Object { viewport, .. } => viewport,
        }
    }
}
impl<V> DerefMut for MainViewport<V> {
    fn deref_mut(&mut self) -> &mut V {
        match self {
            Self::External(viewport) | Self::Object { viewport, .. } => viewport,
        }
    }
}

pub(crate) fn with_object_target<C: NativeViewportConsumer>(
    app: &tauri::AppHandle,
    instance_id: &str,
    generation: u64,
    mapping: ViewportMapping,
    consumer: C,
) -> HostResult<C::Output> {
    MAIN_VIEWPORT.with(|slot| {
        with_object_slot(
            slot,
            instance_id,
            generation,
            || {
                NativeViewport::from_app(app, "main", mapping)
                    .map_err(|error| host_error("unavailable", error.to_string()))
            },
            |viewport, compositor| {
                viewport.resize(mapping).map_err(|error| {
                    host_error("unavailable", format!("object viewport resize: {error:?}"))
                })?;
                viewport
                    .with_borrowed_target(compositor, consumer)
                    .map_err(|error| host_error("unavailable", error.to_string()))
            },
        )
    })
}

/// Called only inside original authority/revision admission. The same helper
/// drives production and the headless controls; refusal never invokes create.
fn with_object_slot<V, T>(
    slot: &RefCell<Option<RetainedViewport<MainViewport<V>>>>,
    instance_id: &str,
    generation: u64,
    create: impl FnOnce() -> HostResult<(V, Compositor)>,
    consume: impl FnOnce(&mut V, &mut Compositor) -> HostResult<T>,
) -> HostResult<T> {
    let mut slot = slot
        .try_borrow_mut()
        .map_err(|_| host_error("unavailable", "native viewport is already borrowed"))?;
    if slot.is_none() {
        let (viewport, compositor) = create()?;
        *slot = Some(RetainedViewport {
            instance_id: instance_id.into(),
            viewport: MainViewport::Object {
                viewport,
                compositor,
                generation,
            },
        });
    }
    let retained = slot.as_mut().expect("original target was admitted");
    if retained.instance_id != instance_id {
        return Err(host_error(
            "wrong_instance",
            "native viewport instance mismatch",
        ));
    }
    match &mut retained.viewport {
        MainViewport::Object {
            viewport,
            compositor,
            generation: actual,
        } if *actual == generation => consume(viewport, compositor),
        MainViewport::Object { .. } => Err(host_error(
            "unavailable",
            "object viewport lifecycle is stale",
        )),
        MainViewport::External(_) => Err(host_error(
            "unavailable",
            "native viewport belongs to another owner",
        )),
    }
}

// The production release path and headless fixture share exact take/admission.
pub(super) fn take_slot<V>(
    slot: &RefCell<Option<RetainedViewport<MainViewport<V>>>>,
    instance: &str,
) -> HostResult<Option<RetainedViewport<MainViewport<V>>>> {
    let mut slot = slot
        .try_borrow_mut()
        .map_err(|_| host_error("unavailable", "native viewport is already borrowed"))?;
    match slot.as_ref() {
        Some(value) if value.instance_id == instance => Ok(slot.take()),
        Some(_) => Err(host_error(
            "wrong_instance",
            "native viewport instance mismatch",
        )),
        None => Ok(None),
    }
}

#[cfg(test)]
pub(crate) struct TestObjectSlot<V>(RefCell<Option<RetainedViewport<MainViewport<V>>>>);
#[cfg(test)]
impl<V> TestObjectSlot<V> {
    pub(crate) fn new() -> Self {
        Self(RefCell::new(None))
    }
    pub(crate) fn external(instance: String, viewport: V) -> Self {
        Self(RefCell::new(Some(RetainedViewport {
            instance_id: instance,
            viewport: MainViewport::External(viewport),
        })))
    }
    pub(crate) fn with_target<T>(
        &self,
        instance: &str,
        generation: u64,
        create: impl FnOnce() -> HostResult<(V, Compositor)>,
        consume: impl FnOnce(&mut V, &mut Compositor) -> HostResult<T>,
    ) -> HostResult<T> {
        with_object_slot(&self.0, instance, generation, create, consume)
    }
    pub(crate) fn release(
        &self,
        instance: &str,
        dispose: impl FnOnce(&mut V) -> Vec<native_engine::resource_leases::WorkId>,
    ) -> HostResult<Vec<native_engine::resource_leases::WorkId>> {
        let retained = take_slot(&self.0, instance)?;
        Ok(retained.map_or_else(Vec::new, |mut value| dispose(&mut value.viewport)))
    }
}
