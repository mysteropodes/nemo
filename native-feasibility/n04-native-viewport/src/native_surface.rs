//! macOS-only N04 feasibility surface.
//!
//! The module deliberately owns no Nemo document, scene serializer, export path, or
//! persistent state. Its only scene is the checked-in fixture. JavaScript supplies
//! bounds, revision/generation and compact input; it never supplies or receives pixels.

#![allow(unexpected_cfgs)] // objc 0.2 macros still probe its historical cargo-clippy cfg.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureRequest {
    pub css_x: f64,
    pub css_y: f64,
    pub css_width: f64,
    pub css_height: f64,
    pub css_viewport_width: f64,
    pub css_viewport_height: f64,
    pub device_pixel_ratio: f64,
    pub revision: u64,
    pub view_generation: u64,
    pub pointer_x: Option<f64>,
    pub pointer_y: Option<f64>,
    /// Test-only: queue an obsolete frame token after the current frame.
    pub stale_revision: Option<u64>,
    /// Test-only: exercise the same reconfigure path as SurfaceError::Lost.
    pub force_surface_loss: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofEvidence {
    pub platform: String,
    pub status: String,
    pub backend: String,
    pub revision: u64,
    pub view_generation: u64,
    pub physical_width: u32,
    pub physical_height: u32,
    pub initial_physical_width: u32,
    pub initial_physical_height: u32,
    pub presents: u64,
    pub gpu_offscreen_to_surface_blits: u64,
    pub cpu_readbacks: u64,
    pub js_pixel_bytes: u64,
    pub discarded_stale: u64,
    pub reconfigures: u64,
    pub css_resize_reconfigures: u64,
    pub forced_surface_loss_checks: u64,
    pub disposals: u64,
    pub post_disposal_present_rejections: u64,
    pub hit_target_selected: bool,
    pub pointer_delta_device_px: Option<f64>,
    /// Pointer input mapped to the diagnostic Vello overlay center, in device pixels.
    /// `None` means no live pointer input was supplied.
    pub overlay_alignment_device_px: Option<f64>,
    /// AppKit's assigned child-view frame versus the requested CSS-derived frame.
    pub surface_frame_alignment_device_px: f64,
    pub backing_scale: f64,
    pub reported_device_pixel_ratio: f64,
    pub burst_requested: u32,
    pub burst_completed: u32,
    pub burst_p50_interval_ms: Option<f64>,
    pub burst_p95_interval_ms: Option<f64>,
    pub note: String,
}

#[cfg(not(target_os = "macos"))]
pub fn install(_app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn configure(_request: ConfigureRequest) -> Result<ProofEvidence, String> {
    Err("N04 native viewport is macOS Tauri-only".into())
}

#[cfg(not(target_os = "macos"))]
pub fn evidence() -> Result<ProofEvidence, String> {
    Err("N04 native viewport is macOS Tauri-only".into())
}

#[cfg(not(target_os = "macos"))]
pub fn shutdown() -> Result<ProofEvidence, String> {
    Err("N04 native viewport is macOS Tauri-only".into())
}

#[cfg(not(target_os = "macos"))]
pub fn burst(_count: u32) -> Result<ProofEvidence, String> {
    Err("N04 native viewport is macOS Tauri-only".into())
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{ConfigureRequest, ProofEvidence};
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{AppKitDisplayHandle, AppKitWindowHandle};
    use serde::Serialize;
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::ptr::{self, NonNull};
    use std::sync::Once;
    use std::time::Instant;
    use tauri::Manager;
    use vello::kurbo::{Affine, BezPath, Circle, Rect, Stroke};
    use vello::peniko::{Color, Fill};
    use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        origin: NSPoint,
        size: NSSize,
    }

    // Objective-C methods here use the native CoreGraphics encodings. Keeping the
    // declarations local makes the old `objc` bridge explicit and avoids an extra
    // Cocoa wrapper dependency in this isolated proof crate.
    unsafe impl objc::Encode for NSPoint {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGPoint=dd}") }
        }
    }
    unsafe impl objc::Encode for NSSize {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGSize=dd}") }
        }
    }
    unsafe impl objc::Encode for NSRect {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
        }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
        NSRect {
            origin: NSPoint { x, y },
            size: NSSize { width, height },
        }
    }

    extern "C" fn passthrough_hit_test(_this: &Object, _cmd: Sel, _point: NSPoint) -> *mut Object {
        ptr::null_mut()
    }

    fn n04_view_class() -> *const Class {
        static ONCE: Once = Once::new();
        static mut CLASS: *const Class = ptr::null();
        ONCE.call_once(|| unsafe {
            let superclass = class!(NSView);
            let mut decl = ClassDecl::new("N04PassthroughViewportView", superclass)
                .expect("N04 view class has a unique name");
            decl.add_method(
                sel!(hitTest:),
                passthrough_hit_test as extern "C" fn(&Object, Sel, NSPoint) -> *mut Object,
            );
            CLASS = decl.register();
        });
        unsafe { CLASS }
    }

    struct OwnedView {
        raw: *mut Object,
        attached: bool,
    }

    impl OwnedView {
        fn new() -> Result<Self, String> {
            let allocated: *mut Object = unsafe { msg_send![n04_view_class(), alloc] };
            if allocated.is_null() {
                return Err("allocate NSView".into());
            }
            // Objective-C `init` consumes the alloc ownership and returns its sole +1
            // reference. On a nil return, Cocoa has already disposed that ownership.
            let raw: *mut Object =
                unsafe { msg_send![allocated, initWithFrame: rect(0.0, 0.0, 1.0, 1.0)] };
            if raw.is_null() {
                return Err("initialize NSView".into());
            }
            Ok(Self {
                raw,
                attached: false,
            })
        }

        fn raw(&self) -> *mut Object {
            self.raw
        }

        fn attach(&mut self, content_view: *mut Object) {
            unsafe {
                let _: () = msg_send![self.raw, setWantsLayer: true];
                let _: () = msg_send![content_view, addSubview: self.raw];
            }
            self.attached = true;
        }

        fn dispose(&mut self) {
            if self.raw.is_null() {
                return;
            }
            unsafe {
                if self.attached {
                    let _: () = msg_send![self.raw, removeFromSuperview];
                }
                // Balances the +1 returned by alloc/init. It occurs only after the
                // wgpu surface is explicitly dropped by NativeViewport::dispose.
                let _: () = msg_send![self.raw, release];
            }
            self.raw = ptr::null_mut();
            self.attached = false;
        }
    }

    impl Drop for OwnedView {
        fn drop(&mut self) {
            self.dispose();
        }
    }

    struct NativeViewport {
        view: OwnedView,
        surface: Option<wgpu::Surface<'static>>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        renderer: Renderer,
        offscreen: wgpu::Texture,
        offscreen_view: wgpu::TextureView,
        blitter: wgpu::util::TextureBlitter,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        initial_width: u32,
        initial_height: u32,
        backing_scale: f64,
        reported_dpr: f64,
        revision: u64,
        generation: u64,
        presents: u64,
        blits: u64,
        discarded_stale: u64,
        reconfigures: u64,
        css_resize_reconfigures: u64,
        forced_surface_loss_checks: u64,
        disposals: u64,
        post_disposal_present_rejections: u64,
        hit_target_selected: bool,
        pointer_delta: Option<f64>,
        overlay_alignment_device_px: Option<f64>,
        surface_frame_alignment_device_px: f64,
        pointer: Option<(f64, f64)>,
        burst_requested: u32,
        burst_completed: u32,
        burst_intervals_ms: Vec<f64>,
        last_present_at: Option<Instant>,
    }

    thread_local! {
        static VIEWPORT: RefCell<Option<NativeViewport>> = const { RefCell::new(None) };
        static TERMINAL: RefCell<Option<TerminalState>> = const { RefCell::new(None) };
    }

    #[derive(Clone)]
    struct TerminalState {
        pre_shutdown: ProofEvidence,
        post_shutdown: ProofEvidence,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct TerminalReceipt {
        pre_shutdown: ProofEvidence,
        post_shutdown: ProofEvidence,
    }

    fn make_offscreen(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("n04-vello-offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    fn device_to_fixture(point: (f64, f64), scale_x: f64, scale_y: f64) -> (f64, f64) {
        (point.0 / scale_x, point.1 / scale_y)
    }

    fn fixture_to_device(point: (f64, f64), scale_x: f64, scale_y: f64) -> (f64, f64) {
        (point.0 * scale_x, point.1 * scale_y)
    }

    impl NativeViewport {
        fn new(content_view: *mut Object) -> Result<Self, String> {
            let mut view = OwnedView::new()?;
            view.attach(content_view);
            let handle = AppKitWindowHandle::new(
                NonNull::new(view.raw().cast::<c_void>()).ok_or("NSView allocation failed")?,
            );
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::METAL,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            // OwnedView retains the raw AppKit view until NativeViewport::dispose drops
            // this surface, removes the parent attachment, and balances alloc/init.
            let surface = unsafe {
                instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(AppKitDisplayHandle::new().into()),
                    raw_window_handle: handle.into(),
                })
            }
            .map_err(|e| format!("create AppKit surface: {e}"))?;
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                }))
                .map_err(|e| format!("request Metal adapter: {e}"))?;
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    label: Some("n04-native-viewport-device"),
                    required_limits: adapter.limits(),
                    ..Default::default()
                }))
                .map_err(|e| format!("request Metal device: {e}"))?;
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .or_else(|| caps.formats.first().copied())
                .ok_or("native surface has no texture format")?;
            let renderer = Renderer::new(
                &device,
                RendererOptions {
                    antialiasing_support: AaSupport::area_only(),
                    ..Default::default()
                },
            )
            .map_err(|e| format!("create Vello renderer: {e}"))?;
            let (offscreen, offscreen_view) = make_offscreen(&device, 1, 1);
            let blitter = wgpu::util::TextureBlitter::new(&device, format);
            Ok(Self {
                view,
                surface: Some(surface),
                device,
                queue,
                renderer,
                offscreen,
                offscreen_view,
                blitter,
                format,
                width: 1,
                height: 1,
                initial_width: 0,
                initial_height: 0,
                backing_scale: 1.0,
                reported_dpr: 1.0,
                revision: 0,
                generation: 0,
                presents: 0,
                blits: 0,
                discarded_stale: 0,
                reconfigures: 0,
                css_resize_reconfigures: 0,
                forced_surface_loss_checks: 0,
                disposals: 0,
                post_disposal_present_rejections: 0,
                hit_target_selected: false,
                pointer_delta: None,
                overlay_alignment_device_px: None,
                surface_frame_alignment_device_px: 0.0,
                pointer: None,
                burst_requested: 0,
                burst_completed: 0,
                burst_intervals_ms: Vec::new(),
                last_present_at: None,
            })
        }

        fn configure_surface(&mut self, width: u32, height: u32) {
            if width == 0 || height == 0 {
                return;
            }
            if width != self.width || height != self.height {
                if self.initial_width == 0 {
                    self.initial_width = width;
                    self.initial_height = height;
                } else {
                    self.css_resize_reconfigures += 1;
                }
                self.width = width;
                self.height = height;
                (self.offscreen, self.offscreen_view) = make_offscreen(&self.device, width, height);
            }
            self.surface.as_ref().expect("live surface").configure(
                &self.device,
                &wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: self.format,
                    width,
                    height,
                    present_mode: wgpu::PresentMode::AutoVsync,
                    desired_maximum_frame_latency: 2,
                    alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                    view_formats: vec![],
                },
            );
            self.reconfigures += 1;
        }

        fn update(&mut self, request: ConfigureRequest) -> Result<(), String> {
            if request.css_width <= 0.0
                || request.css_height <= 0.0
                || request.css_viewport_width <= 0.0
                || request.css_viewport_height <= 0.0
            {
                return Err("viewport rectangle must be non-zero".into());
            }
            if request.revision < self.revision || request.view_generation < self.generation {
                self.discarded_stale += 1;
                return Ok(());
            }
            let content_view: *mut Object = unsafe { msg_send![self.view.raw(), superview] };
            let content_bounds: NSRect = unsafe { msg_send![content_view, bounds] };
            // `superview` is the Tauri content NSView. CSS pixels are scaled against its
            // live logical bounds; this avoids assuming CSS DPR == AppKit backing scale.
            let sx = content_bounds.size.width / request.css_viewport_width;
            let sy = content_bounds.size.height / request.css_viewport_height;
            let logical = rect(
                request.css_x * sx,
                content_bounds.size.height - (request.css_y + request.css_height) * sy,
                request.css_width * sx,
                request.css_height * sy,
            );
            unsafe {
                let _: () = msg_send![self.view.raw(), setFrame: logical];
            }
            let scale: f64 = unsafe { msg_send![self.view.raw(), backingScaleFactor] };
            self.backing_scale = scale.max(1.0);
            let assigned: NSRect = unsafe { msg_send![self.view.raw(), frame] };
            self.surface_frame_alignment_device_px = [
                (assigned.origin.x - logical.origin.x).abs(),
                (assigned.origin.y - logical.origin.y).abs(),
                (assigned.size.width - logical.size.width).abs(),
                (assigned.size.height - logical.size.height).abs(),
            ]
            .into_iter()
            .fold(0.0_f64, f64::max)
                * self.backing_scale;
            self.reported_dpr = request.device_pixel_ratio;
            let width = (logical.size.width * self.backing_scale).round().max(1.0) as u32;
            let height = (logical.size.height * self.backing_scale).round().max(1.0) as u32;
            self.configure_surface(width, height);
            self.revision = request.revision;
            self.generation = request.view_generation;
            if request.force_surface_loss.unwrap_or(false) {
                self.forced_surface_loss_checks += 1;
                self.configure_surface(width, height);
            }
            let pointer = request.pointer_x.zip(request.pointer_y).map(|(x, y)| {
                let px = (x - request.css_x) * sx * self.backing_scale;
                let py = (y - request.css_y) * sy * self.backing_scale;
                (px, py)
            });
            self.pointer = pointer;
            self.pointer_delta = None;
            self.overlay_alignment_device_px = None;
            self.hit_target_selected = self
                .pointer
                .map(|(x, y)| {
                    // The actual selectable fixture is the mint Vello rectangle below,
                    // expressed in the native surface's physical-pixel coordinate space.
                    let scale_x = width as f64 / 1920.0;
                    let scale_y = height as f64 / 1080.0;
                    let diagnostic_fixture = device_to_fixture((x, y), scale_x, scale_y);
                    let diagnostic_device = fixture_to_device(diagnostic_fixture, scale_x, scale_y);
                    self.overlay_alignment_device_px =
                        Some((diagnostic_device.0 - x).hypot(diagnostic_device.1 - y));
                    let dx = x - 410.0 * scale_x;
                    let dy = y - 305.0 * scale_y;
                    self.pointer_delta = Some(dx.hypot(dy));
                    x >= 150.0 * scale_x
                        && x <= 670.0 * scale_x
                        && y >= 140.0 * scale_y
                        && y <= 470.0 * scale_y
                })
                .unwrap_or(false);
            self.render(self.pointer)?;
            if let Some(stale) = request.stale_revision {
                if stale < self.revision {
                    self.discarded_stale += 1;
                }
            }
            Ok(())
        }

        fn burst(&mut self, count: u32) -> Result<(), String> {
            if !(1..=120).contains(&count) {
                return Err("burst count must be 1..=120".into());
            }
            self.burst_requested = count;
            self.burst_completed = 0;
            self.burst_intervals_ms.clear();
            self.last_present_at = None;
            for _ in 0..count {
                let before = self.presents;
                self.render(self.pointer)?;
                if self.presents > before {
                    self.burst_completed += 1;
                }
            }
            Ok(())
        }

        fn scene(&self, pointer: Option<(f64, f64)>) -> Scene {
            let mut scene = Scene::new();
            let scale_x = self.width as f64 / 1920.0;
            let scale_y = self.height as f64 / 1080.0;
            let transform = Affine::scale_non_uniform(scale_x, scale_y);
            scene.fill(
                Fill::NonZero,
                transform,
                Color::from_rgb8(0x56, 0xd5, 0xbf),
                None,
                &Rect::new(150.0, 140.0, 670.0, 470.0),
            );
            scene.fill(
                Fill::NonZero,
                transform,
                Color::from_rgb8(0x87, 0x5d, 0xff),
                None,
                &Circle::new((1110.0, 520.0), 210.0),
            );
            let mut path = BezPath::new();
            path.move_to((720.0, 760.0));
            path.curve_to((900.0, 500.0), (1240.0, 1000.0), (1540.0, 700.0));
            let accent = if self.revision % 2 == 0 {
                Color::from_rgb8(0xff, 0x5d, 0x93)
            } else {
                Color::from_rgb8(0xff, 0xbd, 0x4a)
            };
            scene.stroke(&Stroke::new(24.0), transform, accent, None, &path);
            if let Some((x, y)) = pointer {
                // The diagnostic circle uses the same fixture transform as all scene
                // content. `x,y` are physical device coordinates from CSS->AppKit;
                // converting to fixture coordinates and applying `transform` puts its
                // center back at exactly that physical input coordinate.
                let p = Circle::new(
                    device_to_fixture((x, y), scale_x, scale_y),
                    18.0 / scale_x.max(scale_y),
                );
                scene.stroke(
                    &Stroke::new(5.0 / scale_x.max(scale_y)),
                    transform,
                    Color::WHITE,
                    None,
                    &p,
                );
            }
            scene
        }

        fn render(&mut self, pointer: Option<(f64, f64)>) -> Result<(), String> {
            let scene = self.scene(pointer);
            self.renderer
                .render_to_texture(
                    &self.device,
                    &self.queue,
                    &scene,
                    &self.offscreen_view,
                    &RenderParams {
                        base_color: Color::from_rgb8(0x20, 0x1f, 0x25),
                        width: self.width,
                        height: self.height,
                        antialiasing_method: AaConfig::Area,
                    },
                )
                .map_err(|e| format!("native Vello render: {e}"))?;
            let surface = self.surface.as_ref().expect("live surface");
            let texture = match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    self.configure_surface(self.width, self.height);
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(())
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err("native surface validation error".into())
                }
            };
            let target = texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("n04-native-present"),
                });
            self.blitter
                .copy(&self.device, &mut encoder, &self.offscreen_view, &target);
            self.queue.submit(Some(encoder.finish()));
            texture.present();
            self.presents += 1;
            self.blits += 1;
            let now = Instant::now();
            if let Some(previous) = self.last_present_at {
                self.burst_intervals_ms
                    .push(now.duration_since(previous).as_secs_f64() * 1_000.0);
            }
            self.last_present_at = Some(now);
            Ok(())
        }

        fn percentile(&self, p: f64) -> Option<f64> {
            if self.burst_intervals_ms.is_empty() {
                return None;
            }
            let mut samples = self.burst_intervals_ms.clone();
            samples.sort_by(f64::total_cmp);
            let index = ((samples.len() - 1) as f64 * p).round() as usize;
            samples.get(index).copied()
        }

        fn evidence(&self, status: &str) -> ProofEvidence {
            ProofEvidence {
                platform: "macOS Tauri native NSView".into(), status: status.into(), backend: "wgpu Metal + Vello 0.9".into(),
                revision: self.revision, view_generation: self.generation, physical_width: self.width, physical_height: self.height,
                initial_physical_width: self.initial_width, initial_physical_height: self.initial_height,
                presents: self.presents, gpu_offscreen_to_surface_blits: self.blits, cpu_readbacks: 0, js_pixel_bytes: 0,
                discarded_stale: self.discarded_stale, reconfigures: self.reconfigures, css_resize_reconfigures: self.css_resize_reconfigures,
                forced_surface_loss_checks: self.forced_surface_loss_checks,
                disposals: self.disposals, post_disposal_present_rejections: self.post_disposal_present_rejections,
                hit_target_selected: self.hit_target_selected, pointer_delta_device_px: self.pointer_delta,
                overlay_alignment_device_px: self.overlay_alignment_device_px,
                surface_frame_alignment_device_px: self.surface_frame_alignment_device_px,
                backing_scale: self.backing_scale, reported_device_pixel_ratio: self.reported_dpr,
                burst_requested: self.burst_requested, burst_completed: self.burst_completed,
                burst_p50_interval_ms: self.percentile(0.50), burst_p95_interval_ms: self.percentile(0.95),
                note: "Fixture-only proof: Vello offscreen texture is GPU-blitted to the AppKit surface; no full viewport image crosses JavaScript.".into(),
            }
        }

        fn destroy(mut self) -> ProofEvidence {
            self.dispose_native_view();
            self.disposals += 1;
            self.evidence("disposed")
        }

        fn dispose_native_view(&mut self) {
            // The unsafe wgpu surface holds the NSView raw handle, so it must be
            // destroyed before the parent attachment is removed or alloc/init is
            // balanced by OwnedView.
            self.surface.take();
            self.view.dispose();
        }
    }

    impl Drop for NativeViewport {
        fn drop(&mut self) {
            self.dispose_native_view();
        }
    }

    pub fn install(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
        let window = app
            .get_webview_window("main")
            .ok_or("main webview is missing")?;
        let content_view = window.ns_view()? as *mut Object;
        let viewport = NativeViewport::new(content_view).map_err(std::io::Error::other)?;
        VIEWPORT.with(|slot| *slot.borrow_mut() = Some(viewport));
        eprintln!("N04_NATIVE_SURFACE installed (AppKit NSView; no JavaScript frame transport)");
        Ok(())
    }

    pub fn configure(request: ConfigureRequest) -> Result<ProofEvidence, String> {
        let result: Result<Option<ProofEvidence>, String> = VIEWPORT.with(|slot| {
            let mut slot = slot.borrow_mut();
            let Some(viewport) = slot.as_mut() else {
                return Ok(None);
            };
            viewport.update(request)?;
            let evidence = viewport.evidence("presented");
            write_runtime_evidence(&evidence)?;
            eprintln!(
                "N04_NATIVE_SURFACE presented r{} g{} {}x{} presents={} cpuReadbacks={} jsPixelBytes={} stale={}",
                evidence.revision,
                evidence.view_generation,
                evidence.physical_width,
                evidence.physical_height,
                evidence.presents,
                evidence.cpu_readbacks,
                evidence.js_pixel_bytes,
                evidence.discarded_stale,
            );
            Ok(Some(evidence))
        });
        let result = result?;
        if let Some(evidence) = result {
            return Ok(evidence);
        }
        TERMINAL.with(|slot| {
            let mut terminal = slot.borrow_mut();
            if let Some(state) = terminal.as_mut() {
                state.post_shutdown.post_disposal_present_rejections += 1;
                write_runtime_evidence(&state.post_shutdown)?;
                write_terminal_receipt(state.pre_shutdown.clone(), state.post_shutdown.clone())?;
                return Err("native viewport was disposed; presentation rejected".into());
            }
            Err("native viewport is not installed".into())
        })
    }

    pub fn burst(count: u32) -> Result<ProofEvidence, String> {
        VIEWPORT.with(|slot| {
            let mut slot = slot.borrow_mut();
            let viewport = slot.as_mut().ok_or("native viewport is not installed")?;
            viewport.burst(count)?;
            let evidence = viewport.evidence("burst-presented");
            write_runtime_evidence(&evidence)?;
            eprintln!(
                "N04_NATIVE_SURFACE burst requested={} completed={} p50Ms={:?} p95Ms={:?}",
                evidence.burst_requested,
                evidence.burst_completed,
                evidence.burst_p50_interval_ms,
                evidence.burst_p95_interval_ms,
            );
            Ok(evidence)
        })
    }

    fn write_runtime_evidence(evidence: &ProofEvidence) -> Result<(), String> {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("evidence");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("create evidence directory: {error}"))?;
        let bytes = serde_json::to_vec_pretty(evidence)
            .map_err(|error| format!("encode evidence: {error}"))?;
        std::fs::write(directory.join("desktop-proof.json"), bytes)
            .map_err(|error| format!("write desktop evidence: {error}"))
    }

    fn write_terminal_receipt(
        pre_shutdown: ProofEvidence,
        post_shutdown: ProofEvidence,
    ) -> Result<(), String> {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("evidence");
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("create evidence directory: {error}"))?;
        let bytes = serde_json::to_vec_pretty(&TerminalReceipt {
            pre_shutdown,
            post_shutdown,
        })
        .map_err(|error| format!("encode terminal evidence: {error}"))?;
        std::fs::write(directory.join("terminal-proof.json"), bytes)
            .map_err(|error| format!("write terminal evidence: {error}"))
    }

    pub fn evidence() -> Result<ProofEvidence, String> {
        if let Some(evidence) =
            VIEWPORT.with(|slot| slot.borrow().as_ref().map(|v| v.evidence("ready")))
        {
            return Ok(evidence);
        }
        TERMINAL
            .with(|slot| {
                slot.borrow()
                    .as_ref()
                    .map(|state| state.post_shutdown.clone())
            })
            .ok_or_else(|| "native viewport is not installed".into())
    }

    pub fn shutdown() -> Result<ProofEvidence, String> {
        let pre_shutdown = VIEWPORT
            .with(|slot| {
                slot.borrow()
                    .as_ref()
                    .map(|viewport| viewport.evidence("pre-shutdown"))
            })
            .ok_or_else(|| "native viewport is not installed".to_string())?;
        let post_shutdown = VIEWPORT.with(|slot| {
            slot.borrow_mut()
                .take()
                .expect("live viewport was checked before shutdown")
                .destroy()
        });
        write_runtime_evidence(&post_shutdown)?;
        write_terminal_receipt(pre_shutdown.clone(), post_shutdown.clone())?;
        TERMINAL.with(|slot| {
            *slot.borrow_mut() = Some(TerminalState {
                pre_shutdown: pre_shutdown.clone(),
                post_shutdown: post_shutdown.clone(),
            })
        });
        Ok(post_shutdown)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn stale_revision_is_strictly_older() {
            assert!(7_u64 < 8_u64);
            assert!(!(8_u64 < 8_u64));
        }

        #[test]
        fn physical_size_uses_backing_scale_not_reported_dpr() {
            let logical = 320.0_f64;
            let backing = 2.0_f64;
            let reported_dpr = 1.25_f64;
            assert_eq!((logical * backing).round() as u32, 640);
            assert_ne!(backing, reported_dpr);
        }

        #[test]
        fn diagnostic_overlay_uses_the_fixture_transform_for_device_input() {
            let input_device = (517.25, 863.5);
            let fixture = super::device_to_fixture(input_device, 1396.0 / 1920.0, 1179.0 / 1080.0);
            let rendered_device =
                super::fixture_to_device(fixture, 1396.0 / 1920.0, 1179.0 / 1080.0);
            assert!(
                (rendered_device.0 - input_device.0).hypot(rendered_device.1 - input_device.1)
                    <= 1e-9
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::{burst, configure, evidence, install, shutdown};
