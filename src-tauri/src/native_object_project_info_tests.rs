//! #2371 review: the project-info read is bounded by one hard cap, and hundreds of fully populated rows fit under it.
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::PROJECT_INFO_LIMIT_BYTES;
use crate::native_object_bootstrap::bootstrap_raw;
use serde_json::{json, Value};

fn open(state: &ApplicationMcp, bytes: &str) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"open","instanceId":state.instance_id(),"documentJson":bytes})
                .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}

/// Every project-info row carries kind, folder, blend, opacity and matte facts; the read must not
/// hard-fail at hundreds of fully populated layers (baseline Release to Layers makes one layer per shape).
fn rich_file(count: usize, name_bytes: usize) -> String {
    let uid = |i: usize| format!("ly_{i:05}_mh3k2j9a4f");
    let layers: Vec<_> = (0..count)
        .map(|i| {
            json!({"layerUid":uid(i),"name":format!("Layer {i} {}", "x".repeat(name_bytes)),"visible":true,
                "locked":false,"shy":false,"color":"#5FA875","parentLayerUid":null,"folderId":"f1",
                "blendMode":"multiply","matteMode":"alphaInverted","matteSourceLayerUid":uid((i + 1) % count),
                "mattesMore":[{"uid":uid((i + 2) % count),"mode":"luma"},{"uid":uid((i + 3) % count),"mode":"alpha"}]})
        })
        .collect();
    let holders: Vec<_> = (0..count)
        .map(|i| json!({"target":{"contextId":"scene-root","layerUid":uid(i)},
            "properties":[{"property":"opacity","static":[50.5]}]}))
        .collect();
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,"layers":layers,
            "layerFolders":{"f1":{"name":"F","collapsed":false}},"propertyHolders":holders,"objects":[]},
        "viewportReference":null,"assets":[]})
    .to_string()
}

#[test]
fn project_info_reads_hundreds_of_rich_rows_and_refuses_only_past_the_hard_cap() {
    let state = ApplicationMcp::default();
    let receipt = open(&state, &rich_file(600, 40));
    let id = receipt["documentId"].as_str().unwrap();
    let info = state.native_project_info(id).unwrap();
    let bytes = serde_json::to_vec(&info).unwrap().len();
    assert_eq!(info["layers"].as_array().unwrap().len(), 600);
    assert_eq!(info["layers"][599]["kind"], "draw");
    assert_eq!(info["layers"][599]["mattesMore"].as_array().unwrap().len(), 2);
    // The retired 64 KiB bound would have refused this answer; the hard cap still holds.
    assert!(bytes > 65536 && bytes <= PROJECT_INFO_LIMIT_BYTES, "{bytes}");
    // Past the cap the read is refused, never truncated.
    let big = ApplicationMcp::default();
    let (document, _, _) = big
        .install_native_project_file(rich_file(3, PROJECT_INFO_LIMIT_BYTES / 3).as_bytes())
        .unwrap();
    let err = big.native_project_info(&document).unwrap_err();
    assert!(err.contains("exceeds UI limit"), "{err}");
}
