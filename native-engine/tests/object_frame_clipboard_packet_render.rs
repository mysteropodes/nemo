//! Rendering a real packet back to the oracle tokens, plus the rendering mutants the controls use.
use super::tests::Row;
use super::{CellKind, FrameClipboardPacket, PacketCell};

pub(crate) fn ids_of(records: &[crate::object_document::ObjectRecord]) -> String {
    records
        .iter()
        .map(|record| record.target().stroke_id().to_owned())
        .collect::<Vec<_>>()
        .join(".")
}

pub(crate) fn content_token(cell: &PacketCell) -> String {
    match cell.kind() {
        CellKind::Key => format!("K{}", ids_of(cell.records())),
        CellKind::BlankKey => "B".to_owned(),
        CellKind::Interpolated => format!(
            "{}{}",
            if cell.manual_edit() { 'M' } else { 'T' },
            ids_of(cell.records())
        ),
        CellKind::Held | CellKind::Unexposed => "H".to_owned(),
    }
}

pub(crate) fn paste_token(cell: &PacketCell) -> String {
    if !cell.paste_is_keyframe() {
        return "H".to_owned();
    }
    let list = ids_of(cell.paste_records());
    if list.is_empty() {
        "B".to_owned()
    } else {
        format!("K{list}")
    }
}

pub(crate) fn class_token(cell: &PacketCell) -> String {
    match cell.kind() {
        CellKind::Key => "K".to_owned(),
        CellKind::BlankKey => "B".to_owned(),
        CellKind::Interpolated => (if cell.manual_edit() { "M" } else { "T" }).to_owned(),
        CellKind::Unexposed => "U".to_owned(),
        CellKind::Held => {
            let list = ids_of(cell.records());
            format!(
                "H{}={}",
                cell.source_frame().expect("a held cell shows a key"),
                if list.is_empty() { "-" } else { list.as_str() }
            )
        }
    }
}

/// One switch on the rendering of the REAL packet; each mutant must disagree with the oracle on
/// exactly the rows the generator counted (controls file).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mutant {
    None,
    TweenKept,
    ManualKept,
    HeldDropped,
    BlankAsHold,
    AbsoluteOffsets,
    SortedOrder,
    HeldPastesSource,
    UnexposedAsHeld,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Rendered {
    pub(crate) clip: Vec<(usize, u32, String)>,
    pub(crate) pasted: Vec<String>,
    pub(crate) classes: Vec<String>,
}

pub(crate) fn render(packet: &FrameClipboardPacket, mutant: Mutant) -> Rendered {
    let (origin_layer, origin_frame) = packet.origin();
    let mut cells: Vec<&PacketCell> = packet.cells().iter().collect();
    if mutant == Mutant::SortedOrder {
        cells.sort_by_key(|cell| (cell.rel_layer(), cell.rel_frame()));
    }
    let mut out = Rendered {
        clip: Vec::new(),
        pasted: Vec::new(),
        classes: Vec::new(),
    };
    for cell in cells {
        let kind = cell.kind();
        let (rl, rf) = if mutant == Mutant::AbsoluteOffsets {
            (
                cell.rel_layer() + origin_layer,
                cell.rel_frame() + origin_frame,
            )
        } else {
            (cell.rel_layer(), cell.rel_frame())
        };
        let mut content = content_token(cell);
        let mut paste = paste_token(cell);
        let mut class = class_token(cell);
        match mutant {
            Mutant::TweenKept if kind == CellKind::Interpolated => paste = content.clone(),
            Mutant::ManualKept if kind == CellKind::Interpolated && cell.manual_edit() => {
                paste = content.clone()
            }
            Mutant::HeldDropped if kind == CellKind::Held => class = "U".to_owned(),
            Mutant::BlankAsHold if kind == CellKind::BlankKey => {
                content = "H".to_owned();
                paste = "H".to_owned();
            }
            Mutant::HeldPastesSource if kind == CellKind::Held => {
                let list = ids_of(cell.records());
                paste = if list.is_empty() {
                    "B".to_owned()
                } else {
                    format!("K{list}")
                };
            }
            Mutant::UnexposedAsHeld if kind == CellKind::Unexposed => class = "H0=-".to_owned(),
            _ => {}
        }
        out.clip.push((rl, rf, content));
        out.pasted.push(paste);
        out.classes.push(class);
    }
    out
}

pub(crate) fn expected(row: &Row) -> Rendered {
    Rendered {
        clip: row.clip.clone(),
        pasted: row.pasted.clone(),
        classes: row.classes.clone(),
    }
}
