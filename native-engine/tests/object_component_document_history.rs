//! The tables through the existing owner and history (L04-I): one revision and one
//! undo entry per real change, exact undo/redo, no-op replacement, stale / wrong-owner /
//! malformed candidates that publish nothing, immutable earlier snapshots and project
//! sound resolution at commit. Everything runs on the real `NativeObjectHistory`
//! (ObjectFillOwner, ObjectRevisionOwner, strict storage admission); the documents are
//! the baseline-derived golden document and its table-less base.
use super::assets_tests::{audio_inventory, inventory_of};
use super::tests::{base, count, golden, golden_value, tables, V2};
use super::*;
use crate::commands::ComponentTablesEdit;
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{ProjectEnvelope, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::Value;

const VISIBILITY: &[u8] =
    br#"{"command":"object.layer.visibility.set","layerUid":"ly_toggle","visible":false}"#;

fn project_as(
    instance: &str,
    text: &str,
    inventory: Option<AssetInventory>,
) -> NativeObjectHistory {
    let document = decode_storage_project(text.as_bytes()).unwrap();
    let metadata = ProjectMetadata::new(640, 360, 24.).unwrap();
    let envelope = ProjectEnvelope::no_media(metadata, document).with_asset_inventory(inventory);
    NativeObjectHistory::from_admitted_project(instance, envelope).unwrap()
}
fn project(text: &str) -> NativeObjectHistory {
    project_as(
        "component-doc",
        text,
        Some(audio_inventory(AssetKind::Audio)),
    )
}
fn head(history: &NativeObjectHistory) -> ObjectSnapshot {
    history
        .acquire_snapshot(history.content_revision())
        .unwrap()
}
fn saved(history: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(history).document()).unwrap()
}
fn replace(history: &mut NativeObjectHistory, next: ComponentTables) -> u64 {
    let candidate = history
        .prepare_component_tables(ComponentTablesEdit::Replace(next))
        .unwrap();
    history.commit_component_tables(candidate).unwrap()
}
/// The golden tables without the duplicator table.
fn without_duplicators() -> ComponentTables {
    let mut changed = tables();
    changed.duplicators = Presence::Missing;
    changed
}
fn golden_without_duplicators() -> Value {
    let mut value = golden_value();
    value["componentTables"]
        .as_object_mut()
        .unwrap()
        .remove("duplicators");
    value
}

#[test]
fn replace_commits_one_revision_and_one_undo_entry() {
    let mut history = project(base());
    assert_eq!(
        (history.content_revision(), history.history_depths()),
        (0, (0, 0))
    );
    let candidate = history
        .prepare_component_tables(ComponentTablesEdit::Replace(tables()))
        .unwrap();
    assert!(candidate.changed());
    assert_eq!(
        candidate.document().storyboard().unwrap().modules().len(),
        count("modules")
    );
    assert_eq!(history.commit_component_tables(candidate).unwrap(), 1);
    assert_eq!(history.history_depths(), (1, 0));
    assert_eq!(saved(&history), golden().as_bytes());
}

#[test]
fn undo_restores_the_exact_prior_bytes_and_redo_the_tables() {
    let mut history = project(base());
    replace(&mut history, tables());
    let committed = head(&history);
    assert_eq!(history.undo(1).unwrap(), 2);
    assert_eq!(saved(&history), base().as_bytes());
    assert!(head(&history).document().component_tables().is_none());
    assert_eq!(history.history_depths(), (0, 1));
    assert_eq!(history.redo(2).unwrap(), 3);
    assert_eq!(saved(&history), golden().as_bytes());
    assert_eq!(history.history_depths(), (1, 0));
    // The snapshot taken at revision 1 never changed underneath its holder.
    assert_eq!(
        encode_storage_project(committed.document()).unwrap(),
        golden().as_bytes()
    );
}

#[test]
fn replacing_a_table_then_undoing_returns_the_exact_previous_table() {
    let mut history = project(golden());
    assert_eq!(replace(&mut history, without_duplicators()), 1);
    let after: Value = serde_json::from_slice(&saved(&history)).unwrap();
    assert_eq!(after, golden_without_duplicators());
    assert!(head(&history).document().duplicator_set().is_none());
    assert_eq!(history.undo(1).unwrap(), 2);
    assert_eq!(saved(&history), golden().as_bytes());
    assert!(head(&history).document().duplicator_set().is_some());
}

#[test]
fn remove_makes_the_member_absent_and_undo_brings_it_back() {
    let mut history = project(golden());
    let candidate = history
        .prepare_component_tables(ComponentTablesEdit::Remove)
        .unwrap();
    assert!(candidate.changed());
    assert_eq!(history.commit_component_tables(candidate).unwrap(), 1);
    assert_eq!(saved(&history), base().as_bytes());
    assert!(head(&history).document().component_tables().is_none());
    assert_eq!(history.undo(1).unwrap(), 2);
    assert_eq!(saved(&history), golden().as_bytes());
}

#[test]
fn a_noop_replacement_keeps_presence_bytes_depth_and_redo() {
    let mut history = project(golden());
    replace(&mut history, without_duplicators());
    history.undo(1).unwrap();
    assert_eq!(history.history_depths(), (0, 1));
    let before = saved(&history);
    let snapshot = head(&history).snapshot_id().to_string();
    let candidate = history
        .prepare_component_tables(ComponentTablesEdit::Replace(tables()))
        .unwrap();
    assert!(!candidate.changed(), "identical tables are a no-op");
    assert_eq!(history.commit_component_tables(candidate).unwrap(), 2);
    assert_eq!(history.history_depths(), (0, 1), "redo survives a no-op");
    assert_eq!(saved(&history), before);
    assert_eq!(head(&history).snapshot_id(), snapshot);
    // Removing a member that was never authored is a no-op too and stays absent.
    let mut plain = project(base());
    let candidate = plain
        .prepare_component_tables(ComponentTablesEdit::Remove)
        .unwrap();
    assert!(!candidate.changed());
    assert_eq!(plain.commit_component_tables(candidate).unwrap(), 0);
    assert!(head(&plain).document().component_tables().is_none());
    assert_eq!(plain.history_depths(), (0, 0));
}

#[test]
fn a_dropped_candidate_leaves_revision_depths_bytes_and_redo_untouched() {
    let mut history = project(base());
    replace(&mut history, without_duplicators());
    history.undo(1).unwrap();
    assert_eq!(history.history_depths(), (0, 1));
    let before = saved(&history);
    let snapshot = head(&history).snapshot_id().to_string();
    let revision = history.content_revision();
    let candidate = history
        .prepare_component_tables(ComponentTablesEdit::Replace(tables()))
        .unwrap();
    assert!(candidate.changed(), "the candidate is a real change");
    drop(candidate);
    assert_eq!(history.content_revision(), revision);
    assert_eq!(history.history_depths(), (0, 1), "redo survives a cancel");
    assert_eq!(saved(&history), before);
    assert_eq!(head(&history).snapshot_id(), snapshot);
    // A fresh prepare/commit after the cancel behaves as if nothing was prepared.
    assert_eq!(replace(&mut history, tables()), revision + 1);
    assert_eq!(history.history_depths(), (1, 0));
    assert_eq!(
        head(&history)
            .document()
            .storyboard()
            .unwrap()
            .modules()
            .len(),
        count("modules")
    );
}

#[test]
fn the_snapshot_pin_changes_only_with_a_real_table_change() {
    let mut history = project(base());
    let first = head(&history);
    replace(&mut history, tables());
    let second = head(&history);
    assert_eq!(second.content_revision(), first.content_revision() + 1);
    assert_ne!(second.snapshot_id(), first.snapshot_id());
    assert_eq!(second.document_id(), first.document_id());
    replace(&mut history, tables());
    assert_eq!(head(&history).snapshot_id(), second.snapshot_id());
    // Earlier pins keep their own tables.
    assert!(first.document().component_tables().is_none());
    assert!(second.document().component_set().is_some());
    // An unrelated owner edit moves the pin but carries the tables through untouched.
    let candidate = history.prepare_layer_visibility_json(VISIBILITY).unwrap();
    assert_eq!(history.commit_layer_visibility(candidate).unwrap(), 2);
    let third = head(&history);
    assert_ne!(third.snapshot_id(), second.snapshot_id());
    assert_eq!(
        third.document().component_tables(),
        second.document().component_tables()
    );
}

#[test]
fn a_v1_document_cannot_hold_tables_through_the_owner() {
    let v1 = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L"}],"objects":[]}"#;
    let mut history = project(v1);
    let refused = history.prepare_component_tables(ComponentTablesEdit::Replace(tables()));
    assert_eq!(refused.unwrap_err(), DispatchErrorCode::Unavailable);
    let candidate = history
        .prepare_component_tables(ComponentTablesEdit::Remove)
        .unwrap();
    assert!(!candidate.changed());
    assert_eq!(history.commit_component_tables(candidate).unwrap(), 0);
}

#[test]
fn malformed_tables_cannot_partially_publish() {
    let mut history = project(base());
    replace(&mut history, tables());
    history.undo(1).unwrap();
    let (revision, depths, bytes) = (
        history.content_revision(),
        history.history_depths(),
        saved(&history),
    );
    let mut bad = tables();
    if let Presence::Present(set) = &mut bad.components {
        set.instances[0].symbol_id = "ghost".to_string();
    }
    let refused = history.prepare_component_tables(ComponentTablesEdit::Replace(bad));
    assert_eq!(refused.unwrap_err(), DispatchErrorCode::InvalidRequest);
    // Tables for another document: the instances name layers this scene does not have.
    let tiny = project(V2);
    let refused = tiny.prepare_component_tables(ComponentTablesEdit::Replace(tables()));
    assert_eq!(refused.unwrap_err(), DispatchErrorCode::InvalidRequest);
    assert_eq!(history.content_revision(), revision);
    assert_eq!(history.history_depths(), depths, "the redo entry is intact");
    assert_eq!(saved(&history), bytes);
    assert_eq!(
        (tiny.content_revision(), tiny.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn a_stale_candidate_is_refused_and_a_replay_does_not_execute_twice() {
    let mut history = project(base());
    let first = history
        .prepare_component_tables(ComponentTablesEdit::Replace(tables()))
        .unwrap();
    let replayed = first.clone();
    replace(&mut history, without_duplicators());
    let after = saved(&history);
    assert_eq!(
        history.commit_component_tables(first).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(
        history.commit_component_tables(replayed).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(
        (history.content_revision(), history.history_depths()),
        (1, (1, 0))
    );
    assert_eq!(saved(&history), after);
}

#[test]
fn a_candidate_from_another_owner_is_refused() {
    let a = project_as("owner-a", base(), Some(audio_inventory(AssetKind::Audio)));
    let mut b = project_as("owner-b", base(), Some(audio_inventory(AssetKind::Audio)));
    let mut same_instance = project_as("owner-a", base(), Some(audio_inventory(AssetKind::Audio)));
    let candidate = a
        .prepare_component_tables(ComponentTablesEdit::Replace(tables()))
        .unwrap();
    assert_eq!(
        b.commit_component_tables(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(
        same_instance
            .commit_component_tables(candidate)
            .unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    for other in [&b, &same_instance] {
        assert_eq!(
            (other.content_revision(), other.history_depths()),
            (0, (0, 0))
        );
        assert_eq!(saved(other), base().as_bytes());
    }
    assert_eq!(a.content_revision(), 0);
}

#[test]
fn sound_assets_resolve_in_the_project_inventory_at_commit() {
    // Project with the audio asset: accepted.
    let mut history = project(base());
    assert_eq!(replace(&mut history, tables()), 1);
    // No inventory, another asset id or a non-audio record: nothing is published.
    let cases = [
        (None, DispatchErrorCode::NotFound),
        (
            Some(inventory_of("asset-audio-other", AssetKind::Audio)),
            DispatchErrorCode::NotFound,
        ),
        (
            Some(audio_inventory(AssetKind::Image)),
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    for (inventory, expected) in cases {
        let history = project_as("component-doc", base(), inventory);
        let refused = history.prepare_component_tables(ComponentTablesEdit::Replace(tables()));
        assert_eq!(refused.unwrap_err(), expected);
        assert_eq!(
            (history.content_revision(), history.history_depths()),
            (0, (0, 0))
        );
    }
    // A bare drawing has no inventory either; the same tables without the sound are fine.
    let document = decode_storage_project(base().as_bytes()).unwrap();
    let mut bare = NativeObjectHistory::new("bare", document).unwrap();
    let refused = bare.prepare_component_tables(ComponentTablesEdit::Replace(tables()));
    assert_eq!(refused.unwrap_err(), DispatchErrorCode::NotFound);
    let mut quiet = tables();
    quiet.storyboard = Presence::Missing;
    assert_eq!(replace(&mut bare, quiet), 1);
}
