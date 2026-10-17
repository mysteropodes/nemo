//! Owner join of Image placements (L06-I): the real `check_inventory` / `apply_edit` of lane 9 with the Image arm
//! lifted, against documents decoded by the real codec and the baseline holder binding of the oracle.
//!
//! Oracle rows (generator `tools/gen.js`, baseline `src/js/media-library.js` at fbe4b8a3, the VERBATIM
//! `resolveSrcLayer` and `reinsertKind` run in a Node vm):
//! `R <case> <entry> <layers> <baseline uid|-> <how uid|name|none> <native ok|layer|blank>` and
//! `K <case> <entry> <layerAlive> <reinsertKind>`.

use super::*;
use crate::object_document::media_timing::object_media_link_edit::{
    apply_edit, check_inventory, check_placement, PlacementEdit, PlacementEditError,
    PlacementOutcome, PlacementRefusal,
};
use serde_json::{json, Value};

fn one_image(target: AssetTarget) -> AssetInventory {
    AssetInventory::new(
        vec![image_record("img")],
        vec![placement("p", "img", target, 0)],
        1,
        1,
    )
    .unwrap()
}
fn layers_only(uids: &[String]) -> ObjectDocument {
    let layers: Vec<Value> = uids.iter().map(|uid| json!({"layerUid": uid})).collect();
    document(
        &json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":24,"layers":layers,"objects":[]}),
    )
}
fn place(placement_id: &str, target: AssetTarget, offset: i64) -> PlacementEdit {
    PlacementEdit::Place {
        placement_id: placement_id.to_owned(),
        asset_id: "img".to_owned(),
        target,
        offset_frames: offset,
    }
}
fn changed(outcome: Result<PlacementOutcome, PlacementEditError>) -> AssetInventory {
    match outcome {
        Ok(PlacementOutcome::Changed(next)) => next,
        other => panic!("expected a changed inventory, got {other:?}"),
    }
}

#[test]
fn the_oracle_has_the_documented_row_counts_and_classes() {
    assert_eq!(rows("P").len(), 2);
    assert_eq!(rows("R").len(), 36);
    assert_eq!(rows("K").len(), 36);
    let count =
        |column: usize, value: &str| rows("R").iter().filter(|r| r[column] == value).count();
    // how the BASELINE resolved the holder: by uid, by the name fallback, or not at all
    assert_eq!(
        (count(5, "uid"), count(5, "name"), count(5, "none")),
        (9, 6, 21)
    );
    // what the NATIVE strict holder does with the same entry
    assert_eq!(
        (count(6, "ok"), count(6, "layer"), count(6, "blank")),
        (9, 15, 12)
    );
}

#[test]
fn the_native_holder_is_the_stable_uid_while_the_baseline_also_fell_back_to_the_name() {
    for row in rows("R") {
        let (case, how, baseline, expected) = (row[1], row[5], row[4], row[6]);
        let entry: Value = serde_json::from_str(row[2]).unwrap();
        let mut uids: Vec<String> = serde_json::from_str::<Value>(row[3])
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|layer| layer["layerUid"].as_str().unwrap().to_owned())
            .collect();
        if uids.is_empty() {
            // A document needs one layer; the filler is never an entry uid.
            uids.push("Lfill".to_owned());
        }
        let doc = layers_only(&uids);
        let native = match entry["layerUid"].as_str() {
            // No uid: the entry cannot even be written as a placement.
            None => {
                assert!(
                    AssetPlacement::new("p".to_owned(), id("img"), layer_target(""), 0).is_err(),
                    "{case}"
                );
                "blank"
            }
            Some(uid) => match check_inventory(&doc, &one_image(layer_target(uid))) {
                Ok(()) => "ok",
                Err(PlacementRefusal::Layer) => "layer",
                Err(other) => panic!("{case}: {other:?}"),
            },
        };
        assert_eq!(native, expected, "{case}");
        match how {
            // Where the baseline resolved by uid, native resolves the SAME layer.
            "uid" => assert_eq!(
                (native, baseline),
                ("ok", entry["layerUid"].as_str().unwrap()),
                "{case}"
            ),
            // Where the baseline only found a layer through its name, native refuses: that is the deliberate difference.
            "name" => assert!(baseline != "-" && native != "ok", "{case}"),
            "none" => assert!(baseline == "-" && native != "ok", "{case}"),
            other => panic!("{case}: {other}"),
        }
    }
}

#[test]
fn a_baseline_image_entry_never_depends_on_its_layer_and_a_native_image_asset_outlives_its_holder()
{
    // Baseline: every image entry is re-usable whether its layer is alive or gone.
    let image_rows: Vec<_> = rows("K")
        .into_iter()
        .filter(|r| r[1].starts_with("image/"))
        .collect();
    assert_eq!(image_rows.len(), 12);
    assert!(image_rows.iter().all(|r| r[4] == "image"));
    // Native: the catalog record is independent of any placement. A layer that disappears leaves a DANGLING
    // placement (refused at admission) and the intact asset, which can be placed on a layer that exists.
    let inventory = one_image(layer_target("L"));
    let alive = layers_only(&["L".to_owned(), "M".to_owned()]);
    let gone = layers_only(&["M".to_owned()]);
    assert_eq!(check_inventory(&alive, &inventory), Ok(()));
    assert_eq!(
        check_inventory(&gone, &inventory),
        Err(PlacementRefusal::Layer)
    );
    assert!(inventory.get(&id("img")).is_some());
    assert_eq!(ids(&placements_for_layer(&inventory, "L")), ["p"]);
    let cleared = changed(apply_edit(
        &inventory,
        &gone,
        PlacementEdit::Remove {
            placement_id: "p".to_owned(),
        },
    ));
    assert!(cleared.placements().is_empty());
    assert_eq!(check_inventory(&gone, &cleared), Ok(()));
    let moved = changed(apply_edit(
        &cleared,
        &gone,
        place("p", layer_target("M"), 0),
    ));
    assert_eq!(check_inventory(&gone, &moved), Ok(()));
    assert_eq!(moved.records(), inventory.records());
}

#[test]
fn an_image_placement_resolves_context_then_layer_then_element() {
    let doc = document(&drawing());
    let at = |context: &str, layer: &str, stroke: Option<&str>| {
        check_inventory(
            &doc,
            &one_image(AssetTarget::Drawing {
                context_id: context.to_owned(),
                layer_uid: layer.to_owned(),
                stroke_id: stroke.map(str::to_owned),
            }),
        )
    };
    for (layer, stroke) in [
        ("L", None),
        ("M", None),
        ("L", Some("ink-1")),
        ("L", Some("r0")),
        ("M", Some("r1")),
    ] {
        assert_eq!(
            at("scene-root", layer, stroke),
            Ok(()),
            "{layer}/{stroke:?}"
        );
    }
    assert_eq!(at("shot-9", "L", None), Err(PlacementRefusal::Context));
    assert_eq!(
        at("shot-9", "nope", Some("ghost")),
        Err(PlacementRefusal::Context)
    );
    assert_eq!(at("scene-root", "nope", None), Err(PlacementRefusal::Layer));
    assert_eq!(
        at("scene-root", "nope", Some("ghost")),
        Err(PlacementRefusal::Layer)
    );
    for (layer, stroke) in [
        ("L", "ghost"),
        ("M", "ref-1"),
        ("M", "t0"),
        ("M", "ink-1"),
        ("L", "r1"),
    ] {
        assert_eq!(
            at("scene-root", layer, Some(stroke)),
            Err(PlacementRefusal::Element),
            "{layer}/{stroke}"
        );
    }
}

#[test]
fn an_element_level_refusal_is_not_hidden_by_an_existing_layer() {
    let doc = document(&drawing());
    // The layer exists, the raster item does not: a layer-only check would admit this placement.
    let dangling = one_image(element_target("L", "ghost"));
    assert_eq!(
        check_inventory(&doc, &dangling),
        Err(PlacementRefusal::Element)
    );
    assert_eq!(check_inventory(&doc, &one_image(layer_target("L"))), Ok(()));
    let record = dangling.placements()[0].clone();
    assert_eq!(
        check_placement(&doc, &dangling, &record),
        Err(PlacementRefusal::Element)
    );
}

#[test]
fn placing_an_image_follows_the_edit_kernel_and_refuses_what_does_not_exist() {
    let doc = document(&drawing());
    let catalog = AssetInventory::new(vec![image_record("img")], Vec::new(), 1, 0).unwrap();
    let first = changed(apply_edit(
        &catalog,
        &doc,
        place("p1", element_target("L", "r0"), 12),
    ));
    assert_eq!(first.placements().len(), 1);
    assert_eq!(first.placements()[0].offset_frames(), 12);
    // The identical placement again is a no-op; another id on the same target, or another body, conflicts.
    assert_eq!(
        apply_edit(&first, &doc, place("p1", element_target("L", "r0"), 12)),
        Ok(PlacementOutcome::Unchanged)
    );
    assert_eq!(
        apply_edit(&first, &doc, place("p1", element_target("L", "r0"), 13)),
        Err(PlacementEditError::Conflict)
    );
    assert_eq!(
        apply_edit(&first, &doc, place("p2", element_target("L", "r0"), 12)),
        Err(PlacementEditError::Conflict)
    );
    let cases = [
        (
            place("p2", element_target("L", "ghost"), 0),
            PlacementEditError::Refused(PlacementRefusal::Element),
        ),
        (
            place("p2", layer_target("nope"), 0),
            PlacementEditError::Refused(PlacementRefusal::Layer),
        ),
        (
            place("p2", element_target("M", "t0"), 0),
            PlacementEditError::Refused(PlacementRefusal::Element),
        ),
        (
            place("p2", audio_target("t1"), 0),
            PlacementEditError::KindMismatch,
        ),
        (
            place("p2", text_target("L"), 0),
            PlacementEditError::KindMismatch,
        ),
        (
            place("p2", layer_target(""), 0),
            PlacementEditError::Invalid,
        ),
    ];
    for (n, (edit, expected)) in cases.into_iter().enumerate() {
        let before = first.clone();
        assert_eq!(apply_edit(&first, &doc, edit), Err(expected), "case {n}");
        assert_eq!(first, before, "case {n} changed the input");
    }
}

#[test]
fn image_offsets_move_exactly_within_the_catalog_bound_and_removal_keeps_the_record() {
    let doc = document(&drawing());
    let max = 9_007_199_254_740_991;
    let catalog = AssetInventory::new(vec![image_record("img")], Vec::new(), 1, 0).unwrap();
    let at_max = changed(apply_edit(
        &catalog,
        &doc,
        place("p1", layer_target("L"), max),
    ));
    assert_eq!(at_max.placements()[0].offset_frames(), max);
    let shift = |delta: i64| {
        apply_edit(
            &at_max,
            &doc,
            PlacementEdit::ShiftOffset {
                placement_id: "p1".to_owned(),
                delta_frames: delta,
            },
        )
    };
    assert_eq!(shift(1), Err(PlacementEditError::Overflow));
    let down = changed(shift(-max));
    assert_eq!(down.placements()[0].offset_frames(), 0);
    let negative = changed(apply_edit(
        &down,
        &doc,
        PlacementEdit::SetOffset {
            placement_id: "p1".to_owned(),
            offset_frames: -max,
        },
    ));
    assert_eq!(negative.placements()[0].offset_frames(), -max);
    let removed = changed(apply_edit(
        &negative,
        &doc,
        PlacementEdit::Remove {
            placement_id: "p1".to_owned(),
        },
    ));
    assert!(removed.placements().is_empty());
    assert!(std::sync::Arc::ptr_eq(
        &removed.records()[0],
        &catalog.records()[0]
    ));
}
