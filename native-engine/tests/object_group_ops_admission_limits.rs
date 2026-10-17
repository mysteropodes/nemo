//! L02-G admission limits: the nesting cap and the baseline quirks the codec keeps
//! admitting (a stroke listed by two groups, even twice by one), plus the v2-only
//! foreign empty context. Helpers come from the admission file of this leaf.
use super::object_group_ops_admission::{doc, refusal, tab, IDS};
use crate::object_codec::{decode_ordered_project, decode_storage_project, ObjectCodecErrorKind};
use serde_json::{json, Value};

#[test]
fn nesting_is_admitted_through_sixteen_ancestors_and_refused_at_seventeen() {
    let ids: Vec<String> = (1..=19).map(|n| format!("s{n}")).collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    // c0 lists c1 and s1, c1 lists c2 and s2, ... the last group lists two strokes.
    let chain = |groups: usize| {
        let entries: Vec<Value> = (0..groups)
            .map(|k| {
                let order = if k + 1 < groups {
                    json!([format!("c{}", k + 1), refs[k]])
                } else {
                    json!([refs[k], refs[k + 1]])
                };
                let mut entry = json!({"gid":format!("c{k}"),"order":order});
                if k > 0 {
                    entry["parent"] = json!(format!("c{}", k - 1));
                }
                entry
            })
            .collect();
        tab(json!(entries), json!([]))
    };
    assert!(decode_storage_project(doc(1, &refs, chain(17)).to_string().as_bytes()).is_ok());
    assert_eq!(
        refusal(&doc(1, &refs, chain(18))),
        (
            ObjectCodecErrorKind::Invalid,
            "group nesting is deeper than 16 levels or cyclic".to_string()
        )
    );
}

#[test]
fn baseline_quirks_stay_admitted_and_v2_accepts_a_foreign_empty_context() {
    // combineSelection does not detach: s1 is listed by two groups; a dissolve can
    // promote a leaf into an order that already has it (a stroke twice in one order).
    let shared = tab(
        json!([{"gid":"a","order":["s1","s2"]},{"gid":"b","order":["s1","s3"]},
            {"gid":"d","order":["s4","s4"]}]),
        json!([{"strokeId":"s1","groupId":"b"},{"strokeId":"s2","groupId":"a"},
            {"strokeId":"s4","groupId":"d"}]),
    );
    for version in [1, 2] {
        let document = doc(version, &IDS, shared.clone());
        assert!(
            decode_storage_project(document.to_string().as_bytes()).is_ok(),
            "v{version}"
        );
    }
    let foreign = json!({"contextId":"component-1","entries":[],"members":[]});
    assert!(decode_ordered_project(doc(2, &IDS, foreign.clone()).to_string().as_bytes()).is_ok());
    assert!(decode_storage_project(doc(1, &IDS, foreign).to_string().as_bytes()).is_err());
    // A table whose context holds no strokes of the layer cannot list any.
    let bad =
        json!({"contextId":"component-1","entries":[{"gid":"g","order":["s1","s2"]}],"members":[]});
    assert!(decode_ordered_project(doc(2, &IDS, bad).to_string().as_bytes()).is_err());
}
