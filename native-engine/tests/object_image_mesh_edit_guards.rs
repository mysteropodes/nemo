//! Envelope and sealing guards of the image-mesh edit transaction (L03-L): the shared dispatcher's refusals
//! and replay, sealed candidates, the enlarged envelope bound and the build without the topology kernel.
use super::object_image_mesh_edit_oracle as oracle;
use super::object_image_mesh_edit_rows::*;
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::json;

#[test]
fn every_request_refuses_cancel_stale_foreign_and_replays_without_a_second_execution() {
    let mut cases = vec![
        ("offset", "g4", twin_offset()),
        ("reset", "g4s", payload(ID, reset_offsets())),
    ];
    if cfg!(feature = "evaluation") {
        cases.push(("density", "g4", twin_density()));
    }
    for (op, start, body) in cases {
        let mut owner = owner_with(&oracle::start(start));
        let original = state(&owner);
        for (id, patch) in [
            ("cancel", json!({"cancelledBeforeDispatch":true})),
            ("stale", json!({"expectedRevision":99})),
            ("foreign-instance", json!({"instanceId":"foreign"})),
            ("foreign-document", json!({"documentId":"foreign"})),
            ("wrong-api", json!({"apiVersion":1})),
        ] {
            let mut req = request(&owner, id, body.clone());
            for (member, value) in patch.as_object().unwrap() {
                req[member] = value.clone();
            }
            let reply = dispatch(&mut owner, &req);
            let want = match id {
                "cancel" => "cancelled_before_dispatch",
                "stale" => "stale_revision",
                "foreign-instance" => "wrong_instance",
                "foreign-document" => "wrong_document",
                _ => "invalid_request",
            };
            assert_eq!(code(&reply), want, "{op} {id}");
            assert_eq!(state(&owner), original, "{op} {id}");
        }
        let mut req = request(&owner, "missing", body.clone());
        req.as_object_mut().unwrap().remove("expectedRevision");
        assert_eq!(dispatch(&mut owner, &req)["ok"], false, "{op}");
        assert_eq!(state(&owner), original, "{op}");
        let req = request(&owner, "replay", body.clone());
        let first = dispatch(&mut owner, &req);
        assert_eq!(first["ok"], true, "{op}: {first}");
        assert_eq!(first["result"]["applied"], true, "{op}");
        let applied = state(&owner);
        assert_eq!(
            dispatch(&mut owner, &req),
            first,
            "{op}: same body, same receipt"
        );
        assert_eq!(state(&owner), applied, "{op}: the replay executed nothing");
        let altered = with(req.clone(), |r| r["payload"]["extra"] = json!(1));
        assert_eq!(
            dispatch(&mut owner, &altered)["ok"],
            false,
            "{op}: reused id, changed body"
        );
        assert_eq!(state(&owner), applied, "{op}");
        owner.undo(owner.content_revision()).unwrap();
        let undone = state(&owner);
        assert_eq!(dispatch(&mut owner, &req), first, "{op}: replay after undo");
        assert_eq!(state(&owner), undone, "{op}: a replay cannot run twice");
    }
}

#[test]
fn prepared_candidates_are_sealed_to_their_origin_owner_and_revision() {
    let start = oracle::start("g4");
    let body = twin_offset().to_string();
    let mut source = owner_with(&start);
    let candidate = source
        .prepare_image_mesh_edit_json(body.as_bytes())
        .unwrap();
    let mut other = owner_with(&start);
    let other_before = state(&other);
    assert_eq!(
        other.commit_image_mesh_edit(candidate.clone()).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        decode_storage_project(doc(1, store_of(&[(ID, &start)])).to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        foreign
            .commit_image_mesh_edit(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(state(&other), other_before);
    assert_eq!(foreign.history_depths(), (0, 0));
    // A candidate prepared before another commit is stale: nothing publishes.
    let stale = source
        .prepare_image_mesh_edit_json(body.as_bytes())
        .unwrap();
    let revision = source.commit_image_mesh_edit(candidate.clone()).unwrap();
    assert_eq!(revision, 1);
    let applied = state(&source);
    assert_eq!(
        source.commit_image_mesh_edit(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(
        source.commit_image_mesh_edit(candidate).unwrap_err(),
        DispatchErrorCode::StaleRevision,
        "a candidate commits once"
    );
    assert_eq!(state(&source), applied);
}

#[test]
fn a_candidate_that_changes_nothing_commits_without_a_revision_or_history_entry() {
    let mut owner = owner_with(&oracle::start("g4"));
    let noop = payload(ID, set_offset(8, 0.0, 0.0)).to_string();
    let candidate = owner.prepare_image_mesh_edit_json(noop.as_bytes()).unwrap();
    let before = state(&owner);
    assert_eq!(owner.commit_image_mesh_edit(candidate).unwrap(), before.1);
    assert_eq!(state(&owner), before);
}

#[test]
fn the_envelope_admits_a_request_past_the_default_bound_and_the_edit_itself_judges_it() {
    let mut owner = owner_with(&oracle::start("g4"));
    let before = state(&owner);
    // 5000 bytes is past the 4096-byte message bound but inside the vector-edit bound: the shared
    // envelope lets it through and the edit refuses the unknown member with its own message.
    let body = with(twin_offset(), |p| p["pad"] = json!("x".repeat(5000)));
    let reply = run(&mut owner, "big", body);
    assert_eq!(code(&reply), "invalid_request");
    assert_eq!(
        reply["error"]["message"],
        "Image mesh edit payload or target is invalid or unavailable."
    );
    // Beyond the vector-edit bound the envelope itself refuses it.
    let body = with(twin_offset(), |p| p["pad"] = json!("x".repeat(70_000)));
    let reply = run(&mut owner, "huge", body);
    assert_eq!(code(&reply), "invalid_request");
    assert_eq!(
        reply["error"]["message"],
        "Image mesh edit request refused."
    );
    assert_eq!(state(&owner), before);
}

#[cfg(not(feature = "evaluation"))]
#[test]
fn without_the_topology_kernel_retopology_is_a_typed_unavailable_never_a_guess() {
    for body in [twin_outline(), twin_density()] {
        let mut owner = owner_with(&oracle::start("g4s"));
        let before = state(&owner);
        let reply = run(&mut owner, "no-kernel", body);
        assert_eq!(code(&reply), "unavailable");
        assert_eq!(
            state(&owner),
            before,
            "nothing is published, nothing is approximated"
        );
    }
    let mut owner = owner_with(&oracle::start("g4s"));
    run_ok(&mut owner, "offset", twin_offset());
}
