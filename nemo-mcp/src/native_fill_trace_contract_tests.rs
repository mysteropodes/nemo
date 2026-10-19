//! Persistent native fill fields must survive the same closed public vector shape.
use super::*;

#[test]
fn persisted_fill_provenance_accepts_authored_values_and_refuses_each_malformed_member() {
    for trace in [
        json!({"seed":[0,0]}),
        json!({"seeds":[]}),
        json!({"seed":[1.5,-2],"seeds":[[0,0],[20,20]],"walls":["wall-a","wall-b"],"gapPx":0}),
    ] {
        let mut r = record(created());
        r["vectorExt"] = json!({"fillTrace":trace});
        assert!(request(
            VECTOR,
            &json!({"command":"object.vector.create","record":r})
        ));
    }
    for trace in [
        json!({}),
        json!({"seed":[1]}),
        json!({"seed":[1,2,3]}),
        json!({"seed":null}),
        json!({"seed":[false,0]}),
        json!({"seeds":{}}),
        json!({"seeds":[[1]]}),
        json!({"seed":[0,0],"walls":"wall-a"}),
        json!({"seed":[0,0],"walls":[""]}),
        json!({"seed":[0,0],"walls":[1]}),
        json!({"seed":[0,0],"gapPx":-1}),
        json!({"seed":[0,0],"gapPx":null}),
        json!({"seed":[0,0],"extra":true}),
    ] {
        let mut r = record(created());
        r["vectorExt"] = json!({"fillTrace":trace});
        assert!(!request(
            VECTOR,
            &json!({"command":"object.vector.create","record":r})
        ));
    }
}
