//! Concrete native trim/stamp baker. Only a complete charged producer result
//! escapes: the private baker's scratch charge outlives Slot::resolve's copy.
use crate::evaluation::object_scene_style_kernels::{
    bitmap_brush_resources_stamp::{plan_stamps, StampPath, StampSpec},
    bitmap_brush_resources_store::{BrushBitmapStore, RetirePort},
    object_scene_producer_brush::{self as brush, BrushContext, BrushProduceError, BrushProduction, BrushStrokeSource},
    object_scene_producer_brush_slot::{BakeRefused, BakedInk, TrimAnchor, TrimProvider, TrimSlots, TrimWindow, TrimmedPath},
};
use crate::evaluation::property_follow_path_sample::{path_curves, SegmentIn};
use crate::object_document::BitmapBrushFields;
use crate::object_frame_packet::ObjectFramePacket;
use crate::revision::{ImageResourceBudget, ImageResourceCharge};

#[path = "object_brush_tip_mask.rs"]
mod tip_mask;
use tip_mask::{over, sample_tip, tip_pixels, tip_shape, TIP};
const MAX_PIXEL_VISITS: u64 = 1 << 28;

/// Consume a complete native bake while its raw pixels and scratch charges
/// are still alive. The callback owns charging/bounding anything it retains;
/// this port returns no raw bake, store, renderer or publication authority.
pub(crate) fn consume_full_native<T>(
    fields: &BitmapBrushFields,
    anchor: &TrimAnchor,
    budget: &ImageResourceBudget,
    consume: impl FnOnce(Option<&BakedInk>) -> Result<T, BakeRefused>,
) -> Result<T, BakeRefused> {
    use crate::object_document::bitmap_brush::BitmapBrushValueError;
    crate::object_document::bitmap_brush::validate_fields(fields).map_err(|error| match error {
        BitmapBrushValueError::Invalid(message) | BitmapBrushValueError::Unsupported(message) => BakeRefused(message),
    })?;
    let mut baker = NativeBaker {
        fields,
        budget: budget.clone(),
        scratch: Vec::new(),
        retirement: None,
    };
    let path = original(anchor);
    // Declaration order is intentional: baked pixels drop before the baker's
    // charges on success, refusal or callback unwind.
    let baked = baker.bake(&path)?;
    consume(baked.as_ref())
}

pub(crate) fn produce_native(
    packet: &ObjectFramePacket,
    source: &BrushStrokeSource,
    fields: &BitmapBrushFields,
    store: &mut BrushBitmapStore,
    slots: &mut TrimSlots,
    retire: RetirePort<'_>,
) -> Result<BrushProduction, BrushProduceError> {
    produce_native_guarded(packet, source, fields, store, slots, retire, false)
}
pub(crate) fn produce_native_guarded(
    packet: &ObjectFramePacket,
    source: &BrushStrokeSource,
    fields: &BitmapBrushFields,
    store: &mut BrushBitmapStore,
    slots: &mut TrimSlots,
    retire: RetirePort<'_>,
    guarded: bool,
) -> Result<BrushProduction, BrushProduceError> {
    if crate::object_document::bitmap_brush::validate_fields(fields).is_err() {
        return Err(BrushProduceError::InvalidSource { index: 0 });
    }
    let port = std::rc::Rc::new(std::cell::RefCell::new(retire));
    let bake_port = std::rc::Rc::clone(&port);
    let anchor = crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::anchor_key(
        packet.instance_id(), packet.context_id(), &source.layer_uid, &source.stroke_id);
    let id = crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::slot_brush_id(&anchor);
    let retirement = guarded.then(|| Box::new(move ||
        bake_port.borrow_mut().as_mut().is_some_and(|p| p(std::slice::from_ref(&id)))) as Box<dyn FnMut() -> bool + '_>);
    let mut baker = NativeBaker { fields, budget: store.budget().clone(), scratch: Vec::new(), retirement };
    let has_port = port.borrow().is_some();
    let mut forward = |ids: &[String]| port.borrow_mut().as_mut().is_some_and(|p| p(ids));
    let retire: RetirePort<'_> = if has_port { Some(&mut forward) } else { None };
    // Raw BakedInk never crosses this boundary. The existing producer and slot
    // consume it and drop it before baker (and its scratch charges) can drop.
    let spec = &fields.bitmap_brush_spec;
    let mut bake_bits = vec![spec.tip.len() as u64];
    bake_bits.extend(spec.tip.bytes().map(u64::from));
    bake_bits.push(spec.color.len() as u64);
    bake_bits.extend(spec.color.bytes().map(u64::from));
    brush::produce_with_bake_bits(packet, std::slice::from_ref(source), &mut BrushContext {
        store, slots, provider: &mut baker, retire,
    }, &bake_bits)
}
struct NativeBaker<'a, 'r> {
    fields: &'a BitmapBrushFields,
    budget: ImageResourceBudget,
    scratch: Vec<ImageResourceCharge>,
    retirement: Option<Box<dyn FnMut() -> bool + 'r>>,
}
fn segment(point: [f64; 2]) -> SegmentIn {
    SegmentIn { point, handle_in: Some([0.; 2]), handle_out: Some([0.; 2]) }
}
fn original(anchor: &TrimAnchor) -> TrimmedPath {
    TrimmedPath { segments: anchor.segments.clone(), closed: anchor.closed }
}
fn invalid_path() -> TrimmedPath {
    TrimmedPath { segments: vec![segment([f64::NAN; 2]); 2], closed: false }
}
/// Verbatim original twenty-samples-per-curve polyline convention. Open paths
/// clip, closed paths wrap; a full window preserves the original cubic handles.
fn trim(anchor: &TrimAnchor, window: &TrimWindow) -> TrimmedPath {
    if anchor.segments.len() < 2 || window.start <= 0.001 && window.end >= 99.999 {
        return original(anchor);
    }
    let (start, end) = (window.start + window.offset, window.end + window.offset);
    let empty = || TrimmedPath { segments: Vec::new(), closed: false };
    if end <= start { return empty(); }
    let mut points = vec![anchor.segments[0].point];
    for curve in path_curves(&anchor.segments, anchor.closed) {
        for sample in 1..=20 {
            let t = sample as f64 / 20.;
            let u = 1. - t;
            points.push([
                u*u*u*curve[0]+3.*u*u*t*curve[2]+3.*u*t*t*curve[4]+t*t*t*curve[6],
                u*u*u*curve[1]+3.*u*u*t*curve[3]+3.*u*t*t*curve[5]+t*t*t*curve[7],
            ]);
        }
    }
    let mut lengths = vec![0.];
    for pair in points.windows(2) {
        let dx = pair[1][0] - pair[0][0];
        let dy = pair[1][1] - pair[0][1];
        lengths.push(lengths.last().unwrap() + (dx*dx+dy*dy).sqrt());
    }
    let total = *lengths.last().unwrap();
    if !total.is_finite() { return invalid_path(); }
    if total <= 0. { return original(anchor); }
    let length_at = |pct: f64| {
        let p = if anchor.closed { ((pct % 100.) + 100.) % 100. } else { pct.clamp(0.,100.) };
        p / 100. * total
    };
    let from = length_at(start);
    let to = length_at(if anchor.closed { start + (end-start).min(100.) } else { end });
    let span = if anchor.closed { total.min((end-start)/100.*total) } else { to-from };
    if !span.is_finite() || !from.is_finite() { return invalid_path(); }
    if span <= 0. { return empty(); }
    let point_at = |length: f64| {
        let length = if anchor.closed { ((length % total)+total)%total } else { length.clamp(0.,total) };
        for i in 1..lengths.len() {
            if lengths[i] >= length {
                let size = lengths[i]-lengths[i-1];
                let t = if size > 0. { (length-lengths[i-1])/size } else { 0. };
                return [points[i-1][0]+(points[i][0]-points[i-1][0])*t,
                    points[i-1][1]+(points[i][1]-points[i-1][1])*t];
            }
        }
        *points.last().unwrap()
    };
    let mut out = vec![segment(point_at(from))];
    let step = total/(points.len()-1) as f64;
    let mut distance = step;
    while distance < span {
        if out.len() > 65536 { return invalid_path(); }
        out.push(segment(point_at(from+distance)));
        distance += step;
    }
    out.push(segment(point_at(from+span)));
    TrimmedPath { segments: out, closed: false }
}
impl TrimProvider for NativeBaker<'_, '_> {
    fn trimmed(&mut self,anchor:&TrimAnchor,window:&TrimWindow)->Option<TrimmedPath> { Some(trim(anchor,window)) }
    fn bake(&mut self,path:&TrimmedPath)->Result<Option<BakedInk>,BakeRefused> {
        // Slot cache hits never call bake. A miss retires actual GPU mirrors
        // before any replacement pixel/mask allocation; refusal preserves old ink.
        if self.retirement.as_mut().is_some_and(|retire| !retire()) {
            return Err(BakeRefused("native brush atlas retirement unavailable"));
        }
        let spec=&self.fields.bitmap_brush_spec;
        let stamp=StampSpec {size:spec.size.as_f64().unwrap(),spacing:spec.spacing.as_f64().unwrap(),
            scatter:spec.scatter.as_f64().unwrap(),opacity:spec.opacity.as_f64().unwrap(),
            seed:spec.seed.as_f64().unwrap(),pressure:spec.pressure};
        let profile=self.fields.bitmap_pressure_profile.value().map(|p|p.iter().map(|a|
            [a[0].as_f64().unwrap(),a[1].as_f64().unwrap(),a[2].as_f64().unwrap()]).collect::<Vec<_>>());
        let plan=plan_stamps(&stamp,&StampPath {segments:&path.segments,closed:path.closed,pressure_profile:profile.as_deref()})
            .map_err(|_|BakeRefused("native brush stamp plan unavailable"))?;
        let cost=plan.byte_cost().ok_or(BakeRefused("native brush pixel size overflow"))?;
        let shape=tip_shape(&spec.tip,stamp.seed)?;
        let _mask_charge=self.budget.reserve(std::mem::size_of::<[f64;TIP*TIP]>())
            .map_err(|_|BakeRefused("native brush tip budget unavailable"))?;
        let pixels=tip_pixels(&shape);
        let mut charge=self.budget.reserve(cost).map_err(|_|BakeRefused("native brush bake budget unavailable"))?;
        let mut rgba=charge.allocate().map_err(|_|BakeRefused("native brush bake allocation unavailable"))?;
        let (width,height)=(plan.texture.pixel_width,plan.texture.pixel_height);
        let mut visits=0_u64;
        for dab in &plan.stamps {
            let size=dab.ps.abs();if size==0. {continue;}
            let (left,top)=(dab.px-size/2.,dab.py-size/2.);
            let x0=left.floor().max(0.).min(f64::from(width)) as u32;
            let y0=top.floor().max(0.).min(f64::from(height)) as u32;
            let x1=(left+size).ceil().max(0.).min(f64::from(width)) as u32;
            let y1=(top+size).ceil().max(0.).min(f64::from(height)) as u32;
            visits=visits.saturating_add(u64::from(x1-x0)*u64::from(y1-y0));
            if visits>MAX_PIXEL_VISITS {return Err(BakeRefused("native brush pixel work budget unavailable"));}
            for y in y0..y1 {for x in x0..x1 {
                let alpha=sample_tip(&pixels,(f64::from(x)+0.5-left)/size*64.,(f64::from(y)+0.5-top)/size*64.)*plan.alpha;
                let at=(y as usize*width as usize+x as usize)*4+3;
                rgba[at]=(over(f64::from(rgba[at])/255.,alpha)*255.).round() as u8;
            }}
        }
        let color=crate::object_document::bitmap_brush::rgba(&spec.color)
            .ok_or(BakeRefused("native brush tint unavailable"))?;
        for pixel in rgba.chunks_exact_mut(4) {
            let alpha=(f64::from(pixel[3])*f64::from(color[3])/255.).round() as u8;
            pixel.copy_from_slice(if alpha==0 {&[0,0,0,0]} else {&[color[0],color[1],color[2],alpha]});
        }
        self.scratch.push(charge);
        Ok(Some(BakedInk {width,height,rgba,rect:plan.texture.rect}))
    }
}
#[cfg(test)]
#[path = "../tests/object_brush_trim_bake.rs"]
mod tests;
