//! Strict payload tests for the frame-clipboard commands: each command admits exactly its own fields, every malformed
//! input is `InvalidRequest`, a real packet round-trips through its wire form, and the operation-specific full-message
//! bound is paired with the unchanged native data ceiling.
use super::*;
use crate::commands::ObjectFillOwner;
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_frame_clipboard_paste_rows.rs"]
#[allow(dead_code)]
mod rows;

use build::{document, Build};
use rows::{parse_layers, record_of};

fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn parse(value: &Value) -> Result<FrameClipboardOperation, DispatchErrorCode> {
    parse_clipboard_payload(value.to_string().as_bytes())
}
fn stroke_record(id: &str, z: i64, g: i64) -> ObjectRecord {
    record_of(&build::Stroke {
        id: id.to_owned(),
        z,
        g,
    })
}
fn paste_value() -> Value {
    json!({"command":"object.frames.clipboard.paste","instanceId":"i","documentId":"d",
        "layerUid":"L1","frame":2,
        "cells":[{"relLayer":0,"relFrame":1,"keyframe":true,"records":[stroke_record("a", 1, 10)]},
                 {"relLayer":1,"relFrame":0,"keyframe":false,"records":[]}]})
}
fn duplicate_value() -> Value {
    json!({"command":"object.frames.clipboard.duplicate","layerUid":"L1",
        "selection":[{"layerUid":"L0","frame":0},{"layerUid":"L0","frame":3}]})
}

#[test]
fn each_command_admits_exactly_its_own_fields_and_builds_its_operation() {
    let paste = parse(&paste_value()).unwrap();
    let expected = FrameClipboardOperation::Paste(PasteRequest {
        instance_id: "i".into(),
        document_id: "d".into(),
        anchor_layer: "L1".into(),
        anchor_frame: 2,
        cells: vec![
            PasteCell {
                order_elements: crate::object_document::Presence::Missing,
                rel_layer: 0,
                rel_frame: 1,
                keyframe: true,
                records: vec![stroke_record("a", 1, 10)],
            },
            PasteCell {
                order_elements: crate::object_document::Presence::Missing,
                rel_layer: 1,
                rel_frame: 0,
                keyframe: false,
                records: Vec::new(),
            },
        ],
    });
    assert_eq!(paste, expected);
    let duplicate = parse(&duplicate_value()).unwrap();
    assert_eq!(
        duplicate,
        FrameClipboardOperation::Duplicate {
            selection: vec![
                CellRef {
                    layer_uid: "L0".into(),
                    frame: 0
                },
                CellRef {
                    layer_uid: "L0".into(),
                    frame: 3
                },
            ],
            anchor_layer: "L1".into(),
        }
    );
    let empty = json!({"command":"object.frames.clipboard.paste","instanceId":"i","documentId":"d",
        "layerUid":"L1","frame":0,"cells":[]});
    assert!(
        parse(&empty).is_ok(),
        "an empty clipboard is a well-formed no-op request"
    );
}

#[test]
fn malformed_payloads_refuse_as_invalid_request() {
    let paste = paste_value().to_string();
    let duplicate = duplicate_value().to_string();
    let mut raw: Vec<String> = vec![
        "null".into(),
        "[]".into(),
        r#"["object.frames.clipboard.paste"]"#.into(),
        paste.replace(r#""frame":2"#, r#""frame":2,"frame":3"#),
        paste.replace('}', r#","extra":true}"#),
        paste.replace(r#""frame":2"#, r#""frame":null"#),
        paste.replace(r#""frame":2"#, r#""frame":"2""#),
        paste.replace(r#""frame":2"#, r#""frame":2.5"#),
        paste.replace(r#""frame":2"#, r#""frame":-1"#),
        paste.replace(r#""frame":2"#, r#""frame":4294967296"#),
        paste.replace(r#""relLayer":0"#, r#""relLayer":-1"#),
        paste.replace(r#""keyframe":true"#, r#""keyframe":"yes""#),
        paste.replace(r#""instanceId":"i","#, ""),
        paste.replace(r#""instanceId":"i""#, r#""instanceId":"""#),
        paste.replace(r#""layerUid":"L1""#, r#""layerUid":"""#),
        paste.replace(
            "object.frames.clipboard.paste",
            "object.frames.clipboard.cut",
        ),
        paste.replace("object.frames.clipboard.paste", "object.layer.create"),
        paste.replace(r#""cells":["#, r#""selection":[],"cells":["#),
        duplicate.replace(r#""layerUid":"L1","#, ""),
        duplicate.replace(r#""frame":3"#, r#""frame":-3"#),
        duplicate.replace(r#""layerUid":"L0""#, r#""layerUid":"""#),
        duplicate.replace(r#""selection":["#, r#""frame":1,"selection":["#),
        duplicate.replace(r#""selection":["#, r#""cells":[],"selection":["#),
        duplicate.replace("}]", r#","note":1}]"#),
        format!("{duplicate}{}", " ".repeat(4097)),
    ];
    raw.push(String::from_utf8(vec![b' '; 4097]).unwrap());
    for text in raw {
        assert_eq!(
            parse_clipboard_payload(text.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{text}"
        );
    }
}

#[test]
fn a_real_packet_round_trips_through_the_wire_form() {
    let owner = ObjectFillOwner::new("wire", doc("K=a.1.10;.;K=b.2.20;I=m.3.30|K;.;.;K")).unwrap();
    let selection = [("L0", 0), ("L0", 1), ("L0", 3), ("L1", 0)]
        .iter()
        .map(|(layer, frame)| CellRef {
            layer_uid: (*layer).into(),
            frame: *frame,
        })
        .collect::<Vec<_>>();
    let packet = owner.copy_frame_clipboard(&selection).unwrap();
    let payload = paste_payload(&packet, "L1", 2);
    assert_eq!(payload["instanceId"], "wire");
    assert_eq!(payload["documentId"], json!(owner.document_id()));
    let cells = payload["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 4);
    // a key, a held cell (plain, no strokes), a copied in-between (a full key) and a blank key, in selection order
    let shape: Vec<(u64, u64, bool, usize)> = cells
        .iter()
        .map(|c| {
            (
                c["relLayer"].as_u64().unwrap(),
                c["relFrame"].as_u64().unwrap(),
                c["keyframe"].as_bool().unwrap(),
                c["records"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (0, 0, true, 1),
            (0, 1, false, 0),
            (0, 3, true, 1),
            (1, 0, true, 0)
        ]
    );
    assert_eq!(cells[0]["records"][0]["target"]["strokeId"], "a");
    assert_eq!(
        parse(&payload).unwrap(),
        FrameClipboardOperation::Paste(PasteRequest {
            instance_id: "wire".into(),
            document_id: owner.document_id().into(),
            anchor_layer: "L1".into(),
            anchor_frame: 2,
            cells: packet_paste_cells(&packet),
        })
    );
}

#[test]
fn clipboard_pairing_accepts_original_eight_stroke_packet_above_4096() {
    let many = "K=a.1.1,b.1.2,c.1.3,d.1.4,e.1.5,f.1.6,g.1.7,h.1.8;.;.;.";
    let owner = ObjectFillOwner::new("wire", doc(many)).unwrap();
    let packet = owner
        .copy_frame_clipboard(&[CellRef {
            layer_uid: "L0".into(),
            frame: 0,
        }])
        .unwrap();
    let payload = paste_payload(&packet, "L0", 1);
    assert!(
        payload.to_string().len() > 4096,
        "eight strokes exceed the bound"
    );
    assert!(parse(&payload).is_ok());
}

#[test]
fn ordered_packet_wire_keeps_exact_dtos_and_v1_omits_the_new_field() {
    let legacy = ObjectFillOwner::new("wire", doc("K=a.1.10;.;.;.")).unwrap();
    let selection = [CellRef {
        layer_uid: "L0".into(),
        frame: 0,
    }];
    let old = paste_payload(&legacy.copy_frame_clipboard(&selection).unwrap(), "L0", 1);
    assert!(old["cells"][0].get("orderElements").is_none());
    for form in [Build::V2, Build::V2Extras] {
        let o = ObjectFillOwner::new("wire", rows::ordered_document(form)).unwrap();
        let packet = o.copy_frame_clipboard(&selection).unwrap();
        let payload = paste_payload(&packet, "L1", 1);
        assert_eq!(
            payload["cells"][0]["orderElements"],
            rows::ordered_elements("L0")
        );
        assert_eq!(
            parse(&payload).unwrap(),
            FrameClipboardOperation::Paste(PasteRequest {
                instance_id: o.instance_id().into(),
                document_id: o.document_id().into(),
                anchor_layer: "L1".into(),
                anchor_frame: 1,
                cells: packet_paste_cells(&packet)
            })
        );
        let mut null = payload.clone();
        null["cells"][0]["orderElements"] = Value::Null;
        assert_eq!(parse(&null).unwrap_err(), DispatchErrorCode::InvalidRequest);
        let mut unknown = payload.clone();
        unknown["cells"][0]["orderElements"][0]["extra"] = json!(1);
        assert_eq!(
            parse(&unknown).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
        let raw = payload.to_string().replace(
            "\"naturalPosition\":0",
            "\"naturalPosition\":0,\"naturalPosition\":1",
        );
        assert_ne!(raw, payload.to_string());
        assert_eq!(
            parse_clipboard_payload(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
    }
}

#[path = "object_frame_clipboard_cut_wire.rs"]
mod cut;
