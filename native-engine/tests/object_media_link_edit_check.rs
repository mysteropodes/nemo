//! Owner-join tests for placement targets (G42, I16b): which placements the
//! document admits, and the typed reason for every refusal. The documents are
//! built from the repository's native-object example records (an authored
//! element `ink/path_A` on layer `legacy layer:alpha` and a reference-scope
//! element `0001` on layer `0`).

use super::tests::{embedded_image, id, layer, linked, placement, track};
use super::*;
use crate::object_document::AssetPlacement;
use serde_json::{json, Value};

fn plan_document(contexts: Value) -> ObjectDocument {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    serde_json::from_value(json!({
        "format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
        "objects":cases["records"],
        "compositionOrder":{
            "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":contexts,
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}}))
    .unwrap()
}
fn two_contexts() -> Value {
    json!([
        {"contextId":"scene-root","layers":[
            {"layerUid":"legacy layer:alpha","naturalPosition":0,"order":{}},
            {"layerUid":"0","naturalPosition":1,"order":{}}],"scopes":[]},
        {"contextId":"shot-2","layers":[
            {"layerUid":"0","naturalPosition":0,"order":{}}],"scopes":[]}])
}
fn at(context: &str, layer_uid: &str, stroke: Option<&str>) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: context.to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: stroke.map(str::to_owned),
    }
}
fn single(kind: AssetKind, target: AssetTarget) -> AssetInventory {
    let record = match kind {
        AssetKind::Image => embedded_image("x"),
        other => linked(other, "x"),
    };
    AssetInventory::new(vec![record], vec![placement("p", "x", target, 0)], 1, 1).unwrap()
}
fn check(
    doc: &ObjectDocument,
    kind: AssetKind,
    target: AssetTarget,
) -> Result<(), PlacementRefusal> {
    check_inventory(doc, &single(kind, target))
}

#[test]
fn video_resolves_by_context_layer_and_authored_element() {
    let doc = plan_document(two_contexts());
    for target in [
        at("scene-root", "legacy layer:alpha", None),
        at("scene-root", "legacy layer:alpha", Some("ink/path_A")),
        at("shot-2", "0", None),
        // The layer is a document layer: any listed context may name it.
        at("shot-2", "legacy layer:alpha", None),
    ] {
        assert_eq!(
            check(&doc, AssetKind::Video, target.clone()),
            Ok(()),
            "{target:?}"
        );
    }
}

#[test]
fn unresolved_context_layer_and_element_are_typed() {
    let doc = plan_document(two_contexts());
    let cases = [
        (
            at("nowhere", "legacy layer:alpha", None),
            PlacementRefusal::Context,
        ),
        (at("scene-root", "nope", None), PlacementRefusal::Layer),
        (
            at("scene-root", "legacy layer:alpha", Some("missing")),
            PlacementRefusal::Element,
        ),
        // A reference-scope record is not an authored element.
        (
            at("scene-root", "0", Some("0001")),
            PlacementRefusal::Element,
        ),
        // The element belongs to scene-root, not to shot-2.
        (
            at("shot-2", "legacy layer:alpha", Some("ink/path_A")),
            PlacementRefusal::Element,
        ),
    ];
    for (target, expected) in cases {
        assert_eq!(
            check(&doc, AssetKind::Video, target.clone()),
            Err(expected),
            "{target:?}"
        );
    }
}

#[test]
fn a_document_without_a_plan_has_only_the_scene_root_context() {
    let doc = super::tests::document();
    assert_eq!(
        check(&doc, AssetKind::Video, at("scene-root", "L", None)),
        Ok(())
    );
    assert_eq!(
        check(&doc, AssetKind::Video, at("shot-2", "L", None)),
        Err(PlacementRefusal::Context)
    );
}

#[test]
fn a_plan_replaces_the_implicit_scene_root() {
    let only_shot = json!([{"contextId":"shot-2","layers":[],"scopes":[]}]);
    let doc = plan_document(only_shot);
    assert_eq!(
        check(&doc, AssetKind::Video, at("scene-root", "0", None)),
        Err(PlacementRefusal::Context)
    );
    assert_eq!(
        check(&doc, AssetKind::Video, at("shot-2", "0", None)),
        Ok(())
    );
}

#[test]
fn kinds_without_an_owner_are_typed_refusals_before_any_target_lookup() {
    let doc = super::tests::document();
    let text = AssetTarget::Text {
        context_id: "nowhere".to_owned(),
        layer_uid: "nope".to_owned(),
        stroke_id: None,
    };
    // An image placement now looks its target up like a video does: the context is resolved first.
    assert_eq!(
        check(&doc, AssetKind::Image, at("nowhere", "nope", None)),
        Err(PlacementRefusal::Context)
    );
    assert_eq!(
        check(&doc, AssetKind::Audio, track("t1")),
        Err(PlacementRefusal::AudioTrackOwner)
    );
    assert_eq!(
        check(&doc, AssetKind::Font, text),
        Err(PlacementRefusal::FontOwner)
    );
}

#[test]
fn the_first_refusal_in_placement_order_wins() {
    let doc = super::tests::document();
    let records = vec![
        linked(AssetKind::Video, "v1"),
        linked(AssetKind::Audio, "a1"),
    ];
    let bad_layer: AssetPlacement = placement("p1", "v1", layer("nope"), 0);
    let audio: AssetPlacement = placement("p2", "a1", track("t1"), 0);
    let first = AssetInventory::new(
        records.clone(),
        vec![bad_layer.clone(), audio.clone()],
        2,
        2,
    )
    .unwrap();
    let second = AssetInventory::new(records, vec![audio, bad_layer], 2, 2).unwrap();
    assert_eq!(check_inventory(&doc, &first), Err(PlacementRefusal::Layer));
    assert_eq!(
        check_inventory(&doc, &second),
        Err(PlacementRefusal::AudioTrackOwner)
    );
}

#[test]
fn empty_and_fully_resolved_tables_pass() {
    let doc = super::tests::document();
    let records = vec![linked(AssetKind::Video, "v1")];
    let empty = AssetInventory::new(records.clone(), Vec::new(), 1, 0).unwrap();
    assert_eq!(check_inventory(&doc, &empty), Ok(()));
    let ok =
        AssetInventory::new(records, vec![placement("p1", "v1", layer("L"), -9)], 1, 1).unwrap();
    assert_eq!(check_inventory(&doc, &ok), Ok(()));
}

#[test]
fn a_placement_naming_an_asset_outside_the_catalog_is_an_asset_refusal() {
    let doc = super::tests::document();
    let inventory =
        AssetInventory::new(vec![linked(AssetKind::Video, "v1")], Vec::new(), 1, 0).unwrap();
    let stray = AssetPlacement::new("p".to_owned(), id("other"), layer("L"), 0).unwrap();
    assert_eq!(
        check_placement(&doc, &inventory, &stray),
        Err(PlacementRefusal::Asset)
    );
}

#[test]
fn refusal_messages_are_stable_and_only_lookups_are_unresolved() {
    let all = [
        (
            PlacementRefusal::Asset,
            "asset placement asset unresolved",
            true,
        ),
        (
            PlacementRefusal::ImageOwner,
            "asset image placement unavailable",
            false,
        ),
        (
            PlacementRefusal::AudioTrackOwner,
            "asset audio track unavailable",
            false,
        ),
        (
            PlacementRefusal::FontOwner,
            "asset font placement unavailable",
            false,
        ),
        (
            PlacementRefusal::Context,
            "asset placement context unresolved",
            true,
        ),
        (
            PlacementRefusal::Layer,
            "asset placement layer unresolved",
            true,
        ),
        (
            PlacementRefusal::Element,
            "asset placement element unresolved",
            true,
        ),
    ];
    for (refusal, message, unresolved) in all {
        assert_eq!(refusal.message(), message);
        assert_eq!(refusal.unresolved(), unresolved, "{message}");
    }
}
