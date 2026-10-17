//! L06-L text-block transactions (G38a): create / edit / delete as ONE sealed typed transaction each. Expected
//! values are rows of the baseline oracle (the REAL `buildVectorTextGroup` rebuild of `applyTextPropsEdit`, fbe4b8a3)
//! or literals pinned next to the case. Glyph layouts come from the accepted L06-A kernel over the L06-F stub faces.
use super::support::{
    bytes, drawing, head, items_of, owner, prepare, raster_item, rows, run, state, target,
};
use crate::history::object_history::NativeObjectHistory;
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::LaidOutText;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::oracle::{
    catalog, native_glyphs,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::pin::{
    pins_of, GlyphPin,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::{text_holder_id, ObjectTarget};
use serde_json::{json, Value};

#[path = "object_text_layout_commands_refusals.rs"]
mod refusals;

pub(super) const ORACLE: &str = include_str!("fixtures/object-text-layout-commands/oracle.txt");

pub(super) fn block_of(record: &Value) -> TextBlockRecord {
    TextBlockRecord::decode(&record.to_string()).unwrap()
}
pub(super) fn canonical(record: &Value) -> String {
    block_of(record).encode().unwrap()
}

/// An owner whose drawing already holds the text block at layer `L0`, stroke `t0`.
pub(super) fn seeded(record: Value) -> NativeObjectHistory {
    owner(&drawing(
        json!({"items":{"text":[{"target":target("L0","t0"),"block":record}]}}),
    ))
}

pub(super) fn entry(o: &NativeObjectHistory) -> (ObjectTarget, TextBlockRecord) {
    let entry = head(o).document().item_records().unwrap().text_entries()[0].clone();
    (entry.target, entry.block)
}

pub(super) fn edit(stroke: &str, patch: Value) -> Value {
    json!({"command":"object.text.edit","stableTarget":target("L0",stroke),"patch":patch})
}

fn tuples(value: &Value) -> Vec<[f64; 7]> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|glyph| {
            let mut tuple = [0.0; 7];
            for (slot, number) in tuple.iter_mut().zip(glyph.as_array().unwrap()) {
                *slot = number.as_f64().unwrap();
            }
            tuple
        })
        .collect()
}

fn layout_tuples(block: &TextBlockRecord, holder: &str) -> Vec<[f64; 7]> {
    let faces = catalog();
    let layout = layout_text(&block.layout_request(holder), Some(&faces)).unwrap();
    native_glyphs(&layout)
}

fn anchor_of(record: &Value) -> [f64; 2] {
    [
        record["anchorTopLeft"]["x"].as_f64().unwrap(),
        record["anchorTopLeft"]["y"].as_f64().unwrap(),
    ]
}

#[test]
fn every_baseline_root_is_created_as_the_same_record_and_lays_out_as_the_baseline_glyphs() {
    let all = rows(ORACLE, Some("CREATE"));
    assert_eq!(all.len(), 4);
    for row in all {
        let name = row["name"].as_str().unwrap();
        let mut o = owner(&drawing(json!({})));
        let payload =
            json!({"command":"object.text.create","target":target("L0",""),"block":row["record"]});
        assert_eq!(run(&mut o, &payload), Ok(1), "{name}");
        assert_eq!(o.history_depths(), (1, 0), "{name}");
        let (stored_target, block) = entry(&o);
        assert_eq!(stored_target.stroke_id(), "native-text:1:0", "{name}");
        assert_eq!(block.encode().unwrap(), canonical(&row["record"]), "{name}");
        assert_eq!(
            block.anchor(),
            anchor_of(&row["record"]),
            "{name}: the stored anchor is the authored one"
        );
        let holder = text_holder_id(&stored_target);
        assert_eq!(holder, "scene-root:L0:native-text:1:0");
        assert_eq!(
            layout_tuples(&block, &holder),
            tuples(&row["glyphs"]),
            "{name}: baseline glyphs"
        );
    }
}

#[test]
fn every_baseline_edit_writes_the_patched_record_at_the_same_anchor_and_lays_out_as_the_rebuild() {
    let all = rows(ORACLE, Some("EDIT"));
    assert_eq!(all.len(), 14);
    let mut font_folded = 0;
    for row in all {
        let name = row["name"].as_str().unwrap();
        let mut o = seeded(row["before"].clone());
        let (target_before, block_before) = entry(&o);
        let holder = text_holder_id(&target_before);
        assert_eq!(
            layout_tuples(&block_before, &holder),
            tuples(&row["glyphsBefore"]),
            "{name}: before"
        );
        let before = state(&o);
        assert_eq!(
            run(&mut o, &edit("t0", row["patch"].clone())),
            Ok(1),
            "{name}"
        );
        assert_eq!(o.history_depths(), (before.1 .0 + 1, 0), "{name}");
        let (target_after, block_after) = entry(&o);
        assert_eq!(
            block_after.encode().unwrap(),
            canonical(&row["expected"]),
            "{name}"
        );
        // Feedback #37: the anchor the glyphs are placed from never moves, whatever else the edit changes.
        assert_eq!(
            block_after.anchor(),
            [
                row["anchor"]["x"].as_f64().unwrap(),
                row["anchor"]["y"].as_f64().unwrap()
            ],
            "{name}"
        );
        assert_eq!(block_after.anchor(), block_before.anchor(), "{name}");
        // No re-keying: the stable target, hence the holder every glyph pin is keyed by, is the one it had.
        assert_eq!(target_after, target_before, "{name}");
        assert_eq!(text_holder_id(&target_after), holder, "{name}");
        // The relayout equals the baseline rebuild glyph for glyph (position, size, char / word / line index).
        assert_eq!(
            layout_tuples(&block_after, &holder),
            tuples(&row["glyphsAfter"]),
            "{name}: rebuild"
        );
        let differs = row["differs"].as_array().unwrap();
        if !differs.is_empty() {
            font_folded += 1;
            assert_eq!(
                differs,
                &vec![json!("font")],
                "{name}: only the folded weight key may differ"
            );
        }
    }
    assert_eq!(
        font_folded, 1,
        "the baseline folds bold into the stored font key once in the oracle"
    );
}

#[test]
fn glyph_pins_of_unchanged_ordinals_still_resolve_after_a_text_edit() {
    let all = rows(ORACLE, Some("EDIT"));
    let row = all
        .iter()
        .find(|row| row["name"] == "edit_text_prefix_changed")
        .unwrap();
    let mut o = seeded(row["before"].clone());
    let (stored_target, before) = entry(&o);
    let holder = text_holder_id(&stored_target);
    let faces = catalog();
    let layout_before = layout_text(&before.layout_request(&holder), Some(&faces)).unwrap();
    let pins = pins_of(&layout_before);
    assert_eq!(pins.len(), 5);
    run(&mut o, &edit("t0", json!({"text":"Jello"}))).unwrap();
    let (_, after) = entry(&o);
    let layout_after = layout_text(&after.layout_request(&holder), Some(&faces)).unwrap();
    for pin in &pins {
        let key = pin.key();
        assert_eq!(key, format!("{holder}/glyph/{}", pin.ordinal()));
        let again = GlyphPin::parse(&key).unwrap();
        assert_eq!(&again, pin);
        let (was, now) = (unit_of(&layout_before, pin), unit_of(&layout_after, &again));
        // 'H'(72) -> 'J'(74) at ordinal 0 only; "ello" keeps its ordinals 1..4 and its glyphs.
        assert_eq!(was == now, pin.ordinal() != 0, "ordinal {}", pin.ordinal());
    }
    assert_eq!(unit_of(&layout_after, &pins[0]), 74);
}

#[test]
fn an_anchor_member_is_not_editable_so_a_re_anchoring_edit_cannot_be_expressed() {
    let mut o = seeded(rows(ORACLE, Some("EDIT"))[0]["before"].clone());
    let before = state(&o);
    for patch in [
        json!({"anchorTopLeft":{"x":0,"y":0}}),
        json!({"text":"Hello world","anchorTopLeft":{"x":10,"y":20}}),
        json!({"textOverflow":true}),
        json!({"isText":true}),
    ] {
        assert_eq!(
            run(&mut o, &edit("t0", patch)),
            Err(crate::request_receipts::DispatchErrorCode::InvalidRequest)
        );
        assert_eq!(state(&o), before);
    }
}

#[test]
fn effective_equal_patches_are_exact_noops_that_keep_the_authored_spelling_and_absence() {
    // size 48 spelled as an integer; bold absent; letterSpacing absent; fixedWidth absent (no box).
    let minimal = json!({"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
        "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}});
    let mut o = seeded(minimal.clone());
    let before = state(&o);
    for patch in [
        json!({"size":48.0}),
        json!({"bold":false,"italic":false,"underline":false,"strike":false}),
        json!({"letterSpacing":0,"wordSpacing":0.0}),
        json!({"lineHeightMult":1.25}),
        json!({"textCase":"none"}),
        json!({"fixedWidth":null,"fixedHeight":null}),
        json!({"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"}}),
    ] {
        let prepared = prepare(&o, &edit("t0", patch.clone())).unwrap();
        assert_eq!(prepared.result().result()["applied"], false, "{patch}");
        assert_eq!(o.commit_item_edit(prepared), Ok(before.0));
        assert_eq!(state(&o), before, "{patch}");
    }
    assert_eq!(
        entry(&o).1.encode().unwrap(),
        canonical(&minimal),
        "nothing was written back for a default"
    );
    // A real change next to the defaults writes only what differs and keeps the rest absent.
    run(&mut o, &edit("t0", json!({"bold":false,"size":24}))).unwrap();
    let encoded = entry(&o).1.encode().unwrap();
    assert!(
        encoded.contains(r#""size":24"#) && !encoded.contains("bold"),
        "{encoded}"
    );
}

#[test]
fn text_delete_removes_the_block_and_keeps_a_raster_item_in_the_same_container() {
    let o_doc = drawing(json!({"items":{
        "raster":[{"target":target("L0","r0"),"item":raster_item("img-1", 1, 2, 3, 4)}],
        "text":[{"target":target("L0","t0"),"block":rows(ORACLE, Some("EDIT"))[0]["before"]}]}}));
    let mut o = owner(&o_doc);
    let start = bytes(&o);
    run(
        &mut o,
        &json!({"command":"object.text.delete","stableTarget":target("L0","t0")}),
    )
    .unwrap();
    assert_eq!(
        items_of(&o).as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["raster"]
    );
    assert_eq!(o.undo(1), Ok(2));
    assert_eq!(
        bytes(&o),
        start,
        "undo restores the exact bytes of both lists"
    );
    let mut only_text = seeded(rows(ORACLE, Some("EDIT"))[0]["before"].clone());
    let bare_start = bytes(&only_text);
    run(
        &mut only_text,
        &json!({"command":"object.text.delete","stableTarget":target("L0","t0")}),
    )
    .unwrap();
    assert!(serde_json::to_value(head(&only_text).document())
        .unwrap()
        .get("items")
        .is_none());
    assert_ne!(bytes(&only_text), bare_start);
}

#[test]
fn a_text_block_survives_save_and_reopen_with_the_same_glyph_identity_and_layout() {
    use super::support::{images_only, project};
    let mut o = NativeObjectHistory::from_admitted_project(
        "save-text",
        project(&drawing(json!({})), images_only()),
    )
    .unwrap();
    let record = rows(ORACLE, Some("CREATE"))[1]["record"].clone();
    run(
        &mut o,
        &json!({"command":"object.text.create","target":target("L2",""),"block":record}),
    )
    .unwrap();
    run(
        &mut o,
        &edit_at(
            "L2",
            "native-text:1:0",
            json!({"size":20,"fixedWidth":null}),
        ),
    )
    .unwrap();
    let revision = o.content_revision();
    let snapshot = o.acquire_snapshot(revision).unwrap();
    let file = o
        .serialize_project_file(revision, snapshot.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened-text", &file).unwrap();
    assert_eq!(items_of(&reopened), items_of(&o));
    let (stored_target, block) = entry(&reopened);
    assert_eq!(stored_target.stroke_id(), "native-text:1:0");
    let holder = text_holder_id(&stored_target);
    assert_eq!(holder, "scene-root:L2:native-text:1:0");
    let (_, original) = entry(&o);
    assert_eq!(
        layout_tuples(&block, &holder),
        layout_tuples(&original, &holder)
    );
    let again = reopened
        .serialize_project_file(reopened.content_revision(), head(&reopened).snapshot_id())
        .unwrap();
    assert_eq!(again, file);
}

fn unit_of(layout: &LaidOutText, pin: &GlyphPin) -> u32 {
    u32::from(pin.resolve(layout).unwrap().unit)
}

fn edit_at(layer: &str, stroke: &str, patch: Value) -> Value {
    json!({"command":"object.text.edit","stableTarget":target(layer,stroke),"patch":patch})
}
