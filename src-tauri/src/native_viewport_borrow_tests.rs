//! Real GPU context, original borrowed host and retained-slot admission controls.
use crate::native_application_contract::ViewportInput;
use crate::native_application_viewport::{with_retained_target, RetainedViewport};
use crate::native_viewport::{NativeViewportConsumer, ViewportGpuBinding};
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{
        AcquiredSurfaceFrame, DesktopViewportHost, SettledSurfaceFrame, SurfaceAttempt,
        SurfacePort, SurfacePresentation, SurfaceRecoveryError, ViewportMapping,
    },
};
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

fn mapping() -> ViewportMapping {
    serde_json::from_value::<ViewportInput>(serde_json::json!({
        "cssBounds":{"x":0,"y":0,"width":320,"height":180},
        "physicalExtent":{"width":320,"height":180},
        "compositionExtent":{"width":320,"height":180},"reportedDpr":1
    }))
    .unwrap()
    .admit()
    .unwrap()
}
struct Port(Arc<AtomicUsize>);
struct Frame;
impl AcquiredSurfaceFrame for Frame {
    fn settle(self) -> SettledSurfaceFrame {
        SettledSurfaceFrame::Presented
    }
}
impl SurfacePort for Port {
    type Frame<'p, 's> = Frame;
    fn acquire<'p, 's>(&'p mut self, _: SurfacePresentation<'s>) -> SurfaceAttempt<Frame> {
        panic!("borrowing must not acquire a frame itself")
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn host(disposals: &Arc<AtomicUsize>) -> DesktopViewportHost<Port> {
    DesktopViewportHost::new(Port(disposals.clone()), mapping())
}
struct Consumer {
    calls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
    gpu_address: usize,
    host_address: usize,
    mode: u8,
}
impl Drop for Consumer {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
impl NativeViewportConsumer for Consumer {
    type Output = usize;
    fn consume<P: SurfacePort>(
        self,
        gpu: &mut Compositor,
        view: &mut DesktopViewportHost<P>,
    ) -> Result<usize, String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(gpu as *mut _ as usize, self.gpu_address);
        assert_eq!(view as *mut _ as usize, self.host_address);
        match self.mode {
            1 => Err("consumer refused".into()),
            2 => panic!("consumer interrupted"),
            3 => {
                view.dispose();
                Ok(7)
            }
            _ => Ok(7),
        }
    }
}
fn consumer(gpu: &Compositor, view: &DesktopViewportHost<Port>, mode: u8) -> Consumer {
    Consumer {
        calls: Arc::new(AtomicUsize::new(0)),
        drops: Arc::new(AtomicUsize::new(0)),
        gpu_address: gpu as *const _ as usize,
        host_address: view as *const _ as usize,
        mode,
    }
}

#[test]
fn gpu_borrow_preserves_original_addresses_and_refuses_foreign_absent_and_disposed_before_consumer()
{
    let mut gpu = Compositor::new().unwrap();
    let mut foreign = Compositor::new().unwrap();
    let mut binding = ViewportGpuBinding::capture(&gpu);
    let disposals = Arc::new(AtomicUsize::new(0));
    let mut view = host(&disposals);
    for (context, present) in [(&mut foreign, true), (&mut gpu, false)] {
        let probe = consumer(context, &view, 0);
        let (calls, drops) = (probe.calls.clone(), probe.drops.clone());
        assert!(binding
            .consume(present.then_some(&mut view), context, probe)
            .is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
    let probe = consumer(&gpu, &view, 0);
    assert_eq!(
        binding.consume(Some(&mut view), &mut gpu, probe).unwrap(),
        7
    );
    let probe = consumer(&gpu, &view, 1);
    assert!(binding
        .consume(Some(&mut view), &mut gpu, probe)
        .unwrap_err()
        .to_string()
        .contains("consumer refused"));
    let probe = consumer(&gpu, &view, 3);
    assert_eq!(
        binding.consume(Some(&mut view), &mut gpu, probe).unwrap(),
        7
    );
    assert_eq!(disposals.load(Ordering::SeqCst), 1);
    let probe = consumer(&gpu, &view, 0);
    let calls = probe.calls.clone();
    assert!(binding.consume(Some(&mut view), &mut gpu, probe).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn gpu_unwind_keeps_cleanup_debt_until_retire_and_same_context_rebind() {
    let mut gpu = Compositor::new().unwrap();
    let foreign = Compositor::new().unwrap();
    let mut binding = ViewportGpuBinding::capture(&gpu);
    let disposals = Arc::new(AtomicUsize::new(0));
    let mut view = Some(host(&disposals));
    let probe = consumer(&gpu, view.as_ref().unwrap(), 2);
    let drops = probe.drops.clone();
    assert!(binding
        .consume(view.as_mut(), &mut gpu, probe)
        .unwrap_err()
        .to_string()
        .contains("panicked"));
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(disposals.load(Ordering::SeqCst), 0);
    let probe = consumer(&gpu, view.as_ref().unwrap(), 0);
    let calls = probe.calls.clone();
    assert!(binding.consume(view.as_mut(), &mut gpu, probe).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(binding
        .rebind(&mut view, &gpu, || panic!(
            "occupied host must reject before rebuilding"
        ))
        .is_err());
    assert!(binding.retire(&mut view).is_empty());
    assert!(view.is_none());
    assert_eq!(disposals.load(Ordering::SeqCst), 1);
    assert!(binding.retire(&mut view).is_empty());
    assert_eq!(disposals.load(Ordering::SeqCst), 1);
    assert!(binding
        .rebind(&mut view, &foreign, || panic!(
            "foreign GPU must reject before rebuilding"
        ))
        .is_err());
    assert!(view.is_none());
    binding
        .rebind(&mut view, &gpu, || Ok(host(&disposals)))
        .unwrap();
    let probe = consumer(&gpu, view.as_ref().unwrap(), 0);
    assert_eq!(binding.consume(view.as_mut(), &mut gpu, probe).unwrap(), 7);
    binding.retire(&mut view);
    assert_eq!(disposals.load(Ordering::SeqCst), 2);
}

#[test]
fn gpu_foreign_thread_never_invokes_the_borrowed_consumer() {
    let mut gpu = Compositor::new().unwrap();
    let mut binding = ViewportGpuBinding::capture(&gpu);
    std::thread::spawn(move || {
        let mut view = host(&Arc::new(AtomicUsize::new(0)));
        let probe = consumer(&gpu, &view, 0);
        let calls = probe.calls.clone();
        assert!(binding
            .consume(Some(&mut view), &mut gpu, probe)
            .unwrap_err()
            .to_string()
            .contains("thread"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    })
    .join()
    .unwrap();
}

#[test]
fn retained_slot_refuses_missing_wrong_instance_and_reentry_without_invoking_callback() {
    let slot = RefCell::new(None::<RetainedViewport<usize>>);
    assert!(with_retained_target::<_, ()>(&slot, "main", |_| panic!("missing")).is_err());
    *slot.borrow_mut() = Some(RetainedViewport {
        instance_id: "main".into(),
        viewport: 7,
    });
    assert!(with_retained_target::<_, ()>(&slot, "foreign", |_| panic!("foreign")).is_err());
    with_retained_target(&slot, "main", |value| {
        assert_eq!(*value, 7);
        assert!(with_retained_target::<_, ()>(&slot, "main", |_| panic!("reentrant")).is_err());
        *value = 8;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        with_retained_target(&slot, "main", |value| Ok(*value)).unwrap(),
        8
    );
}
