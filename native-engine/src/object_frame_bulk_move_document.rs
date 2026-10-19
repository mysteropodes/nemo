//! Private child of the existing key-retime timing owner. Reuse its exact span algorithm;
//! never normalize copied samples/plain cells through the clipboard paste representation.
use crate::object_document::timing::object_exposure_edit_retime_timing::Working;
use crate::object_document::timing::{LayerTiming, LayerTimingError, MAX_ENTRIES};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct BulkCell {
    pub(crate) layer: usize,
    pub(crate) frame: u32,
}
pub(crate) struct BulkMoveLayer<'a> {
    pub(crate) timing: Option<&'a LayerTiming>,
    pub(crate) locked: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BulkRetimePair {
    pub(crate) layer: usize,
    pub(crate) from_a: u32,
    pub(crate) from_b: u32,
    pub(crate) to_a: i64,
    pub(crate) to_b: i64,
}
#[derive(Debug)]
pub(crate) struct BulkMovePlan {
    pub(crate) timings: Vec<(usize, LayerTiming)>,
    pub(crate) writes: Vec<(BulkCell, Option<BulkCell>)>,
    pub(crate) pairs: Vec<BulkRetimePair>,
    pub(crate) selection: Option<Vec<BulkCell>>,
    pub(crate) checkpoint_required: bool,
    pub(crate) shifted: bool,
}
impl BulkMovePlan {
    fn idle() -> Self {
        Self {
            timings: vec![],
            writes: vec![],
            pairs: vec![],
            selection: None,
            checkpoint_required: false,
            shifted: false,
        }
    }
}
fn timing<'a>(
    layers: &[BulkMoveLayer<'a>],
    index: usize,
    total: u32,
) -> Result<&'a LayerTiming, LayerTimingError> {
    let timing = layers
        .get(index)
        .and_then(|l| l.timing)
        .ok_or(LayerTimingError::Unsupported)?;
    timing.validate(total)?;
    Ok(timing)
}
fn target(cell: BulkCell, dl: i64, df: i64, layers: usize, total: u32) -> Option<BulkCell> {
    let layer = i64::try_from(cell.layer).ok()?.checked_add(dl)?;
    let frame = i64::from(cell.frame).checked_add(df)?;
    if layer < 0 || layer >= i64::try_from(layers).ok()? || frame < 0 || frame >= i64::from(total) {
        return None;
    }
    Some(BulkCell {
        layer: layer as usize,
        frame: frame as u32,
    })
}
/// Absolute destination of the selection bounds. Capture every selected cell, clear ALL
/// sources, write captured cells, then reuse the baseline's existing span-retime phases.
pub(crate) fn plan_move_frames(
    layers: &[BulkMoveLayer<'_>],
    selection: &[BulkCell],
    destination: BulkCell,
    total: u32,
) -> Result<BulkMovePlan, LayerTimingError> {
    if selection.is_empty() {
        return Ok(BulkMovePlan::idle());
    }
    if total == 0 || destination.layer >= layers.len() || selection.len() > MAX_ENTRIES {
        return Err(LayerTimingError::Invalid);
    }
    let mut unique = BTreeSet::new();
    for cell in selection {
        if cell.layer >= layers.len() || cell.frame >= total || !unique.insert(*cell) {
            return Err(LayerTimingError::Invalid);
        }
    }
    let min_l = selection.iter().map(|c| c.layer).min().unwrap();
    let min_f = selection.iter().map(|c| c.frame).min().unwrap();
    let dl = i64::try_from(destination.layer).map_err(|_| LayerTimingError::Invalid)?
        - i64::try_from(min_l).map_err(|_| LayerTimingError::Invalid)?;
    let mut df = i64::from(destination.frame) - i64::from(min_f);
    if dl == 0 && df == 0 {
        return Ok(BulkMovePlan::idle());
    }
    let mut cells = selection.to_vec();
    if cells.len() == 1 && dl == 0 && !layers[cells[0].layer].locked {
        let source = cells[0];
        let t = timing(layers, source.layer, total)?;
        if t.keys
            .binary_search_by_key(&source.frame, |k| k.frame)
            .is_ok()
        {
            let later: Vec<u32> = t
                .keys
                .iter()
                .filter(|k| k.frame > source.frame)
                .map(|k| k.frame)
                .collect();
            if df > 0 {
                if let Some(last) = later.last() {
                    df = df.min(i64::from(total - 1) - i64::from(*last));
                }
            }
            cells.extend(later.into_iter().map(|frame| BulkCell {
                layer: source.layer,
                frame,
            }));
        }
    }
    cells.retain(|cell| {
        if layers[cell.layer].locked {
            return false;
        }
        let index = i64::try_from(cell.layer).unwrap() + dl;
        index < 0 || index >= layers.len() as i64 || !layers[index as usize].locked
    });
    if cells.is_empty() {
        return Ok(BulkMovePlan::idle());
    }
    if cells.len() > MAX_ENTRIES {
        return Err(LayerTimingError::Invalid);
    }
    let mut touched = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for cell in &cells {
        touched.insert(cell.layer);
        sources.insert(cell.layer);
        if let Some(dest) = target(*cell, dl, df, layers.len(), total) {
            touched.insert(dest.layer);
        }
    }
    let mut work = BTreeMap::new();
    for &index in &touched {
        work.insert(index, Working::of(timing(layers, index, total)?));
    }
    let captured: Vec<_> = cells
        .iter()
        .map(|cell| (*cell, work[&cell.layer].cells.get(&cell.frame).cloned()))
        .collect();
    let mut writes = BTreeMap::new();
    for cell in &cells {
        work.get_mut(&cell.layer).unwrap().cells.remove(&cell.frame);
        writes.insert(*cell, None);
    }
    let mut selected = Vec::new();
    for (source, value) in captured {
        if let Some(dest) = target(source, dl, df, layers.len(), total) {
            let layer = work.get_mut(&dest.layer).unwrap();
            match value {
                Some(value) => {
                    layer.cells.insert(dest.frame, value);
                }
                None => {
                    layer.cells.remove(&dest.frame);
                }
            }
            writes.insert(dest, Some(source));
            selected.push(dest);
        }
    }
    let mut pairs = Vec::new();
    for &index in &sources {
        let moves: Vec<_> = cells
            .iter()
            .filter(|cell| cell.layer == index)
            .filter_map(|cell| {
                target(*cell, dl, df, layers.len(), total).map(|dest| (cell.frame, dest.frame))
            })
            .collect();
        let layer = work.get_mut(&index).unwrap();
        let (old_pairs, spans) = timing(layers, index, total)?.retime_spans(&moves, layer);
        pairs.extend(old_pairs.into_iter().map(|p| BulkRetimePair {
            layer: index,
            from_a: p.from_a,
            from_b: p.from_b,
            to_a: i64::from(p.to_a),
            to_b: i64::from(p.to_b),
        }));
        for span in &spans {
            layer.clear_stale(span);
        }
        for span in &spans {
            layer.lay(span, total);
            layer.fill_holes(span)?;
        }
    }
    let mut timings = Vec::new();
    for (index, layer) in work {
        let (next, retimed) = layer.finish(timing(layers, index, total)?, total)?;
        for (frame, source) in retimed {
            writes.insert(
                BulkCell {
                    layer: index,
                    frame,
                },
                source.map(|frame| BulkCell {
                    layer: index,
                    frame,
                }),
            );
        }
        timings.push((index, next));
    }
    Ok(BulkMovePlan {
        timings,
        writes: writes.into_iter().collect(),
        pairs,
        selection: Some(selected),
        checkpoint_required: true,
        shifted: false,
    })
}
/// Content-only helper: discard the old row and carry ONLY cells with drawn content.
/// The separate caller's Motion/range/history transaction is deliberately not performed here.
pub(crate) fn plan_shift_frames(
    layers: &[BulkMoveLayer<'_>],
    index: usize,
    delta: i64,
    drawn: &BTreeSet<u32>,
    total: u32,
) -> Result<BulkMovePlan, LayerTimingError> {
    let layer = layers.get(index).ok_or(LayerTimingError::Invalid)?;
    if delta == 0 || layer.locked {
        return Ok(BulkMovePlan::idle());
    }
    let base = timing(layers, index, total)?;
    if drawn.iter().any(|f| *f >= total) {
        return Err(LayerTimingError::Invalid);
    }
    let old = Working::of(base);
    let mut next = Working {
        cells: BTreeMap::new(),
        writes: BTreeMap::new(),
    };
    let mut writes = BTreeMap::new();
    for frame in old.cells.keys().copied().chain(drawn.iter().copied()) {
        writes.insert(
            BulkCell {
                layer: index,
                frame,
            },
            None,
        );
    }
    for &frame in drawn {
        let source = BulkCell {
            layer: index,
            frame,
        };
        if let Some(dest) = target(source, 0, delta, layers.len(), total) {
            if let Some(value) = old.cells.get(&frame) {
                next.cells.insert(dest.frame, value.clone());
            }
            writes.insert(dest, Some(source));
        }
    }
    let (timing, _) = next.finish(base, total)?;
    let pairs = base
        .keys
        .windows(2)
        .map(|w| {
            Ok(BulkRetimePair {
                layer: index,
                from_a: w[0].frame,
                from_b: w[1].frame,
                to_a: i64::from(w[0].frame)
                    .checked_add(delta)
                    .ok_or(LayerTimingError::Invalid)?,
                to_b: i64::from(w[1].frame)
                    .checked_add(delta)
                    .ok_or(LayerTimingError::Invalid)?,
            })
        })
        .collect::<Result<Vec<_>, LayerTimingError>>()?;
    Ok(BulkMovePlan {
        timings: vec![(index, timing)],
        writes: writes.into_iter().collect(),
        pairs,
        selection: None,
        checkpoint_required: false,
        shifted: true,
    })
}
