//! T12 / #2232 acceptance point 6 on the real desktop host: two real lossless
//! fixture videos whose pixels AND alpha are a known function of (x, y, frame),
//! the real ffmpeg decoder factory, the real native project owner, its real
//! placement-edit dispatch, real PNG export through the desktop artifact port and
//! real GPU preview readback. Export output is checked independently of the
//! producer: dimensions from the PNG header, frame counts from the published
//! directory, and pixels decoded by the ffmpeg sidecar, never by the engine's
//! own encoder. Needs macOS (Metal + the bundled sidecar).
use super::tests::{ffmpeg_shared, gen_video, test_ffmpeg_path};
use crate::native_application_ports::DesktopArtifactPort;
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{
        AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent, SettledSurfaceFrame,
        SurfaceAttempt, SurfacePort, SurfacePresentation, SurfaceRecoveryError, ViewportMapping,
        ViewportStatus,
    },
    export_job::{ExternalEffectDisposition, JobStatus},
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
};
use serde_json::{json, Value};
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

const W: u32 = 32;
const H: u32 = 16;

// Both sources are W x H, exactly the canvas: the contain fit is the identity, so
// every output pixel is one source texel (no resampling).
// Source A, 20 frames: (10n, 8x, 16y), alpha 255 left of x=16, 8x (128..248) right.
fn source_a() -> String {
    gen_video(
        "t12_host_a_32x16_20f.mov",
        "nullsrc=s=32x16:r=10:d=2,format=rgba,geq=r='10*N':g='8*X':b='16*Y':a='if(lt(X,16),255,8*X)'",
        &["-c:v", "png"],
    )
    .to_string_lossy()
    .into_owned()
}
fn a(x: u32, y: u32, n: u32) -> [u8; 4] {
    let alpha = if x < 16 { 255 } else { 8 * x };
    [10 * n, 8 * x, 16 * y, alpha].map(|v| v as u8)
}
// Source B, 12 frames: (200, 20n, 8x), alpha 255 above y=8, 16y (128..240) below.
fn source_b() -> String {
    gen_video(
        "t12_host_b_32x16_12f.mov",
        "nullsrc=s=32x16:r=12:d=1,format=rgba,geq=r=200:g='20*N':b='8*X':a='if(lt(Y,8),255,16*Y)'",
        &["-c:v", "png"],
    )
    .to_string_lossy()
    .into_owned()
}
fn b(x: u32, y: u32, n: u32) -> [u8; 4] {
    let alpha = if y < 8 { 255 } else { 16 * y };
    [200, 20 * n, 8 * x, alpha].map(|v| v as u8)
}

/// The fixtures are lossless (PNG in MOV, rgba piped by the decoder), so a
/// texel arrives exact. Alpha must match exactly and opaque RGB exactly. Where
/// alpha < 255 the pinned Vello 0.9 fine stage composites premultiplied and the
/// target holds straight RGBA (object_compositor.rs), so RGB takes one
/// premultiply/unpremultiply round trip: error <= 255/(2a) + 0.5 < 2 for the
/// a >= 128 the fixtures use. That rounding is the only tolerance.
fn assert_frame(rgba: &[u8], expected: impl Fn(u32, u32) -> [u8; 4], what: &str) {
    assert_eq!(rgba.len(), (W * H * 4) as usize, "{what}: byte length");
    for y in 0..H {
        for x in 0..W {
            let at = ((y * W + x) * 4) as usize;
            let actual: [u8; 4] = rgba[at..at + 4].try_into().unwrap();
            let want = expected(x, y);
            let tolerance = if want[3] == 255 { 0 } else { 2 };
            assert!(
                actual[3] == want[3]
                    && actual[..3]
                        .iter()
                        .zip(&want[..3])
                        .all(|(p, q)| p.abs_diff(*q) <= tolerance),
                "{what}: pixel ({x},{y}) is {actual:?}, expected {want:?}"
            );
        }
    }
}

/// Independent PNG reading: the IHDR dimensions, and straight RGBA decoded by
/// the checked ffmpeg sidecar (not by the engine that encoded it).
fn decode_png(path: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let bytes = fs::read(path).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR");
    let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    let output = std::process::Command::new(test_ffmpeg_path())
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (be(16), be(20), output.stdout)
}

fn project(path_a: &str, path_b: &str) -> NativeObjectHost {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    // Transparent canvas: the exported alpha is the video's own alpha.
    let wire = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":W,"canvasH":H,"fps":24,"canvasBg":"#00000000"},
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":48,
            "layers":[{"layerUid":"V","visible":true,"timing":timing}],"objects":[],
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
                "documentFormatVersion":2,"contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"V","naturalPosition":0,"order":{}}],"scopes":[]}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"}},
        "viewportReference":null,
        "assets":[
            {"id":"va","name":"take-a","kind":"video","source":{"kind":"linked","locator":path_a}},
            {"id":"vb","name":"take-b","kind":"video","source":{"kind":"linked","locator":path_b}}],
        "assetInventory":{"assetIds":["va","vb"],"placements":[
            {"id":"p1","assetId":"va","target":{"kind":"drawing","contextId":"scene-root","layerUid":"V"},
             "offsetFrames":0}]}});
    NativeObjectHost::from_project_file("t12-media-host".into(), wire.to_string().as_bytes())
        .unwrap()
}

/// One placement edit through the host's real dispatch; it must apply.
fn edit(host: &mut NativeObjectHost, id: &str, payload: Value) {
    let request = json!({"apiVersion":2,"requestId":id,"instanceId":host.instance_id(),
        "documentId":host.document_id(),"expectedRevision":host.content_revision(),
        "operation":"command.document.object.project.placement.edit","payload":payload});
    let reply = serde_json::to_value(
        host.dispatch(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply["result"]["applied"], true, "{id}: {reply}");
}

/// The current pins, as the desktop shell reads them.
struct Pins {
    instance: String,
    document: String,
    snapshot: String,
    revision: u64,
}
impl Pins {
    fn of(host: &NativeObjectHost) -> Self {
        let info = host.project_file_info().unwrap();
        Self {
            instance: host.instance_id().into(),
            document: host.document_id().into(),
            snapshot: info["documentSnapshotId"].as_str().unwrap().into(),
            revision: host.content_revision(),
        }
    }
    fn at(&self, frame: u32) -> ObjectFrameSelector<'_> {
        ObjectFrameSelector {
            instance_id: &self.instance,
            document_id: &self.document,
            content_revision: self.revision,
            document_snapshot_id: &self.snapshot,
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame,
        }
    }
}

/// What the desktop export and preview routes do once their read returns
/// (`reserve_motion` then `seal_object_motion_diagnostics`, which calls this same
/// owner finalizer): close the read's Motion batch. A Motion-free document needs
/// no revision and the commit changes nothing; without this step the next read
/// at a new source is refused, which the shipped route never leaves behind.
fn seal_motion(host: &mut NativeObjectHost, pins: &Pins) {
    let mut frames = host.pending_object_motion_frames();
    frames.sort_unstable();
    frames.dedup();
    if let Some(&first) = frames.first() {
        assert!(!host.object_motion_requires_revision(), "Motion-free reads");
        let commit = host
            .finalize_object_motion_effects(&pins.at(first), &frames)
            .unwrap()
            .expect("a pending Motion-free batch is finalized");
        assert!(!commit.changed(), "a Motion-free seal never edits history");
    }
    assert!(host.pending_object_motion_frames().is_empty());
}

fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", W, H, "srgb", "straight").unwrap()
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-t12-host-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Export `frames` as PNG through the real desktop port and return each
/// published frame's independently decoded RGBA, in request order.
fn export(
    host: &mut NativeObjectHost,
    gpu: &mut Compositor,
    request: &str,
    frames: &[u32],
) -> Vec<Vec<u8>> {
    let scratch = Scratch::new();
    let mut port = DesktopArtifactPort::new(
        scratch.0.join("stage"),
        [("object-output".into(), scratch.0.join("published"))],
    )
    .unwrap();
    let pins = Pins::of(host);
    let outcome = host
        .export_pinned_object_png_frames(
            &pins.at(frames[0]),
            frames,
            request.into(),
            "object-output".into(),
            output(),
            1 << 20,
            &mut port,
            gpu,
        )
        .unwrap_or_else(|error| panic!("{request}: {error}"));
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded, "{request}");
    assert_eq!(
        receipt.external_effect_disposition,
        ExternalEffectDisposition::Committed
    );
    assert_eq!(receipt.document_snapshot_id, pins.snapshot);
    assert!(outcome.release.cleanup_complete);
    seal_motion(host, &pins);
    let files = receipt.artifact.unwrap().files;
    // Frame count from the actual output directory, not only the receipt.
    let published = scratch.0.join("published");
    assert_eq!(fs::read_dir(&published).unwrap().count(), frames.len());
    assert_eq!(files.len(), frames.len());
    files
        .iter()
        .map(|name| {
            let (width, height, rgba) = decode_png(&published.join(name));
            assert_eq!((width, height), (W, H), "{request}/{name}: dimensions");
            rgba
        })
        .collect()
}

// ---- GPU preview readback ---------------------------------------------------

struct Shown;
impl AcquiredSurfaceFrame for Shown {
    fn settle(self) -> SettledSurfaceFrame {
        SettledSurfaceFrame::Presented
    }
}
/// A surface that keeps a full copy of every texture it is asked to present.
struct Capture(Rc<RefCell<Vec<Vec<u8>>>>);
impl SurfacePort for Capture {
    type Frame<'p, 's> = Shown;
    fn acquire<'p, 's>(&'p mut self, source: SurfacePresentation<'s>) -> SurfaceAttempt<Shown> {
        self.0.borrow_mut().push(read_texture(&source));
        SurfaceAttempt::Acquired(Shown)
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {}
}
fn read_texture(source: &SurfacePresentation<'_>) -> Vec<u8> {
    const ROW: u32 = 256; // W * 4 = 128, padded to wgpu's copy alignment
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("t12-preview-readback"),
        size: u64::from(ROW * H),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = source
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: source.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW),
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    source.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |value| tx.send(value).unwrap());
    source
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let mapped = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for row in 0..H {
        let at = (row * ROW) as usize;
        rgba.extend_from_slice(&mapped[at..at + (W * 4) as usize]);
    }
    drop(mapped);
    buffer.unmap();
    rgba
}

/// Present `frame` and return the presented pixels.
fn preview(
    host: &mut NativeObjectHost,
    gpu: &mut Compositor,
    view: &mut DesktopViewportHost<Capture>,
    shown: &Rc<RefCell<Vec<Vec<u8>>>>,
    frame: u32,
) -> Vec<u8> {
    let pins = Pins::of(host);
    let receipt = host
        .present_object_preview(&pins.at(frame), output(), gpu, view)
        .unwrap_or_else(|error| panic!("preview of frame {frame}: {error}"));
    assert_eq!(
        receipt.status(),
        &ViewportStatus::Presented,
        "frame {frame}"
    );
    seal_motion(host, &pins);
    shown.borrow_mut().pop().expect("a presented texture")
}
fn viewport() -> (DesktopViewportHost<Capture>, Rc<RefCell<Vec<Vec<u8>>>>) {
    let shown = Rc::new(RefCell::new(Vec::new()));
    let extent = PhysicalExtent {
        width: W,
        height: H,
    };
    let bounds = CssBounds {
        x: 0.,
        y: 0.,
        width: f64::from(W),
        height: f64::from(H),
    };
    let mapping = ViewportMapping::new(bounds, extent, extent, 1.).unwrap();
    (
        DesktopViewportHost::new(Capture(shown.clone()), mapping),
        shown,
    )
}

fn install_decoder() {
    crate::native_dispatch::video::install_video_decoder(super::boxed_decoder);
}

// ---- acceptance ---------------------------------------------------------------

#[test]
fn two_real_sources_drive_preview_and_export_through_replacement_offset_and_remap() {
    let (path_a, path_b) = (source_a(), source_b());
    let _ffmpeg = ffmpeg_shared(); // after fixture generation: never nested
    install_decoder();
    let mut gpu = Compositor::new().expect("real Metal required");
    let (mut view, shown) = viewport();
    let mut host = project(&path_a, &path_b);

    // 1. Source A, offset 0: timeline frame n draws source frame n.
    let exported = export(&mut host, &mut gpu, "a-offset-0", &[3, 7, 12]);
    for (rgba, n) in exported.iter().zip([3, 7, 12]) {
        assert_frame(rgba, |x, y| a(x, y, n), &format!("export A@{n}"));
    }
    let shown_a = preview(&mut host, &mut gpu, &mut view, &shown, 7);
    assert_frame(&shown_a, |x, y| a(x, y, 7), "preview A@7");
    assert_eq!(shown_a, exported[1], "preview and export agree on A@7");
    // A second preview must not be blocked by the first one's decoded frames.
    let again = preview(&mut host, &mut gpu, &mut view, &shown, 12);
    assert_frame(&again, |x, y| a(x, y, 12), "preview A@12");

    // 2. Replace the source: the same layer now draws B, in preview and export.
    edit(
        &mut host,
        "remove-a",
        json!({"kind":"remove","placementId":"p1"}),
    );
    edit(
        &mut host,
        "place-b",
        json!({"kind":"place","placement":{"id":"p2","assetId":"vb",
            "target":{"kind":"drawing","contextId":"scene-root","layerUid":"V"},"offsetFrames":0}}),
    );
    let exported_b = export(&mut host, &mut gpu, "b-offset-0", &[7]);
    assert_frame(&exported_b[0], |x, y| b(x, y, 7), "export B@7");
    assert_ne!(exported_b[0], exported[1], "replacement changed the export");
    let shown_b = preview(&mut host, &mut gpu, &mut view, &shown, 7);
    assert_frame(&shown_b, |x, y| b(x, y, 7), "preview B@7");
    assert_ne!(shown_b, shown_a, "replacement changed the preview");

    // 3. Offset 5: timeline 2 clamps to source 0, 9 -> 4, 30 clamps to the last (11).
    edit(
        &mut host,
        "offset-b",
        json!({"kind":"setOffset","placementId":"p2","offsetFrames":5}),
    );
    let offset = export(&mut host, &mut gpu, "b-offset-5", &[2, 9, 30]);
    for (rgba, n) in offset.iter().zip([0, 4, 11]) {
        assert_frame(
            rgba,
            |x, y| b(x, y, n),
            &format!("export B offset 5 -> {n}"),
        );
    }
    let shown_offset = preview(&mut host, &mut gpu, &mut view, &shown, 9);
    assert_frame(&shown_offset, |x, y| b(x, y, 4), "preview B offset 5 @9");

    // 4. A reversed time remap (source = 11 - frame, clamped after the last key)
    //    replaces the offset: timeline 2 -> 9, 9 -> 2, 30 -> 0.
    edit(
        &mut host,
        "remap-on",
        json!({"kind":"enableTimeRemap","placementId":"p2","inFrame":0,"outFrame":11,"lastSourceFrame":11}),
    );
    edit(
        &mut host,
        "remap-0",
        json!({"kind":"setTimeRemapKey","placementId":"p2","frame":0,"value":11}),
    );
    edit(
        &mut host,
        "remap-11",
        json!({"kind":"setTimeRemapKey","placementId":"p2","frame":11,"value":0}),
    );
    let remapped = export(&mut host, &mut gpu, "b-remap", &[2, 9, 30]);
    for (rgba, n) in remapped.iter().zip([9, 2, 0]) {
        assert_frame(rgba, |x, y| b(x, y, n), &format!("export B remap -> {n}"));
    }
    let shown_remap = preview(&mut host, &mut gpu, &mut view, &shown, 9);
    assert_frame(&shown_remap, |x, y| b(x, y, 2), "preview B remap @9");

    let release = host.release_project().unwrap();
    assert!(release.application.exports.cleanup_complete);
}

#[test]
fn the_two_sources_differ_in_every_channel_including_alpha_at_every_timestamp() {
    // Guards the acceptance test against fixtures that could not tell A from B
    // or one frame from another: decoded straight from the real sidecar.
    use native_engine::native_media_frame_export::NativeVideoDecoder;
    let (path_a, path_b) = (source_a(), source_b());
    let _ffmpeg = ffmpeg_shared(); // after fixture generation: never nested
    let mut decoder = super::DesktopVideoDecoder::default();
    let ia = decoder.open(&path_a).unwrap();
    let ib = decoder.open(&path_b).unwrap();
    assert_eq!((ia.width, ia.height, ia.frame_count), (W, H, 20));
    assert_eq!((ib.width, ib.height, ib.frame_count), (W, H, 12));
    for n in [0, 4, 7, 11] {
        let fa = decoder.decode(ia.session, n).unwrap();
        let fb = decoder.decode(ib.session, n).unwrap();
        // The sidecar's lossless pipe returns the exact generated texels.
        for y in 0..H {
            for x in 0..W {
                let at = ((y * W + x) * 4) as usize;
                assert_eq!(fa[at..at + 4], a(x, y, n), "A({x},{y},{n})");
                assert_eq!(fb[at..at + 4], b(x, y, n), "B({x},{y},{n})");
            }
        }
        assert!(fa.chunks_exact(4).any(|p| p[3] != 255), "A has real alpha");
        assert!(fb.chunks_exact(4).any(|p| p[3] != 255), "B has real alpha");
    }
    decoder.close(ia.session).unwrap();
    decoder.close(ib.session).unwrap();
}
