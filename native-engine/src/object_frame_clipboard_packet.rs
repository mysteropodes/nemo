//! L05-H (#2111, F12): detached frame-clipboard packet read from a pinned ObjectSnapshot.
//!
//! The baseline copy (src/js/timeline.js copyFrames at fbe4b8a3) walks the selected cells in
//! selection order and keeps, per cell, its offset from the selection bounds plus a deep copy of
//! the dense cell. pasteFrames later turns a generated in-between (isInterpolated) into a normal
//! full key and drops isManualEdit. The native timing model is sparse, so this kernel classifies
//! each cell with the single existing reader, LayerTiming::exposure, and copies owned records:
//! a key with records, an explicit blank key, a generated in-between, a held cell (with the
//! effective records of the key it shows) and an unexposed cell stay distinguishable.
//!
//! Read-only and std-only beyond the document types: it takes an immutable snapshot, mutates
//! nothing, owns every byte it returns (no reference or Arc into the snapshot survives), and
//! refuses typed before cloning past its cell and byte bounds. Paste, cut, delete and every
//! history effect belong to the sealed clipboard planner; nothing here is a document writer.
use crate::object_document::{FrameScopeKind, ObjectRecord, OrderElement, Presence};
use crate::revision::ObjectSnapshot;
use std::collections::{HashMap, HashSet};

const CONTEXT: &str = "scene-root";
/// Hard ceiling on selected cells; a caller limit can only tighten it.
pub(crate) const MAX_PACKET_CELLS: usize = 4096;
/// Hard ceiling on the encoded JSON bytes of copied records and v2 order arrays (held cells count again).
pub(crate) const MAX_PACKET_BYTES: usize = 4 * 1024 * 1024;

/// Caller bounds; each is clamped to the hard ceiling above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PacketLimits {
    pub(crate) max_cells: usize,
    pub(crate) max_bytes: usize,
}
impl PacketLimits {
    pub(crate) const DEFAULT: Self = Self {
        max_cells: MAX_PACKET_CELLS,
        max_bytes: MAX_PACKET_BYTES,
    };
}

/// Identity the caller expects the snapshot to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PacketPins<'a> {
    pub(crate) instance_id: &'a str,
    pub(crate) document_id: &'a str,
    pub(crate) content_revision: u64,
    pub(crate) document_snapshot_id: &'a str,
}

/// One selected cell: a stable layer identity and an absolute frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedCell<'a> {
    pub(crate) layer_uid: &'a str,
    pub(crate) frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellKind {
    /// Exposure key owning at least one authored record.
    Key,
    /// Exposure key owning no authored record: an explicit blank, not an absence.
    BlankKey,
    /// Generated in-between (sparse sample); the manual-edit flag is kept as a source fact.
    Interpolated,
    /// Plain cell shown from the last key at or before it.
    Held,
    /// Plain cell before any key: nothing is shown.
    Unexposed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardPacketError {
    WrongInstance,
    WrongDocument,
    WrongRevision,
    WrongSnapshot,
    /// A document version outside the admitted v1/v2 clipboard formats.
    OrderedDocumentUnavailable,
    EmptySelection,
    UnknownLayer,
    FrameOutOfRange,
    DuplicateCell,
    /// A selected layer has no timing model; a missing model is not a blank key.
    TimingUnavailable,
    /// LayerTiming::exposure reported a cell kind this packet does not know.
    ExposureUnavailable,
    TooManyCells,
    TooManyBytes,
    RecordUnavailable,
}

/// One detached cell. Every field is owned.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PacketCell {
    rel_layer: usize,
    rel_frame: u32,
    layer_uid: String,
    frame: u32,
    kind: CellKind,
    source_frame: Option<u32>,
    manual_edit: bool,
    records: Vec<ObjectRecord>,
    order_elements: Presence<Vec<OrderElement>>,
}
impl PacketCell {
    /// Layer index minus the smallest selected layer index (baseline `rl`).
    pub(crate) fn rel_layer(&self) -> usize {
        self.rel_layer
    }
    /// Frame minus the smallest selected frame (baseline `rf`).
    pub(crate) fn rel_frame(&self) -> u32 {
        self.rel_frame
    }
    pub(crate) fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub(crate) fn frame(&self) -> u32 {
        self.frame
    }
    pub(crate) fn kind(&self) -> CellKind {
        self.kind
    }
    /// Frame whose authored records the cell shows: itself for a key or in-between, the key for a
    /// held cell, none when unexposed.
    pub(crate) fn source_frame(&self) -> Option<u32> {
        self.source_frame
    }
    /// Source fact of an in-between; never survives a paste.
    pub(crate) fn manual_edit(&self) -> bool {
        self.manual_edit
    }
    /// Effective records, in document order: own records of a key or in-between, the shown key's
    /// records for a held cell, none for a blank key or an unexposed cell.
    pub(crate) fn records(&self) -> &[ObjectRecord] {
        &self.records
    }
    /// Exact authored order DTOs of the shown source cell; absent only for legacy v1.
    pub(crate) fn order_elements(&self) -> &Presence<Vec<OrderElement>> {
        &self.order_elements
    }
    /// Baseline paste: a key, a blank key and a copied in-between all land as a full key
    /// (isKeyframe true, isInterpolated false, isManualEdit removed); a plain cell lands plain.
    pub(crate) fn paste_is_keyframe(&self) -> bool {
        !matches!(self.kind, CellKind::Held | CellKind::Unexposed)
    }
    /// Records a paste writes: the baseline copy of a plain cell has no strokes, so a held cell
    /// pastes no records even though `records` carries the shown key's.
    pub(crate) fn paste_records(&self) -> &[ObjectRecord] {
        if self.paste_is_keyframe() {
            self.records.as_slice()
        } else {
            &[]
        }
    }
}

/// Detached, immutable copy of a selection. Pins the snapshot it came from by value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrameClipboardPacket {
    instance_id: String,
    document_id: String,
    content_revision: u64,
    document_snapshot_id: String,
    origin_layer: usize,
    origin_frame: u32,
    extent_layers: usize,
    extent_frames: u32,
    encoded_bytes: usize,
    cells: Vec<PacketCell>,
}
impl FrameClipboardPacket {
    pub(crate) fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub(crate) fn document_id(&self) -> &str {
        &self.document_id
    }
    pub(crate) fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub(crate) fn document_snapshot_id(&self) -> &str {
        &self.document_snapshot_id
    }
    pub(crate) fn context_id(&self) -> &'static str {
        CONTEXT
    }
    /// Source layer index and frame of the selection bounds (baseline minL, minF).
    pub(crate) fn origin(&self) -> (usize, u32) {
        (self.origin_layer, self.origin_frame)
    }
    /// Layers and frames spanned by the bounds (baseline maxL-minL+1, maxF-minF+1).
    pub(crate) fn extent(&self) -> (usize, u32) {
        (self.extent_layers, self.extent_frames)
    }
    /// Encoded JSON bytes of copied records and v2 order arrays, held cells counted per cell.
    pub(crate) fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }
    /// Cells in selection order, as copyFrames pushes them.
    pub(crate) fn cells(&self) -> &[PacketCell] {
        &self.cells
    }
}

/// Copies `selection` out of `snapshot`. Every refusal is raised before any partial result can
/// escape; the snapshot and the caller's inputs are only borrowed.
pub(crate) fn copy_frame_cells(
    snapshot: &ObjectSnapshot,
    pins: &PacketPins<'_>,
    selection: &[SelectedCell<'_>],
    limits: &PacketLimits,
) -> Result<FrameClipboardPacket, ClipboardPacketError> {
    use ClipboardPacketError as E;
    if pins.instance_id != snapshot.instance_id() {
        return Err(E::WrongInstance);
    }
    if pins.document_id != snapshot.document_id() {
        return Err(E::WrongDocument);
    }
    if pins.content_revision != snapshot.content_revision() {
        return Err(E::WrongRevision);
    }
    if pins.document_snapshot_id != snapshot.snapshot_id() {
        return Err(E::WrongSnapshot);
    }
    let document = snapshot.document();
    if !matches!(document.format_version(), 1 | 2) {
        return Err(E::OrderedDocumentUnavailable);
    }
    if selection.is_empty() {
        return Err(E::EmptySelection);
    }
    let max_cells = limits.max_cells.min(MAX_PACKET_CELLS);
    let max_bytes = limits.max_bytes.min(MAX_PACKET_BYTES);
    if selection.len() > max_cells {
        return Err(E::TooManyCells);
    }
    let layers = document.layers();
    let objects = document.objects();
    let layer_index: HashMap<&str, usize> = layers
        .iter()
        .enumerate()
        .map(|(index, layer)| (layer.layer_uid(), index))
        .collect();
    let mut resolved: Vec<(usize, u32)> = Vec::with_capacity(selection.len());
    let mut seen: HashSet<(usize, u32)> = HashSet::new();
    for cell in selection {
        let index = *layer_index.get(cell.layer_uid).ok_or(E::UnknownLayer)?;
        if cell.frame >= document.total_frames() {
            return Err(E::FrameOutOfRange);
        }
        if !seen.insert((index, cell.frame)) {
            return Err(E::DuplicateCell);
        }
        resolved.push((index, cell.frame));
    }
    let min_layer = resolved.iter().map(|cell| cell.0).min().unwrap_or(0);
    let max_layer = resolved.iter().map(|cell| cell.0).max().unwrap_or(0);
    let min_frame = resolved.iter().map(|cell| cell.1).min().unwrap_or(0);
    let max_frame = resolved.iter().map(|cell| cell.1).max().unwrap_or(0);
    let extent_frames = (max_frame - min_frame)
        .checked_add(1)
        .ok_or(E::FrameOutOfRange)?;

    // Authored scene-root records by (layer, frame); Reference-scope records never enter.
    let mut by_cell: HashMap<(&str, u32), Vec<usize>> = HashMap::new();
    for (index, record) in objects.iter().enumerate() {
        let target = record.target();
        if target.context_id() == CONTEXT && target.scope_kind() == &FrameScopeKind::Authored {
            by_cell
                .entry((target.layer_uid(), target.frame()))
                .or_default()
                .push(index);
        }
    }

    let mut cells: Vec<PacketCell> = Vec::with_capacity(resolved.len());
    let mut encoded_bytes = 0usize;
    for &(layer_position, frame) in &resolved {
        let layer = &layers[layer_position];
        let timing = layer.timing.value().ok_or(E::TimingUnavailable)?;
        let (label, source_frame, manual_edit) = timing.exposure(frame);
        let indices: &[usize] = source_frame
            .and_then(|source| by_cell.get(&(layer.layer_uid(), source)))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let kind = match label {
            "key" if indices.is_empty() => CellKind::BlankKey,
            "key" => CellKind::Key,
            "interpolated" => CellKind::Interpolated,
            "held" => CellKind::Held,
            "unexposed" => CellKind::Unexposed,
            _ => return Err(E::ExposureUnavailable),
        };
        let mut records = Vec::with_capacity(indices.len());
        for &index in indices {
            let record = &objects[index];
            let cost = serde_json::to_vec(record)
                .map_err(|_| E::RecordUnavailable)?
                .len();
            encoded_bytes = encoded_bytes
                .checked_add(cost)
                .filter(|total| *total <= max_bytes)
                .ok_or(E::TooManyBytes)?;
            records.push(record.clone());
        }
        let order_elements = match &document.composition_order {
            Presence::Missing => Presence::Missing,
            Presence::Present(plan) => {
                // Charge the array delimiters first, then one element and separator at a time.
                // Never encode the whole selected array before enforcing the caller's budget.
                encoded_bytes = encoded_bytes
                    .checked_add(2)
                    .filter(|total| *total <= max_bytes)
                    .ok_or(E::TooManyBytes)?;
                let mut elements = Vec::new();
                for element in plan
                    .contexts
                    .iter()
                    .filter(|context| context.context_id == CONTEXT)
                    .flat_map(|context| &context.scopes)
                    .filter(|scope| {
                        scope.frame_scope.kind == FrameScopeKind::Authored
                            && Some(scope.frame_scope.frame) == source_frame
                    })
                    .flat_map(|scope| &scope.elements)
                    .filter(|element| element.layer_uid == layer.layer_uid())
                {
                    if !records
                        .iter()
                        .any(|record| record.target().stroke_id() == element.stroke_id.as_str())
                    {
                        return Err(E::RecordUnavailable);
                    }
                    let cost = serde_json::to_vec(element)
                        .map_err(|_| E::RecordUnavailable)?
                        .len();
                    encoded_bytes = encoded_bytes
                        .checked_add(cost)
                        .and_then(|total| total.checked_add(usize::from(!elements.is_empty())))
                        .filter(|total| *total <= max_bytes)
                        .ok_or(E::TooManyBytes)?;
                    elements.push(element.clone());
                }
                if elements.len() != records.len() {
                    return Err(E::RecordUnavailable);
                }
                Presence::Present(elements)
            }
        };
        cells.push(PacketCell {
            rel_layer: layer_position - min_layer,
            rel_frame: frame - min_frame,
            layer_uid: layer.layer_uid().to_owned(),
            frame,
            kind,
            source_frame,
            manual_edit,
            records,
            order_elements,
        });
    }
    Ok(FrameClipboardPacket {
        instance_id: snapshot.instance_id().to_owned(),
        document_id: snapshot.document_id().to_owned(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().to_owned(),
        origin_layer: min_layer,
        origin_frame: min_frame,
        extent_layers: max_layer - min_layer + 1,
        extent_frames,
        encoded_bytes,
        cells,
    })
}

#[cfg(test)]
#[path = "../tests/object_frame_clipboard_packet_bounds.rs"]
mod bounds;
#[cfg(test)]
#[path = "../tests/object_frame_clipboard_packet_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_frame_clipboard_packet_literals.rs"]
mod literals;
#[cfg(test)]
#[path = "../tests/object_frame_clipboard_packet_render.rs"]
mod rendering;
#[cfg(test)]
#[path = "../tests/object_frame_clipboard_packet.rs"]
mod tests;
