//! D27 immutable range reader shared by frame selection and layer edits.
//! Baseline app.js at fbe4b8a3: layerIn/OutPoint, resolveLinkedTime and blank defaults.
//! No document writes, total-frame arrays, guest execution or fallback writer.
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectLayer, ObjectRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeError {
    ContextUnavailable,
    FrameOutOfRange,
    LayerMissing,
    TimingUnavailable,
    OffsetExpressionUnavailable,
    OffsetUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ResolvedRange {
    pub(crate) in_point: f64,
    pub(crate) out_point: f64,
}
impl ResolvedRange {
    pub(crate) fn contains(self, frame: u32) -> bool {
        let frame = f64::from(frame);
        frame >= self.in_point && frame <= self.out_point
    }
}

/// All inputs (including offsets, timing and item presence) belong to this SAME snapshot.
/// `sample_frame` is explicit: native fixed-frame callers have no ambient UI currentFrame.
/// Legacy exports that retained a different UI frame must supply that frame explicitly in
/// their own admission, rather than claiming these request-frame offsets reproduce it.
pub(crate) struct RangeEvaluator<'a> {
    document: &'a ObjectDocument,
    context: &'a str,
    layers: &'a [ObjectLayer],
    records: &'a [ObjectRecord],
    total: u32,
    sample_frame: u32,
}
impl<'a> RangeEvaluator<'a> {
    /// Internal borrowed working-document route for the sealed native edit owner.
    /// No mutation or fabricated snapshot is exposed by this reader.
    pub(crate) fn for_document(
        document: &'a ObjectDocument,
        context: &'a str,
        sample_frame: u32,
    ) -> Result<Self, RangeError> {
        let (layers, records, total) = if context == "scene-root" {
            (
                document.layers(),
                document.objects(),
                document.total_frames(),
            )
        } else {
            let component = document
                .component_set()
                .and_then(|set| {
                    set.components
                        .iter()
                        .find(|item| item.symbol_id() == context)
                })
                .ok_or(RangeError::ContextUnavailable)?;
            (
                component.layers(),
                component.objects(),
                component.total_frames(),
            )
        };
        if sample_frame >= total {
            return Err(RangeError::FrameOutOfRange);
        }
        Ok(Self {
            document,
            context,
            layers,
            records,
            total,
            sample_frame,
        })
    }

    /// Raw edges retain stored values outside the timeline. A linked SOURCE always uses
    /// its clamped edge, matching layerIn/OutPointRaw -> resolveLinkedTime in the baseline.
    /// Folder intersection preserves an inverted (empty) range; it never swaps endpoints.
    pub(crate) fn resolve(&self, uid: &str, clamped: bool) -> Result<ResolvedRange, RangeError> {
        let index = self
            .layers
            .iter()
            .position(|layer| layer.layer_uid() == uid)
            .ok_or(RangeError::LayerMissing)?;
        Ok(ResolvedRange {
            in_point: self.edge(index, true, clamped, &mut Vec::new(), 0)?,
            out_point: self.edge(index, false, clamped, &mut Vec::new(), 0)?,
        })
    }

    /// Baseline layerHasTimeRange, including inherited folder windows. Missing/null
    /// edges and marker-only records do not activate auto-bound clipping by themselves.
    pub(crate) fn gates(&self, uid: &str) -> Result<bool, RangeError> {
        let mut index = self
            .layers
            .iter()
            .position(|layer| layer.layer_uid() == uid)
            .ok_or(RangeError::LayerMissing)?;
        for depth in 0..=8 {
            let layer = &self.layers[index];
            if layer.range_gates_frames() {
                return Ok(true);
            }
            if depth == 8 {
                break;
            }
            let Some(parent) = self.folder_parent(layer) else {
                break;
            };
            index = parent;
        }
        Ok(false)
    }

    fn clamp(&self, value: f64) -> f64 {
        value.max(0.0).min(f64::from(self.total - 1))
    }

    fn folder_parent(&self, layer: &ObjectLayer) -> Option<usize> {
        let uid = layer.parent_layer_uid.value()?.as_deref()?;
        self.layers.iter().position(|parent| {
            parent.layer_uid() == uid
                && parent.layer_uid() != layer.layer_uid()
                && parent.is_folder_layer()
        })
    }

    fn edge(
        &self,
        index: usize,
        incoming: bool,
        clamped: bool,
        seen: &mut Vec<usize>,
        depth: u32,
    ) -> Result<f64, RangeError> {
        let layer = &self.layers[index];
        let linked = self.linked(index, incoming, seen, depth)?;
        let mut value = if let Some(value) = linked {
            value
        } else {
            let range = layer.range.value();
            let explicit = range
                .and_then(|range| {
                    if incoming {
                        range.in_point.value()
                    } else {
                        range.out_point.value()
                    }
                })
                .copied()
                .flatten();
            match explicit {
                Some(frame) => frame as f64,
                None => {
                    let (start, end) = self.auto_bounds(layer)?;
                    f64::from(if incoming { start } else { end })
                }
            }
        };
        if clamped {
            value = self.clamp(value);
        }
        if depth < 8 {
            if let Some(parent) = self.folder_parent(layer) {
                let boundary = self.edge(parent, incoming, clamped, seen, depth + 1)?;
                value = if incoming {
                    value.max(boundary)
                } else {
                    value.min(boundary)
                };
            }
        }
        Ok(value)
    }

    fn linked(
        &self,
        index: usize,
        incoming: bool,
        seen: &mut Vec<usize>,
        depth: u32,
    ) -> Result<Option<f64>, RangeError> {
        let layer = &self.layers[index];
        let Some(link) = layer
            .range
            .value()
            .and_then(|range| range.time_link.value())
            .and_then(Option::as_ref)
        else {
            return Ok(None);
        };
        let mode = link.mode.value().map(String::as_str).unwrap_or("both");
        if (incoming && mode == "out") || (!incoming && mode == "in") {
            return Ok(None);
        }
        let Some(source) = self.layers.iter().position(|candidate| {
            candidate.layer_uid() == link.uid && candidate.layer_uid() != layer.layer_uid()
        }) else {
            return Ok(None);
        };
        if seen.contains(&index) || depth > 16 {
            return Ok(None);
        }
        seen.push(index);
        let source_incoming = link
            .src_anchor
            .value()
            .map_or(incoming, |anchor| anchor == "in");
        let base = self.edge(source, source_incoming, true, seen, depth + 1)?;
        Ok(Some(self.clamp(base + self.offset(layer, incoming)?)))
    }

    fn offset(&self, layer: &ObjectLayer, incoming: bool) -> Result<f64, RangeError> {
        let property = if incoming {
            "timeLinkInOffset"
        } else {
            "timeLinkOutOffset"
        };
        let document = self.document;
        let holder = document
            .property_holders()
            .unwrap_or(&[])
            .iter()
            .find(|holder| {
                let target = holder.target();
                target.context_id() == self.context
                    && target.layer_uid() == layer.layer_uid()
                    && target.stroke_id().is_none()
            });
        let Some(holder) = holder else {
            return Ok(0.0);
        };
        let record = holder
            .properties()
            .iter()
            .find(|record| record.property() == property);
        // E2 is a private bounded runtime, not the complete production expression provider.
        // Never call RAW sampling through an active expression (including empty code).
        if record.is_some_and(|record| record.expression.value().is_some_and(|expr| expr.enabled)) {
            return Err(RangeError::OffsetExpressionUnavailable);
        }
        // Admission metadata marks these properties nonanimatable: ANY stored track,
        // including an empty one, is refused by object_properties::property. No key
        // interpolation is invented here. The explicit request frame is preserved for
        // a future production expression provider, which is currently unavailable.
        let _requested_frame = self.sample_frame;
        if record.is_some_and(|record| record.track.value().is_some()) {
            return Err(RangeError::OffsetUnavailable);
        }
        match record
            .and_then(|record| record.static_value.value())
            .map(Vec::as_slice)
        {
            None => Ok(0.0),
            Some([value]) => value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or(RangeError::OffsetUnavailable),
            _ => Err(RangeError::OffsetUnavailable),
        }
    }

    fn has_content(&self, uid: &str, frame: u32) -> bool {
        let belongs = |target: &crate::object_document::ObjectTarget| {
            target.context_id() == self.context
                && target.layer_uid() == uid
                && target.frame() == frame
                && target.scope_kind() == &FrameScopeKind::Authored
        };
        self.records.iter().any(|record| belongs(record.target()))
            || (self.context == "scene-root"
                && self
                    .document
                    .item_records()
                    .is_some_and(|items| items.targets().any(|target| belongs(target))))
    }

    /// Only KEY cells update blank state. Interpolated/manual samples do not change
    /// the auto-range defaults. Work scales with sparse keys and stored records.
    fn auto_bounds(&self, layer: &ObjectLayer) -> Result<(u32, u32), RangeError> {
        if self.context == "scene-root"
            && self.document.component_set().is_some_and(|set| {
                set.instances
                    .iter()
                    .any(|item| item.layer_uid() == layer.layer_uid())
            })
        {
            return Ok((0, self.total - 1));
        }
        // Structural folders have no drawing, so their missing edges are full timeline.
        if layer.is_folder_layer() {
            return Ok((0, self.total - 1));
        }
        let Some(timing) = layer.timing.value() else {
            // Never infer authored blank keys from records alone. A range that needs
            // an automatic edge must wait for its real timing provider.
            return Err(RangeError::TimingUnavailable);
        };
        let mut first = None;
        let mut last = None;
        let mut prior_nonblank = false;
        let mut prior_key = 0;
        for key in timing.key_frames() {
            if prior_nonblank {
                last = Some(key.saturating_sub(1));
            }
            prior_nonblank = self.has_content(layer.layer_uid(), key);
            if prior_nonblank {
                first.get_or_insert(key);
            }
            prior_key = key;
        }
        if prior_nonblank && prior_key < self.total {
            last = Some(self.total - 1);
        }
        Ok((
            first.filter(|frame| *frame > 0).unwrap_or(0),
            last.filter(|frame| *frame < self.total - 1)
                .unwrap_or(self.total - 1),
        ))
    }
}

#[cfg(all(test, feature = "commands", feature = "evaluation"))]
#[path = "../tests/object_layer_range_frames.rs"]
mod frame_tests;
#[cfg(all(test, feature = "commands"))]
#[path = "../tests/object_layer_range_evaluate.rs"]
mod tests;
