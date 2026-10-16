//! Original AppKit viewport construction, borrowing, rebind and terminal disposal.
use crate::native_viewport::{
    platform::{create_surface, occlusion, MacOsSurfacePort, NativeViewport, OwnedView},
    NativeViewportConsumer, NativeViewportError, ViewportGpuBinding,
};
use native_engine::{
    compositor::{CompositionResult, Compositor, CompositorInstance},
    desktop_viewport::{DesktopViewportHost, PresentationReceipt, ViewportError, ViewportMapping},
    render_scene::ScheduledFrameIdentity,
    scheduler::WorkId,
};
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use tauri::Manager;

// Keep the NSWindow opaque; only its WKWebView needs a transparent backing.
// Tauri's pinned macos-private-api feature enables Wry's implementation. On
// the main thread the dispatcher applies this synchronously before attachment.
fn underlay_parent(window: &tauri::WebviewWindow) -> Result<*mut Object, NativeViewportError> {
    let main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
    if !main {
        return Err(NativeViewportError::new(
            "native viewport requires the AppKit main thread",
        ));
    }
    let parent = window
        .ns_view()
        .map_err(|error| NativeViewportError::new(format!("resolve AppKit view: {error}")))?
        as *mut Object;
    if parent.is_null() {
        return Err(NativeViewportError::new(
            "native AppKit parent view is absent",
        ));
    }
    let webview: &tauri::Webview = window.as_ref();
    webview
        .set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)))
        .map_err(|error| {
            NativeViewportError::new(format!("prepare native viewport underlay: {error}"))
        })?;
    Ok(parent)
}

impl NativeViewport {
    pub(crate) fn from_app(
        app: &tauri::AppHandle,
        window_label: &str,
        mapping: ViewportMapping,
    ) -> Result<(Self, Compositor), NativeViewportError> {
        let window = app.get_webview_window(window_label).ok_or_else(|| {
            NativeViewportError::new(format!(
                "native viewport unavailable: Tauri webview window '{window_label}' is absent"
            ))
        })?;
        Self::from_window(&window, mapping)
    }

    fn from_window(
        window: &tauri::WebviewWindow,
        mapping: ViewportMapping,
    ) -> Result<(Self, Compositor), NativeViewportError> {
        let occlusion = occlusion::WindowOcclusionObserver::new(window)?;
        let content_view = underlay_parent(window)?;
        let construction = CompositorInstance::new();
        let view = OwnedView::new(content_view, mapping)?;
        let surface = create_surface(construction.instance(), &view)?;
        let compositor = construction.create_for_surface(&surface)?;
        let port = MacOsSurfacePort::new(surface, view, &compositor, mapping)?;
        Ok((
            Self {
                host: Some(DesktopViewportHost::new(port, mapping)),
                occlusion: Some(occlusion),
                window: window.clone(),
                mapping,
                gpu: ViewportGpuBinding::capture(&compositor),
            },
            compositor,
        ))
    }

    /// Retire the old surface and its view before any replacement core mutation.
    pub(crate) fn retire_replacement(&mut self) -> Vec<WorkId> {
        self.dispose()
    }

    /// Build an unpresented host from the retained window and the existing GPU context.
    pub(crate) fn rebind_existing(
        &mut self,
        compositor: &Compositor,
    ) -> Result<(), NativeViewportError> {
        self.gpu.require_context(compositor)?;
        if self.host.is_some() {
            return Err(NativeViewportError::new("native viewport was not retired"));
        }
        let occlusion = occlusion::WindowOcclusionObserver::new(&self.window)?;
        let content_view = underlay_parent(&self.window)?;
        let view = OwnedView::new(content_view, self.mapping)?;
        let surface = create_surface(compositor.instance(), &view)?;
        let port = MacOsSurfacePort::new(surface, view, compositor, self.mapping)?;
        self.gpu.rebind(&mut self.host, compositor, || {
            Ok(DesktopViewportHost::new(port, self.mapping))
        })?;
        self.occlusion = Some(occlusion);
        Ok(())
    }

    pub(crate) fn with_borrowed_target<C: NativeViewportConsumer>(
        &mut self,
        compositor: &mut Compositor,
        consumer: C,
    ) -> Result<C::Output, NativeViewportError> {
        let main: bool = unsafe { msg_send![class!(NSThread), isMainThread] };
        if !main {
            return Err(NativeViewportError::new(
                "native viewport requires the AppKit main thread",
            ));
        }
        self.gpu.consume(self.host.as_mut(), compositor, consumer)
    }

    pub(crate) fn register(
        &mut self,
        identity: ScheduledFrameIdentity,
    ) -> Result<(), ViewportError> {
        self.host
            .as_mut()
            .ok_or(ViewportError::Disposed)?
            .register(identity)
    }

    pub(crate) fn resize(&mut self, mapping: ViewportMapping) -> Result<(), ViewportError> {
        self.host
            .as_mut()
            .ok_or(ViewportError::Disposed)?
            .resize(mapping)?;
        self.mapping = mapping;
        Ok(())
    }

    pub(crate) fn present(
        &mut self,
        compositor: &Compositor,
        result: &CompositionResult,
    ) -> Result<PresentationReceipt, ViewportError> {
        self.host
            .as_mut()
            .ok_or(ViewportError::Disposed)?
            .present_composition(compositor, result)
    }

    pub(crate) fn dispose(&mut self) -> Vec<WorkId> {
        // Stop window hints before releasing the host; rebind owns a fresh observer.
        self.occlusion.take();
        self.gpu.retire(&mut self.host)
    }
}

impl Drop for NativeViewport {
    fn drop(&mut self) {
        self.dispose();
    }
}
