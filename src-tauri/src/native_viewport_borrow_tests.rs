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

// Exercise the final production TextureBlitter transfer on the compositor's
// sole device/queue. No AppKit window, monitor profile or replacement shader.
#[cfg(target_os = "macos")]
fn read_encoded_rgba8(
    gpu: &Compositor,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    const ROW: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("native-viewport-transfer-readback"),
        size: u64::from(ROW * 2),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW),
                rows_per_image: Some(2),
            },
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue().submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    gpu.device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let mapped = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity(16);
    for row in 0..2 {
        let offset = (row * ROW) as usize;
        rgba.extend_from_slice(&mapped[offset..offset + 8]);
    }
    drop(mapped);
    buffer.unmap();
    // Raw BGRA memory order is not a colour conversion.
    if matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    rgba
}

#[cfg(target_os = "macos")]
#[test]
fn gpu_selected_surface_blit_preserves_encoded_rgba8_bytes() {
    use crate::native_viewport::NativeViewport;
    use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};

    let gpu = Compositor::new().unwrap();
    let errors = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
    let extent = wgpu::Extent3d {
        width: 2,
        height: 2,
        depth_or_array_layers: 1,
    };
    let source = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("native-viewport-retained-encoded-rgba8"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    // Original Camera ink, white, partial alpha and transparent black. Distinct
    // positions also detect a channel, orientation or coverage mistake.
    let encoded = [
        17, 34, 51, 255, 255, 255, 255, 255, 17, 34, 51, 128, 0, 0, 0, 0,
    ];
    gpu.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &encoded,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        extent,
    );
    assert_eq!(read_encoded_rgba8(&gpu, &source, Rgba8Unorm), encoded);
    assert!(pollster::block_on(errors.pop()).is_none());
    for formats in [[Bgra8Unorm, Bgra8UnormSrgb], [Rgba8Unorm, Rgba8UnormSrgb]] {
        let format = NativeViewport::select_surface_format(&formats).unwrap();
        let errors = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
        let target = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("native-viewport-selected-attachment"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let blitter = wgpu::util::TextureBlitter::new(gpu.device(), format);
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        blitter.copy(
            gpu.device(),
            &mut encoder,
            &source.create_view(&wgpu::TextureViewDescriptor::default()),
            &target.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        gpu.queue().submit(Some(encoder.finish()));
        let actual = read_encoded_rgba8(&gpu, &target, format);
        assert!(pollster::block_on(errors.pop()).is_none());
        assert_eq!(actual, encoded, "selected native attachment {format:?}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn surface_format_requires_byte_preserving_unorm_and_prefers_bgra_then_rgba() {
    use crate::native_viewport::NativeViewport;
    use wgpu::TextureFormat::{
        Bgra8Unorm, Bgra8UnormSrgb, Rgb10a2Unorm, Rgba16Float, Rgba8Unorm, Rgba8UnormSrgb,
    };

    for (formats, expected) in [
        (vec![Bgra8Unorm], Bgra8Unorm),
        (vec![Rgba8Unorm], Rgba8Unorm),
        (vec![Rgba8Unorm, Bgra8Unorm], Bgra8Unorm),
        (vec![Bgra8Unorm, Rgba8Unorm], Bgra8Unorm),
        (vec![Rgba8UnormSrgb, Rgba16Float, Rgba8Unorm], Rgba8Unorm),
        (vec![Bgra8UnormSrgb, Rgba8Unorm, Bgra8Unorm], Bgra8Unorm),
    ] {
        assert_eq!(
            NativeViewport::select_surface_format(&formats).unwrap(),
            expected
        );
    }
    for formats in [
        vec![],
        vec![Bgra8UnormSrgb],
        vec![Rgba8UnormSrgb],
        vec![Rgba16Float, Rgb10a2Unorm],
        vec![Bgra8UnormSrgb, Rgba16Float, Rgba8UnormSrgb],
    ] {
        assert!(NativeViewport::select_surface_format(&formats)
            .unwrap_err()
            .to_string()
            .contains("no byte-preserving 8-bit unorm texture format"));
    }
}
