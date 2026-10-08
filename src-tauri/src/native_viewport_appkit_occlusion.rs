//! Exact-window visibility hints; presentation remains the surface host's decision.

use crate::native_viewport::NativeViewportError;
use block::ConcreteBlock;
use native_engine::{compositor::CompositionResult, desktop_viewport::ViewportMapping};
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use serde_json::{json, Value};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    marker::PhantomData,
    rc::Rc,
    time::Instant,
};
use tauri::{Emitter, Manager};

const RECORD_LIMIT: u16 = 256;
const RECORD_BYTES: usize = 2048;

struct Observations {
    enabled: bool,
    records: u16,
    epoch: u64,
    started: Instant,
    active: Option<Value>,
    #[cfg(test)]
    capture: Option<Vec<Value>>,
}

impl Observations {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            records: 0,
            epoch: 0,
            started: Instant::now(),
            active: None,
            #[cfg(test)]
            capture: None,
        }
    }

    fn emit(&mut self, event: &'static str, fields: Value) {
        if !self.enabled || self.records >= RECORD_LIMIT {
            return;
        }
        self.records += 1;
        let truncated = self.records == RECORD_LIMIT;
        let record = json!({"diagnostic":"native-occlusion-mapping/v1",
            "sequence":self.records,"elapsedMs":self.started.elapsed().as_millis(),
            "exposureEpoch":self.epoch,"preview":self.active,
            "event":if truncated { "truncated" } else { event },
            "fields":if truncated { Value::Null } else { fields }});
        let Ok(mut line) = serde_json::to_vec(&record) else {
            return;
        };
        if line.len() > RECORD_BYTES {
            return;
        }
        #[cfg(test)]
        if let Some(capture) = self.capture.as_mut() {
            capture.push(record);
            return;
        }
        line.push(b'\n');
        let _ = std::io::stderr().lock().write_all(&line);
    }
}

thread_local! {
    static OBSERVATIONS: RefCell<Observations> = RefCell::new(Observations::new(
        std::env::var_os("NEMO_NATIVE_OCCLUSION_DIAGNOSTICS").as_deref() == Some(std::ffi::OsStr::new("1"))));
}

fn identifier(value: &str) -> Value {
    if !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        json!(value)
    } else {
        json!("MISSING")
    }
}

/// Only bounded scalar data lives across this synchronous preview; no AppKit
/// pointer, native authority, callback or resource is retained by this guard.
pub(crate) struct PreviewObservation {
    active: bool,
    finished: Cell<bool>,
    previous: Option<Value>,
    _main_thread: PhantomData<Rc<()>>,
}

pub(crate) fn observe_preview(instance: &str, result: &CompositionResult) -> PreviewObservation {
    let mut observation = PreviewObservation {
        active: false,
        finished: Cell::new(false),
        previous: None,
        _main_thread: PhantomData,
    };
    OBSERVATIONS.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return;
        };
        if !state.enabled || state.records >= RECORD_LIMIT {
            return;
        }
        let identity = result.scheduled_identity();
        observation.previous = state.active.take();
        state.active = Some(json!({"instanceId":identifier(instance),
            "documentId":identifier(result.document_id()),"revision":result.content_revision(),
            "frame":identity.evaluation_key().frame(),"workId":identity.work_id().value(),
            "viewGeneration":identity.view_generation().value(),"jsLifecycleGeneration":"MISSING",
            "correlation":state.records + 1}));
        observation.active = true;
        state.emit("preview-begin", Value::Null);
    });
    observation
}

impl PreviewObservation {
    pub(crate) fn finish(&self, status: &'static str) {
        if !self.active || self.finished.replace(true) {
            return;
        }
        OBSERVATIONS.with(|state| {
            if let Ok(mut state) = state.try_borrow_mut() {
                state.emit("preview-end", json!({"status":status}));
            }
        });
    }
}

impl Drop for PreviewObservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        OBSERVATIONS.with(|state| {
            if let Ok(mut state) = state.try_borrow_mut() {
                if !self.finished.get() {
                    state.emit(
                        "preview-no-receipt",
                        json!({"status":"MISSING",
                        "unwinding":std::thread::panicking()}),
                    );
                }
                state.active = self.previous.take();
            }
        });
    }
}

pub(super) fn acquire(
    surface: &wgpu::Surface<'_>,
    view: &super::OwnedView,
    mapping: ViewportMapping,
) -> wgpu::CurrentSurfaceTexture {
    OBSERVATIONS.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else { return; };
        if !state.enabled || state.records >= RECORD_LIMIT { return; }
        let raw = view.raw;
        if raw.is_null() || !is_main_thread() {
            state.emit("mapping", json!({"mapping":"MISSING"}));
            return;
        }
        unsafe {
            let window: *mut Object = msg_send![raw, window];
            let parent: *mut Object = msg_send![raw, superview];
            let frame: super::NSRect = msg_send![raw, frame];
            let bounds: Option<[f64; 4]> = (!parent.is_null()).then(|| {
                let b: super::NSRect = msg_send![parent, bounds];
                [b.origin.x, b.origin.y, b.size.width, b.size.height]
            });
            let window_state = window_fields(window);
            let css = mapping.css_bounds();
            let physical = mapping.physical_extent();
            state.emit("mapping", json!({"window":window_state,"attached":view.attached,
                "superviewPresent":!parent.is_null(),"frame":[frame.origin.x,frame.origin.y,frame.size.width,frame.size.height],
                "superviewBounds":bounds,"cssBounds":[css.x,css.y,css.width,css.height],
                "physicalExtent":[physical.width,physical.height],"reportedDpr":mapping.reported_dpr()}));
        }
    });
    // This is the original acquisition, exactly once, also when diagnostics are OFF.
    let outcome = surface.get_current_texture();
    OBSERVATIONS.with(|state| {
        if let Ok(mut state) = state.try_borrow_mut() {
            if !state.enabled || state.records >= RECORD_LIMIT {
                return;
            }
            let label = match &outcome {
                wgpu::CurrentSurfaceTexture::Success(_) => "success",
                wgpu::CurrentSurfaceTexture::Suboptimal(_) => "suboptimal",
                wgpu::CurrentSurfaceTexture::Lost => "lost",
                wgpu::CurrentSurfaceTexture::Outdated => "outdated",
                wgpu::CurrentSurfaceTexture::Timeout => "timeout",
                wgpu::CurrentSurfaceTexture::Occluded => "occluded",
                wgpu::CurrentSurfaceTexture::Validation => "validation",
            };
            state.emit("surface-acquisition", json!({"outcome":label}));
        }
    });
    outcome
}

unsafe fn window_fields(window: *mut Object) -> Value {
    if window.is_null() {
        return json!("MISSING");
    }
    let occlusion: usize = msg_send![window, occlusionState];
    let key: bool = msg_send![window, isKeyWindow];
    let minimized: bool = msg_send![window, isMiniaturized];
    let visible: bool = msg_send![window, isVisible];
    let number: isize = msg_send![window, windowNumber];
    json!({"windowNumber":number,"occlusionVisible":WindowOcclusionHint::from_state(occlusion).visible,
        "isVisible":visible,"key":key,"minimized":minimized})
}

fn observe_exposure(window: *mut Object) {
    OBSERVATIONS.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return;
        };
        if !state.enabled || state.records >= RECORD_LIMIT {
            return;
        }
        state.epoch = state.epoch.saturating_add(1);
        state.emit("exposure", unsafe { window_fields(window) });
    });
}

#[cfg(test)]
pub(crate) struct TestObservations(Option<Observations>);
#[cfg(test)]
pub(crate) fn test_observations() -> TestObservations {
    OBSERVATIONS.with(|state| {
        let mut observed = Observations::new(true);
        observed.capture = Some(Vec::new());
        TestObservations(Some(state.replace(observed)))
    })
}
#[cfg(test)]
impl TestObservations {
    pub(crate) fn validate_last(&self, expected: &str) {
        OBSERVATIONS.with(|state| {
            let state = state.borrow();
            let records = state.capture.as_ref().unwrap();
            assert_eq!(records.last().unwrap()["fields"]["status"], expected);
            assert!(
                state.active.is_none(),
                "preview scope must not survive return"
            );
        });
    }
}
#[cfg(test)]
impl Drop for TestObservations {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            OBSERVATIONS.with(|state| state.replace(previous));
        }
    }
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {
    static NSWindowDidChangeOcclusionStateNotification: *mut Object;
}

#[derive(Clone, serde::Serialize)]
struct WindowOcclusionHint {
    visible: bool,
}

impl WindowOcclusionHint {
    fn from_state(state: usize) -> Self {
        // NSWindowOcclusionStateVisible is bit 1, not NSWindow.isVisible.
        Self {
            visible: state & (1 << 1) != 0,
        }
    }
}

fn is_main_thread() -> bool {
    unsafe { msg_send![class!(NSThread), isMainThread] }
}

/// Constructed on the main thread; the registration's raw pointers make this
/// owner !Send/!Sync, so normal Rust ownership also confines its drop there.
pub(super) struct WindowOcclusionObserver {
    _registration: NotificationRegistration,
}

impl WindowOcclusionObserver {
    pub(super) fn new(window: &tauri::WebviewWindow) -> Result<Self, NativeViewportError> {
        if !is_main_thread() {
            return Err(NativeViewportError::new(
                "AppKit occlusion binding requires the main thread",
            ));
        }
        let raw = window
            .ns_window()
            .map_err(|_| NativeViewportError::new("resolve AppKit window for occlusion binding"))?
            as *mut Object;
        let view = window
            .ns_view()
            .map_err(|_| NativeViewportError::new("resolve AppKit view for occlusion binding"))?
            as *mut Object;
        let attached_window: *mut Object = unsafe { msg_send![view, window] };
        if raw.is_null() || attached_window != raw {
            return Err(NativeViewportError::new(
                "AppKit occlusion window is detached",
            ));
        }
        let queue: *mut Object = unsafe { msg_send![class!(NSOperationQueue), mainQueue] };
        if queue.is_null() {
            return Err(NativeViewportError::new(
                "AppKit main operation queue is unavailable",
            ));
        }
        let app = window.app_handle().clone();
        let label = window.label().to_owned();
        // The block owns no viewport, registration token or retained NSWindow.
        // A queued hint is cancellable and can never manufacture a frame receipt.
        let registration = unsafe {
            NotificationRegistration::new(raw, queue, move |source| {
                if !is_main_thread() {
                    return;
                }
                let state: usize = msg_send![source, occlusionState];
                observe_exposure(source);
                let _ = app.emit_to(
                    label.as_str(),
                    "nemo-native-window-occlusion",
                    WindowOcclusionHint::from_state(state),
                );
            })?
        };
        Ok(Self {
            _registration: registration,
        })
    }
}

/// The center owns a copied block; this guard owns +1 references to the center,
/// token and exact observed object. Registration and removal use the same thread.
struct NotificationRegistration {
    center: *mut Object,
    object: *mut Object,
    token: *mut Object,
    active: Arc<AtomicBool>,
}

impl NotificationRegistration {
    /// SAFETY: object is live and may be retained on this thread. An AppKit
    /// object requires the main thread and a main-queue handler. Tests may use
    /// Foundation-only objects with synchronous delivery (nil queue).
    unsafe fn new(
        object: *mut Object,
        queue: *mut Object,
        handler: impl Fn(*mut Object) + Send + Sync + 'static,
    ) -> Result<Self, NativeViewportError> {
        let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
        if object.is_null() || center.is_null() {
            return Err(NativeViewportError::new(
                "AppKit occlusion observer is unavailable",
            ));
        }
        let mut registration = Self {
            center: msg_send![center, retain],
            object: msg_send![object, retain],
            token: ptr::null_mut(),
            active: Arc::new(AtomicBool::new(true)),
        };
        let active = Arc::clone(&registration.active);
        let expected = object as usize;
        let callback = ConcreteBlock::new(move |notification: *mut Object| {
            // Removal does not retract an already queued block. Check before
            // reading its object; no raw observed pointer is captured/dereferenced.
            if !active.load(Ordering::Acquire) || notification.is_null() {
                return;
            }
            let source: *mut Object = msg_send![notification, object];
            if source as usize == expected {
                // No Rust panic may unwind across the Objective-C block ABI.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(source)));
            }
        })
        .copy();
        let token: *mut Object = msg_send![center,
            addObserverForName: NSWindowDidChangeOcclusionStateNotification
            object: object
            queue: queue
            usingBlock: &*callback
        ];
        if token.is_null() {
            return Err(NativeViewportError::new(
                "register AppKit occlusion observer",
            ));
        }
        registration.token = msg_send![token, retain];
        Ok(registration)
    }

    fn dispose(&mut self) {
        self.active.store(false, Ordering::Release);
        unsafe {
            if !self.token.is_null() {
                let _: () = msg_send![self.center, removeObserver: self.token];
                let _: () = msg_send![self.token, release];
                self.token = ptr::null_mut();
            }
            // Always unregister before the observed NSWindow can deallocate.
            if !self.object.is_null() {
                let _: () = msg_send![self.object, release];
                self.object = ptr::null_mut();
            }
            if !self.center.is_null() {
                let _: () = msg_send![self.center, release];
                self.center = ptr::null_mut();
            }
        }
    }
}

impl Drop for NotificationRegistration {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn diagnostic_identifiers_and_budget_reject_private_text_and_stop() {
        for invalid in ["", "/private", "\n", "../key", &"a".repeat(65)] {
            assert_eq!(identifier(invalid), json!("MISSING"));
        }
        assert_eq!(identifier("document-1_A"), json!("document-1_A"));
        let mut state = Observations::new(false);
        state.capture = Some(Vec::new());
        state.emit("preview-begin", Value::Null);
        assert_eq!(state.records, 0);
        assert!(state.capture.as_ref().unwrap().is_empty());
        state.enabled = true;
        for _ in 0..RECORD_LIMIT + 10 {
            state.emit("preview-begin", Value::Null);
        }
        let records = state.capture.as_ref().unwrap();
        assert_eq!(records.len(), usize::from(RECORD_LIMIT));
        assert_eq!(records.last().unwrap()["event"], "truncated");
        assert!(records.iter().all(|r| r.to_string().len() <= RECORD_BYTES));
        assert_eq!(records[0].as_object().unwrap().len(), 7);
    }

    #[test]
    fn visibility_uses_only_the_documented_occlusion_bit() {
        for (state, visible) in [(0, false), (8192, false), (2, true), (8194, true)] {
            let hint = WindowOcclusionHint::from_state(state);
            assert_eq!(
                serde_json::to_value(hint).unwrap(),
                serde_json::json!({ "visible": visible })
            );
        }
    }

    #[test]
    fn exact_object_notifications_stop_after_idempotent_disposal_and_rebind() {
        // Foundation-only probes exercise the real notification center without
        // constructing NSWindow/NSView or needing an AppKit UI/GPU runtime.
        unsafe {
            let pool: *mut Object = msg_send![class!(NSAutoreleasePool), new];
            let first: *mut Object = msg_send![class!(NSObject), new];
            let second: *mut Object = msg_send![class!(NSObject), new];
            let count = Arc::new(AtomicUsize::new(0));
            let seen = Arc::clone(&count);
            let mut registration =
                NotificationRegistration::new(first, ptr::null_mut(), move |_| {
                    seen.fetch_add(1, Ordering::SeqCst);
                })
                .unwrap();
            let center = registration.center;
            let post = |object: *mut Object| {
                let _: () = msg_send![center,
                    postNotificationName: NSWindowDidChangeOcclusionStateNotification
                    object: object
                ];
            };
            post(second);
            assert_eq!(count.load(Ordering::SeqCst), 0);
            post(first);
            post(first);
            assert_eq!(count.load(Ordering::SeqCst), 2);
            let retired = Arc::clone(&registration.active);
            registration.dispose();
            registration.dispose();
            assert!(!retired.load(Ordering::Acquire));
            post(first);
            assert_eq!(count.load(Ordering::SeqCst), 2);
            let seen = Arc::clone(&count);
            let rebound = NotificationRegistration::new(second, ptr::null_mut(), move |_| {
                seen.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
            post(first);
            post(second);
            assert_eq!(count.load(Ordering::SeqCst), 3);
            drop(rebound);
            post(second);
            assert_eq!(count.load(Ordering::SeqCst), 3);
            let _: () = msg_send![first, release];
            let _: () = msg_send![second, release];
            let _: () = msg_send![pool, drain];
        }
    }
}
