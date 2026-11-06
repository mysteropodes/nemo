//! #2372 Select Delete of a bitmap brush: the anchor takes its baked companion image and the image's catalog entry in the same revision.
//! Baseline `SM.deleteSelStrokes` / `stripAnyBrushTexture`. Split from object_vector_batch_bitmap.rs to keep that file within the size budget.
use super::super::support::send;
use super::*;

#[test]
fn deleting_a_bitmap_anchor_takes_its_companion_and_its_image_in_the_same_revision() {
    let mut owner = drawn();
    let before = project_state(&owner);
    assert_eq!(before["drawing"]["items"]["raster"].as_array().unwrap().len(), 1);
    let r = send(
        &mut owner,
        "delete",
        batch(vec![json!({"op":"delete","stableTarget":id()})]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (2, (2, 0)),
        "one revision, one history entry for the gesture"
    );
    let after = project_state(&owner);
    assert!(after["drawing"]["objects"].as_array().unwrap().is_empty(), "{after}");
    assert!(
        after["drawing"]["items"]["raster"].as_array().is_none_or(|r| r.is_empty()),
        "the companion goes with its anchor: {after}"
    );
    assert!(after["assets"].as_array().unwrap().is_empty(), "the baked image leaves the catalog: {}", after["assets"]);
    let reopened =
        NativeObjectHistory::from_project_file("desktop", &serde_json::to_vec(&after).unwrap())
            .unwrap();
    assert_eq!(project_state(&reopened), after, "save and reload");
    owner.undo(2).unwrap();
    assert_eq!(project_state(&owner), before, "one undo restores the anchor, the companion and the catalog");
    owner.redo(3).unwrap();
    assert_eq!(project_state(&owner), after);
}
