//! L01-G controls: each one fails if the behavior it guards is mutated away, so a green main replay cannot
//! hide a vacuous comparison. Sensitivity controls assert on the oracle rows that the wrong implementation
//! WOULD differ from Paper.js; origin controls forge or age a sealed candidate.
use super::support::*;
#[cfg(feature = "evaluation")]
use super::vsupport::{owner_of, state};
#[cfg(feature = "evaluation")]
use super::{rect_ext, rect_owner, shape_doc, t1};
#[cfg(feature = "evaluation")]
use crate::history::NativeObjectHistory;
#[cfg(feature = "evaluation")]
use crate::object_codec::decode_storage_project;
#[cfg(feature = "evaluation")]
use crate::request_receipts::DispatchErrorCode;
#[cfg(feature = "evaluation")]
use serde_json::json;

#[test]
fn the_trigonometric_bound_is_not_vacuous_and_exact_comparison_sees_one_bit() {
    let base: Segs = vec![[100.0, 2.0, 0.5, 0.0, 0.0, 0.0]];
    let mut near = base.clone();
    near[0][0] += 1e-13;
    assert!(within_bound(&near, &base).is_ok());
    assert!(
        same_bits(&near, &base).is_err(),
        "one ulp-scale step is visible to exact comparison"
    );
    let mut off = base.clone();
    off[0][0] += 1e-6;
    assert!(
        within_bound(&off, &base).is_err(),
        "a real mistake is far outside the bound"
    );
    assert!(same_bits(&base, &base).is_ok());
    let negative_zero: Segs = vec![[-0.0, 2.0, 0.5, 0.0, 0.0, 0.0]];
    let positive_zero: Segs = vec![[0.0, 2.0, 0.5, 0.0, 0.0, 0.0]];
    assert!(
        same_bits(&negative_zero, &positive_zero).is_err(),
        "-0 and 0 are different bits"
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn skipping_the_rect_rebuild_would_be_visible_in_the_oracle_and_in_the_stored_anchors() {
    let rebuilt: Vec<Vec<&str>> = rows("sc")
        .into_iter()
        .filter(|row| row[2].starts_with('R') && row[3] != "-" && hexf(row[4]) == 0.0)
        .filter(|row| same_bits(&segs(row[8]), &segs(row[9])).is_err())
        .collect();
    assert!(
        rebuilt.len() >= 8,
        "{} rows distinguish scale-only from rebuild",
        rebuilt.len()
    );
    let row = &rebuilt[0];
    let mut owner = owner_of(&shape_doc(
        1,
        Some(ext_json(row[2], frame(row[3]), 0.0)),
        &segs(row[7]),
    ));
    let response = send(
        &mut owner,
        "c",
        OP_PARAM,
        scale(t1(), pair(row[5]), pair(row[6])),
    );
    assert_eq!(response["ok"], true, "{response}");
    let after = state(&owner);
    let got = stored_segments(&after["objects"][0]);
    assert!(
        same_bits(&got, &segs(row[9])).is_ok(),
        "stored anchors are the rebuild"
    );
    assert!(
        same_bits(&got, &segs(row[8])).is_err(),
        "and not the plain scale"
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn a_negative_factor_must_reorder_the_box_and_the_oracle_proves_the_naive_order_wrong() {
    let mut naive_wrong = 0;
    let mut checked = 0;
    for row in rows("sc") {
        let (factors, pivot) = (pair(row[5]), pair(row[6]));
        let Some(b) = frame(row[3]) else { continue };
        if (factors[0] >= 0.0 && factors[1] >= 0.0) || row[2] == "-" {
            continue;
        }
        let nx1 = pivot[0] + (b[0] - pivot[0]) * factors[0];
        let nx2 = pivot[0] + (b[2] - pivot[0]) * factors[0];
        let ny1 = pivot[1] + (b[1] - pivot[1]) * factors[1];
        let ny2 = pivot[1] + (b[3] - pivot[1]) * factors[1];
        if nx1 > nx2 || ny1 > ny2 {
            naive_wrong += 1;
        }
        let want = frame(row[10]).expect("scaled box");
        assert!(
            want[0] <= want[2] && want[1] <= want[3],
            "{} keeps min/max order",
            row[1]
        );
        let mut owner = owner_of(&shape_doc(
            1,
            Some(ext_json(row[2], Some(b), hexf(row[4]))),
            &segs(row[7]),
        ));
        let response = send(&mut owner, "c", OP_PARAM, scale(t1(), factors, pivot));
        assert_eq!(response["ok"], true, "{response}");
        let stored = stored_frame(&state(&owner)["objects"][0]).expect("stored box");
        assert!(
            stored[0] <= stored[2] && stored[1] <= stored[3],
            "{}",
            row[1]
        );
        checked += 1;
    }
    assert!(checked >= 20, "{checked}");
    assert!(
        naive_wrong >= 20,
        "unsorted corners would be wrong on {naive_wrong} rows"
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn a_candidate_prepared_before_another_edit_is_stale_and_cannot_publish() {
    let mut owner = rect_owner();
    let body = translate(t1(), 1.0, 1.0).to_string();
    let first = owner
        .owner
        .prepare_parametric_edit_json(body.as_bytes())
        .expect("first");
    let second = owner
        .owner
        .prepare_parametric_edit_json(body.as_bytes())
        .expect("second");
    assert_eq!(owner.owner.commit_parametric_edit(first), Ok(1));
    assert_eq!(
        owner.owner.commit_parametric_edit(second),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        owner.content_revision(),
        1,
        "the refused commit published nothing"
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn a_candidate_from_another_owner_cannot_publish_here() {
    let source = rect_owner();
    let body = translate(t1(), 1.0, 1.0).to_string();
    let for_twin = source
        .owner
        .prepare_parametric_edit_json(body.as_bytes())
        .expect("twin");
    let for_foreign = source
        .owner
        .prepare_parametric_edit_json(body.as_bytes())
        .expect("foreign");
    // Same instance name, fresh document incarnation.
    let mut twin = rect_owner();
    assert_eq!(
        twin.owner.commit_parametric_edit(for_twin),
        Err(DispatchErrorCode::WrongDocument)
    );
    assert_eq!(
        (twin.content_revision(), twin.history_depths()),
        (0, (0, 0))
    );
    let document = shape_doc(1, Some(rect_ext()), &straight_rect());
    let mut foreign = NativeObjectHistory::new(
        "foreign",
        decode_storage_project(document.to_string().as_bytes()).expect("foreign document"),
    )
    .expect("foreign owner");
    assert_eq!(
        foreign.owner.commit_parametric_edit(for_foreign),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(
        (foreign.content_revision(), foreign.history_depths()),
        (0, (0, 0))
    );
}

#[cfg(feature = "evaluation")]
#[test]
fn an_accepted_edit_never_changes_the_snapshot_it_was_prepared_from() {
    let mut owner = rect_owner();
    let pinned = owner.acquire_snapshot(0).expect("snapshot 0");
    let source = serde_json::to_value(pinned.document()).expect("source");
    let response = send(
        &mut owner,
        "e",
        OP_PARAM,
        set(t1(), json!({"kind": "rect", "tl": 8})),
    );
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(
        serde_json::to_value(pinned.document()).expect("again"),
        source
    );
    assert_ne!(state(&owner)["objects"][0], source["objects"][0]);
}
