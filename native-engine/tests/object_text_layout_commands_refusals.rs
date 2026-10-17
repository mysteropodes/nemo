//! Typed refusals of the text transactions: each leaves revision, history depths, snapshot id and the canonical bytes
//! exactly as they were. A missing font provider is never a fallback: font assets resolve to the stable reference only.
use super::super::support::{
    bare, dispatch, drawing, layers_with_widget, legacy_record, owner, raster_item, request, rows,
    run, state, target,
};
use super::{edit, entry, seeded, ORACLE};
use crate::history::object_history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode as E;
use serde_json::{json, Value};

fn record() -> Value {
    rows(ORACLE, Some("EDIT"))[0]["before"].clone()
}

/// L0 holds text `t0` and raster `r0`; the locked L1 holds text `tl`; L2 holds the legacy path record `v0`; W is a widget layer.
fn world() -> NativeObjectHistory {
    owner(&drawing(json!({
        "layers":layers_with_widget(),
        "objects":[legacy_record("L2","v0")],
        "items":{"raster":[{"target":target("L0","r0"),"item":raster_item("img-1",1,2,3,4)}],
                 "text":[{"target":target("L0","t0"),"block":record()},
                         {"target":target("L1","tl"),"block":record()}]}
    })))
}

fn create_at(target: Value, block: Value) -> Value {
    json!({"command":"object.text.create","target":target,"block":block})
}

fn with(member: &str, value: Value) -> Value {
    let mut block = record();
    block[member] = value;
    block
}

#[test]
fn every_refusal_has_its_typed_code_and_publishes_nothing() {
    let t = |layer: &str, stroke: &str| target(layer, stroke);
    let mut frame8 = t("L0", "");
    frame8["frameScope"]["frame"] = json!(8);
    let mut reference = t("L0", "");
    reference["frameScope"]["kind"] = json!("reference");
    let asset = |id: &str| json!({"kind":"asset","assetId":id});
    let cases: Vec<(&str, Value, E)> = vec![
        (
            "create on the locked layer",
            create_at(t("L1", ""), record()),
            E::Unavailable,
        ),
        (
            "create on an unknown layer",
            create_at(t("L9", ""), record()),
            E::NotFound,
        ),
        (
            "create on a widget layer",
            create_at(t("W", ""), record()),
            E::InvalidRequest,
        ),
        (
            "create past the last frame",
            create_at(frame8, record()),
            E::InvalidRequest,
        ),
        (
            "create in a reference scope",
            create_at(reference, record()),
            E::Unavailable,
        ),
        (
            "create with a caller chosen stroke id",
            create_at(t("L0", "mine"), record()),
            E::InvalidRequest,
        ),
        (
            "create over a missing font asset",
            create_at(t("L0", ""), with("font", asset("nope"))),
            E::NotFound,
        ),
        (
            "create over an image as font",
            create_at(t("L0", ""), with("font", asset("img-1"))),
            E::InvalidRequest,
        ),
        (
            "create with blank text",
            create_at(t("L0", ""), with("text", json!("  \t\n "))),
            E::InvalidRequest,
        ),
        (
            "create with empty text",
            create_at(t("L0", ""), with("text", json!(""))),
            E::InvalidRequest,
        ),
        (
            "create with a null size",
            create_at(t("L0", ""), with("size", Value::Null)),
            E::InvalidRequest,
        ),
        (
            "create with a zero size",
            create_at(t("L0", ""), with("size", json!(0))),
            E::InvalidRequest,
        ),
        (
            "create with a justify align",
            create_at(t("L0", ""), with("align", json!("justify"))),
            E::InvalidRequest,
        ),
        (
            "create with a negative box",
            create_at(t("L0", ""), with("fixedWidth", json!(-5))),
            E::InvalidRequest,
        ),
        (
            "create with a stored overflow flag",
            create_at(t("L0", ""), with("textOverflow", json!(true))),
            E::InvalidRequest,
        ),
        (
            "create with an unknown bundled font",
            create_at(
                t("L0", ""),
                with("font", json!({"kind":"bundled","fontKey":"Comic"})),
            ),
            E::InvalidRequest,
        ),
        (
            "create with a raster member",
            json!({"command":"object.text.create","target":t("L0",""),"block":record(),"item":raster_item("img-1",1,2,3,4)}),
            E::InvalidRequest,
        ),
        (
            "create without the block",
            json!({"command":"object.text.create","target":t("L0","")}),
            E::InvalidRequest,
        ),
        (
            "edit an unknown stroke",
            edit("nope", json!({"size":20})),
            E::NotFound,
        ),
        (
            "edit on the locked layer",
            json!({"command":"object.text.edit","stableTarget":t("L1","tl"),"patch":{"size":20}}),
            E::Unavailable,
        ),
        (
            "edit a raster item",
            edit("r0", json!({"size":20})),
            E::Unavailable,
        ),
        (
            "edit a path record",
            json!({"command":"object.text.edit","stableTarget":t("L2","v0"),"patch":{"size":20}}),
            E::Unavailable,
        ),
        (
            "edit with an empty patch",
            edit("t0", json!({})),
            E::InvalidRequest,
        ),
        (
            "edit to blank text",
            edit("t0", json!({"text":" \n"})),
            E::InvalidRequest,
        ),
        (
            "edit to a zero size",
            edit("t0", json!({"size":0})),
            E::InvalidRequest,
        ),
        (
            "edit to a bad color",
            edit("t0", json!({"color":"red"})),
            E::InvalidRequest,
        ),
        (
            "edit to an unknown case",
            edit("t0", json!({"textCase":"weird"})),
            E::InvalidRequest,
        ),
        (
            "edit to a zero line height",
            edit("t0", json!({"lineHeightMult":0})),
            E::InvalidRequest,
        ),
        (
            "edit to a zero box",
            edit("t0", json!({"fixedHeight":0})),
            E::InvalidRequest,
        ),
        (
            "edit to a missing font asset",
            edit("t0", json!({"font":asset("nope")})),
            E::NotFound,
        ),
        (
            "edit to an image as font",
            edit("t0", json!({"font":asset("img-1")})),
            E::InvalidRequest,
        ),
        (
            "edit with a null text",
            edit("t0", json!({"text":null})),
            E::InvalidRequest,
        ),
        (
            "edit without the patch",
            json!({"command":"object.text.edit","stableTarget":t("L0","t0")}),
            E::InvalidRequest,
        ),
        (
            "edit with a block member",
            json!({"command":"object.text.edit","stableTarget":t("L0","t0"),"patch":{"size":20},"block":record()}),
            E::InvalidRequest,
        ),
        (
            "delete an unknown stroke",
            json!({"command":"object.text.delete","stableTarget":t("L0","nope")}),
            E::NotFound,
        ),
        (
            "delete on the locked layer",
            json!({"command":"object.text.delete","stableTarget":t("L1","tl")}),
            E::Unavailable,
        ),
        (
            "delete a raster item",
            json!({"command":"object.text.delete","stableTarget":t("L0","r0")}),
            E::Unavailable,
        ),
        (
            "delete with a patch",
            json!({"command":"object.text.delete","stableTarget":t("L0","t0"),"patch":{}}),
            E::InvalidRequest,
        ),
        (
            "an unknown text command",
            json!({"command":"object.text.move","stableTarget":t("L0","t0")}),
            E::InvalidRequest,
        ),
    ];
    assert_eq!(cases.len(), 39);
    for (label, payload, code) in cases {
        let mut o = world();
        let before = state(&o);
        assert_eq!(run(&mut o, &payload), Err(code), "{label}");
        assert_eq!(
            state(&o),
            before,
            "{label}: nothing may be partially published"
        );
    }
}

#[test]
fn font_assets_resolve_only_to_the_stable_reference_and_a_bare_drawing_has_no_assets() {
    let mut o = owner(&drawing(json!({})));
    let font = with("font", json!({"kind":"asset","assetId":"font-1"}));
    assert_eq!(
        run(&mut o, &create_at(target("L0", ""), font.clone())),
        Ok(1)
    );
    assert_eq!(
        entry(&o).1.font().layout_key(),
        "font-1",
        "the reference is kept, no fallback face is substituted"
    );
    let mut b = bare(&drawing(json!({})));
    let before = state(&b);
    assert_eq!(
        run(&mut b, &create_at(target("L0", ""), font)),
        Err(E::Unavailable)
    );
    assert_eq!(state(&b), before);
    // A bundled font needs no inventory at all: it works in a bare drawing too.
    assert_eq!(run(&mut b, &create_at(target("L0", ""), record())), Ok(1));
    assert_eq!(entry(&b).0.stroke_id(), "native-text:1:0");
}

#[test]
fn the_existing_ingress_bound_is_unchanged_a_long_text_is_refused_not_truncated() {
    let mut o = owner(&drawing(json!({})));
    let fits = create_at(target("L0", ""), with("text", json!("a".repeat(3000))));
    let ok = request(&o, "long-ok", &fits);
    assert_eq!(dispatch(&mut o, &ok)["ok"], true);
    assert_eq!(entry(&o).1.text().len(), 3000);
    let before = state(&o);
    let too_long = create_at(target("L0", ""), with("text", json!("a".repeat(4200))));
    assert_eq!(run(&mut o, &too_long), Err(E::InvalidRequest));
    let big = request(&o, "long-big", &too_long);
    assert_eq!(dispatch(&mut o, &big)["error"]["code"], "invalid_request");
    assert_eq!(state(&o), before);
}

#[test]
fn a_holder_that_cannot_be_a_glyph_holder_is_refused() {
    let long_layer = format!("L{}", "x".repeat(250));
    let mut doc = drawing(json!({}));
    doc["layers"] = json!([{"layerUid":long_layer}]);
    let mut o = owner(&doc);
    let before = state(&o);
    assert_eq!(
        run(&mut o, &create_at(target(&long_layer, ""), record())),
        Err(E::InvalidRequest)
    );
    assert_eq!(state(&o), before);
}

#[test]
fn the_baseline_guard_a_blank_edit_changes_nothing_holds_for_the_native_edit() {
    for row in rows(ORACLE, Some("GUARD")) {
        let name = row["name"].as_str().unwrap();
        let mut o = seeded(row["before"].clone());
        let before = state(&o);
        // The baseline `if(!text.trim())return;` rebuilds nothing; the native edit refuses and publishes nothing.
        assert_eq!(
            run(&mut o, &edit("t0", json!({"text":row["text"]}))),
            Err(E::InvalidRequest),
            "{name}"
        );
        assert_eq!(state(&o), before, "{name}");
    }
    assert_eq!(rows(ORACLE, Some("GUARD")).len(), 2);
}

#[test]
fn text_edits_retain_their_dispatch_receipts_like_every_object_operation() {
    let mut o = world();
    let r = request(&o, "text-1", &edit("t0", json!({"size":20})));
    let first = dispatch(&mut o, &r);
    assert_eq!(first["ok"], true);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1,"operation":"text.edit","strokeId":"t0"})
    );
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &r), first);
    assert_eq!(state(&o), after);
    let noop = request(&o, "text-2", &edit("t0", json!({"size":20.0})));
    let reply = dispatch(&mut o, &noop);
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(state(&o), after);
    let mut stale = request(&o, "text-3", &edit("t0", json!({"size":30})));
    stale.expected_revision = Some(0);
    assert_eq!(dispatch(&mut o, &stale)["error"]["code"], "stale_revision");
    assert_eq!(state(&o), after);
}
