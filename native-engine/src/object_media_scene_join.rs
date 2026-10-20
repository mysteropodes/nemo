//! T12 / T28.media: native video placements drawn at their layer's paint slot from
//! the decoded source frames the export or preview gate holds.
//!
//! Two halves, both immutable:
//! - [`slots`] (scene preparation, no I/O): every video record the packet selected
//!   (`object_frame_video.rs`: a `Video` placement on a visible layer present at
//!   the frame, the gate a raster item of that layer passes) becomes one
//!   [`VideoSlot`]: the placement, its asset and the raw layer opacity, no pixels;
//! - [`MediaDraws::prepare`] (after decoding): each slot finds the decoded frame
//!   its placement was given (`media:<asset>:<source frame>`, chosen by the L09-A
//!   kernel through the caller's needs), and is placed exactly like a raster item:
//!   the baseline contain-fit rect of the video on the canvas
//!   (`engine-bridge.js` nativeVideo branch: `s = min(cw / w, ch / h)`, centred),
//!   then the same Motion parent chain and `transformImageRect` order through
//!   [`prepare_raster_scene_image`].
//!
//! A slot without its decoded frame, a frame of another size, an element-bound
//! video placement, a video layer that also carries drawn records (baseline
//! masks) or path effects are typed refusals: never a blank or partial frame.
//! A blend pair (the baseline `blend` interpolation of a remapped video) is
//! mixed exactly like `_blendPixels`, and a `motion` pair by the ported optical
//! flow ([`motion_flow`], byte-identical to geometry-wasm); duplicator and 3D projection of a video
//! layer have no native snapshot field yet and are not invented here.
use crate::evaluation::motion_parent_chain::ImageRect;
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::raster_scene::unavailable;
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
    SceneResourceKind, SceneResourceRetention,
};
use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::{
    prepare_raster_scene_image, RasterPixelPort, RasterPixels, RasterSceneSource,
};
use crate::object_render_scene::{ObjectPaintPlan, ObjectSceneError, ObjectSceneResourceSet};
use std::sync::Arc;
use vello::kurbo::Affine;
use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

/// The decoded frame (or blend pair) a frame gives one video placement:
/// `frame` is its `media:<asset>:<source>` resource; `blend` names the `to`
/// frame and its 1/32-step share (`_blendPixels`, native-video-bridge.js);
/// `motion` mixes the pair by optical flow instead (`_mixed`, `motion` mode).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VideoFrameDraw {
    pub(crate) placement_id: String,
    pub(crate) frame: String,
    pub(crate) blend: Option<(String, f64)>,
    pub(crate) motion: bool,
}

#[path = "object_media_motion_flow.rs"]
pub(crate) mod motion_flow;

/// `_blendPixels`: `(a * (256 - w) + b * w) >> 8` per byte, `w = round(t * 256)`.
fn blend_rgba(a: &[u8], b: &[u8], weight: f64) -> Vec<u8> {
    let w = (weight * 256.0).round().clamp(0.0, 256.0) as u32;
    a.iter()
        .zip(b)
        .map(|(&a, &b)| ((u32::from(a) * (256 - w) + u32::from(b) * w) >> 8) as u8)
        .collect()
}

/// One video placement the frame must draw, at its layer's slot.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VideoSlot {
    pub(crate) layer_index: usize,
    pub(crate) placement_id: String,
    pub(crate) asset_id: String,
    pub(crate) opacity: f64,
}

/// The decoded pixels of one source frame, retained by the media scene resource
/// that declares it. `lease` is the decoder's handle (the export job's frame
/// token): dropping the last holder of this value releases the frame.
pub(crate) struct DecodedVideoFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
    /// Held, never read: the decoder lease ends when this value is dropped.
    #[allow(dead_code)]
    pub(crate) lease: Box<dyn std::any::Any + Send + Sync>,
}

/// The video slots of `packet`, in placement order (the packet's own selection).
pub(crate) fn slots(packet: &ObjectFramePacket) -> Result<Vec<VideoSlot>, ObjectSceneError> {
    let layers = packet.source_layers();
    let mut slots = Vec::with_capacity(packet.video_records().len());
    for video in packet.video_records() {
        let layer_index = video.layer_index();
        if video.element_bound() {
            return Err(unavailable(
                layer_index,
                "element-bound video placement is unavailable",
            ));
        }
        if layers[layer_index]
            .path_fx()
            .is_some_and(|effects| !effects.is_empty())
            || packet
                .records()
                .any(|r| r.source_layer_index() == layer_index)
            || packet
                .raster_records()
                .any(|r| r.layer_index() == layer_index)
        {
            return Err(unavailable(
                layer_index,
                "a video layer with drawn records or path effects is unavailable",
            ));
        }
        let opacity = packet
            .layer_opacity(layer_index)
            .map_err(|_| unavailable(layer_index, "raw layer opacity is unavailable"))?;
        slots.push(VideoSlot {
            layer_index,
            placement_id: video.placement_id().to_owned(),
            asset_id: video.asset_id().to_owned(),
            opacity,
        });
    }
    Ok(slots)
}

/// The baseline contain-fit rect of a `width x height` video on the canvas.
pub(crate) fn contain_rect(canvas: (u32, u32), width: u32, height: u32) -> ImageRect {
    let (cw, ch) = (f64::from(canvas.0), f64::from(canvas.1));
    let (w, h) = (f64::from(width), f64::from(height));
    let scale = (cw / w).min(ch / h);
    let (dw, dh) = (w * scale, h * scale);
    ImageRect {
        x: (cw - dw) / 2.0,
        y: (ch - dh) / 2.0,
        width: dw,
        height: dh,
        rotation: 0.0,
    }
}

/// The drawn image's size under its id (a decoded frame or a blend of two).
struct Pixels<'a>(u32, u32, &'a str);
impl RasterPixelPort for Pixels<'_> {
    type Handle = ();
    fn pixels(&self, id: &str) -> Option<RasterPixels<()>> {
        (id == self.2).then(|| RasterPixels {
            handle: (),
            width: self.0,
            height: self.1,
            version: self.2.to_owned(),
        })
    }
}

/// One video slot ready to draw: its image, placement and opacity.
#[derive(Debug, Clone)]
pub(crate) struct MediaDraw {
    image: ImageData,
    placement: Affine,
    opacity: f32,
}

/// Every video slot of a plan with its decoded frame, in slot order.
#[derive(Debug, Clone)]
pub(crate) struct MediaDraws {
    draws: Vec<MediaDraw>,
    resources: Arc<ObjectSceneResourceSet>,
}

impl MediaDraws {
    /// `frames` gives each placement its decoded frame (or blend pair);
    /// `media` is the set that declares and retains those frames.
    pub(crate) fn prepare(
        plan: &ObjectPaintPlan,
        media: &Arc<ObjectSceneResourceSet>,
        frames: &[VideoFrameDraw],
    ) -> Result<Self, ObjectSceneError> {
        let canvas = plan
            .packet()
            .snapshot()
            .project()
            .map(|project| project.project().canvas_dimensions())
            .ok_or_else(|| unavailable(0, "video placement needs the project canvas"))?;
        let mut draws = Vec::new();
        for slot in plan.video_slots() {
            let missing = || unavailable(slot.layer_index, "decoded video frame is unavailable");
            let wanted = frames
                .iter()
                .find(|draw| draw.placement_id == slot.placement_id)
                .ok_or_else(missing)?;
            let fetch = |id: &str| {
                let entry = media
                    .get(id)
                    .filter(|entry| entry.kind() == SceneResourceKind::Media)
                    .ok_or_else(missing)?;
                let frame = decoded(entry.retention()).ok_or_else(missing)?;
                if u64::try_from(frame.rgba.len()).ok()
                    != u64::from(frame.width).checked_mul(u64::from(frame.height) * 4)
                    || frame.width == 0
                    || frame.height == 0
                {
                    return Err(missing());
                }
                Ok(frame)
            };
            let first = fetch(&wanted.frame)?;
            // A copy, never the leased frame: Vello's atlas keeps every drawn
            // ImageData resident until eviction, which would hold the lease
            // past the witness. The lease ends when the last witness drops.
            let (rgba, id) = match &wanted.blend {
                None => (first.rgba.clone(), wanted.frame.clone()),
                Some((to, weight)) => {
                    let second = fetch(to)?;
                    if (second.width, second.height) != (first.width, first.height) {
                        return Err(missing());
                    }
                    let mode = if wanted.motion { "~" } else { "@" };
                    let id = format!("{}+{to}{mode}{weight}", wanted.frame);
                    let mixed = if wanted.motion {
                        let (w, h) = (first.width as usize, first.height as usize);
                        motion_flow::motion_interpolate(
                            &first.rgba,
                            &second.rgba,
                            w,
                            h,
                            *weight as f32,
                        )
                        .ok_or_else(missing)?
                    } else {
                        blend_rgba(&first.rgba, &second.rgba, *weight)
                    };
                    (mixed, id)
                }
            };
            let (width, height, id) = (first.width, first.height, id.as_str());
            let chain = plan
                .native_parent_chains()
                .chain(slot.layer_index)
                .map_err(|error| unavailable(slot.layer_index, error.reason()))?;
            let source = RasterSceneSource {
                asset_id: id.to_owned(),
                rect: contain_rect(canvas, width, height),
                opacity: slot.opacity,
            };
            let placed = prepare_raster_scene_image(&source, &chain, &Pixels(width, height, id))
                .map_err(|_| unavailable(slot.layer_index, "video placement is unavailable"))?;
            draws.push(MediaDraw {
                image: ImageData {
                    data: Blob::new(Arc::new(rgba)),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width,
                    height,
                },
                placement: placed.placement(),
                opacity: placed.opacity_f32(),
            });
        }
        Ok(Self {
            draws,
            resources: Arc::clone(media),
        })
    }

    /// The image, placement and opacity of video slot `index`.
    pub(crate) fn draw(&self, index: usize) -> Option<(&ImageData, Affine, f32)> {
        let draw = self.draws.get(index)?;
        Some((&draw.image, draw.placement, draw.opacity))
    }

    pub(crate) fn resources(&self) -> &Arc<ObjectSceneResourceSet> {
        &self.resources
    }
}

fn decoded(retention: &SceneResourceRetention) -> Option<Arc<DecodedVideoFrame>> {
    Arc::clone(retention).downcast::<DecodedVideoFrame>().ok()
}

/// Insert one `Video` paint per slot at its layer's place in the numeric layer
/// order of `mixed` (back to front). A video layer holds no other entry (see
/// [`slots`]), so the slot goes after every entry of a lower layer position.
pub(crate) fn insert_slots(
    scene: &crate::object_render_scene::ObjectRenderScene,
    mut mixed: Vec<crate::object_render_scene::PaintIndex>,
) -> Result<
    Vec<crate::object_render_scene::PaintIndex>,
    crate::object_render_scene::ObjectPaintPlanError,
> {
    use crate::object_render_scene::{ObjectPaintPlanError, PaintIndex};
    if scene.videos.is_empty() {
        return Ok(mixed);
    }
    let order = scene
        .packet()
        .numeric_order()
        .ok_or(ObjectPaintPlanError::Unavailable)?;
    let position = |layer: usize| {
        order
            .layers()
            .iter()
            .position(|entry| entry.source_layer_index() == layer)
            .ok_or(ObjectPaintPlanError::InvalidOrder)
    };
    let layer_of = |index: &PaintIndex| -> Result<usize, ObjectPaintPlanError> {
        Ok(match index {
            PaintIndex::Contour(i) => scene
                .entry(*i)
                .ok_or(ObjectPaintPlanError::InvalidOrder)?
                .source()
                .source_layer_index(),
            PaintIndex::Raster(i) => scene.rasters[*i].layer_index,
            PaintIndex::Text(i) => scene.texts.entries[*i].layer_index,
            PaintIndex::Component(i) => scene.components.paints[*i].host_layer_index,
            PaintIndex::Video(i) => scene.videos[*i].layer_index,
            PaintIndex::Duplicator(i) => scene.duplicators.paints[*i].layer_index,
        })
    };
    for (slot, video) in scene.videos.iter().enumerate() {
        let at = position(video.layer_index)?;
        let mut insert = mixed.len();
        for (index, entry) in mixed.iter().enumerate() {
            if position(layer_of(entry)?)? > at {
                insert = index;
                break;
            }
        }
        mixed.insert(insert, PaintIndex::Video(slot));
    }
    Ok(mixed)
}

impl crate::object_render_scene::ObjectCompositionWitness {
    /// Attach the decoded frames of every video slot. `media` must be the set the
    /// gate decoded for this frame; it is merged with the witness's own resources
    /// so the scheduler key declares (and leases) both.
    pub(crate) fn with_media_frames(
        mut self,
        media: &Arc<ObjectSceneResourceSet>,
        frames: &[VideoFrameDraw],
    ) -> Result<Self, ObjectSceneError> {
        if self.plan().video_slots().is_empty() {
            return Ok(self);
        }
        let draws = MediaDraws::prepare(self.plan(), media, frames)?;
        let merged = match &self.resources {
            None => Arc::clone(draws.resources()),
            Some(own) => Arc::new(
                ObjectSceneResourceSet::new(
                    own.entries()
                        .iter()
                        .chain(media.entries())
                        .map(|entry| {
                            crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::SceneResourceEntry::new(
                                entry.resource_id(),
                                entry.version(),
                                entry.kind(),
                                entry.charged_bytes(),
                                Arc::clone(entry.retention()),
                            )
                        })
                        .collect(),
                )
                .map_err(|_| unavailable(0, "video and scene resources cannot be declared together"))?,
            ),
        };
        self.resources = Some(merged);
        self.media = Some(Arc::new(draws));
        Ok(self)
    }

    /// The image, placement and opacity of video slot `index`, once decoded.
    pub(crate) fn video_image(&self, index: usize) -> Option<(&ImageData, Affine, f32)> {
        self.media.as_ref()?.draw(index)
    }
}
