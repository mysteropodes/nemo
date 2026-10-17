// Shared builders of the L06-H tests. This file has no module of its own: every
// test module of the leaf `include!`s it, so a test in any layer (revision,
// read-queries, commands, history) reaches the same helpers from its own subtree.
// Fully qualified paths only, so it never collides with the including module's
// imports. The oracle is `fixtures/object-raster-asset-attach/oracle.txt`
// (tools/gen.js: baseline media-library.js run in a Node vm, Node crypto digests,
// PNG bytes from the repo's frozen corpus and refusals derived by the generator).

#[allow(dead_code)]
const ATTACH_ORACLE: &str = include_str!("fixtures/object-raster-asset-attach/oracle.txt");

#[allow(dead_code)]
fn oracle_rows(kind: &str) -> Vec<serde_json::Value> {
    ATTACH_ORACLE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("oracle row is JSON"))
        .filter(|row| row["kind"] == kind)
        .collect()
}

#[allow(dead_code)]
fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len() % 2 == 0, "odd hex length");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex digit pair"))
        .collect()
}

#[allow(dead_code)]
fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The oracle row named `name` among the png and foreign rows.
#[allow(dead_code)]
fn oracle_row(name: &str) -> serde_json::Value {
    let mut rows = oracle_rows("png");
    rows.extend(oracle_rows("foreign"));
    rows.into_iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("oracle row {name}"))
}

#[allow(dead_code)]
fn oracle_bytes(name: &str) -> Vec<u8> {
    unhex(oracle_row(name)["hex"].as_str().expect("hex member"))
}

/// The four valid png rows in file order: blue, red, rgba-3x1, rgb-2x2.
#[allow(dead_code)]
fn valid_names() -> Vec<String> {
    oracle_rows("png")
        .into_iter()
        .filter(|row| row["expect"] == "new")
        .map(|row| row["name"].as_str().expect("name").to_string())
        .collect()
}

#[allow(dead_code)]
fn blank_project_file() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":21,"layers":[{"layerUid":"drawing"}],"objects":[]},
        "viewportReference":null,"assets":[]}))
    .expect("project JSON")
}

#[allow(dead_code)]
fn open_history() -> crate::history::NativeObjectHistory {
    crate::history::NativeObjectHistory::from_project_file("desktop", &blank_project_file())
        .expect("blank project history")
}

#[allow(dead_code)]
fn current_snapshot_id(owner: &crate::history::NativeObjectHistory) -> String {
    owner
        .acquire_snapshot(owner.content_revision())
        .expect("current snapshot")
        .snapshot_id()
        .to_string()
}

/// An asset command stamped with the owner's current identity and revision.
#[allow(dead_code)]
fn asset_request(
    owner: &crate::history::NativeObjectHistory,
    id: &str,
    action: &str,
    payload: serde_json::Value,
) -> crate::request_receipts::OpacityRequest {
    crate::request_receipts::OpacityRequest::history_stage(
        id,
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        format!("command.document.object.asset.{action}"),
        payload,
    )
}

#[allow(dead_code)]
fn attach_request(
    owner: &crate::history::NativeObjectHistory,
    id: &str,
    name: &str,
    media_type: &str,
) -> crate::request_receipts::OpacityRequest {
    asset_request(
        owner,
        id,
        "attach",
        serde_json::json!({"documentSnapshotId":current_snapshot_id(owner),
            "name":name,"mediaType":media_type}),
    )
}

#[allow(dead_code)]
fn remove_request(
    owner: &crate::history::NativeObjectHistory,
    id: &str,
    asset_id: &str,
) -> crate::request_receipts::OpacityRequest {
    asset_request(
        owner,
        id,
        "remove",
        serde_json::json!({"documentSnapshotId":current_snapshot_id(owner),"assetId":asset_id}),
    )
}

/// Attach the oracle png `png` as `name`; the response must be a success.
#[allow(dead_code)]
fn attach_png(
    owner: &mut crate::history::NativeObjectHistory,
    id: &str,
    name: &str,
    png: &str,
) -> crate::request_receipts::ResponseEnvelope {
    let request = attach_request(owner, id, name, "image/png");
    let response = owner
        .dispatch_asset_command(&request, Some(&oracle_bytes(png)))
        .expect("attach dispatch");
    assert!(response.is_ok(), "attach {id} refused: {response:?}");
    response
}

#[allow(dead_code)]
fn records_of(
    owner: &crate::history::NativeObjectHistory,
) -> Vec<std::sync::Arc<crate::object_document::AssetRecord>> {
    owner
        .acquire_snapshot(owner.content_revision())
        .expect("current snapshot")
        .project()
        .expect("project")
        .asset_inventory()
        .map(|inventory| inventory.records().to_vec())
        .unwrap_or_default()
}

#[allow(dead_code)]
fn names_of(owner: &crate::history::NativeObjectHistory) -> Vec<String> {
    records_of(owner)
        .iter()
        .map(|record| record.name().to_string())
        .collect()
}

#[allow(dead_code)]
fn ids_of(owner: &crate::history::NativeObjectHistory) -> Vec<String> {
    records_of(owner)
        .iter()
        .map(|record| record.id().as_str().to_string())
        .collect()
}

/// The canonical saved bytes of the owner's current revision.
#[allow(dead_code)]
fn saved_bytes(owner: &crate::history::NativeObjectHistory) -> Vec<u8> {
    owner
        .serialize_project_file(owner.content_revision(), &current_snapshot_id(owner))
        .expect("serialize")
}

/// (dispatch code, machine reason) of a refusal response.
#[allow(dead_code)]
fn refusal_of(
    response: &crate::request_receipts::ResponseEnvelope,
) -> (crate::request_receipts::DispatchErrorCode, Option<String>) {
    assert!(!response.is_ok(), "expected a refusal: {response:?}");
    let error = response.error().expect("error member");
    let reason = error
        .details
        .as_ref()
        .and_then(|details| details["reason"].as_str().map(str::to_string));
    (error.code(), reason)
}

/// A project snapshot that already holds the given `(request id, name, oracle png)` attachments.
#[allow(dead_code)]
fn snapshot_with_assets(assets: &[(&str, &str, &str)]) -> crate::revision::ObjectSnapshot {
    let mut owner = open_history();
    for (id, name, png) in assets {
        attach_png(&mut owner, id, name, png);
    }
    owner
        .acquire_snapshot(owner.content_revision())
        .expect("current snapshot")
}
