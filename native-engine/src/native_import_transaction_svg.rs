//! SVG plan -> batch records (L10-H, G35 + G36 vector frames): each exploded path of an L10-D
//! `SvgImportPlan` becomes one `editable-vector-path` record (the native form of what the baseline
//! `serP` persists for the inserted Paper path), and everything the plan reports but cannot place
//! (skipped elements, unclaimed holes) becomes a typed skip entry. Nothing is dropped silently:
//! `records + skipped` always covers every path, every skipped node and every unclaimed hole.
//!
//! Baseline `serP` facts kept (oracle `pair` rows):
//! - segment coordinates and handles go through `_r3`: `Math.round(v * 1000) / 1000`, `0` for a
//!   non-finite value, and a negative zero persists as `0` (`JSON.stringify(-0)`);
//! - `hasRealStroke` is whether the path has a stroke colour; a strokeless path persists the
//!   `#ffffff` fallback with `hasRealStroke: false`, which natively is an explicit `null` stroke;
//! - cap, join, `miterLimit`, `dashOffset`, `dashArray` (only when non-empty), `strokeWidth` and
//!   `opacity` are copied as Paper holds them, so a zero width or zero opacity stays zero;
//! - numbers are spelled as `JSON.stringify` spells them: an integral value is an integer.
//! The kernel's `LeafStyle` carries only fill, stroke, width and opacity, so cap, join, miter limit
//! and dashes come from the adapter as [`StrokeExtras`] (one per exploded path, in plan order).
//! Owner tags (`ownerId`...) are a team-review feature and have no native record.

use crate::history::object_history::object_shot_kernels::native_import_svg_explode::plan::SvgImportPlan;
use crate::history::object_history::object_shot_kernels::native_import_svg_explode::{
    ExplodedPath, Seg,
};
use crate::history::object_history::object_shot_kernels::native_import_transaction::{
    ImportBatch, ImportRecord, ImportSkip, SkipKind,
};
use crate::history::object_history::object_shot_kernels::object_shot_chain::modules::js_round;
use crate::object_document::{
    FrameScope, FrameScopeKind, ObjectGeometry, ObjectPoint, ObjectRecord, ObjectSegment,
    ObjectTarget, Presence, SolidFill, VectorBody, VECTOR_FAMILY,
};
use serde_json::Number;

/// Anchor bounds of the vector family (`object_vector_codec.rs`, private there). A test pins this
/// copy against the real storage admission so a drift fails.
pub(crate) const MIN_SEGMENTS: usize = 2;
pub(crate) const MAX_SEGMENTS: usize = 256;

/// The stroke members the kernel's `LeafStyle` does not carry, read from the Paper path.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StrokeExtras {
    pub(crate) cap: String,
    pub(crate) join: String,
    pub(crate) miter_limit: Option<f64>,
    pub(crate) dash_array: Vec<f64>,
    pub(crate) dash_offset: Option<f64>,
}

impl StrokeExtras {
    /// What a Paper `Path` holds when no style attribute set them (oracle: `paper-default-line`).
    pub(crate) fn paper_default() -> Self {
        Self {
            cap: "butt".into(),
            join: "miter".into(),
            miter_limit: Some(10.0),
            dash_array: Vec::new(),
            dash_offset: Some(0.0),
        }
    }
}

/// Why one path cannot become a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecordError {
    /// The contour has fewer than 2 or more than 256 anchors.
    SegmentCount(usize),
    /// The paint token is not the `#rrggbb` / `#rrggbbaa` the baseline persists (gradient, pattern...).
    UnsupportedPaint { field: &'static str, token: String },
    /// A width, opacity, miter limit or dash value is NaN or infinite.
    NonFinite(&'static str),
}

/// Why a plan cannot become a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SvgBatchError {
    /// `extras` must hold exactly one entry per exploded path.
    ExtrasMismatch { paths: usize, extras: usize },
    /// The leaf reports do not add up to the plan's paths.
    LeafReportMismatch,
    /// Path `index` (plan order) cannot become a record.
    Record { index: usize, error: RecordError },
}

/// Where the records land: the layer reference (a batch layer's source id or an existing uid), the
/// context, the frame and the prefix of the source ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SvgPlacement {
    pub(crate) layer_ref: String,
    pub(crate) context_id: String,
    pub(crate) frame: u32,
    pub(crate) id_prefix: String,
}

/// `JSON.stringify` spelling of a finite double: `-0` and `0` are `0`, an integral value below
/// 2^53 is an integer, everything else keeps its shortest f64 form. `None` for NaN and infinity.
pub(crate) fn js_number(value: f64) -> Option<Number> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(Number::from(0u8));
    }
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return Some(Number::from(value as i64));
    }
    Number::from_f64(value)
}

/// `_r3`: `isFinite(v) ? Math.round(v * 1000) / 1000 : 0` (the rounding never yields `-0`).
pub(crate) fn r3(value: f64) -> f64 {
    if value.is_finite() {
        js_round(value * 1000.0) / 1000.0
    } else {
        0.0
    }
}

fn coordinate(value: f64) -> Result<Number, RecordError> {
    js_number(r3(value)).ok_or(RecordError::NonFinite("coordinate"))
}

fn number(field: &'static str, value: f64) -> Result<Number, RecordError> {
    js_number(value).ok_or(RecordError::NonFinite(field))
}

/// The `colorHex8` token (`#rrggbb` or `#rrggbbaa`) as a native solid paint (byte / 255).
fn paint(field: &'static str, token: &str) -> Result<SolidFill, RecordError> {
    let unsupported = || RecordError::UnsupportedPaint {
        field,
        token: token.to_owned(),
    };
    let hex = token.strip_prefix('#').ok_or_else(unsupported)?;
    if (hex.len() != 6 && hex.len() != 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(unsupported());
    }
    let channel = |index: usize| -> Result<Number, RecordError> {
        let byte =
            u8::from_str_radix(&hex[2 * index..2 * index + 2], 16).map_err(|_| unsupported())?;
        number(field, f64::from(byte) / 255.0)
    };
    Ok(SolidFill {
        kind: "solid".into(),
        r: channel(0)?,
        g: channel(1)?,
        b: channel(2)?,
        a: if hex.len() == 8 {
            channel(3)?
        } else {
            Number::from(1u8)
        },
    })
}

fn point(pair: [f64; 2]) -> Result<ObjectPoint, RecordError> {
    Ok(ObjectPoint {
        x: coordinate(pair[0])?,
        y: coordinate(pair[1])?,
    })
}

fn segment(seg: &Seg) -> Result<ObjectSegment, RecordError> {
    Ok(ObjectSegment {
        point: point(seg.point)?,
        handle_in: point(seg.handle_in)?,
        handle_out: point(seg.handle_out)?,
    })
}

/// One exploded path as the `editable-vector-path` record the baseline `serP` would persist.
/// The target arrives with an empty stroke id (the owner mints it).
pub(crate) fn path_record(
    path: &ExplodedPath,
    extras: &StrokeExtras,
    target: ObjectTarget,
) -> Result<ObjectRecord, RecordError> {
    let count = path.contour.segments.len();
    if !(MIN_SEGMENTS..=MAX_SEGMENTS).contains(&count) {
        return Err(RecordError::SegmentCount(count));
    }
    let segments = path
        .contour
        .segments
        .iter()
        .map(segment)
        .collect::<Result<Vec<_>, _>>()?;
    let fill = path
        .style
        .fill
        .as_deref()
        .map(|token| paint("fill", token))
        .transpose()?;
    let stroke = path
        .style
        .stroke
        .as_deref()
        .map(|token| paint("stroke", token))
        .transpose()?;
    let dash_array = if extras.dash_array.is_empty() {
        Presence::Missing
    } else {
        Presence::Present(
            extras
                .dash_array
                .iter()
                .map(|dash| number("dashArray", *dash))
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    let miter_limit = match extras.miter_limit {
        None => Presence::Missing,
        Some(limit) => Presence::Present(number("miterLimit", limit)?),
    };
    let dash_offset = match extras.dash_offset {
        None => Presence::Missing,
        Some(offset) => Presence::Present(number("dashOffset", offset)?),
    };
    let has_real_stroke = stroke.is_some();
    Ok(ObjectRecord {
        schema_version: 1,
        family: VECTOR_FAMILY.to_owned(),
        target,
        geometry: ObjectGeometry {
            kind: "cubic-path".into(),
            closed: path.contour.closed,
            coordinate_space: "document-world".into(),
            handle_space: "relative".into(),
            segments,
        },
        fill: Presence::Present(fill),
        vector: Presence::Present(VectorBody {
            stroke: Presence::Present(stroke),
            has_real_stroke,
            stroke_width: number("strokeWidth", path.style.stroke_width)?,
            stroke_cap: extras.cap.clone(),
            stroke_join: extras.join.clone(),
            miter_limit,
            dash_array,
            dash_offset,
            paint_order: Presence::Missing,
            opacity: number("opacity", path.style.opacity)?,
        }),
        // An SVG path carries no pressure profile, paramShape or boxAngle (L01-E): absent on the wire.
        vector_ext: Presence::Missing,
        effects: Presence::Missing,
    })
}

/// One SVG plan as one batch of records (no layers: the caller adds a `NewLayer` when the records
/// land in a layer the import creates). Source ids: `<prefix>:<node>:<n>` per path (pre-order node
/// index of the leaf, ordinal inside the leaf), `<prefix>:<node>` per skipped element and
/// `<prefix>:<node>:hole:<child>` per hole no exterior claimed (the baseline drops those silently;
/// here each one is a typed skip).
pub(crate) fn svg_plan_batch(
    plan: &SvgImportPlan,
    placement: &SvgPlacement,
    extras: &[StrokeExtras],
) -> Result<ImportBatch, SvgBatchError> {
    if extras.len() != plan.paths.len() {
        return Err(SvgBatchError::ExtrasMismatch {
            paths: plan.paths.len(),
            extras: extras.len(),
        });
    }
    let prefix = &placement.id_prefix;
    let mut records = Vec::with_capacity(plan.paths.len());
    let mut skipped = Vec::new();
    let mut at = 0usize;
    for leaf in &plan.leaves {
        for ordinal in 0..leaf.produced {
            let path = plan
                .paths
                .get(at)
                .ok_or(SvgBatchError::LeafReportMismatch)?;
            let target = ObjectTarget {
                context_id: placement.context_id.clone(),
                frame_scope: FrameScope {
                    kind: FrameScopeKind::Authored,
                    frame: placement.frame,
                },
                layer_uid: placement.layer_ref.clone(),
                stroke_id: String::new(),
            };
            let record = path_record(path, &extras[at], target)
                .map_err(|error| SvgBatchError::Record { index: at, error })?;
            records.push(ImportRecord {
                source_id: format!("{prefix}:{}:{ordinal}", leaf.node_index),
                record,
            });
            at += 1;
        }
        for hole in &leaf.unclaimed_holes {
            skipped.push(ImportSkip {
                kind: SkipKind::Element,
                source_id: format!("{prefix}:{}:hole:{hole}", leaf.node_index),
                reason: "unclaimed-hole".into(),
            });
        }
    }
    if at != plan.paths.len() {
        return Err(SvgBatchError::LeafReportMismatch);
    }
    for element in &plan.skipped {
        skipped.push(ImportSkip {
            kind: SkipKind::Element,
            source_id: format!("{prefix}:{}", element.node_index),
            reason: format!("unsupported-element:{}", element.class_name),
        });
    }
    Ok(ImportBatch {
        layers: Vec::new(),
        records,
        skipped,
        unavailable: Vec::new(),
    })
}
