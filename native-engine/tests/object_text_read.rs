//! Actual native owner transactions and immutable reads, not a JavaScript owner imitation.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};
#[path = "object_raster_asset_commands_support.rs"]
mod support;
const OP: &str = "query.document.object.text";
fn block(text: &str) -> Value {
    json!({"text":text,"font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}})
}
fn scope() -> Value {
    json!({"contextId":"scene-root","layerUid":"L0","frameScope":{"kind":"authored","frame":0}})
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    support::head(o)
}
fn request(p: &ObjectSnapshot, view: Value) -> OpacityRequest {
    OpacityRequest::query(
        "text-read",
        p.instance_id(),
        p.document_id(),
        OP,
        json!({"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),"view":view}),
    )
}
fn read(p: &ObjectSnapshot, view: Value) -> Value {
    let r = p.dispatch(request(p, view)).unwrap();
    assert!(r.ok, "{:?}", r.error);
    assert!(serde_json::to_vec(&r).unwrap().len() <= 4096);
    r.result.unwrap()
}
fn inspect(p: &ObjectSnapshot, id: &str) -> Value {
    read(
        p,
        json!({"kind":"inspect","target":support::target("L0",id)}),
    )
}
fn text(p: &ObjectSnapshot, id: &str) -> String {
    let mut offset = 0;
    let mut out = String::new();
    loop {
        let r = read(
            p,
            json!({"kind":"text","target":support::target("L0",id),"offset":offset,"limit":128}),
        );
        let chunk = r["text"].as_str().unwrap();
        out.push_str(chunk);
        assert_eq!(r["offset"], offset);
        assert!(chunk.len() <= 128);
        match r["nextOffset"].as_u64() {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => {
                assert_eq!(out.len() as u64, r["textBytes"]);
                break;
            }
        }
    }
    out
}
fn seeded(rows: Vec<Value>) -> NativeObjectHistory {
    support::bare(&support::drawing(json!({"items":{"text":rows}})))
}
#[test]
fn created_native_identity_and_presence_survive_edit_undo_redo_and_storage_reopen() {
    let mut o = support::bare(&support::drawing(json!({})));
    support::run(&mut o,&json!({"command":"object.text.create","target":support::target("L0",""),"block":block("Hello 雪")})).unwrap();
    let id = "native-text:1:0";
    let initial = pin(&o);
    let before = inspect(&initial, id);
    assert_eq!(before["target"], support::target("L0", id));
    assert_eq!(text(&initial, id), "Hello 雪");
    assert!(before["block"].get("text").is_none());
    assert!(before["block"].get("fixedWidth").is_none());
    let page = read(
        &initial,
        json!({"kind":"targets","scope":scope(),"offset":0,"limit":8}),
    );
    assert_eq!(page["items"], json!([{"target":support::target("L0",id)}]));
    assert_eq!(page["nextOffset"], Value::Null);
    // Absent -> null is an effective no-op in the existing patch contract.
    // Exercise a real authored width -> explicit null transition instead.
    assert_eq!(
        support::run(
            &mut o,
            &json!({"command":"object.text.edit", "stableTarget":support::target("L0",id), "patch":{"fixedWidth":128}})
        ),
        Ok(2)
    );
    assert_eq!(inspect(&pin(&o), id)["block"]["fixedWidth"], 128);
    support::run(&mut o,&json!({"command":"object.text.edit","stableTarget":support::target("L0",id),
        "patch":{"text":"Changed","font":{"kind":"bundled","fontKey":"Roboto-Bold"},"fixedWidth":null}})).unwrap();
    let after = inspect(&pin(&o), id);
    assert_eq!(after["block"]["font"]["fontKey"], "Roboto-Bold");
    assert!(after["block"].get("fixedWidth").unwrap().is_null());
    assert_eq!(
        after["block"]["anchorTopLeft"],
        before["block"]["anchorTopLeft"]
    );
    assert_eq!(o.undo(3), Ok(4));
    assert_eq!(text(&pin(&o), id), "Hello 雪");
    assert_eq!(o.redo(4), Ok(5));
    assert_eq!(text(&pin(&o), id), "Changed");
    let stored = encode_storage_project(pin(&o).document()).unwrap();
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&stored).unwrap()).unwrap();
    assert_eq!(
        inspect(&pin(&reopened), id)["target"],
        support::target("L0", id)
    );
    assert_eq!(inspect(&pin(&reopened), id)["block"], after["block"]);
    assert_eq!(text(&pin(&reopened), id), "Changed");
    assert_eq!(
        text(&initial, id),
        "Hello 雪",
        "retained snapshot immutable"
    );
}
#[test]
fn pages_are_exact_native_targets_without_cross_layer_or_frame_inference() {
    let rows: Vec<_> = (0..11)
        .map(|i| json!({"target":support::target("L0",&format!("text-{i}")),"block":block("A")}))
        .collect();
    let o = seeded(rows);
    let p = pin(&o);
    let before = support::state(&o);
    let a = read(
        &p,
        json!({"kind":"targets","scope":scope(),"offset":0,"limit":8}),
    );
    let b = read(
        &p,
        json!({"kind":"targets","scope":scope(),"offset":a["nextOffset"],"limit":8}),
    );
    assert_eq!(a["total"], 11);
    assert_eq!(a["items"].as_array().unwrap().len(), 8);
    assert_eq!(b["items"].as_array().unwrap().len(), 3);
    assert_eq!(b["nextOffset"], Value::Null);
    assert_eq!(a["items"][0]["target"], support::target("L0", "text-0"));
    let mut other = scope();
    other["frameScope"]["frame"] = json!(1);
    assert_eq!(
        read(
            &p,
            json!({"kind":"targets","scope":other,"offset":0,"limit":8})
        )["items"],
        json!([])
    );
    assert_eq!(
        support::state(&o),
        before,
        "query has no history/document effects"
    );
}
#[test]
fn unicode_and_json_escaping_are_lossless_across_bounded_text_chunks() {
    let original = "\"\\\n雪🦊".repeat(170);
    let o = seeded(vec![
        json!({"target":support::target("L0","text"),"block":block(&original)}),
    ]);
    let p = pin(&o);
    assert_eq!(text(&p, "text"), original);
    let mut r = request(
        &p,
        json!({"kind":"text","target":support::target("L0","text"),"offset":4,"limit":128}),
    );
    // Offset 4 is the second UTF-8 byte of 雪, not an authored-text character boundary.
    assert_eq!(
        p.dispatch(r.clone()).unwrap().error.unwrap().code,
        DispatchErrorCode::InvalidRequest
    );
    r.payload["view"]["offset"] = json!(0);
    r.payload["view"]["limit"] = json!(1025);
    assert_eq!(
        p.dispatch(r).unwrap().error.unwrap().code,
        DispatchErrorCode::InvalidRequest
    );
}
#[test]
fn owner_revision_snapshot_context_target_and_closed_shape_refusals_do_not_mutate() {
    let o = seeded(vec![
        json!({"target":support::target("L0","t"),"block":block("Hi")}),
    ]);
    let p = pin(&o);
    let before = support::state(&o);
    let good = request(
        &p,
        json!({"kind":"inspect","target":support::target("L0","t")}),
    );
    for (field, value, code) in [
        (
            "instanceId",
            json!("foreign"),
            DispatchErrorCode::WrongInstance,
        ),
        (
            "documentId",
            json!("foreign"),
            DispatchErrorCode::WrongDocument,
        ),
    ] {
        let mut v = serde_json::to_value(&good).unwrap();
        v[field] = value;
        let r: OpacityRequest = serde_json::from_value(v).unwrap();
        assert_eq!(p.dispatch(r).unwrap().error.unwrap().code, code);
    }
    for (field, value) in [
        ("atRevision", json!(1)),
        ("documentSnapshotId", json!("stale")),
    ] {
        let mut r = good.clone();
        r.payload[field] = value;
        assert_eq!(
            p.dispatch(r).unwrap().error.unwrap().code,
            DispatchErrorCode::StaleRevision
        );
    }
    let mut r = good.clone();
    r.payload["view"]["target"]["strokeId"] = json!("missing");
    assert_eq!(
        p.dispatch(r).unwrap().error.unwrap().code,
        DispatchErrorCode::NotFound
    );
    let mut r = good.clone();
    r.payload["view"]["target"]["contextId"] = json!("component");
    assert_eq!(
        p.dispatch(r).unwrap().error.unwrap().code,
        DispatchErrorCode::Unavailable
    );
    for view in [
        json!({"kind":"inspect","target":support::target("L0","t"),"extra":1}),
        json!(["inspect", support::target("L0", "t")]),
        json!({"kind":"targets","scope":scope(),"offset":0,"limit":0}),
    ] {
        assert_eq!(
            p.dispatch(request(&p, view)).unwrap().error.unwrap().code,
            DispatchErrorCode::InvalidRequest
        );
    }
    let mut selector=json!({"instanceId":p.instance_id(),"documentId":p.document_id(),"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),"view":good.payload["view"]}).to_string();
    selector = selector.replacen(
        "\"kind\":\"inspect\"",
        "\"kind\":\"inspect\",\"kind\":\"inspect\"",
        1,
    );
    assert_eq!(
        p.query_text_json(selector.as_bytes()).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(support::state(&o), before);
}
#[test]
fn escaped_chunks_shrink_with_explicit_progress_instead_of_breaking_the_reply_cap() {
    let authored = "\n\t\"\\".repeat(1300);
    let o = seeded(vec![
        json!({"target":support::target("L0","t"),"block":block(&authored)}),
    ]);
    let p = pin(&o);
    let mut offset = 0;
    let mut joined = String::new();
    let mut count = 0;
    loop {
        let r = read(
            &p,
            json!({"kind":"text","target":support::target("L0","t"),"offset":offset,"limit":1024}),
        );
        joined.push_str(r["text"].as_str().unwrap());
        count += 1;
        match r["nextOffset"].as_u64() {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(joined, authored);
    assert!(count > 1);
}
#[cfg(feature = "font_asset_wire")]
#[test]
fn document_asset_font_identity_and_actual_encoded_font_survive_bind_undo_redo_and_project_reopen()
{
    use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource};
    use std::sync::Arc;
    let font_bytes = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
    let font = Arc::new(
        AssetRecord::new(
            AssetId::new("document-font".into()).unwrap(),
            "Roboto".into(),
            AssetKind::Font,
            AssetSource::admitted_embedded(
                "font/ttf".into(),
                Arc::new(font_bytes.to_vec()),
                1 << 20,
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let inventory = AssetInventory::new(vec![font], vec![], 1024, 0).unwrap();
    // A MIME label is not native font admission: the actual owner must refuse it.
    let invalid = NativeObjectHistory::from_admitted_project(
        "unadmitted-font-text",
        support::project(&support::drawing(json!({})), inventory),
    );
    assert_eq!(invalid.unwrap_err(), "asset media codec unavailable");
    let mut o = NativeObjectHistory::from_admitted_project(
        "font-text",
        support::project(
            &support::drawing(json!({})),
            AssetInventory::new(vec![], vec![], 1024, 0).unwrap(),
        ),
    )
    .unwrap();
    support::run(&mut o,&json!({"command":"object.text.create","target":support::target("L0",""),"block":block("Font")})).unwrap();
    let id = "native-text:1:0";
    let mut request = OpacityRequest::command(
        "text-font-import",
        o.instance_id(),
        o.document_id(),
        o.content_revision(),
        json!({"command":"object.asset.font.import", "documentSnapshotId":pin(&o).snapshot_id(),
            "lifecycleGeneration":1, "source":{"kind":"bundled", "fontKey":"Roboto-Regular"}}),
    );
    request.operation = "command.document.object.asset.font.import".into();
    assert!(o.font_import_status(&request).unwrap().is_none());
    let imported = o.dispatch_font_import(&request, font_bytes).unwrap();
    assert!(imported.ok, "{:?}", imported.error);
    let result = imported.result.unwrap();
    assert_eq!(result["historyEntriesAdded"], 1);
    assert_eq!(result["encodedBytes"], font_bytes.len());
    let asset_id = result["assetId"].as_str().unwrap().to_owned();
    assert_eq!(o.content_revision(), 2);
    let admitted = pin(&o);
    let stored_font = admitted
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .records()
        .iter()
        .find(|record| record.id().as_str() == asset_id)
        .unwrap();
    assert!(stored_font.source().has_font_admission());
    assert_eq!(
        stored_font
            .source()
            .encoded_allocation()
            .unwrap()
            .as_slice(),
        font_bytes
    );
    support::run(&mut o,&json!({"command":"object.text.edit","stableTarget":support::target("L0",id),"patch":{"font":{"kind":"asset","assetId":asset_id}}})).unwrap();
    assert_eq!(
        inspect(&pin(&o), id)["block"]["font"],
        json!({"kind":"asset","assetId":asset_id})
    );
    o.undo(3).unwrap();
    assert_eq!(inspect(&pin(&o), id)["block"]["font"]["kind"], "bundled");
    o.redo(4).unwrap();
    let h = pin(&o);
    let file = o.serialize_project_file(5, h.snapshot_id()).unwrap();
    let reopened = NativeObjectHistory::from_project_file("font-text-reopen", &file).unwrap();
    assert_eq!(
        inspect(&pin(&reopened), id)["block"],
        inspect(&h, id)["block"]
    );
    assert_eq!(
        reopened
            .serialize_project_file(0, pin(&reopened).snapshot_id())
            .unwrap(),
        file,
        "actual project contains identical font bytes and references"
    );
}

#[test]
fn a_component_context_text_block_reads_through_its_own_layers_and_frames() {
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let target = json!({"contextId":"comp","layerUid":"inner","strokeId":"words",
        "frameScope":{"kind":"authored","frame":1}});
    let o = support::bare(&json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":2,
        "layers":[{"layerUid":"host","timing":timing}],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
            "layers":[{"layerUid":"host","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[]}]}]},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"comp","totalFrames":4,"layers":[{"layerUid":"inner","timing":timing}],"objects":[]}],
            "instances":[{"contextId":"scene-root","layerUid":"host","symbolId":"comp"}]}},
        "items":{"text":[{"target":target,"block":block("Inside")}]}}));
    let p = pin(&o);
    // Frame 1 exists only in the component (four frames) and the layer only inside it.
    let inspected = read(&p, json!({"kind":"inspect","target":target}));
    assert_eq!(inspected["kind"], "inspect", "{inspected}");
    let text = read(&p, json!({"kind":"text","target":target,"offset":0,"limit":128}));
    assert_eq!(text["text"], "Inside");
    let scope = json!({"contextId":"comp","layerUid":"inner","frameScope":{"kind":"authored","frame":1}});
    let page = read(&p, json!({"kind":"targets","scope":scope,"offset":0,"limit":8}));
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1), "{page}");
    let code = |target: Value| {
        p.dispatch(request(&p, json!({"kind":"inspect","target":target})))
            .unwrap()
            .error
            .unwrap()
            .code
    };
    let mut beyond = target.clone();
    beyond["frameScope"]["frame"] = json!(4);
    assert_eq!(code(beyond), DispatchErrorCode::InvalidRequest);
    let mut scene_layer = target.clone();
    scene_layer["layerUid"] = json!("host");
    assert_eq!(code(scene_layer), DispatchErrorCode::NotFound);
    let mut undeclared = target;
    undeclared["contextId"] = json!("other");
    assert_eq!(code(undeclared), DispatchErrorCode::Unavailable);
}
