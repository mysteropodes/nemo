//! Completed diagnostic batches publish before proof resolution. ALL UNRUN.
use crate::application_mcp::revision_sync::motion_finalize::tests::{
    acknowledge, batch, fill, fixture, owner_state, pins, subscribe,
};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use serde_json::{json, Value};
use std::time::Duration;

fn diagnostic(s: &crate::application_mcp::ApplicationMcp, frame: f64) -> Option<Value> {
    let mut authority = s.native.lock().unwrap();
    let g = authority.active_generation().unwrap();
    let owner = authority
        .active_mut(g)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    owner.motion_diagnostic_for_test(frame)
}

#[test]
fn pending_r1_proof_publishes_actual_error_then_clear_before_return_without_ack() {
    let (s, origin, output, g) = fixture("frame === 0 ? 'wrong' : [frame,0]");
    let binding = subscribe(&s);
    let delivery = s
        .revisions
        .dispatch(
            s.instance_id(),
            &s.native,
            fill(&s, &origin, "diagnostic-r1"),
            true,
        )
        .unwrap();
    let (r1, mut receiver) = delivery.notification.unwrap();
    let current = owner_state(&s).0;
    let event = serde_json::to_value(&r1).unwrap();
    assert_eq!(
        diagnostic(&s, 0.),
        None,
        "unvisited is not a healthy result"
    );
    for frame in [0, 1] {
        let p = pins(&current, frame);
        s.with_object_preview_owner_for_preview(
            g,
            &p,
            Some((binding["subscriptionId"].as_str().unwrap(), &event)),
            |owner| {
                owner.with_prepared_object_preview_for_revision_sync(&p, output.clone(), |_| Ok(()))
            },
        )
        .unwrap();
        // No extra query, async worker or ACK is needed to publish the result.
        assert!(s.revisions.0.lock().unwrap().motion_reservation.is_none());
        assert_eq!(
            (owner_state(&s).0.content_revision(), owner_state(&s).1),
            (1, (1, 0))
        );
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        if frame == 0 {
            assert_eq!(
                diagnostic(&s, 0.),
                Some(json!({"kind":"mustReturnNumberOrXY"}))
            );
        } else {
            assert_eq!(
                diagnostic(&s, 1.),
                Some(Value::Null),
                "real successful clear"
            );
            assert_eq!(diagnostic(&s, 0.), None, "original last-result semantics");
        }
    }
    acknowledge(&s, &binding, &r1);
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(s.revisions.settle(&s.native, &r1, true));
    assert_eq!(owner_state(&s).1, (1, 0));
}

#[tokio::test(flavor = "current_thread")]
async fn later_timeout_cannot_merge_into_closed_diagnostic_batch_or_await_its_ack() {
    let (s, origin, output, g) = fixture("frame === 0 ? 'wrong' : (() => { while(true){} })()");
    // This ordinary refused read closes exactly frame0; the production wrapper
    // seals it synchronously after these read guards return.
    assert!(s
        .with_object_preview_owner(g, &pins(&origin, 0), |owner| {
            owner.with_prepared_object_preview(&pins(&origin, 0), output.clone(), |_| Ok(()))
        })
        .is_err());
    let first = batch(&s);
    assert!(
        !s.revisions
            .0
            .lock()
            .unwrap()
            .motion_reservation
            .as_ref()
            .unwrap()
            .requires_revision
    );
    let blocked = s
        .with_object_preview_owner::<()>(g, &pins(&origin, 1), |_| {
            panic!("later frame must not enter the VM before the completed batch seals")
        })
        .unwrap_err();
    assert!(blocked.contains("post-read finalizer"));
    assert_eq!(batch(&s), first);
    assert_eq!(s.seal_object_motion_diagnostics().unwrap(), Some(true));
    assert_eq!(
        diagnostic(&s, 0.),
        Some(json!({"kind":"mustReturnNumberOrXY"}))
    );
    assert!(s
        .with_object_preview_owner(g, &pins(&origin, 1), |owner| {
            owner.with_prepared_object_preview(&pins(&origin, 1), output, |_| Ok(()))
        })
        .is_err());
    let second = batch(&s);
    assert_ne!(first, second);
    assert!(s
        .complete_object_motion_effects(
            &first,
            |_| panic!("stale completion"),
            Duration::from_secs(1)
        )
        .await
        .is_err());
    assert_eq!(
        batch(&s),
        second,
        "stale completion cannot drain the newer batch"
    );
    assert_eq!(
        s.seal_object_motion_diagnostics().unwrap(),
        None,
        "timeout stays pending"
    );
    assert_eq!(
        diagnostic(&s, 1.),
        None,
        "timeout diagnostic waits for R2 seal"
    );
    assert_eq!(owner_state(&s).1, (0, 0));
    let binding = subscribe(&s);
    s.complete_object_motion_effects(
        &second,
        |event| {
            acknowledge(&s, &binding, &event);
            Ok(())
        },
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(owner_state(&s).1, (1, 0));
    assert_eq!(diagnostic(&s, 1.), Some(json!({"kind":"timeout"})));
}

// This calls the real public NativeDispatch entry, not the projection helper.
fn public_property_read(
    s: &crate::application_mcp::ApplicationMcp,
    frame: u32,
    kind: &str,
) -> Value {
    let mut authority = s.native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    let owner = authority
        .active_mut(generation)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    let snapshot = owner.motion_state_for_test().0;
    let request = native_engine::commands::OpacityRequest::query(
        format!("diagnostic-public/{kind}/{frame}"),
        snapshot.instance_id(),
        snapshot.document_id(),
        "query.document.object.property.read",
        json!({
            "atRevision":snapshot.content_revision(), "documentSnapshotId":snapshot.snapshot_id(),
            "view":{"kind":kind,"target":{"contextId":"scene-root","layerUid":"L0"},
                "property":"dupOffsetPos","frame":frame}
        }),
    );
    let response = owner.dispatch(request).unwrap();
    assert!(
        response.is_ok(),
        "actual public property read refused: {response:?}"
    );
    response.result().unwrap().clone()
}

#[test]
fn public_host_dispatch_projects_real_published_error_then_clear_and_keeps_raw_reads_unchanged() {
    let (s, origin, output, g) = fixture("frame === 0 ? 'wrong' : [frame,0]");
    let saved = native_engine::object_codec::encode_storage_project(origin.document()).unwrap();
    assert_eq!(
        public_property_read(&s, 0, "expressionState")["evaluation"],
        json!({"state":"unavailable","reason":"not-evaluated"})
    );
    for frame in [0, 1] {
        let p = pins(&origin, frame);
        let prepared = s.with_object_preview_owner(g, &p, |owner| {
            owner.with_prepared_object_preview(&p, output.clone(), |_| Ok(()))
        });
        if frame == 0 {
            assert!(prepared.is_err());
            assert_eq!(s.seal_object_motion_diagnostics().unwrap(), Some(true));
        } else {
            prepared.unwrap(); // Successful reads publish their real clear before returning.
        }
        let result = public_property_read(&s, frame, "expressionState");
        let expected = if frame == 0 {
            json!({"kind":"mustReturnNumberOrXY"})
        } else {
            Value::Null
        };
        assert_eq!(
            result["evaluation"],
            json!({"state":"evaluated","diagnostic":expected})
        );
        assert_eq!(result["expression"], json!({"present":true,"enabled":true}));
        assert_eq!(result["frame"].as_f64(), Some(f64::from(frame)));
        let raw = public_property_read(&s, frame, "rawSample");
        assert_eq!(raw["kind"], "rawSample");
        assert_eq!(raw["value"], json!([0.0, 0.0]));
        assert!(raw.get("expression").is_none());
        assert!(raw.get("evaluation").is_none());
    }
    assert_eq!(
        public_property_read(&s, 0, "expressionState")["evaluation"],
        json!({"state":"unavailable","reason":"not-evaluated"}),
        "retired frame is unavailable"
    );
    let (current, depths, _) = owner_state(&s);
    assert_eq!((current.content_revision(), depths), (0, (0, 0)));
    assert_eq!(
        native_engine::object_codec::encode_storage_project(current.document()).unwrap(),
        saved
    );
}
