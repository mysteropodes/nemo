//! Staged N16 macOS Tauri surface binding.
//!
//! Construction is deliberately two-stage: the AppKit surface is created from
//! `CompositorInstance::instance`, then that surface participates in adapter
//! selection before the compositor creates its only device and queue. This
//! module adds no startup selection and exposes no CPU or JavaScript pixels.

use native_engine::compositor::CompositorError;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
#[rustfmt::skip]
pub(crate) struct NativeViewportError { message: String }

#[rustfmt::skip]
impl NativeViewportError {
    fn new(message: impl Into<String>) -> Self { Self { message: message.into() } }
}

#[rustfmt::skip]
impl Display for NativeViewportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result { formatter.write_str(&self.message) }
}

impl std::error::Error for NativeViewportError {}

#[rustfmt::skip]
impl From<CompositorError> for NativeViewportError {
    fn from(error: CompositorError) -> Self { Self::new(error.to_string()) }
}

/// The generic method prevents the private platform port or a borrowed frame
/// from escaping as an associated output. The caller retains its original GPU.
pub(crate) trait NativeViewportConsumer {
    type Output;
    fn consume<P: native_engine::desktop_viewport::SurfacePort>(
        self,
        compositor: &mut native_engine::compositor::Compositor,
        viewport: &mut native_engine::desktop_viewport::DesktopViewportHost<P>,
    ) -> Result<Self::Output, String>;
}

pub(crate) struct ViewportGpuBinding {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    thread: std::thread::ThreadId,
    unwound: bool,
}
impl ViewportGpuBinding {
    pub(crate) fn capture(gpu: &native_engine::compositor::Compositor) -> Self {
        Self {
            instance: gpu.instance().clone(),
            adapter: gpu.adapter().clone(),
            device: gpu.device().clone(),
            queue: gpu.queue().clone(),
            thread: std::thread::current().id(),
            unwound: false,
        }
    }
    fn require_context(
        &self,
        gpu: &native_engine::compositor::Compositor,
    ) -> Result<(), NativeViewportError> {
        if self.thread != std::thread::current().id() {
            return Err(NativeViewportError::new("native viewport thread mismatch"));
        }
        if self.instance != *gpu.instance()
            || self.adapter != *gpu.adapter()
            || self.device != *gpu.device()
            || self.queue != *gpu.queue()
        {
            return Err(NativeViewportError::new(
                "native viewport GPU context mismatch",
            ));
        }
        Ok(())
    }
    pub(crate) fn retire<P: native_engine::desktop_viewport::SurfacePort>(
        &mut self,
        host: &mut Option<native_engine::desktop_viewport::DesktopViewportHost<P>>,
    ) -> Vec<native_engine::scheduler::WorkId> {
        host.take()
            .map_or_else(Vec::new, |mut value| value.dispose())
    }
    pub(crate) fn rebind<P: native_engine::desktop_viewport::SurfacePort>(
        &mut self,
        host: &mut Option<native_engine::desktop_viewport::DesktopViewportHost<P>>,
        compositor: &native_engine::compositor::Compositor,
        create: impl FnOnce() -> Result<
            native_engine::desktop_viewport::DesktopViewportHost<P>,
            NativeViewportError,
        >,
    ) -> Result<(), NativeViewportError> {
        self.require_context(compositor)?;
        if host.is_some() {
            return Err(NativeViewportError::new("native viewport was not retired"));
        }
        *host = Some(create()?);
        self.unwound = false;
        Ok(())
    }
    pub(crate) fn consume<
        P: native_engine::desktop_viewport::SurfacePort,
        C: NativeViewportConsumer,
    >(
        &mut self,
        host: Option<&mut native_engine::desktop_viewport::DesktopViewportHost<P>>,
        compositor: &mut native_engine::compositor::Compositor,
        consumer: C,
    ) -> Result<C::Output, NativeViewportError> {
        self.require_context(compositor)?;
        if self.unwound {
            return Err(NativeViewportError::new(
                "native viewport consumer unwound; retire and rebind required",
            ));
        }
        let host =
            host.ok_or_else(|| NativeViewportError::new("native viewport is retired or absent"))?;
        // This existing read checks the engine's disposed/fatal latch without
        // resizing, registering work or acquiring a surface frame.
        host.pointer_intent(native_engine::desktop_viewport::CssPoint { x: 0., y: 0. })
            .map_err(|error| {
                NativeViewportError::new(format!("native viewport is unavailable: {error:?}"))
            })?;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            consumer.consume(compositor, host)
        })) {
            Ok(result) => result.map_err(NativeViewportError::new),
            Err(_) => {
                self.unwound = true;
                Err(NativeViewportError::new(
                    "native viewport consumer panicked; cleanup is required",
                ))
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[path = "native_viewport_appkit.rs"]
mod platform;

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::*;
    use native_engine::{
        compositor::{CompositionResult, Compositor},
        desktop_viewport::{PresentationReceipt, ViewportError, ViewportMapping},
        render_scene::ScheduledFrameIdentity,
        scheduler::WorkId,
    };

    pub(crate) struct NativeViewport;

    #[rustfmt::skip]
    impl NativeViewport {
        pub(crate) fn from_app(_app: &tauri::AppHandle, _window_label: &str, _mapping: ViewportMapping) -> Result<(Self, Compositor), NativeViewportError> { Err(NativeViewportError::new("native viewport unavailable: the staged N16 surface is macOS-only")) }
        pub(crate) fn with_borrowed_target<C: NativeViewportConsumer>(&mut self, _compositor: &mut Compositor, _consumer: C) -> Result<C::Output, NativeViewportError> { Err(NativeViewportError::new("native viewport unavailable: macOS only")) }
        pub(crate) fn register(&mut self, _identity: ScheduledFrameIdentity) -> Result<(), ViewportError> { Err(ViewportError::Disposed) }
        pub(crate) fn resize(&mut self, _mapping: ViewportMapping) -> Result<(), ViewportError> { Err(ViewportError::Disposed) }
        pub(crate) fn present(&mut self, _compositor: &Compositor, _result: &CompositionResult) -> Result<PresentationReceipt, ViewportError> { Err(ViewportError::Disposed) }
        pub(crate) fn retire_replacement(&mut self) -> Vec<WorkId> { Vec::new() }
        pub(crate) fn rebind_existing(&mut self, _compositor: &Compositor) -> Result<(), NativeViewportError> { Err(NativeViewportError::new("native viewport unavailable: macOS only")) }
        pub(crate) fn dispose(&mut self) -> Vec<WorkId> { Vec::new() }
    }
}

pub(crate) use platform::NativeViewport;

#[cfg(target_os = "macos")]
pub(crate) use platform::occlusion::{observe_preview, PreviewObservation};
#[cfg(all(test, target_os = "macos"))]
pub(crate) use platform::occlusion::test_observations;

/// The composed frame a preview diagnostic correlates: the opacity path's `CompositionResult`,
/// or an object preview's request pins and composed work ([`ScheduledPreview`]).
pub(crate) trait PreviewSubject {
    fn document_id(&self) -> &str;
    fn content_revision(&self) -> u64;
    fn frame(&self) -> u32;
    fn work_id(&self) -> u64;
    fn view_generation(&self) -> u64;
}

impl PreviewSubject for native_engine::compositor::CompositionResult {
    fn document_id(&self) -> &str {
        Self::document_id(self)
    }
    fn content_revision(&self) -> u64 {
        Self::content_revision(self)
    }
    fn frame(&self) -> u32 {
        self.scheduled_identity().evaluation_key().frame()
    }
    fn work_id(&self) -> u64 {
        self.scheduled_identity().work_id().value()
    }
    fn view_generation(&self) -> u64 {
        self.scheduled_identity().view_generation().value()
    }
}

/// An object preview's request pins and composed work (`ObjectPreviewDiagnostic`).
pub(crate) struct ScheduledPreview<'a> {
    pub(crate) document_id: &'a str,
    pub(crate) content_revision: u64,
    pub(crate) frame: u32,
    pub(crate) work_id: u64,
    pub(crate) view_generation: u64,
}

impl PreviewSubject for ScheduledPreview<'_> {
    fn document_id(&self) -> &str {
        self.document_id
    }
    fn content_revision(&self) -> u64 {
        self.content_revision
    }
    fn frame(&self) -> u32 {
        self.frame
    }
    fn work_id(&self) -> u64 {
        self.work_id
    }
    fn view_generation(&self) -> u64 {
        self.view_generation
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) struct PreviewObservation;
#[cfg(not(target_os = "macos"))]
impl PreviewObservation {
    pub(crate) fn finish(&self, _status: &'static str) {}
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn observe_preview(
    _instance: &str,
    _result: &impl PreviewSubject,
) -> PreviewObservation {
    PreviewObservation
}

#[cfg(all(test, target_os = "macos"))]
mod appkit_detach_tests {
    use crate::native_viewport::platform::OwnedView;
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    };

    static DETACH_CALLS: AtomicUsize = AtomicUsize::new(0);
    static PROBE_CLASS: OnceLock<usize> = OnceLock::new();

    extern "C" fn record_detach(_object: &Object, _selector: Sel) {
        DETACH_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    fn probe_class() -> *const Class {
        *PROBE_CLASS.get_or_init(|| {
            let mut declaration = ClassDecl::new("N20R4DetachProbe", class!(NSObject))
                .expect("unique low-level detach probe class");
            unsafe {
                declaration.add_method(
                    sel!(removeFromSuperview),
                    record_detach as extern "C" fn(&Object, Sel),
                );
            }
            declaration.register() as *const Class as usize
        }) as *const Class
    }

    #[test]
    fn owned_view_dispose_sends_detach_selector_once_to_injected_receiver() {
        // This invokes the production disposal method with a non-UI Objective-C
        // receiver; it does not construct or present an AppKit NSView.
        let allocated: *mut Object = unsafe { msg_send![probe_class(), alloc] };
        assert!(!allocated.is_null());
        let raw: *mut Object = unsafe { msg_send![allocated, init] };
        assert!(!raw.is_null());
        let before = DETACH_CALLS.load(Ordering::SeqCst);
        let mut view = OwnedView {
            raw,
            attached: true,
        };
        view.dispose();
        view.dispose();
        assert_eq!(DETACH_CALLS.load(Ordering::SeqCst), before + 1);
        assert!(view.raw.is_null());
        assert!(!view.attached);
    }
}

#[cfg(test)]
pub(crate) mod replacement_test_support {
    #[cfg(target_os = "macos")]
    pub(crate) use super::test_observations;
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn test_observations() {}
    use native_engine::compositor::{CompositionResult, Compositor};
    use native_engine::desktop_viewport::{
        AcquiredSurfaceFrame, DesktopViewportHost, PresentationReceipt, SettledSurfaceFrame,
        SurfaceAttempt, SurfacePort, SurfacePresentation, SurfaceRecoveryError, ViewportError,
        ViewportMapping,
    };
    use native_engine::render_scene::ScheduledFrameIdentity;
    use native_engine::scheduler::WorkId;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    pub(crate) struct SurfaceState {
        pub(crate) attached_snapshot: Option<String>,
        pub(crate) disposals: usize,
        pub(crate) rebinds: usize,
        pub(crate) recreates: usize,
        pub(crate) reconfigures: usize,
        pub(crate) next_outcome: Option<FakeOutcome>,
        context: Option<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue)>,
    }

    #[derive(Clone, Copy, Debug)]
    pub(crate) enum FakeOutcome {
        Timeout,
        Occluded,
        Lost,
        Outdated,
        ValidationFailure,
        DeviceLost,
    }

    struct FakeSurface {
        state: Arc<Mutex<SurfaceState>>,
    }

    struct FakeFrame {
        state: Arc<Mutex<SurfaceState>>,
        snapshot: String,
        settled: SettledSurfaceFrame,
    }

    impl AcquiredSurfaceFrame for FakeFrame {
        fn settle(self) -> SettledSurfaceFrame {
            if self.settled == SettledSurfaceFrame::Presented {
                self.state.lock().unwrap().attached_snapshot = Some(self.snapshot);
            }
            self.settled
        }
    }

    impl SurfacePort for FakeSurface {
        type Frame<'port, 'source> = FakeFrame;

        fn acquire<'port, 'source>(
            &'port mut self,
            source: SurfacePresentation<'source>,
        ) -> SurfaceAttempt<Self::Frame<'port, 'source>> {
            let mut state = self.state.lock().unwrap();
            let context = (
                source.instance.clone(),
                source.adapter.clone(),
                source.device.clone(),
                source.queue.clone(),
            );
            if let Some(old) = &state.context {
                assert!(
                    old.0 == context.0
                        && old.1 == context.1
                        && old.2 == context.2
                        && old.3 == context.3,
                    "replacement presentation must reuse the original GPU context"
                );
            } else {
                state.context = Some(context);
            }
            let settled = match state.next_outcome.take() {
                Some(FakeOutcome::Timeout) => return SurfaceAttempt::Timeout,
                Some(FakeOutcome::Occluded) => return SurfaceAttempt::Occluded,
                Some(FakeOutcome::Lost) => return SurfaceAttempt::Lost,
                Some(FakeOutcome::Outdated) => return SurfaceAttempt::Outdated,
                Some(FakeOutcome::ValidationFailure) => SettledSurfaceFrame::ValidationFailure,
                Some(FakeOutcome::DeviceLost) => SettledSurfaceFrame::DeviceLost,
                None => SettledSurfaceFrame::Presented,
            };
            drop(state);
            SurfaceAttempt::Acquired(FakeFrame {
                state: Arc::clone(&self.state),
                snapshot: source
                    .identity
                    .evaluation_key()
                    .document_snapshot_id()
                    .to_owned(),
                settled,
            })
        }

        fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
            self.state.lock().unwrap().recreates += 1;
            Ok(())
        }
        fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
            self.state.lock().unwrap().reconfigures += 1;
            Ok(())
        }
        fn dispose(&mut self) {
            let mut state = self.state.lock().unwrap();
            state.attached_snapshot = None;
            state.disposals += 1;
        }
    }

    pub(crate) struct FakeViewport {
        host: Option<DesktopViewportHost<FakeSurface>>,
        state: Arc<Mutex<SurfaceState>>,
        mapping: ViewportMapping,
    }

    impl FakeViewport {
        pub(crate) fn new(mapping: ViewportMapping, state: Arc<Mutex<SurfaceState>>) -> Self {
            Self {
                host: Some(DesktopViewportHost::new(
                    FakeSurface {
                        state: Arc::clone(&state),
                    },
                    mapping,
                )),
                state,
                mapping,
            }
        }

        pub(crate) fn present(
            &mut self,
            compositor: &Compositor,
            result: &CompositionResult,
            identity: ScheduledFrameIdentity,
        ) -> Result<(WorkId, u64, PresentationReceipt), ViewportError> {
            let host = self.host.as_mut().expect("fake viewport must be attached");
            host.register(identity.clone())?;
            let receipt = host.present_composition(compositor, result)?;
            Ok((
                identity.work_id(),
                identity.view_generation().value(),
                receipt,
            ))
        }

        pub(crate) fn retire_replacement(&mut self) -> Vec<WorkId> {
            self.host
                .take()
                .map_or_else(Vec::new, |mut host| host.dispose())
        }

        pub(crate) fn rebind_existing(&mut self, compositor: &Compositor) -> Result<(), String> {
            assert!(
                self.host.is_none(),
                "A surface must retire before B is bound"
            );
            let mut state = self.state.lock().unwrap();
            let old = state.context.as_ref().expect("A must have been presented");
            assert!(
                old.0 == *compositor.instance()
                    && old.1 == *compositor.adapter()
                    && old.2 == *compositor.device()
                    && old.3 == *compositor.queue(),
                "blank replacement host must use the retained compositor"
            );
            assert!(
                state.attached_snapshot.is_none(),
                "replacement host must start blank"
            );
            state.rebinds += 1;
            drop(state);
            self.host = Some(DesktopViewportHost::new(
                FakeSurface {
                    state: Arc::clone(&self.state),
                },
                self.mapping,
            ));
            Ok(())
        }
    }
}
