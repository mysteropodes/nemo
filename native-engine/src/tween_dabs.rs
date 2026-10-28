//! Tween-specific pressure/opacity adaptation around the canonical native dab stamper.
use crate::object_document::brush_dab::{
    presets::from_catalog, sampling::SeededRng, stamp::build_brush_dabs,
};
use crate::object_document::BrushPresetRecord;
use crate::request_receipts::DispatchErrorCode;
use crate::tween_kernels::*;

#[derive(Serialize)]
pub(super) struct Dab {
    pub segments: Vec<SegOut>,
    pub opacity: f64,
}

pub(super) fn build_dabs(
    s: &StrokeIn,
    p: &BrushPresetRecord,
    base_width: f64,
    seed: u32,
    mul: f64,
) -> R<Vec<Dab>> {
    // Preserve the command's bounded custom-preset admission before stamping.
    if p.poly_sides.value().copied().unwrap_or(5.) > 256.
        || p.bristle_count.value().copied().unwrap_or(5.) > 900.
        || p.scribble_count.value().copied().unwrap_or(8.) > 900.
        || p.custom_stamp
            .value()
            .is_some_and(|s| s.segments.len() > 256)
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    let mut base = if base_width == 0. { 3. } else { base_width };
    let mut profile = vec![];
    let pressure = is_vb(s);
    let segments: Vec<[f64; 6]> = if pressure {
        let center = s.center_segments.as_ref().unwrap();
        let mut lengths = vec![0.];
        for pair in center.windows(2) {
            lengths.push(
                lengths.last().unwrap()
                    + (pair[1].point[0] - pair[0].point[0])
                        .hypot(pair[1].point[1] - pair[0].point[1]),
            );
        }
        let total = lengths.last().copied().filter(|&n| n != 0.).unwrap_or(1.);
        profile = center
            .iter()
            .zip(lengths)
            .map(|(s, l)| (l / total, if s.width == 0. { base } else { s.width }))
            .collect();
        base = profile.iter().map(|(_, width)| width).sum::<f64>() / profile.len() as f64;
        center
            .iter()
            .map(|s| {
                let i = s.handle_in;
                let o = s.handle_out;
                [s.point[0], s.point[1], i[0], i[1], o[0], o[1]]
            })
            .collect()
    } else {
        s.segments
            .iter()
            .map(|s| {
                let i = s.handle_in;
                let o = s.handle_out;
                [s.point[0], s.point[1], i[0], i[1], o[0], o[1]]
            })
            .collect()
    };
    let preset = from_catalog(p).ok_or(DispatchErrorCode::Unavailable)?;
    let mut rng = SeededRng::new(seed);
    build_brush_dabs(
        &segments,
        s.closed && !pressure,
        &preset,
        base,
        &mut rng,
        Some(&profile),
    )
    .map_err(|_| DispatchErrorCode::Unavailable)?
    .into_iter()
    .map(|dab| {
        // Original dabRecordsForTween uses the anchor colour unchanged; the common stamper
        // still consumes value-jitter random draws in the original order.
        let out = Dab {
            opacity: dab.opacity * mul,
            segments: dab
                .path
                .segments
                .into_iter()
                .map(|s| SegOut {
                    point: s.p,
                    handle_in: s.h_in,
                    handle_out: s.h_out,
                })
                .collect(),
        };
        if !out.opacity.is_finite()
            || out.segments.iter().any(|s| {
                s.point
                    .iter()
                    .chain(&s.handle_in)
                    .chain(&s.handle_out)
                    .any(|v| !v.is_finite())
            })
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        Ok(out)
    })
    .collect()
}
