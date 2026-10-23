//! Real range admission now reaches immutable exposure selection; original 21 controls retained.
use super::{document, head, owner, Build};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use serde_json::{json, Value};

#[test]
fn admitted_ranges_select_the_literal_exposed_sources() {
    let mut checked = 0;
    for (range, visible) in [
        (json!({"inPoint":1}), [false, true, true]),
        (json!({"outPoint":0}), [true, false, false]),
        (json!({"timeLink":{"uid":"L1"}}), [true, true, true]),
        (
            json!({"inPoint":null,"outPoint":null,"timeLink":null}),
            [true, true, true],
        ),
        (
            json!({"markers":[{"frame":1,"name":"m"}]}),
            [true, true, true],
        ),
        (json!({}), [true, true, true]),
        (Value::Null, [true, true, true]),
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
            let packet =
                prepare_object_exposure_frame(&snapshot, &selector).expect("range evaluated");
            let shown = visible[frame as usize];
            assert_eq!(
                packet.source_frame_for_layer(0),
                shown.then_some(0),
                "{range} @{frame}"
            );
            assert_eq!(packet.source_frame_for_layer(1), Some(0));
            let layers: Vec<_> = packet
                .records()
                .map(|record| record.record().target().layer_uid().to_owned())
                .collect();
            assert_eq!(layers, if shown { vec!["L0", "L1"] } else { vec!["L1"] });
            checked += 1;
        }
    }
    assert_eq!(checked, 21);
}
