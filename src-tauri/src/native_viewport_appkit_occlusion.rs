//! Exact-window visibility hints; presentation remains the surface host's decision.

use crate::native_viewport::NativeViewportError;
use block::ConcreteBlock;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{Emitter, Manager};

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
