//! The baseline vector-brush presets (`BRUSH_PRESETS` in `src/js/tools.js` @1065e9d8, generated verbatim from that
//! object): every member a preset may set, with JavaScript's "missing" kept as `None`, so each reader applies the
//! baseline's own default (`!== undefined ? v : d`) or falsy fallback (`v || d`) where the stamper reads it. A project
//! catalog preset maps onto the same struct.
use crate::object_document::{BrushPresetRecord, Presence};

/// A captured stamp: unit segments `[x, y, inX, inY, outX, outY]` scaled by the dab size.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CustomStamp {
    pub(crate) segments: Vec<[f64; 6]>,
    pub(crate) closed: bool,
}

/// One preset; `None` is a member the preset does not set.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct DabPreset {
    pub(crate) nib_size: Option<f64>,
    pub(crate) roundness: Option<f64>,
    pub(crate) spacing: Option<f64>,
    pub(crate) space_jitter: Option<f64>,
    pub(crate) rotation_mode: Option<String>,
    pub(crate) rotation_jitter: Option<f64>,
    pub(crate) fixed_angle: Option<f64>,
    pub(crate) size_jitter: Option<f64>,
    pub(crate) opacity: Option<f64>,
    pub(crate) opacity_jitter: Option<f64>,
    pub(crate) scatter: Option<f64>,
    pub(crate) scatter_distribution: Option<String>,
    pub(crate) dash_gap: Option<f64>,
    pub(crate) value_jitter: Option<f64>,
    pub(crate) tip_shape: Option<String>,
    pub(crate) tip_corner: Option<f64>,
    pub(crate) poly_sides: Option<f64>,
    pub(crate) edge_noise: Option<f64>,
    pub(crate) bristle_count: Option<f64>,
    pub(crate) scribble_count: Option<f64>,
    pub(crate) scribble_len: Option<f64>,
    pub(crate) scribble_len_jitter: Option<f64>,
    pub(crate) scribble_width: Option<f64>,
    pub(crate) scribble_spread: Option<f64>,
    pub(crate) scribble_angle_spread: Option<f64>,
    pub(crate) pressure_start: Option<f64>,
    pub(crate) pressure_mid: Option<f64>,
    pub(crate) pressure_end: Option<f64>,
    pub(crate) sharpness: Option<f64>,
    pub(crate) roughness: Option<f64>,
    pub(crate) grain: Option<f64>,
    pub(crate) noise: Option<f64>,
    pub(crate) marker_tip: Option<bool>,
    pub(crate) custom_stamp: Option<CustomStamp>,
}

/// `BRUSH_MAX_DABS`: the hard ceiling on dabs per stroke.
/// `resolveBrushPreset(key)`'s second source, a project catalog preset, read field for field as the stamper reads it.
/// A stamp segment without both handles has no baseline reading (`s.handleIn[0]` throws), so the preset has none either.
pub(crate) fn from_catalog(p: &BrushPresetRecord) -> Option<DabPreset> {
    let n = |v: &Presence<f64>| v.value().copied();
    let s = |v: &Presence<String>| v.value().cloned();
    let custom_stamp = match p.custom_stamp.value() {
        None => None,
        Some(stamp) => Some(CustomStamp {
            segments: stamp
                .segments
                .iter()
                .map(|g| {
                    let (i, o) = (g.handle_in.value()?, g.handle_out.value()?);
                    Some([g.point[0], g.point[1], i[0], i[1], o[0], o[1]])
                })
                .collect::<Option<Vec<_>>>()?,
            closed: stamp.closed.value().copied().unwrap_or(false),
        }),
    };
    Some(DabPreset {
        nib_size: n(&p.nib_size),
        roundness: n(&p.roundness),
        spacing: n(&p.spacing),
        space_jitter: n(&p.space_jitter),
        rotation_mode: s(&p.rotation_mode),
        rotation_jitter: n(&p.rotation_jitter),
        fixed_angle: n(&p.fixed_angle),
        size_jitter: n(&p.size_jitter),
        opacity: n(&p.opacity),
        opacity_jitter: n(&p.opacity_jitter),
        scatter: n(&p.scatter),
        scatter_distribution: s(&p.scatter_distribution),
        dash_gap: n(&p.dash_gap),
        value_jitter: n(&p.value_jitter),
        tip_shape: s(&p.tip_shape),
        tip_corner: n(&p.tip_corner),
        poly_sides: n(&p.poly_sides),
        edge_noise: n(&p.edge_noise),
        bristle_count: n(&p.bristle_count),
        scribble_count: n(&p.scribble_count),
        scribble_len: n(&p.scribble_len),
        scribble_len_jitter: n(&p.scribble_len_jitter),
        scribble_width: n(&p.scribble_width),
        scribble_spread: n(&p.scribble_spread),
        scribble_angle_spread: n(&p.scribble_angle_spread),
        pressure_start: n(&p.pressure_start),
        pressure_mid: n(&p.pressure_mid),
        pressure_end: n(&p.pressure_end),
        sharpness: n(&p.sharpness),
        roughness: n(&p.roughness),
        grain: n(&p.grain),
        noise: n(&p.noise),
        marker_tip: p.marker_tip.value().copied(),
        custom_stamp,
    })
}

pub(crate) const BRUSH_MAX_DABS: usize = 900;
/// `SHARPNESS_LAYERS`: concentric layers of a soft-edged dab.
pub(crate) const SHARPNESS_LAYERS: usize = 3;

/// JavaScript truthiness of an optional number (`undefined`, `0` and `NaN` are falsy).
pub(crate) fn truthy(value: Option<f64>) -> bool {
    value.is_some_and(|v| v != 0.0 && !v.is_nan())
}
/// `v || fallback`.
pub(crate) fn or(value: Option<f64>, fallback: f64) -> f64 {
    if truthy(value) {
        value.unwrap_or(fallback)
    } else {
        fallback
    }
}

/// A built-in preset by key (`BRUSH_PRESETS[key]`); `None` for any other key, including `'none'`.
#[rustfmt::skip]
pub(crate) fn builtin(key: &str) -> Option<DabPreset> {
    Some(match key {
        "chalk-blunt" => DabPreset { nib_size: Some(1.3), roundness: Some(0.85), spacing: Some(0.55), space_jitter: Some(0.25), rotation_mode: Some("random".into()), rotation_jitter: Some(60.0), size_jitter: Some(0.25), opacity: Some(0.55), opacity_jitter: Some(0.25), scatter: Some(0.18), dash_gap: Some(0.0), value_jitter: Some(0.12), ..DabPreset::default() },
        "chalk-round" => DabPreset { nib_size: Some(1.1), roundness: Some(1.0), spacing: Some(0.4), space_jitter: Some(0.2), rotation_mode: Some("random".into()), rotation_jitter: Some(30.0), size_jitter: Some(0.18), opacity: Some(0.5), opacity_jitter: Some(0.2), scatter: Some(0.12), dash_gap: Some(0.0), value_jitter: Some(0.1), ..DabPreset::default() },
        "chalk-scribble" => DabPreset { nib_size: Some(1.0), roundness: Some(0.7), spacing: Some(0.7), space_jitter: Some(0.4), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.4), opacity: Some(0.4), opacity_jitter: Some(0.3), scatter: Some(0.3), dash_gap: Some(0.15), value_jitter: Some(0.15), ..DabPreset::default() },
        "charcoal-feather" => DabPreset { nib_size: Some(1.4), roundness: Some(0.6), spacing: Some(0.6), space_jitter: Some(0.35), rotation_mode: Some("random".into()), rotation_jitter: Some(90.0), size_jitter: Some(0.45), opacity: Some(0.3), opacity_jitter: Some(0.35), scatter: Some(0.35), dash_gap: Some(0.1), value_jitter: Some(0.14), ..DabPreset::default() },
        "charcoal-pencil" => DabPreset { nib_size: Some(0.8), roundness: Some(0.9), spacing: Some(0.3), space_jitter: Some(0.15), rotation_mode: Some("tangent".into()), rotation_jitter: Some(15.0), size_jitter: Some(0.15), opacity: Some(0.7), opacity_jitter: Some(0.15), scatter: Some(0.08), dash_gap: Some(0.0), value_jitter: Some(0.08), ..DabPreset::default() },
        "charcoal-rough" => DabPreset { nib_size: Some(1.5), roundness: Some(0.65), spacing: Some(0.65), space_jitter: Some(0.45), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.55), opacity: Some(0.4), opacity_jitter: Some(0.35), scatter: Some(0.4), dash_gap: Some(0.18), value_jitter: Some(0.15), ..DabPreset::default() },
        "charcoal-rounded" => DabPreset { nib_size: Some(1.3), roundness: Some(1.0), spacing: Some(0.45), space_jitter: Some(0.2), rotation_mode: Some("random".into()), rotation_jitter: Some(45.0), size_jitter: Some(0.15), opacity: Some(0.55), opacity_jitter: Some(0.2), scatter: Some(0.15), dash_gap: Some(0.0), value_jitter: Some(0.1), ..DabPreset::default() },
        "charcoal-smooth" => DabPreset { nib_size: Some(1.2), roundness: Some(1.0), spacing: Some(0.25), space_jitter: Some(0.1), rotation_mode: Some("random".into()), rotation_jitter: Some(20.0), size_jitter: Some(0.1), opacity: Some(0.65), opacity_jitter: Some(0.1), scatter: Some(0.06), dash_gap: Some(0.0), value_jitter: Some(0.08), ..DabPreset::default() },
        "charcoal-soft" => DabPreset { nib_size: Some(1.6), roundness: Some(0.75), spacing: Some(0.5), space_jitter: Some(0.3), rotation_mode: Some("random".into()), rotation_jitter: Some(90.0), size_jitter: Some(0.3), opacity: Some(0.32), opacity_jitter: Some(0.3), scatter: Some(0.25), dash_gap: Some(0.05), value_jitter: Some(0.13), ..DabPreset::default() },
        "charcoal-tapered" => DabPreset { nib_size: Some(1.2), roundness: Some(0.8), spacing: Some(0.45), space_jitter: Some(0.2), rotation_mode: Some("tangent".into()), rotation_jitter: Some(25.0), size_jitter: Some(0.2), opacity: Some(0.55), opacity_jitter: Some(0.2), scatter: Some(0.15), dash_gap: Some(0.0), value_jitter: Some(0.1), ..DabPreset::default() },
        "charcoal-thick" => DabPreset { nib_size: Some(2.2), roundness: Some(0.9), spacing: Some(0.4), space_jitter: Some(0.2), rotation_mode: Some("random".into()), rotation_jitter: Some(40.0), size_jitter: Some(0.15), opacity: Some(0.6), opacity_jitter: Some(0.15), scatter: Some(0.12), dash_gap: Some(0.0), value_jitter: Some(0.1), ..DabPreset::default() },
        "charcoal-thin" => DabPreset { nib_size: Some(0.6), roundness: Some(0.9), spacing: Some(0.35), space_jitter: Some(0.15), rotation_mode: Some("random".into()), rotation_jitter: Some(30.0), size_jitter: Some(0.15), opacity: Some(0.65), opacity_jitter: Some(0.15), scatter: Some(0.1), dash_gap: Some(0.0), value_jitter: Some(0.09), ..DabPreset::default() },
        "charcoal-varied" => DabPreset { nib_size: Some(1.3), roundness: Some(0.7), spacing: Some(0.6), space_jitter: Some(0.5), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.65), opacity: Some(0.4), opacity_jitter: Some(0.4), scatter: Some(0.3), dash_gap: Some(0.12), value_jitter: Some(0.15), ..DabPreset::default() },
        "pencil-feather" => DabPreset { nib_size: Some(0.7), roundness: Some(0.6), spacing: Some(0.6), space_jitter: Some(0.35), rotation_mode: Some("random".into()), rotation_jitter: Some(90.0), size_jitter: Some(0.3), opacity: Some(0.3), opacity_jitter: Some(0.3), scatter: Some(0.2), dash_gap: Some(0.15), value_jitter: Some(0.12), ..DabPreset::default() },
        "pencil-thick" => DabPreset { nib_size: Some(1.8), roundness: Some(0.9), spacing: Some(0.3), space_jitter: Some(0.15), rotation_mode: Some("tangent".into()), rotation_jitter: Some(15.0), size_jitter: Some(0.15), opacity: Some(0.7), opacity_jitter: Some(0.15), scatter: Some(0.08), dash_gap: Some(0.0), value_jitter: Some(0.09), ..DabPreset::default() },
        "pencil-thin" => DabPreset { nib_size: Some(0.5), roundness: Some(0.9), spacing: Some(0.3), space_jitter: Some(0.1), rotation_mode: Some("tangent".into()), rotation_jitter: Some(10.0), size_jitter: Some(0.1), opacity: Some(0.8), opacity_jitter: Some(0.1), scatter: Some(0.05), dash_gap: Some(0.0), value_jitter: Some(0.08), ..DabPreset::default() },
        "marker-flat" => DabPreset { nib_size: Some(1.6), roundness: Some(0.4), spacing: Some(0.25), space_jitter: Some(0.05), rotation_mode: Some("fixed".into()), fixed_angle: Some(35.0), rotation_jitter: Some(4.0), size_jitter: Some(0.05), opacity: Some(0.85), opacity_jitter: Some(0.05), scatter: Some(0.02), dash_gap: Some(0.0), tip_shape: Some("rect".into()), tip_corner: Some(0.1), ..DabPreset::default() },
        "ink-chisel" => DabPreset { nib_size: Some(1.2), roundness: Some(0.3), spacing: Some(0.3), space_jitter: Some(0.08), rotation_mode: Some("tangent".into()), rotation_jitter: Some(8.0), size_jitter: Some(0.1), opacity: Some(0.9), opacity_jitter: Some(0.05), scatter: Some(0.03), dash_gap: Some(0.0), tip_shape: Some("rect".into()), tip_corner: Some(0.05), ..DabPreset::default() },
        "pastel-chip" => DabPreset { nib_size: Some(1.7), roundness: Some(0.85), spacing: Some(0.5), space_jitter: Some(0.3), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.3), opacity: Some(0.5), opacity_jitter: Some(0.25), scatter: Some(0.2), dash_gap: Some(0.05), tip_shape: Some("polygon".into()), poly_sides: Some(6.0), edge_noise: Some(0.12), value_jitter: Some(0.12), ..DabPreset::default() },
        "chalk-facet" => DabPreset { nib_size: Some(1.3), roundness: Some(0.9), spacing: Some(0.45), space_jitter: Some(0.25), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.25), opacity: Some(0.45), opacity_jitter: Some(0.3), scatter: Some(0.15), dash_gap: Some(0.0), tip_shape: Some("polygon".into()), poly_sides: Some(5.0), edge_noise: Some(0.18), value_jitter: Some(0.12), ..DabPreset::default() },
        "ink-splatter" => DabPreset { nib_size: Some(1.4), roundness: Some(1.0), spacing: Some(1.1), space_jitter: Some(0.6), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.7), opacity: Some(0.75), opacity_jitter: Some(0.2), scatter: Some(0.5), dash_gap: Some(0.1), tip_shape: Some("splatter".into()), edge_noise: Some(0.08), ..DabPreset::default() },
        "drybrush-bristle" => DabPreset { nib_size: Some(2.2), roundness: Some(1.0), spacing: Some(0.35), space_jitter: Some(0.15), rotation_mode: Some("tangent".into()), rotation_jitter: Some(12.0), size_jitter: Some(0.3), opacity: Some(0.55), opacity_jitter: Some(0.3), scatter: Some(0.6), dash_gap: Some(0.0), tip_shape: Some("bristle".into()), bristle_count: Some(7.0), ..DabPreset::default() },
        "watercolor-edge" => DabPreset { nib_size: Some(2.6), roundness: Some(0.7), spacing: Some(0.5), space_jitter: Some(0.2), rotation_mode: Some("random".into()), rotation_jitter: Some(60.0), size_jitter: Some(0.35), opacity: Some(0.22), opacity_jitter: Some(0.3), scatter: Some(0.3), dash_gap: Some(0.0), tip_shape: Some("ellipse".into()), edge_noise: Some(0.25), ..DabPreset::default() },
        "airbrush-soft" => DabPreset { nib_size: Some(0.9), roundness: Some(1.0), spacing: Some(0.3), space_jitter: Some(0.3), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.4), opacity: Some(0.18), opacity_jitter: Some(0.3), scatter: Some(1.6), dash_gap: Some(0.0), scatter_distribution: Some("normal".into()), ..DabPreset::default() },
        "marker-dry" => DabPreset { nib_size: Some(1.1), roundness: Some(0.6), spacing: Some(0.35), space_jitter: Some(0.2), rotation_mode: Some("tangent".into()), rotation_jitter: Some(12.0), size_jitter: Some(0.25), opacity: Some(0.5), opacity_jitter: Some(0.25), scatter: Some(0.5), dash_gap: Some(0.0), scatter_distribution: Some("edge".into()), ..DabPreset::default() },
        "graphite-scribble" => DabPreset { nib_size: Some(1.4), roundness: Some(1.0), spacing: Some(0.5), space_jitter: Some(0.25), rotation_mode: Some("tangent".into()), rotation_jitter: Some(0.0), size_jitter: Some(0.0), opacity: Some(0.5), opacity_jitter: Some(0.3), scatter: Some(0.0), dash_gap: Some(0.0), tip_shape: Some("scribble".into()), scribble_count: Some(9.0), scribble_len: Some(1.5), scribble_len_jitter: Some(0.4), scribble_width: Some(0.1), scribble_spread: Some(0.7), scribble_angle_spread: Some(75.0), value_jitter: Some(0.1), ..DabPreset::default() },
        "calligraphy-taper" => DabPreset { nib_size: Some(1.4), roundness: Some(0.3), spacing: Some(0.25), space_jitter: Some(0.05), rotation_mode: Some("fixed".into()), fixed_angle: Some(35.0), rotation_jitter: Some(3.0), size_jitter: Some(0.05), opacity: Some(0.9), opacity_jitter: Some(0.05), scatter: Some(0.02), dash_gap: Some(0.0), tip_shape: Some("rect".into()), tip_corner: Some(0.05), pressure_start: Some(0.15), pressure_mid: Some(1.0), pressure_end: Some(0.15), ..DabPreset::default() },
        "airbrush-blur" => DabPreset { nib_size: Some(1.6), roundness: Some(1.0), spacing: Some(0.35), space_jitter: Some(0.2), rotation_mode: Some("random".into()), rotation_jitter: Some(180.0), size_jitter: Some(0.2), opacity: Some(0.35), opacity_jitter: Some(0.2), scatter: Some(0.1), dash_gap: Some(0.0), sharpness: Some(0.35), ..DabPreset::default() },
        _ => return None,
    })
}
