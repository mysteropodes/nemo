//! Oracle tests for the document admission of the lane-4 tables (L04-I, I14/I35).
//!
//! `fixtures/object-component-document/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_oracle.cjs` of the L04-I leaf from baseline fbe4b8a3: the symbol
//! `sym_comp` and its seven instance layers (loop / once / pingpong / single, speed,
//! held `componentFrame`, `blankOverride`) from `tests/fixtures/components`, a second
//! component `sym_comp_b`, a Time Remap track and a `symMatrix` from the verbatim
//! `enableTimeRemap` / `symMatrixSet`, the montage-segment instance of the verbatim
//! `enterMontageView` literal, three `expose()` entries per the verbatim
//! `exposed-properties.js`, a StoryBoard built by the verbatim `storyboard.js` functions
//! (legacy migration, `addInstanceAuto`, `addMontage`, the chain joins) and the
//! duplicator / effector records of `toggleLayerDuplicator`, the object-duplicator and
//! add-effector literals and the migration fixture. `G` is that whole state as a v2
//! native document written by the generator in the Rust field order, so a byte-equal
//! re-encode is a real claim about serialization, not a tautology.
//!
//! Rows are TAB separated, `-` is absent: `G`/`B`/`T` document / document without the
//! tables / tables JSON; `N` name count; `Y` component; `I` instance; `X` exposed entry;
//! `M` StoryBoard module; `D` duplicator; `E` effector layer; `A` active montage; `P`/`Q`
//! legacy project listing. Every expected token beside the JSON was read from the
//! baseline objects, not from the JSON.
use super::*;
use crate::object_codec::{
    decode_ordered_project, decode_project, decode_storage_project, encode_ordered_project,
    encode_project, encode_storage_project, ObjectCodecError, ObjectCodecErrorKind,
};
use crate::object_document::OrderedObjectDocument;
use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-component-document/oracle.txt");
pub(crate) const V1: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[]}"#;
pub(crate) const V2: &str = r#"{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],"scopes":[]}],"migration":"explicit-plan-required","referencePolicy":"unresolved"}}"#;

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == kind)
        .collect()
}
pub(crate) fn golden() -> &'static str {
    rows("G")[0][1]
}
pub(crate) fn base() -> &'static str {
    rows("B")[0][1]
}
pub(crate) fn tables_json() -> &'static str {
    rows("T")[0][1]
}
pub(crate) fn golden_value() -> Value {
    serde_json::from_str(golden()).expect("golden json")
}
pub(crate) fn tables() -> ComponentTables {
    serde_json::from_str(tables_json()).expect("tables")
}
pub(crate) fn document() -> ObjectDocument {
    decode_storage_project(golden().as_bytes()).expect("golden document")
}
pub(crate) fn count(name: &str) -> usize {
    rows("N")
        .iter()
        .find(|f| f[1] == name)
        .map(|f| f[2].parse().expect("count"))
        .expect("count row")
}
pub(crate) fn admit(value: &Value) -> Result<OrderedObjectDocument, ObjectCodecError> {
    decode_ordered_project(value.to_string().as_bytes())
}
/// The same document with every component removed while instances still name them.
pub(crate) fn without_components(mut doc: ObjectDocument) -> ObjectDocument {
    if let Presence::Present(tables) = &mut doc.component_tables {
        if let Presence::Present(set) = &mut tables.components {
            set.components.clear();
        }
    }
    doc
}
/// `text` with `member` (a complete `"key":value` fragment) appended to the top-level object.
pub(crate) fn with_member(text: &str, member: &str) -> String {
    format!("{},{}}}", &text[..text.len() - 1], member)
}
fn tok(token: &str) -> Option<f64> {
    if token == "-" {
        None
    } else {
        Some(token.parse::<f64>().expect("number token"))
    }
}
fn opt(token: &'static str) -> Option<&'static str> {
    if token == "-" {
        None
    } else {
        Some(token)
    }
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains("Baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(ORACLE.contains("byte-identical to the baseline blob"));
    for (kind, expected) in [
        ("G", 1),
        ("B", 1),
        ("T", 1),
        ("Y", 2),
        ("I", 10),
        ("X", 4),
        ("M", 7),
        ("D", 4),
        ("E", 1),
        ("A", 1),
        ("P", 1),
        ("Q", 7),
    ] {
        assert_eq!(rows(kind).len(), expected, "row kind {kind}");
    }
}

#[test]
fn golden_document_round_trips_to_the_exact_authored_bytes() {
    let document = document();
    let saved = encode_storage_project(&document).unwrap();
    assert_eq!(saved, golden().as_bytes());
    let reopened = decode_storage_project(&saved).unwrap();
    assert_eq!(reopened, document);
    assert_eq!(encode_storage_project(&reopened).unwrap(), saved);
}

#[test]
fn ordered_and_storage_codecs_carry_the_same_tables() {
    let ordered = decode_ordered_project(golden().as_bytes()).unwrap();
    assert_eq!(
        encode_ordered_project(&ordered).unwrap(),
        golden().as_bytes()
    );
    assert_eq!(ordered.into_storage(), document());
}

#[test]
fn base_document_without_the_member_keeps_its_exact_bytes_and_stays_absent() {
    let doc = decode_storage_project(base().as_bytes()).unwrap();
    assert!(doc.component_tables().is_none());
    assert!(doc.component_set().is_none());
    assert!(doc.storyboard().is_none() && doc.duplicator_set().is_none());
    assert_eq!(encode_storage_project(&doc).unwrap(), base().as_bytes());
    assert!(!base().contains("componentTables"));
}

#[test]
fn the_member_is_the_only_difference_between_base_and_golden() {
    let mut value: Value = serde_json::from_str(base()).unwrap();
    value["componentTables"] = serde_json::from_str(tables_json()).unwrap();
    assert_eq!(value, golden_value());
}

#[test]
fn existing_v1_and_v2_documents_keep_their_exact_bytes() {
    for bytes in [V1, V2] {
        let doc = decode_storage_project(bytes.as_bytes()).unwrap();
        assert!(doc.component_tables().is_none());
        assert_eq!(encode_storage_project(&doc).unwrap(), bytes.as_bytes());
    }
}

#[test]
fn the_v1_format_cannot_carry_the_member() {
    let with = with_member(V1, "\"componentTables\":{}");
    let error = decode_project(with.as_bytes()).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::Unsupported);
    assert!(error.message().contains("componentTables"));
    assert!(decode_storage_project(with.as_bytes()).is_err());
    // The same refusal at encode time for an in-memory v1 document.
    let mut v1 = decode_project(V1.as_bytes()).unwrap();
    assert!(encode_project(&v1).is_ok());
    v1.component_tables = Presence::Present(tables());
    let error = encode_project(&v1).unwrap_err();
    assert_eq!(error.kind(), ObjectCodecErrorKind::Unsupported);
}

#[test]
fn absent_present_empty_and_partial_tables_stay_distinct() {
    let empty = with_member(base(), "\"componentTables\":{}");
    let doc = decode_storage_project(empty.as_bytes()).unwrap();
    assert!(doc.component_tables().is_some());
    assert!(doc.component_set().is_none() && doc.storyboard().is_none());
    assert!(doc.duplicator_set().is_none());
    assert_eq!(encode_storage_project(&doc).unwrap(), empty.as_bytes());
    assert_ne!(
        doc,
        decode_storage_project(base().as_bytes()).unwrap(),
        "an empty member is not an absent one"
    );
    let all = tables_json();
    let t: Value = serde_json::from_str(all).unwrap();
    for name in ["components", "duplicators"] {
        let member = json!({ name: t[name].clone() });
        let text = with_member(base(), &format!("\"componentTables\":{member}"));
        let partial = decode_storage_project(text.as_bytes()).unwrap();
        let saved: Value =
            serde_json::from_slice(&encode_storage_project(&partial).unwrap()).unwrap();
        assert_eq!(saved["componentTables"], member);
        assert_eq!(partial.component_set().is_some(), name == "components");
        assert_eq!(partial.duplicator_set().is_some(), name == "duplicators");
        assert!(partial.storyboard().is_none());
    }
    let board = r#""componentTables":{"storyboard":{"format":"nemo.native-storyboard","formatVersion":1,"nextId":1,"modules":[]}}"#;
    let text = with_member(base(), board);
    let only = decode_storage_project(text.as_bytes()).unwrap();
    assert_eq!(only.storyboard().unwrap().modules().len(), 0);
    assert_eq!(only.storyboard().unwrap().next_id(), 1);
    assert!(only.component_set().is_none());
    assert_eq!(encode_storage_project(&only).unwrap(), text.as_bytes());
}

#[test]
fn table_sizes_match_the_baseline_counts() {
    let doc = document();
    let set = doc.component_set().unwrap();
    assert_eq!(doc.layers().len(), count("scene_layers"));
    assert_eq!(doc.total_frames() as usize, count("total_frames"));
    assert_eq!(set.components.len(), count("components"));
    assert_eq!(set.instances.len(), count("instances"));
    assert_eq!(doc.storyboard().unwrap().modules().len(), count("modules"));
    let dup = doc.duplicator_set().unwrap();
    assert_eq!(dup.duplicators.len(), count("duplicators"));
    assert_eq!(dup.effector_layers.len(), count("effector_layers"));
    assert_eq!(
        doc.storyboard().unwrap().next_id() as usize,
        count("next_id")
    );
    assert_eq!(
        doc.storyboard().unwrap().active_montage_id(),
        Some(rows("A")[0][1])
    );
}

#[test]
fn each_component_matches_its_baseline_symbol() {
    let doc = document();
    let set = doc.component_set().unwrap();
    for row in rows("Y") {
        let component = set.component(row[1]).unwrap();
        assert_eq!(component.total_frames().to_string(), row[2]);
        assert_eq!(component.layers().len().to_string(), row[3]);
        assert_eq!(component.objects().len().to_string(), row[4]);
        assert_eq!(component.exposed_props().len().to_string(), row[5]);
        assert!(component
            .objects()
            .iter()
            .all(|o| o.target.context_id == row[1]));
    }
}

#[test]
fn each_instance_matches_its_baseline_layer() {
    let doc = document();
    let set = doc.component_set().unwrap();
    for row in rows("I") {
        let instance = set.instance("scene-root", row[1]).unwrap();
        assert_eq!(instance.symbol_id(), row[2]);
        assert_eq!(instance.play_mode(), Some(row[3]));
        assert_eq!(instance.speed(), tok(row[4]));
        assert_eq!(instance.placed_at(), tok(row[5]));
        assert_eq!(instance.single_frame(), tok(row[6]));
        assert_eq!(instance.frame_keys().len().to_string(), row[7]);
        assert_eq!(instance.matrix().is_some(), row[8] == "1");
        let remap = instance.time_remap().map_or(0, |t| t.keys.len());
        assert_eq!(remap.to_string(), row[9]);
        assert_eq!(instance.trim_in(), tok(row[10]));
        assert_eq!(instance.trim_out(), tok(row[11]));
    }
    let modes: Vec<&str> = rows("I").iter().map(|r| r[3]).collect();
    for mode in ["loop", "once", "pingpong", "single"] {
        assert!(modes.contains(&mode), "no instance placed with {mode}");
    }
}

#[test]
fn held_component_frame_and_blank_override_come_through_as_authored() {
    let doc = document();
    let set = doc.component_set().unwrap();
    let hold = set.instance("scene-root", "ly_comp_hold").unwrap();
    let key = &hold.frame_keys()[0];
    assert_eq!(
        (key.frame(), key.component_frame(), key.blank_override()),
        (0, Some(3.0), None)
    );
    let blank = set.instance("scene-root", "ly_comp_blank").unwrap();
    let key = &blank.frame_keys()[0];
    assert_eq!(
        (key.frame(), key.component_frame(), key.blank_override()),
        (12, None, Some(true))
    );
}

#[test]
fn exposed_props_equal_the_verbatim_expose_entries() {
    let doc = document();
    let set = doc.component_set().unwrap();
    for row in rows("X") {
        let component = set.component(row[4]).unwrap();
        let prop = component
            .exposed_props()
            .iter()
            .find(|p| p.key == row[1])
            .expect("exposed prop");
        assert_eq!(prop.target_stroke_id, row[2]);
        assert_eq!(prop.target_field, row[3]);
    }
}

#[test]
fn storyboard_modules_match_the_verbatim_storyboard_functions() {
    let doc = document();
    let board = doc.storyboard().unwrap();
    for row in rows("M") {
        let module = board.module(row[1]).unwrap();
        assert_eq!(module.kind(), row[2]);
        assert_eq!(module.symbol_id(), opt(row[3]));
        let chain: Vec<&str> = module.chain().iter().map(String::as_str).collect();
        match row[4] {
            "-" => {
                assert!(module.chain.missing());
                assert!(chain.is_empty());
            }
            "=" => {
                assert!(!module.chain.missing());
                assert!(chain.is_empty());
            }
            list => assert_eq!(chain, list.split(',').collect::<Vec<_>>()),
        }
        let audio: Vec<String> = module
            .audio()
            .iter()
            .map(|a| format!("{}@{}", a.module_id, a.offset_frames))
            .collect();
        match row[5] {
            "-" => {
                assert!(module.audio.missing());
                assert!(audio.is_empty());
            }
            "=" => {
                assert!(!module.audio.missing());
                assert!(audio.is_empty());
            }
            list => assert_eq!(audio.join(","), list),
        }
        assert_eq!(module.asset_id.value().map(String::as_str), opt(row[6]));
    }
}

#[test]
fn duplicators_and_effector_layers_match_the_baseline_rows() {
    let doc = document();
    let dup = doc.duplicator_set().unwrap();
    for row in rows("D") {
        let record = dup.duplicator("scene-root", row[1]).unwrap();
        assert_eq!(record.duplicator.mode_label(), opt(row[2]));
        assert_eq!(
            record.duplicator.source_mode.value().map(String::as_str),
            opt(row[3])
        );
        assert_eq!(record.duplicator.effectors().len().to_string(), row[4]);
    }
    for row in rows("E") {
        let found = dup.effector_layers_for("scene-root", row[2]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].layer_uid, row[1]);
    }
}

#[test]
fn component_inner_layers_stay_out_of_the_scene_layer_list_and_the_order_plan() {
    let ordered = decode_ordered_project(golden().as_bytes()).unwrap();
    let scene: Vec<&str> = ordered.layers().iter().map(|l| l.layer_uid()).collect();
    assert!(!scene.contains(&"ly_comp_ia") && !scene.contains(&"ly_comp_ib"));
    // The only order context is the scene root, so the scene-root layer list is still
    // knowable and the expression views' ComponentLayers refusal is not triggered by it.
    assert_eq!(ordered.composition_order.contexts.len(), 1);
    assert_eq!(
        ordered.composition_order.contexts[0].context_id,
        "scene-root"
    );
}

#[cfg(feature = "commands")]
#[test]
fn snapshot_construction_admits_the_golden_document_and_refuses_a_broken_one() {
    use crate::object_snapshot::ObjectSnapshot;
    let doc = document();
    let snapshot = ObjectSnapshot::new("component-doc", doc.clone()).unwrap();
    assert_eq!(snapshot.document(), &doc);
    assert!(ObjectSnapshot::new("component-doc", without_components(doc)).is_err());
}
