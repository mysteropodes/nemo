//! T12: the stored video timing members of a drawing placement (`timeRemap`,
//! `interpMode`, `frameBlend`) round trip, keep absence distinct from a value,
//! and refuse `null`, wrong types and non-video targets.
use super::*;
use crate::object_codec::project::{decode_project, encode_project, ProjectWireLimits};
use crate::object_document::ProjectEnvelope;

const HEAD: &str = r#"{"format":"nemo.native-project-document","formatVersion":2,"project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,"layers":[{"layerUid":"L"}],"objects":[]},"viewportReference":null,"assets":[{"id":"v1","name":"take-1","kind":"video","source":{"kind":"linked","locator":"clips/take-1.mov"}},{"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"audio/score.wav"}}],"assetInventory":{"assetIds":["v1","a1"],"placements":["#;
const VIDEO: &str = r#"{"id":"p1","assetId":"v1","target":{"kind":"drawing","contextId":"scene-root","layerUid":"L"},"offsetFrames":-3"#;
const AUDIO: &str =
    r#"{"id":"p2","assetId":"a1","target":{"kind":"audioTrack","trackUid":"t1"},"offsetFrames":0"#;
const REMAP: &str =
    r#""timeRemap":{"keys":[{"frame":0,"v":[4]},{"frame":12,"v":[16],"hold":true}]}"#;

fn limits() -> ProjectWireLimits {
    ProjectWireLimits::new(1 << 20, 1 << 16).unwrap()
}
fn wire(placement: &str, members: &str) -> String {
    let members = if members.is_empty() {
        String::new()
    } else {
        format!(",{members}")
    };
    format!("{HEAD}{placement}{members}}}]}}}}")
}
fn decode(text: &str) -> Result<ProjectEnvelope, ProjectWireError> {
    decode_project(text.as_bytes(), limits())
}
fn timing(project: &ProjectEnvelope) -> VideoTiming {
    project.asset_inventory().unwrap().placements()[0]
        .timing()
        .clone()
}
fn tail(text: &str) -> &str {
    &text[text.find("\"assetInventory\"").unwrap()..]
}

#[test]
fn each_member_round_trips_byte_for_byte_and_absence_stays_absent() {
    for members in [
        String::new(),
        REMAP.to_owned(),
        r#""interpMode":"blend""#.to_owned(),
        r#""frameBlend":false"#.to_owned(),
        format!(r#"{REMAP},"interpMode":"motion","frameBlend":true"#),
    ] {
        let text = wire(VIDEO, &members);
        let project = decode(&text).unwrap_or_else(|e| panic!("{members}: {e:?}"));
        let saved = String::from_utf8(encode_project(&project, limits()).unwrap()).unwrap();
        assert_eq!(tail(&saved), tail(&text), "{members}");
        assert_eq!(timing(&project).is_empty(), members.is_empty());
    }
}

#[test]
fn decoded_members_reach_the_placement_record() {
    let text = wire(
        VIDEO,
        &format!(r#"{REMAP},"interpMode":"blend","frameBlend":true"#),
    );
    let decoded = timing(&decode(&text).unwrap());
    assert_eq!(decoded.interp_mode.as_deref(), Some("blend"));
    assert_eq!(decoded.frame_blend, Some(true));
    assert_eq!(decoded.remap_value(6.0), Some(10.0));
    assert_eq!(decoded.remap_value(30.0), Some(16.0));
}

#[test]
fn null_wrong_types_bad_tracks_and_non_video_targets_are_refused() {
    let refused = |placement: &str, members: &str| {
        assert!(
            matches!(
                decode(&wire(placement, members)),
                Err(ProjectWireError::Invalid)
            ),
            "{members} on {placement}"
        );
    };
    for members in [
        r#""timeRemap":null"#,
        r#""interpMode":null"#,
        r#""frameBlend":null"#,
        r#""frameBlend":1"#,
        r#""frameBlend":"true""#,
        r#""interpMode":7"#,
        r#""interpMode":"""#,
        r#""timeRemap":[]"#,
        r#""timeRemap":{"keys":[{"frame":3,"v":[0]},{"frame":1,"v":[1]}]}"#,
        r#""timeRemap":{"keys":[{"frame":0,"v":[0]}],"extra":1}"#,
    ] {
        refused(VIDEO, members);
    }
    let long = format!(r#""interpMode":"{}""#, "m".repeat(MAX_INTERP_MODE + 1));
    assert!(matches!(
        decode(&wire(VIDEO, &long)),
        Err(ProjectWireError::Budget)
    ));
    for members in [REMAP, r#""interpMode":"blend""#, r#""frameBlend":false"#] {
        refused(AUDIO, members);
    }
    // The audio placement itself, without timing, is still admitted.
    assert!(decode(&wire(AUDIO, "")).is_ok());
}
