//! #2358 GHOST-UNDER: the onion request of a desktop preview is strict, viewport-only identity.
use super::*;

#[test]
fn onion_request_is_strict_viewport_identity_and_echoes_what_the_renderer_did() {
    let f = Fixture::new();
    let onion = json!({"layerUid":"L0","onion":true,"ghostAll":false,"from":0,"to":0,"previousOpacity":30,"nextOpacity":30,"mode":"tinted"});
    for bad in [
        json!({"layerUid":"L0","onion":true,"ghostAll":false,"from":0,"to":0,"previousOpacity":130,"nextOpacity":30,"mode":"tinted"}),
        json!({"layerUid":"L0","onion":false,"ghostAll":false,"from":0,"to":0,"previousOpacity":30,"nextOpacity":30,"mode":"tinted"}),
        json!({"layerUid":"","onion":true,"ghostAll":false,"from":0,"to":0,"previousOpacity":30,"nextOpacity":30,"mode":"tinted"}),
        json!({"layerUid":"L0","onion":true,"ghostAll":false,"from":0,"to":0,"previousOpacity":30,"nextOpacity":30,"mode":"loud"}),
        json!({"layerUid":"L0","onion":true,"ghostAll":false,"from":0,"to":0,"previousOpacity":30,"nextOpacity":30,"mode":"tinted","matrix":[1]}),
        json!(true),
    ] {
        let mut raw = f.raw.clone();
        raw["ghosts"] = bad;
        assert!(f.run(&raw.to_string()).is_err());
    }
    assert_eq!(
        f.creations.get(),
        0,
        "a malformed onion request never reaches the GPU"
    );
    let plain = f.run(&f.raw.to_string()).unwrap();
    assert!(plain.ghosts.is_none());
    assert!(serde_json::to_value(&plain)
        .unwrap()
        .get("ghosts")
        .is_none());
    let mut raw = f.raw.clone();
    raw["ghosts"] = onion.clone();
    f.trace.borrow_mut().color = [0, 0, 255, 255];
    let shown = f.run(&raw.to_string()).unwrap();
    let echoed = serde_json::to_value(&shown).unwrap()["ghosts"].clone();
    assert_eq!(
        echoed["status"], "empty",
        "a one-frame project has nothing to ghost"
    );
    assert_eq!(echoed["selected"]["layerUid"], "L0");
    assert_eq!(echoed["selected"]["mode"], "tinted");
    // Another onion setting is another presentation identity, so no cached frame is reused for it.
    raw["ghosts"]["mode"] = json!("outline");
    let outlined = f.run(&raw.to_string()).unwrap();
    assert_ne!(outlined.work_id, shown.work_id);
}
