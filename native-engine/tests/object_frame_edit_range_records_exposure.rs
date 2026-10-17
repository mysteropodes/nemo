//! A layer whose stored range hides frames keeps the exposure producer typed unavailable: the native
//! evaluator does not consume in/out points or time links yet (D27), and painting every frame of such a
//! layer would be a fake success. Ranges that gate nothing (null members, markers only) do not block it.
//! Needs the `evaluation` feature (run with `--features test-history,evaluation`).
use super::{document, head, owner, Build};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, ObjectFrameError, ObjectFrameSelector,
};
use serde_json::{json, Value};

#[test]
fn a_range_that_hides_frames_keeps_the_exposure_packet_unavailable() {
    let mut checked = 0;
    for (range, gates) in [
        (json!({"inPoint":1}), true),
        (json!({"outPoint":0}), true),
        (json!({"timeLink":{"uid":"L1"}}), true),
        (
            json!({"inPoint":null,"outPoint":null,"timeLink":null}),
            false,
        ),
        (json!({"markers":[{"frame":1,"name":"m"}]}), false),
        (json!({}), false),
        (Value::Null, false),
    ] {
        let o = owner(document(
            3,
            &[(false, range.clone()), (false, Value::Null)],
            Build::V1,
        ));
        let snapshot = head(&o);
        for frame in 0..3 {
            let selector = ObjectFrameSelector {
                instance_id: snapshot.instance_id(),
                document_id: snapshot.document_id(),
                content_revision: snapshot.content_revision(),
                document_snapshot_id: snapshot.snapshot_id(),
                context_id: "scene-root",
                scope_kind: FrameScopeKind::Authored,
                frame,
            };
            let result = prepare_object_exposure_frame(&snapshot, &selector);
            assert_eq!(result.is_err(), gates, "{range} frame {frame}");
            if gates {
                assert_eq!(result.err(), Some(ObjectFrameError::Unavailable), "{range}");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 21);
}
