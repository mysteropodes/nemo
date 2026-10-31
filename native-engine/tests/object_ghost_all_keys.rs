//! #2358 GHOST-ALL-KEYS: Ghost All draws the produced brush items of a key frame, not only its stored geometry, and the
//! evaluated-frame cache is charged for the vector data it keeps alive.
//! Baseline `renderGhostAll` draws a key frame's stored strokes in one purple outline (`Color(.68,.6,1,op*1.6)`, fill
//! cleared), skips rasters and returns early for a component layer; a brush-texture stroke's dabs are such strokes.
use super::fidelity::{component_owner, dab_owner, ink, project, rect, timing};
use super::*;
use crate::application::{prepare_ghost_underlay, prepared_ghost_frames};
use std::sync::{Arc, Weak};

fn all_keys_request(layer: &str, to: u32) -> GhostUnderlayRequest {
    GhostUnderlayRequest {
        layer_uid: layer.into(),
        onion: false,
        ghost_all: true,
        from: 0,
        to,
        previous_opacity: 60.,
        next_opacity: 30.,
        mode: GhostMode::Default,
        previous_tint: None,
        next_tint: None,
        shift: None,
    }
}

#[test]
fn gpu_ghost_all_draws_the_dabs_of_a_texture_brush_key_in_the_ghost_all_outline() {
    let mut gpu = Compositor::new().expect("ghost all keys requires real Metal");
    let owner = dab_owner();
    let s = snapshot(&owner);
    let request = all_keys_request("L0", 3);
    let underlay = prepare_ghost_underlay(&s, &request, 3);
    assert_eq!(underlay.receipt().status, "composited");
    // Key frame 2 holds the seeded anchor: its produced dabs are ghost pieces (they were dropped for Ghost All).
    assert!(
        underlay.prepared_count() > 0,
        "dabs of the other key are ghost pieces, got {:?}",
        underlay.counts()
    );
    let plain = rgba(&mut gpu, &s, &viewport(&s, 3, None));
    let ghosted = rgba(&mut gpu, &s, &viewport(&s, 3, Some(&request)));
    assert_eq!(ink(&plain), 0, "frame 3 has no dab line of its own");
    assert!(ink(&ghosted) > 0, "Ghost All shows the dab line of key 2");
    // The Ghost All paint: a purple outline (blue saturated, red below it), never the charcoal dab colour.
    let pixels = &ghosted;
    let purple = (24..40)
        .flat_map(|y| (0..64).map(move |x| (x, y)))
        .map(|(x, y)| at(pixels, x, y))
        .any(|p| p[2] >= 250 && p[0] < 235 && p[1] < 235 && p[0] > 120);
    assert!(purple, "dabs are outlined in the Ghost All purple");
}

#[test]
fn ghost_all_never_evaluates_a_layer_without_produced_brush_items_and_skips_component_layers() {
    // A plain vector layer: stored key geometry only, no evaluated frame at all.
    let plain = owner();
    let s = snapshot(&plain);
    let before = prepared_ghost_frames();
    let vector = prepare_ghost_underlay(&s, &all_keys_request("L0", 3), 3);
    assert_eq!(vector.receipt().status, "composited");
    assert_eq!(
        (vector.prepared_count(), prepared_ghost_frames()),
        (0, before)
    );
    // A dab layer evaluates its key frames (one other key here).
    let dabs = dab_owner();
    let d = snapshot(&dabs);
    let before = prepared_ghost_frames();
    prepare_ghost_underlay(&d, &all_keys_request("L0", 3), 3);
    assert_eq!(prepared_ghost_frames() - before, 1);
    // Baseline `renderGhostAll` returns early for a component layer: nothing is ghosted, nothing evaluated.
    let comp = component_owner();
    let c = snapshot(&comp);
    let before = prepared_ghost_frames();
    let receipt = prepare_ghost_underlay(&c, &all_keys_request("host", 8), 6);
    assert_eq!(
        (receipt.receipt().status, receipt.receipt().frames),
        ("empty", 0),
        "{:?}",
        receipt.receipt()
    );
    assert_eq!(
        (receipt.prepared_count(), prepared_ghost_frames()),
        (0, before)
    );
}

/// `frames` keys of `per` small squares each: a heavy vector document that decodes no image.
fn many_paths(per: i32, frames: u32) -> NativeObjectHistory {
    let keys: Vec<u32> = (0..frames).collect();
    let mut objects = Vec::new();
    let mut scopes = Vec::new();
    for f in 0..frames {
        let mut elements = Vec::new();
        for i in 0..per {
            let id = format!("s{f}-{i}");
            objects.push(rect(
                "scene-root",
                "L0",
                &id,
                f,
                [i % 60, 2, i % 60 + 3, 5],
                [0., 0., 1.],
            ));
            elements.push(json!({"layerUid":"L0","strokeId":id,"naturalPosition":i,"order":{}}));
        }
        scopes.push(json!({"frameScope":{"kind":"authored","frame":f},"elements":elements}));
    }
    project(
        json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":frames,
        "layers":[{"layerUid":"L0","timing":timing(&keys)}],"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":scopes}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}}),
    )
}
fn evaluated(s: &ObjectSnapshot, frame: u32, revision: &Revision) -> Evaluated {
    let witness = prepare_object_composition(&plan(s, frame), &pins(s, frame)).unwrap();
    Evaluated {
        revision: revision.clone(),
        frame,
        bytes: witness_bytes(&witness),
        witness,
    }
}

#[test]
fn evaluated_frames_of_a_many_path_document_are_charged_and_evicted_by_bytes() {
    let owner = many_paths(60, 6);
    let s = snapshot(&owner);
    let revision = s.revision_identity();
    let one = evaluated(&s, 0, &revision);
    // No image anywhere, yet a vector-heavy frame is not free (it used to count 0 and never evict).
    assert_eq!(one.witness.raster_images().count(), 0);
    assert!(one.bytes > 60 * 56, "paths are charged, got {}", one.bytes);
    // A budget that holds two frames and a half: five admitted frames leave the two newest.
    let budget = one.bytes * 5 / 2;
    let mut memo = Vec::new();
    for frame in 0..5 {
        admit_witness(&mut memo, evaluated(&s, frame, &revision), budget);
    }
    let kept: Vec<u32> = memo.iter().map(|m| m.frame).collect();
    assert_eq!(kept, vec![4, 3], "newest first, bounded by estimated bytes");
    assert!(memo.iter().map(|m| m.bytes).sum::<usize>() <= budget);
    // One frame alone over the budget is never retained.
    let mut tiny = Vec::new();
    admit_witness(&mut tiny, evaluated(&s, 0, &revision), one.bytes - 1);
    assert!(tiny.is_empty());
}

#[test]
fn admitting_a_new_revision_drops_the_old_revisions_entries_of_that_document_only() {
    let owner = many_paths(4, 3);
    let s = snapshot(&owner);
    let old: Arc<dyn std::any::Any + Send + Sync> = Arc::new(0u8);
    let new: Arc<dyn std::any::Any + Send + Sync> = Arc::new(1u8);
    let (old_rev, new_rev): (Revision, Revision) = (Arc::downgrade(&old), Arc::downgrade(&new));
    let other = many_paths(4, 3);
    let o = snapshot(&other);
    assert_ne!(o.document_id(), s.document_id());
    let mut memo = Vec::new();
    admit_witness(&mut memo, evaluated(&s, 0, &old_rev), usize::MAX);
    admit_witness(&mut memo, evaluated(&s, 1, &old_rev), usize::MAX);
    admit_witness(
        &mut memo,
        evaluated(&o, 0, &o.revision_identity()),
        usize::MAX,
    );
    assert_eq!(memo.len(), 3);
    // The document is edited: its first evaluated frame of the new revision removes both old-revision entries at once
    // (they each hold the whole superseded snapshot alive); another open document keeps its entry.
    admit_witness(&mut memo, evaluated(&s, 2, &new_rev), usize::MAX);
    let left: Vec<(String, u32)> = memo
        .iter()
        .map(|m| (document_of(&m.witness).to_owned(), m.frame))
        .collect();
    assert_eq!(
        left,
        vec![
            (s.document_id().to_owned(), 2),
            (o.document_id().to_owned(), 0)
        ]
    );
    assert!(Weak::ptr_eq(&memo[0].revision, &new_rev));
}
