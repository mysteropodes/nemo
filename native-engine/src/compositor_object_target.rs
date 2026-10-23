//! Owned RGBA8 target lifetime for object and desktop public compositions.
//! Feature-off public targets retain their separate, earlier behavior.
use crate::compositor::{Compositor, CompositorError};

#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(super) use crate::render_scene::{
    reserve_readback_bytes as reserve_target_bytes, ReadbackReservation as TargetReservation,
};
// Earlier feature-off targets retain their prior behavior. The desktop app
// enables both features; only that configuration claims the shared image quota.
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
pub(super) struct TargetReservation;
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
pub(super) fn reserve_target_bytes(_: usize) -> Result<TargetReservation, &'static str> {
    Ok(TargetReservation)
}

pub(super) struct ObjectTarget {
    texture: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    reservation: Option<TargetReservation>,
    queue: wgpu::Queue,
    may_be_submitted: bool,
}
impl ObjectTarget {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        self.texture.as_ref().expect("owned object texture")
    }
    pub(super) fn view(&self) -> &wgpu::TextureView {
        self.view.as_ref().expect("owned object texture view")
    }
    pub(super) fn render(
        &mut self,
        render: impl FnOnce(&wgpu::TextureView) -> Result<(), CompositorError>,
    ) -> Result<(), CompositorError> {
        // Vello may submit before returning an error or unwinding. Mark BEFORE
        // handing it the view; scheduler cancellation does not retire GPU work.
        self.may_be_submitted = true;
        render(self.view())
    }
}
impl Drop for ObjectTarget {
    fn drop(&mut self) {
        let texture = self.texture.take().expect("owned object texture");
        let view = self.view.take();
        let reservation = self.reservation.take().expect("owned target reservation");
        // Close admission NOW, before registering the completion fence. A
        // cloned private handle must not submit work beyond that fence. wgpu
        // retains native storage for work already submitted before destruction.
        texture.destroy();
        let retire = move || {
            drop(view);
            // Keep the owned storage handle and charge through admitted work.
            drop(texture);
            drop(reservation);
        };
        if self.may_be_submitted {
            // Covers render AND later preview/readback submissions before drop.
            // Capture no queue/device in its own completion callback.
            self.queue.on_submitted_work_done(retire);
        } else {
            retire();
        }
    }
}

fn target_bytes(width: u32, height: u32, device_max: u32) -> Result<usize, CompositorError> {
    let invalid = || CompositorError::new("object target dimensions exceed capacity");
    if width == 0 || height == 0 || width > device_max || height > device_max {
        return Err(invalid());
    }
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .filter(|bytes| *bytes <= isize::MAX as usize)
        .ok_or_else(invalid)
}

impl Compositor {
    pub(super) fn make_object_target(
        &self,
        width: u32,
        height: u32,
    ) -> Result<ObjectTarget, CompositorError> {
        self.object_target_with(width, height, reserve_target_bytes, |_, _| Ok(()))
    }

    // Tests inject quota/checkpoints into the actual production allocation path.
    pub(super) fn object_target_with(
        &self,
        width: u32,
        height: u32,
        reserve: impl FnOnce(usize) -> Result<TargetReservation, &'static str>,
        mut checkpoint: impl FnMut(&'static str, Option<&wgpu::Texture>) -> Result<(), CompositorError>,
    ) -> Result<ObjectTarget, CompositorError> {
        let cost = target_bytes(width, height, self.device.limits().max_texture_dimension_2d)?;
        // Retire completed callbacks before quota admission. Otherwise a full
        // quota could prevent the next submission that would service them.
        self.device.poll(wgpu::PollType::Poll).map_err(|error| {
            CompositorError::new(format!("poll object target retirement: {error}"))
        })?;
        let reservation = reserve(cost)
            .map_err(|_| CompositorError::new("object target payload budget refused"))?;
        checkpoint("reserved", None)?;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nemo-private-object-output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        // Install the owner before view creation or any injected fallible work.
        let mut target = ObjectTarget {
            texture: Some(texture),
            view: None,
            reservation: Some(reservation),
            queue: self.queue.clone(),
            may_be_submitted: false,
        };
        checkpoint("texture-created", Some(target.texture()))?;
        target.view = Some(
            target
                .texture()
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        checkpoint("view-created", Some(target.texture()))?;
        Ok(target)
    }
}

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/compositor_object_target.rs"]
mod tests;
