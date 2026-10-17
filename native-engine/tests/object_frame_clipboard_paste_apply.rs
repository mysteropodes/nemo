//! Running one oracle row through the REAL history owner, as the operation the baseline function stands for: a typed
//! paste, a typed cut (returns the packet) or a dispatched duplicate selection; plus the deliberate mistakes the
//! negative controls feed through the same path. Support only (no tests).
use super::build::uid;
use super::rows::{Row, OP_DUPLICATE};
use super::{dispatch, request};
use crate::commands::{FrameClipboardOperation, FrameClipboardPacket, PasteRequest};
use crate::history::NativeObjectHistory;
use serde_json::json;

/// One deliberate mistake a faulty transaction could make, expressed as the request it would amount to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mistake {
    None,
    InBetweenAsPlain,
    SkipPlain,
    KeysWithoutStrokes,
    CutForgetsLast,
    DupAtMinLayer,
    DupOverlapping,
}

fn code(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

/// Run one row through the REAL history owner, as the operation the baseline function stands for: a typed paste, a
/// typed cut (returns the packet), or a dispatched duplicate. Returns the cut packet.
pub(crate) fn apply(
    row: &Row,
    o: &mut NativeObjectHistory,
    mistake: Mistake,
) -> Result<Option<FrameClipboardPacket>, String> {
    match row.op.as_str() {
        "paste" => {
            let mut cells = row.paste_cells();
            match mistake {
                Mistake::InBetweenAsPlain => {
                    for (cell, entry) in cells.iter_mut().zip(&row.clip) {
                        if "IMN".contains(entry.cell.flag) {
                            cell.keyframe = false;
                            cell.records.clear();
                        }
                    }
                }
                Mistake::SkipPlain => cells.retain(|cell| cell.keyframe),
                Mistake::KeysWithoutStrokes => {
                    for cell in cells.iter_mut() {
                        cell.records.clear();
                    }
                }
                _ => {}
            }
            let (layer, frame) = row.paste_base();
            let paste = PasteRequest {
                instance_id: o.instance_id().to_owned(),
                document_id: o.document_id().to_owned(),
                anchor_layer: uid(layer),
                anchor_frame: frame,
                cells,
            };
            let candidate = o
                .prepare_frame_clipboard(&FrameClipboardOperation::Paste(paste))
                .map_err(code)?;
            o.commit_frame_clipboard(candidate).map_err(code)?;
            Ok(None)
        }
        "cut" => {
            let mut selection = row.cell_refs();
            if mistake == Mistake::CutForgetsLast {
                selection.pop();
            }
            o.cut_frame_cells(&selection).map_err(code)
        }
        "dupsel" if mistake == Mistake::DupOverlapping => {
            let packet = o.copy_frame_clipboard(&row.cell_refs()).map_err(code)?;
            let (_, first) = packet.origin();
            let (_, span) = packet.extent();
            let paste = PasteRequest {
                instance_id: o.instance_id().to_owned(),
                document_id: o.document_id().to_owned(),
                anchor_layer: uid(row.dup_anchor()),
                anchor_frame: first + span - 1,
                cells: crate::commands::packet_paste_cells(&packet),
            };
            let candidate = o
                .prepare_frame_clipboard(&FrameClipboardOperation::Paste(paste))
                .map_err(code)?;
            o.commit_frame_clipboard(candidate).map_err(code)?;
            Ok(None)
        }
        "dupsel" => {
            let layer = if mistake == Mistake::DupAtMinLayer {
                row.min_selected_layer()
            } else {
                row.dup_anchor()
            };
            let selection: Vec<serde_json::Value> = row
                .selection()
                .iter()
                .map(|(l, f)| json!({"layerUid":uid(*l),"frame":f}))
                .collect();
            let payload = json!({"command":"object.frames.clipboard.duplicate",
                "layerUid":uid(layer),"selection":selection});
            let submitted = request(o, "row", OP_DUPLICATE, payload);
            let reply = dispatch(o, &submitted);
            if reply["ok"] == true {
                Ok(None)
            } else {
                Err(reply.to_string())
            }
        }
        other => panic!("unknown op {other}"),
    }
}
