//! Native SVG frame serializer over the pinned `ObjectPaintPlan` (G43 kernel).
//!
//! One `<path>` per retained contour in the SAME mixed paint order the PNG
//! compositor consumes, including Text glyphs and decorations. Path data is the
//! retained closed cubic contour (absolute `M`, every closing `C`, then `Z`).
//! Vector straight RGBA stays unquantized; Text uses the provider's already
//! composed straight RGBA8, without applying its opacity a second time. The document
//! wrapper is byte-identical to `svgDocument` in baseline
//! `src/js/adapters/export-svg-frame.js`. Plan geometry/paint outside what the
//! plan can carry today is a typed `Unsupported`, never dropped, never Paper.
//!
//! Output is detached (owned bytes and pin strings). The byte cap is checked by
//! an exact counting pass BEFORE the output buffer is allocated.
use crate::object_render_scene::{
    ordered_entries, ObjectPaintCommand, ObjectPaintPlan, PaintEntry,
};
use crate::render_geometry::ClosedCubicPath;
use crate::scheduler::OutputSpec;

pub(crate) const XML_PREAMBLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
/// Same ceiling as the PNG compositor's byte budget (`object_compositor.rs` MAX_BYTES).
pub(crate) const MAX_SVG_BYTES: usize = 64 * 1024 * 1024;
const DOCUMENT_TAIL: &str = "\n</svg>";
const FAMILY: &str = "closed-solid-fill-cubic-path";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SvgExportError {
    /// Pinned output width or height is zero.
    InvalidSize,
    /// Byte cap is zero or above `MAX_SVG_BYTES`.
    InvalidByteCap,
    /// A record the plan can carry but SVG has no representation for yet.
    Unsupported {
        source_object_index: usize,
        reason: &'static str,
    },
    /// A placed component has layer/inner-target provenance, not a root object index.
    UnsupportedComponent {
        source_layer_index: usize,
        reason: &'static str,
    },
    /// A straight RGBA channel is not a finite number in 0..=1.
    UnrepresentablePaint { source_object_index: usize },
    /// The exact serialized size exceeds the caller's cap.
    OutputTooLarge { limit: usize },
}

/// Pinned output size (from the job's `OutputSpec`) and the caller's byte cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SvgFrameSpec {
    width: u32,
    height: u32,
    max_bytes: usize,
}

impl SvgFrameSpec {
    pub(crate) fn new(width: u32, height: u32, max_bytes: usize) -> Result<Self, SvgExportError> {
        if width == 0 || height == 0 {
            return Err(SvgExportError::InvalidSize);
        }
        if max_bytes == 0 || max_bytes > MAX_SVG_BYTES {
            return Err(SvgExportError::InvalidByteCap);
        }
        Ok(Self {
            width,
            height,
            max_bytes,
        })
    }

    /// Only the pinned dimensions are read; kind/format tokens belong to the job.
    pub(crate) fn from_output(
        output: &OutputSpec,
        max_bytes: usize,
    ) -> Result<Self, SvgExportError> {
        let (width, height) = output.dimensions();
        Self::new(width, height, max_bytes)
    }

    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Identity copied out of the plan's packet; owned, so detached from the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SvgFramePins {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) document_snapshot_id: String,
    pub(crate) context_id: String,
    pub(crate) frame: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SvgFrame {
    pub(crate) bytes: Vec<u8>,
    pub(crate) pins: SvgFramePins,
    pub(crate) dimensions: (u32, u32),
    pub(crate) path_count: usize,
}

/// Mirror of baseline `svgDocument(inner, width, height)`.
pub(crate) fn svg_document(inner: &str, width: u32, height: u32) -> String {
    format!("{}{inner}{DOCUMENT_TAIL}", document_head(width, height))
}

fn document_head(width: u32, height: u32) -> String {
    format!(
        "{XML_PREAMBLE}<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" \
         height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n"
    )
}

/// Shortest round-trip decimal, never an exponent; -0 is `0` like JS `String(-0)`.
/// Differs from JS only where JS uses an exponent (|x| < 1e-6 or >= 1e21).
pub(crate) fn format_number(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value}")
    }
}

/// A channel in 0..=1 as a percentage: the shortest decimal digits with the
/// point moved two places right (exact on the digits, no float multiply).
pub(crate) fn channel_percent(value: f64) -> String {
    let text = format_number(value);
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut digits = fraction.to_owned();
    while digits.len() < 2 {
        digits.push('0');
    }
    let (moved, rest) = digits.split_at(2);
    let joined = format!("{whole}{moved}");
    let int = match joined.trim_start_matches('0') {
        "" => "0",
        trimmed => trimmed,
    };
    let rest = rest.trim_end_matches('0');
    if rest.is_empty() {
        int.to_owned()
    } else {
        format!("{int}.{rest}")
    }
}

/// What a record must be for SVG to represent it exactly.
pub(crate) struct RecordFacts<'a> {
    pub(crate) family: &'a str,
    pub(crate) geometry_kind: &'a str,
    pub(crate) closed: bool,
    pub(crate) coordinate_space: &'a str,
    pub(crate) handle_space: &'a str,
    pub(crate) fill_kind: &'a str,
}

pub(crate) fn admit_record(
    source_object_index: usize,
    facts: &RecordFacts<'_>,
) -> Result<(), SvgExportError> {
    let unsupported = |reason| SvgExportError::Unsupported {
        source_object_index,
        reason,
    };
    if facts.family != FAMILY {
        return Err(unsupported(
            "record family has no SVG representation (strokes, gradients, images, effects, 3D)",
        ));
    }
    if facts.geometry_kind != "cubic-path"
        || !facts.closed
        || facts.coordinate_space != "document-world"
        || facts.handle_space != "relative"
    {
        return Err(unsupported(
            "geometry is not a closed relative cubic path in document-world space",
        ));
    }
    if facts.fill_kind != "solid" {
        return Err(unsupported("paint is not a solid fill"));
    }
    Ok(())
}

/// A finite channel in 0..=1, the compositor's `color()` admission rule.
pub(crate) fn unit_interval(value: Option<f64>) -> Option<f64> {
    value.filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
}

/// Straight RGBA, validated before any byte is written.
fn straight_rgba(
    source_object_index: usize,
    command: ObjectPaintCommand<'_>,
) -> Result<[f64; 4], SvgExportError> {
    let mut rgba = [0.0; 4];
    for (slot, number) in rgba.iter_mut().zip(command.paint()) {
        *slot = unit_interval(number.as_f64()).ok_or(SvgExportError::UnrepresentablePaint {
            source_object_index,
        })?;
    }
    Ok(rgba)
}

/// Exact byte counter (cap-checked); writes too when it owns a buffer.
struct Sink {
    cap: usize,
    len: usize,
    out: Option<Vec<u8>>,
}

impl Sink {
    fn put(&mut self, text: &str) -> Result<(), SvgExportError> {
        self.len = self
            .len
            .checked_add(text.len())
            .filter(|len| *len <= self.cap)
            .ok_or(SvgExportError::OutputTooLarge { limit: self.cap })?;
        if let Some(out) = self.out.as_mut() {
            out.extend_from_slice(text.as_bytes());
        }
        Ok(())
    }
    fn number(&mut self, value: f64) -> Result<(), SvgExportError> {
        self.put(&format_number(value))
    }
    fn point(&mut self, point: [f64; 2]) -> Result<(), SvgExportError> {
        self.number(point[0])?;
        self.put(",")?;
        self.number(point[1])
    }
}

fn emit_command(
    sink: &mut Sink,
    source_object_index: usize,
    command: ObjectPaintCommand<'_>,
) -> Result<(), SvgExportError> {
    let record = command.source().record();
    if record.has_enabled_effects() {
        return Err(SvgExportError::Unsupported {
            source_object_index,
            reason: "per-element effects have no admitted SVG representation",
        });
    }
    admit_record(
        source_object_index,
        &RecordFacts {
            family: &record.family,
            geometry_kind: &record.geometry.kind,
            closed: record.geometry.closed,
            coordinate_space: &record.geometry.coordinate_space,
            handle_space: &record.geometry.handle_space,
            fill_kind: record.solid_fill().map_or("", |fill| fill.kind.as_str()),
        },
    )?;
    let rgba = straight_rgba(source_object_index, command)?;
    emit_contour(sink, source_object_index, command.contour(), rgba)
}

fn emit_contour(
    sink: &mut Sink,
    source_object_index: usize,
    contour: &ClosedCubicPath,
    rgba: [f64; 4],
) -> Result<(), SvgExportError> {
    emit_contour_with_error(
        sink,
        contour,
        rgba,
        SvgExportError::Unsupported {
            source_object_index,
            reason: "empty contour",
        },
    )
}

fn emit_contour_with_error(
    sink: &mut Sink,
    contour: &ClosedCubicPath,
    rgba: [f64; 4],
    invalid: SvgExportError,
) -> Result<(), SvgExportError> {
    let segments = contour.segments();
    let first = segments.first().ok_or(invalid)?;
    sink.put("<path d=\"M")?;
    sink.point(first.point)?;
    for (index, from) in segments.iter().enumerate() {
        let to = &segments[(index + 1) % segments.len()];
        let out = [
            from.point[0] + from.handle_out[0],
            from.point[1] + from.handle_out[1],
        ];
        let into = [to.point[0] + to.handle_in[0], to.point[1] + to.handle_in[1]];
        sink.put(" C")?;
        sink.point(out)?;
        sink.put(" ")?;
        sink.point(into)?;
        sink.put(" ")?;
        sink.point(to.point)?;
    }
    sink.put(" Z\" fill=\"rgb(")?;
    for (index, channel) in rgba[..3].iter().enumerate() {
        if index > 0 {
            sink.put(",")?;
        }
        sink.put(&channel_percent(*channel))?;
        sink.put("%")?;
    }
    sink.put(")\" fill-opacity=\"")?;
    sink.number(rgba[3])?;
    sink.put("\" fill-rule=\"nonzero\"/>")
}

fn unsupported_raster(plan: &ObjectPaintPlan, index: usize) -> SvgExportError {
    // The sealed mixed plan validates every index when it joins its producers.
    SvgExportError::Unsupported {
        source_object_index: plan.raster_sources()[index].source_index,
        reason: "retained raster paint has no SVG representation",
    }
}

/// T12: a decoded video frame is a bitmap; SVG export refuses it like a raster.
fn unsupported_video(plan: &ObjectPaintPlan, index: usize) -> SvgExportError {
    SvgExportError::UnsupportedComponent {
        source_layer_index: plan.video_slots()[index].layer_index,
        reason: "video paint has no SVG representation",
    }
}

fn emit_frame(
    plan: &ObjectPaintPlan,
    spec: &SvgFrameSpec,
    sink: &mut Sink,
) -> Result<usize, SvgExportError> {
    // Unsupported mixed content must refuse the whole frame before even the
    // document header is counted, never disappear behind a successful subset.
    if let Some(index) = ordered_entries(plan).find_map(|entry| match entry {
        PaintEntry::Raster(index) => Some(index),
        _ => None,
    }) {
        return Err(unsupported_raster(plan, index));
    }
    // Refuse an unsupported component paint before counting/writing ANY header,
    // matching the existing all-or-nothing mixed raster policy.
    for entry in ordered_entries(plan) {
        if let PaintEntry::Video(index) = entry {
            return Err(unsupported_video(plan, index));
        }
        if let PaintEntry::Duplicator(copy) = entry {
            if copy.svg_contour.is_none() || copy.svg_rgba.is_none() {
                return Err(SvgExportError::Unsupported {
                    source_object_index: copy.source_index,
                    reason: "duplicator style has no admitted SVG representation",
                });
            }
        }
        if let PaintEntry::Component(component) = entry {
            if component.contour.is_none() || component.svg_rgba.is_none() {
                return Err(SvgExportError::UnsupportedComponent {
                    source_layer_index: component.host_layer_index,
                    reason: "component style has no admitted SVG representation",
                });
            }
        }
    }
    sink.put(&document_head(spec.width, spec.height))?;
    let mut count = 0;
    for entry in ordered_entries(plan) {
        match entry {
            PaintEntry::Contour(command) => {
                if count > 0 {
                    sink.put("\n")?;
                }
                emit_command(sink, command.source().source_object_index(), command)?;
                count += 1;
            }
            PaintEntry::Text(text) => {
                for (contour, rgba8) in text.item.painted() {
                    if count > 0 {
                        sink.put("\n")?;
                    }
                    // The shared provider already applies glyph/layer opacity
                    // and rounds the straight color to RGBA8 for native paint.
                    let rgba = rgba8.map(|channel| f64::from(channel) / 255.0);
                    emit_contour(sink, text.source_index, &contour.contour, rgba)?;
                    count += 1;
                }
            }
            PaintEntry::Component(component) => {
                let refusal = SvgExportError::UnsupportedComponent {
                    source_layer_index: component.host_layer_index,
                    reason: "component style has no admitted SVG representation",
                };
                let contour = component.contour.as_ref().ok_or(refusal)?;
                let rgba = component.svg_rgba.ok_or(refusal)?;
                if count > 0 {
                    sink.put("\n")?;
                }
                emit_contour_with_error(sink, contour, rgba, refusal)?;
                count += 1;
            }
            PaintEntry::Duplicator(copy) => {
                let refusal = SvgExportError::Unsupported {
                    source_object_index: copy.source_index,
                    reason: "duplicator style has no admitted SVG representation",
                };
                let contour = copy.svg_contour.as_ref().ok_or(refusal)?;
                let rgba = copy.svg_rgba.ok_or(refusal)?;
                if count > 0 {
                    sink.put("\n")?;
                }
                emit_contour_with_error(sink, contour, rgba, refusal)?;
                count += 1;
            }
            PaintEntry::Raster(index) => return Err(unsupported_raster(plan, index)),
            PaintEntry::Video(index) => return Err(unsupported_video(plan, index)),
        }
    }
    sink.put(DOCUMENT_TAIL)?;
    Ok(count)
}

/// Exact serialized size, or the first refusal, without allocating output.
pub(crate) fn measure_svg_frame(
    plan: &ObjectPaintPlan,
    spec: SvgFrameSpec,
) -> Result<usize, SvgExportError> {
    let mut sink = Sink {
        cap: spec.max_bytes,
        len: 0,
        out: None,
    };
    emit_frame(plan, &spec, &mut sink)?;
    Ok(sink.len)
}

pub(crate) fn serialize_svg_frame(
    plan: &ObjectPaintPlan,
    spec: SvgFrameSpec,
) -> Result<SvgFrame, SvgExportError> {
    let exact = measure_svg_frame(plan, spec)?;
    let mut sink = Sink {
        cap: spec.max_bytes,
        len: 0,
        out: Some(Vec::with_capacity(exact)),
    };
    let path_count = emit_frame(plan, &spec, &mut sink)?;
    debug_assert_eq!(sink.len, exact);
    let packet = plan.packet();
    Ok(SvgFrame {
        bytes: sink.out.unwrap_or_default(),
        pins: SvgFramePins {
            instance_id: packet.instance_id().to_owned(),
            document_id: packet.document_id().to_owned(),
            content_revision: packet.content_revision(),
            document_snapshot_id: packet.document_snapshot_id().to_owned(),
            context_id: packet.context_id().to_owned(),
            frame: packet.frame(),
        },
        dimensions: spec.dimensions(),
        path_count,
    })
}

#[cfg(test)]
#[path = "../tests/native_vector_export_svg_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/native_vector_export_svg_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/native_vector_export_svg_oracle.rs"]
mod oracle;
#[cfg(test)]
#[path = "../tests/native_vector_export_svg_reader.rs"]
pub(crate) mod reader;
#[cfg(test)]
#[path = "../tests/native_vector_export_svg.rs"]
mod tests;
