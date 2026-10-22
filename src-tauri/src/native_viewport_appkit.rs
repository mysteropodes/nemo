//! macOS AppKit surface lifetime and same-context native viewport binding.

use crate::native_viewport::{NativeViewportError, ViewportGpuBinding};
use native_engine::compositor::Compositor;
use native_engine::desktop_viewport::{
    AcquiredSurfaceFrame, DesktopViewportHost, SettledSurfaceFrame, SurfaceAttempt, SurfacePort,
    SurfacePresentation, SurfaceRecoveryError, ViewportMapping,
};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};
use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle};
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Once,
};

#[path = "native_viewport_appkit_occlusion.rs"]
pub(super) mod occlusion;

#[repr(C)]
    #[derive(Clone, Copy)]
    #[rustfmt::skip]
    struct NSPoint { x: f64, y: f64 }

#[repr(C)]
    #[derive(Clone, Copy)]
    #[rustfmt::skip]
    struct NSSize { width: f64, height: f64 }

#[repr(C)]
    #[derive(Clone, Copy)]
    #[rustfmt::skip]
    struct NSRect { origin: NSPoint, size: NSSize }

#[rustfmt::skip]
    unsafe impl objc::Encode for NSPoint {
        fn encode() -> objc::Encoding { unsafe { objc::Encoding::from_str("{CGPoint=dd}") } }
    }

#[rustfmt::skip]
    unsafe impl objc::Encode for NSSize {
        fn encode() -> objc::Encoding { unsafe { objc::Encoding::from_str("{CGSize=dd}") } }
    }

#[rustfmt::skip]
    unsafe impl objc::Encode for NSRect {
        fn encode() -> objc::Encoding { unsafe { objc::Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") } }
    }

#[rustfmt::skip]
    fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
        NSRect { origin: NSPoint { x, y }, size: NSSize { width, height } }
    }

extern "C" fn passthrough_hit_test(_this: &Object, _cmd: Sel, _point: NSPoint) -> *mut Object {
    ptr::null_mut()
}

fn native_view_class() -> *const Class {
    static ONCE: Once = Once::new();
    static mut CLASS: *const Class = ptr::null();
    ONCE.call_once(|| unsafe {
        let mut declaration = ClassDecl::new("N16NativeViewportView", class!(NSView))
            .expect("N16 native viewport class name is unique");
        declaration.add_method(
            sel!(hitTest:),
            passthrough_hit_test as extern "C" fn(&Object, Sel, NSPoint) -> *mut Object,
        );
        CLASS = declaration.register();
    });
    unsafe { CLASS }
}

#[rustfmt::skip]
    pub(super) struct OwnedView { pub(super) raw: *mut Object, pub(super) attached: bool }

impl OwnedView {
    fn new(
        content_view: *mut Object,
        mapping: ViewportMapping,
    ) -> Result<Self, NativeViewportError> {
        let allocated: *mut Object = unsafe { msg_send![native_view_class(), alloc] };
        if allocated.is_null() {
            return Err(NativeViewportError::new("allocate native AppKit viewport"));
        }
        let raw: *mut Object =
            unsafe { msg_send![allocated, initWithFrame: rect(0.0, 0.0, 1.0, 1.0)] };
        if raw.is_null() {
            return Err(NativeViewportError::new(
                "initialize native AppKit viewport",
            ));
        }
        let mut view = Self {
            raw,
            attached: false,
        };
        unsafe {
            let _: () = msg_send![raw, setWantsLayer: true];
            // NSWindowBelow with a nil sibling keeps the existing WKWebView above
            // this surface. DOM menus and editing overlays must composite over it.
            let _: () = msg_send![content_view, addSubview: raw
                positioned: -1_isize relativeTo: ptr::null_mut::<Object>()];
        }
        view.attached = true;
        view.apply_mapping(mapping)?;
        Ok(view)
    }

    fn raw(&self) -> *mut Object {
        self.raw
    }

    fn apply_mapping(&self, mapping: ViewportMapping) -> Result<(), NativeViewportError> {
        let parent: *mut Object = unsafe { msg_send![self.raw, superview] };
        if self.raw.is_null() || parent.is_null() {
            return Err(NativeViewportError::new(
                "native AppKit viewport is detached",
            ));
        }
        let bounds: NSRect = unsafe { msg_send![parent, bounds] };
        let flipped: bool = unsafe { msg_send![parent, isFlipped] };
        let css = mapping.css_bounds();
        let x = bounds.origin.x + css.x;
        let y = if flipped {
            bounds.origin.y + css.y
        } else {
            bounds.origin.y + bounds.size.height - css.y - css.height
        };
        unsafe {
            let _: () = msg_send![self.raw, setFrame: rect(x, y, css.width, css.height)];
        }
        Ok(())
    }

    pub(super) fn dispose(&mut self) {
        if self.raw.is_null() {
            return;
        }
        unsafe {
            if self.attached {
                let _: () = msg_send![self.raw, removeFromSuperview];
            }
            let _: () = msg_send![self.raw, release];
        }
        self.raw = ptr::null_mut();
        self.attached = false;
    }
}

#[rustfmt::skip]
    impl Drop for OwnedView { fn drop(&mut self) { self.dispose(); } }

fn create_surface(
    instance: &wgpu::Instance,
    view: &OwnedView,
) -> Result<wgpu::Surface<'static>, NativeViewportError> {
    let handle = AppKitWindowHandle::new(
        NonNull::new(view.raw().cast::<c_void>())
            .ok_or_else(|| NativeViewportError::new("native AppKit viewport is disposed"))?,
    );
    // SAFETY: `OwnedView` keeps the +1 AppKit reference alive. The port's
    // disposal path drops `surface` before detaching and releasing `view`.
    unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(AppKitDisplayHandle::new().into()),
            raw_window_handle: handle.into(),
        })
    }
    .map_err(|error| NativeViewportError::new(format!("create AppKit surface: {error}")))
}

// The retained compositor output and the final attachment share this selector.
impl NativeViewport {
    pub(crate) fn select_surface_format(
        formats: &[wgpu::TextureFormat],
    ) -> Result<wgpu::TextureFormat, NativeViewportError> {
        formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| formats.first().copied())
            .ok_or_else(|| NativeViewportError::new("native surface has no texture format"))
    }
}

fn surface_contract(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::TextureFormat, wgpu::CompositeAlphaMode), NativeViewportError> {
    let capabilities = surface.get_capabilities(adapter);
    let format = NativeViewport::select_surface_format(&capabilities.formats)?;
    let alpha_mode = capabilities
        .alpha_modes
        .iter()
        .copied()
        .find(|mode| *mode == wgpu::CompositeAlphaMode::Opaque)
        .or_else(|| capabilities.alpha_modes.first().copied())
        .ok_or_else(|| NativeViewportError::new("native surface has no alpha mode"))?;
    Ok((format, alpha_mode))
}

#[rustfmt::skip]
    struct MacOsSurfacePort { surface: Option<wgpu::Surface<'static>>, view: OwnedView, instance: wgpu::Instance, adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat, alpha_mode: wgpu::CompositeAlphaMode, blitter: wgpu::util::TextureBlitter, mapping: ViewportMapping, device_lost: Arc<AtomicBool> }

impl MacOsSurfacePort {
    fn new(
        surface: wgpu::Surface<'static>,
        view: OwnedView,
        compositor: &Compositor,
        mapping: ViewportMapping,
    ) -> Result<Self, NativeViewportError> {
        let (format, alpha_mode) = match surface_contract(&surface, compositor.adapter()) {
            Ok(contract) => contract,
            Err(error) => {
                // Preserve the raw-handle lifetime rule on construction failure too.
                drop(surface);
                drop(view);
                return Err(error);
            }
        };
        let blitter = wgpu::util::TextureBlitter::new(compositor.device(), format);
        let device_lost = Arc::new(AtomicBool::new(false));
        let lost_callback = Arc::clone(&device_lost);
        compositor
            .device()
            .set_device_lost_callback(move |_, _| lost_callback.store(true, Ordering::Release));
        let port = Self {
            surface: Some(surface),
            view,
            instance: compositor.instance().clone(),
            adapter: compositor.adapter().clone(),
            device: compositor.device().clone(),
            queue: compositor.queue().clone(),
            format,
            alpha_mode,
            blitter,
            mapping,
            device_lost,
        };
        port.configure()?;
        Ok(port)
    }

    fn configure(&self) -> Result<(), NativeViewportError> {
        if self.device_lost.load(Ordering::Acquire) {
            return Err(NativeViewportError::new("native GPU device is lost"));
        }
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| NativeViewportError::new("native surface is disposed"))?;
        let extent = self.mapping.physical_extent();
        let errors = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: extent.width,
                height: extent.height,
                present_mode: wgpu::PresentMode::AutoVsync,
                desired_maximum_frame_latency: 2,
                alpha_mode: self.alpha_mode,
                view_formats: vec![],
            },
        );
        let _ = self.device.poll(wgpu::PollType::Poll);
        match pollster::block_on(errors.pop()) {
            Some(error) => Err(NativeViewportError::new(format!(
                "configure native surface: {error}"
            ))),
            None if self.device_lost.load(Ordering::Acquire) => {
                Err(NativeViewportError::new("native GPU device is lost"))
            }
            None => Ok(()),
        }
    }

    fn replace_surface(&mut self, mapping: ViewportMapping) -> Result<(), NativeViewportError> {
        self.view.apply_mapping(mapping)?;
        self.surface.take();
        let surface = create_surface(&self.instance, &self.view)?;
        let (format, alpha_mode) = surface_contract(&surface, &self.adapter)?;
        if format != self.format {
            self.blitter = wgpu::util::TextureBlitter::new(&self.device, format);
        }
        self.surface = Some(surface);
        self.format = format;
        self.alpha_mode = alpha_mode;
        self.mapping = mapping;
        self.configure()
    }
}

enum MacOsFrame<'port, 'source> {
    Acquired {
        texture: wgpu::SurfaceTexture,
        blitter: &'port wgpu::util::TextureBlitter,
        device: &'source wgpu::Device,
        queue: &'source wgpu::Queue,
        source: &'source wgpu::TextureView,
        device_lost: &'port AtomicBool,
    },
    Failed(SettledSurfaceFrame),
}

impl AcquiredSurfaceFrame for MacOsFrame<'_, '_> {
    fn settle(self) -> SettledSurfaceFrame {
        match self {
            Self::Acquired {
                texture,
                blitter,
                device,
                queue,
                source,
                device_lost,
            } => {
                let errors = device.push_error_scope(wgpu::ErrorFilter::Validation);
                let target = texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("n16-native-viewport-blit"),
                });
                blitter.copy(device, &mut encoder, source, &target);
                queue.submit(Some(encoder.finish()));
                texture.present();
                let _ = device.poll(wgpu::PollType::Poll);
                if device_lost.load(Ordering::Acquire) {
                    SettledSurfaceFrame::DeviceLost
                } else if pollster::block_on(errors.pop()).is_some() {
                    SettledSurfaceFrame::ValidationFailure
                } else {
                    SettledSurfaceFrame::Presented
                }
            }
            Self::Failed(outcome) => outcome,
        }
    }
}

impl SurfacePort for MacOsSurfacePort {
    type Frame<'port, 'source> = MacOsFrame<'port, 'source>;

    fn acquire<'port, 'source>(
        &'port mut self,
        source: SurfacePresentation<'source>,
    ) -> SurfaceAttempt<Self::Frame<'port, 'source>> {
        if self.device_lost.load(Ordering::Acquire) {
            return SurfaceAttempt::Acquired(MacOsFrame::Failed(SettledSurfaceFrame::DeviceLost));
        }
        if source.instance != &self.instance
            || source.adapter != &self.adapter
            || source.device != &self.device
            || source.queue != &self.queue
            || source.surface_extent != self.mapping.physical_extent()
        {
            return SurfaceAttempt::Acquired(MacOsFrame::Failed(
                SettledSurfaceFrame::ValidationFailure,
            ));
        }
        let Some(surface) = self.surface.as_ref() else {
            return SurfaceAttempt::Lost;
        };
        match occlusion::acquire(surface, &self.view, self.mapping) {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                SurfaceAttempt::Acquired(MacOsFrame::Acquired {
                    texture,
                    blitter: &self.blitter,
                    device: source.device,
                    queue: source.queue,
                    source: source.texture_view,
                    device_lost: &self.device_lost,
                })
            }
            wgpu::CurrentSurfaceTexture::Lost => SurfaceAttempt::Lost,
            wgpu::CurrentSurfaceTexture::Outdated => SurfaceAttempt::Outdated,
            wgpu::CurrentSurfaceTexture::Timeout => SurfaceAttempt::Timeout,
            wgpu::CurrentSurfaceTexture::Occluded => SurfaceAttempt::Occluded,
            wgpu::CurrentSurfaceTexture::Validation => {
                SurfaceAttempt::Acquired(MacOsFrame::Failed(SettledSurfaceFrame::ValidationFailure))
            }
        }
    }

    fn recreate(&mut self, mapping: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        self.replace_surface(mapping).map_err(|_| {
            if self.device_lost.load(Ordering::Acquire) {
                SurfaceRecoveryError::DeviceLost
            } else {
                SurfaceRecoveryError::Failed
            }
        })
    }

    fn reconfigure(&mut self, mapping: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        let extent_changed = self.mapping.physical_extent() != mapping.physical_extent();
        self.view
            .apply_mapping(mapping)
            .map_err(|_| SurfaceRecoveryError::Failed)?;
        self.mapping = mapping;
        if extent_changed {
            self.configure().map_err(|_| {
                if self.device_lost.load(Ordering::Acquire) {
                    SurfaceRecoveryError::DeviceLost
                } else {
                    SurfaceRecoveryError::Failed
                }
            })
        } else {
            Ok(())
        }
    }

    fn dispose(&mut self) {
        self.surface.take();
        self.view.dispose();
    }
}

impl Drop for MacOsSurfacePort {
    fn drop(&mut self) {
        self.dispose();
    }
}

pub(crate) struct NativeViewport {
    gpu: ViewportGpuBinding,
    host: Option<DesktopViewportHost<MacOsSurfacePort>>,
    occlusion: Option<occlusion::WindowOcclusionObserver>,
    window: tauri::WebviewWindow,
    mapping: ViewportMapping,
}

#[path = "native_viewport_appkit_binding.rs"]
mod binding;
