//! Pinned literals for the fields a split piece carries, the matte and `layerUid`
//! rules, the combine groups and the promoted element Motion (oracle `P` rows:
//! fieldsAll, fieldsFalsy, fieldsEmpty, fieldsNull, matte*, groups*, motion).

use super::pinned_tests::{all_fields, carried, frame, layer, s, words, ALL_FIELDS};
use super::{plan_split, PieceMatte};
use crate::evaluation::object_component_kernels::object_context_split_facts::{
    CarryMode, CarryWhen, ElementMotion, FieldState, LayerFacts, StrokeFacts, CARRY_RULES,
};

#[test]
fn carried_fields_follow_the_baseline_order_and_tests() {
    // oracle: P fieldsAll / fieldsFalsy / fieldsEmpty / fieldsNull
    assert_eq!(carried(&all_fields(FieldState::Truthy)), ALL_FIELDS);
    let without_expr_controls: Vec<&str> = ALL_FIELDS
        .iter()
        .copied()
        .filter(|f| *f != "exprControls")
        .collect();
    assert_eq!(
        carried(&all_fields(FieldState::Empty)),
        without_expr_controls
    );
    let falsy = [
        "blendMode",
        "parentLayerUid",
        "parentLayerUidB",
        "inPoint",
        "outPoint",
        "threeD",
        "motionBlur",
        "effects",
        "pathFx",
        "blendKeys",
        "parentKeys",
        "parentsMore",
        "followPath",
        "markers",
        "textAnimators",
        "shy",
        "keyLock",
        "channel",
        "effectsFrom",
        "folderId",
        "linkGroupId",
        "isTextLayer",
    ];
    assert_eq!(carried(&all_fields(FieldState::Falsy)), falsy);
    let null = [
        "blendMode",
        "parentLayerUid",
        "parentLayerUidB",
        "inPoint",
        "outPoint",
        "threeD",
        "motionBlur",
    ];
    assert_eq!(carried(&all_fields(FieldState::Null)), null);
    assert!(carried(&all_fields(FieldState::Undefined)).is_empty());
    // Deep copies versus shared values, and the duplicator is never carried.
    let deep: Vec<&str> = CARRY_RULES
        .iter()
        .filter(|r| r.mode == CarryMode::DeepCopy)
        .map(|r| r.field)
        .collect();
    assert_eq!(
        deep,
        [
            "timeLink",
            "expressions",
            "exprControls",
            "effects",
            "pathFx",
            "blendKeys",
            "parentKeys",
            "parentsMore",
            "followPath",
            "markers",
            "textAnimators"
        ]
    );
    assert!(CARRY_RULES.iter().all(|r| r.field != "duplicator"));
    assert_eq!(
        CARRY_RULES
            .iter()
            .filter(|r| r.when == CarryWhen::NonEmpty)
            .count(),
        1
    );
}

#[test]
fn matte_layer_uid_and_groups_follow_the_baseline() {
    // oracle: P matteUid / matteLegacy / matteMoreEmpty / matteUndefMode / groups*
    let two = || layer(vec![frame(true, false, vec![s("a", true), s("b", true)])]);
    let matte = |l: &LayerFacts| -> Vec<PieceMatte> {
        plan_split(0, l, &words())
            .unwrap()
            .pieces
            .iter()
            .map(|p| p.matte)
            .collect()
    };
    let m = |mode, source_uid, more| PieceMatte {
        mode,
        source_uid,
        more,
    };
    let mut l = two();
    for f in ["matteMode", "matteSourceLayerUid", "mattesMore", "layerUid"] {
        l.fields.insert(f.to_string(), FieldState::Truthy);
    }
    assert_eq!(matte(&l), [m(true, true, true), m(true, true, true)]);
    let uid: Vec<bool> = plan_split(0, &l, &words())
        .unwrap()
        .pieces
        .iter()
        .map(|p| p.keeps_layer_uid)
        .collect();
    assert_eq!(uid, [false, true]);
    l.fields.insert("mattesMore".to_string(), FieldState::Empty);
    assert_eq!(matte(&l), [m(true, true, false), m(true, true, false)]);
    l.fields.remove("matteMode");
    assert_eq!(matte(&l), [m(false, true, false), m(false, true, false)]);
    // No frozen source: the legacy matte survives on the last piece only.
    let mut legacy = two();
    legacy
        .fields
        .insert("matteMode".to_string(), FieldState::Truthy);
    assert_eq!(
        matte(&legacy),
        [m(false, false, false), m(true, false, false)]
    );

    let mut g = layer(vec![frame(
        true,
        false,
        vec![
            s("a", true),
            StrokeFacts {
                group_id: Some("g1".to_string()),
                ..s("b", true)
            },
            StrokeFacts {
                group_id: Some("g1".to_string()),
                ..s("c", true)
            },
            StrokeFacts {
                group_id: Some("g2".to_string()),
                ..s("d", true)
            },
        ],
    )]);
    g.groups = Some(vec!["g1".to_string(), "g2".to_string(), "g3".to_string()]);
    let kept: Vec<Vec<String>> = plan_split(0, &g, &words())
        .unwrap()
        .pieces
        .iter()
        .map(|p| p.kept_groups.clone())
        .collect();
    assert_eq!(kept, [vec![], vec![], vec![], vec!["g2".to_string()]]);
    g.groups = Some(Vec::new());
    assert!(plan_split(0, &g, &words())
        .unwrap()
        .pieces
        .iter()
        .all(|p| p.kept_groups.is_empty()));
}

#[test]
fn element_motion_is_promoted_to_its_own_piece() {
    // oracle: P motion 0..2
    let mut l = layer(vec![frame(
        true,
        false,
        vec![s("a", true), s("b", true), s("c", true)],
    )]);
    for (id, m, st) in [("a", true, false), ("b", true, true), ("c", false, true)] {
        l.element_motion.insert(
            id.to_string(),
            ElementMotion {
                has_motion: m,
                has_motion_static: st,
            },
        );
    }
    let got: Vec<(bool, bool)> = plan_split(0, &l, &words())
        .unwrap()
        .pieces
        .iter()
        .map(|p| (p.promote_motion, p.promote_motion_static))
        .collect();
    assert_eq!(got, [(true, false), (true, true), (false, true)]);
}
