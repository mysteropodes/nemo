//! #2371: the baseline export honours layer solo (export.js `exportBuildFrame` resolves it through `layerIsEffectivelyVisible`), so
//! the Solo set the export request carries reaches the PNG witnesses of the original owner; a later export without it is plain.
use super::{decode_png, output, selector, Scratch};
use crate::native_dispatch::NativeObjectHost;
use native_engine::{compositor::Compositor, export_job::JobStatus, object_codec::decode_storage_project};
use serde_json::{json, Value};
use std::fs;

/// L0 blue square at 4..28, L1 red square at 36..60, on a 64 px canvas.
fn host() -> NativeObjectHost {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let square = |layer: &str, id: &str, x: i32, rgb: [u8; 3]| {
        let mut record = cases["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":layer,"strokeId":id});
        record["geometry"]["segments"] = json!([[x, 4], [x + 24, 4], [x + 24, 28], [x, 28]]
            .map(|[x, y]| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        record["fill"] = json!({"kind":"solid","r":rgb[0],"g":rgb[1],"b":rgb[2],"a":1});
        record
    };
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let document = json!({
        "format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
        "layers":[{"layerUid":"L0","timing":timing},{"layerUid":"L1","timing":timing}],
        "objects":[square("L0", "a", 4, [0, 0, 1]), square("L1", "b", 36, [1, 0, 0])],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}},{"layerUid":"L1","naturalPosition":1,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},
                    "elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}},{"layerUid":"L1","strokeId":"b","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}
    });
    NativeObjectHost::new("desktop-object-export-solo".into(), decode_storage_project(&serde_json::to_vec(&document).unwrap()).unwrap()).unwrap()
}

#[test]
#[cfg(target_os = "macos")]
fn the_png_export_paints_only_the_soloed_layers_and_the_next_export_is_plain_again() {
    let mut owner = host();
    let snapshot = owner.history.acquire_snapshot(0).unwrap();
    let expected = selector(&snapshot);
    let mut gpu = Compositor::new().unwrap();
    // A published frame is never overwritten, so every export gets its own scratch destination.
    let mut run = |owner: &mut NativeObjectHost, id: &str, solo: &[&str]| {
        let scratch = Scratch::new();
        let mut port = scratch.port();
        owner.set_export_solo(solo.iter().map(|s| (*s).to_owned()).collect());
        let outcome = owner
            .export_pinned_object_png(&expected, id.into(), "object-output".into(), output(), 65536, &mut port, &mut gpu)
            .unwrap();
        assert_eq!(outcome.receipt.unwrap().status, JobStatus::Succeeded);
        let rgba = decode_png(&fs::read(scratch.0.join("published/frame_0001.png")).unwrap());
        let at = |x: usize, y: usize| rgba[(y * 64 + x) * 4..][..4].to_vec();
        (at(16, 16), at(48, 16))
    };
    let (left, right) = run(&mut owner, "plain", &[]);
    assert_eq!((left, right), (vec![0, 0, 255, 255], vec![255, 0, 0, 255]), "no solo: both layers paint");
    let (left, right) = run(&mut owner, "solo-l1", &["L1"]);
    assert_eq!(right, vec![255, 0, 0, 255], "the soloed L1 paints");
    assert_eq!(left, vec![255; 4], "L0 is left out: the white document background shows");
    let (left, right) = run(&mut owner, "solo-l0", &["L0"]);
    assert_eq!((left, right), (vec![0, 0, 255, 255], vec![255; 4]));
    let (left, right) = run(&mut owner, "plain-again", &[]);
    assert_eq!((left, right), (vec![0, 0, 255, 255], vec![255, 0, 0, 255]), "an export with no set is plain again");
}
