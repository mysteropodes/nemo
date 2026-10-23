//! Pinned literals for the element-splitting plan: the baseline facts written out by
//! hand on small layers (each cross-checked against the oracle rows named in its
//! comment), so a regression names the exact rule that moved.

use super::{plan_manual_split, plan_split, PieceFrame, SplitRefusal};
use crate::evaluation::object_component_kernels::object_context_split_facts::{
    element_label, layer_elements, unstamped_positions, ElementWords, FieldState, FrameFacts,
    LayerFacts, StrokeFacts, CARRY_RULES,
};

pub(crate) fn words() -> ElementWords {
    ElementWords {
        shape: "Forme".to_string(),
        stroke: "Trait".to_string(),
    }
}

pub(crate) fn s(id: &str, fill: bool) -> StrokeFacts {
    StrokeFacts {
        stroke_id: Some(id.to_string()),
        has_fill: fill,
        ..StrokeFacts::default()
    }
}

pub(crate) fn frame(key: bool, interp: bool, strokes: Vec<StrokeFacts>) -> FrameFacts {
    FrameFacts {
        is_keyframe: key,
        is_interpolated: interp,
        strokes,
    }
}

/// A layer whose reference strokes are the strokes of its first frame.
pub(crate) fn layer(frames: Vec<FrameFacts>) -> LayerFacts {
    LayerFacts {
        name: Some("Layer1".to_string()),
        reference_strokes: frames[0].strokes.clone(),
        frames,
        ..LayerFacts::default()
    }
}

pub(crate) fn names(l: &LayerFacts) -> Vec<String> {
    plan_split(0, l, &words())
        .unwrap()
        .pieces
        .into_iter()
        .map(|p| p.name)
        .collect()
}

pub(crate) fn pf(key: bool, interp: bool, idx: &[usize]) -> PieceFrame {
    PieceFrame {
        is_keyframe: key,
        is_interpolated: interp,
        stroke_indices: idx.to_vec(),
    }
}

pub(crate) fn all_fields(state: FieldState) -> LayerFacts {
    let mut l = layer(vec![frame(true, false, vec![s("a", true), s("b", true)])]);
    for r in CARRY_RULES {
        l.fields.insert(r.field.to_string(), state);
    }
    l
}

pub(crate) fn carried(l: &LayerFacts) -> Vec<&'static str> {
    plan_split(0, l, &words()).unwrap().pieces[0]
        .carry
        .iter()
        .map(|c| c.field)
        .collect()
}

pub(crate) const ALL_FIELDS: [&str; 25] = [
    "blendMode",
    "parentLayerUid",
    "parentLayerUidB",
    "timeLink",
    "inPoint",
    "outPoint",
    "expressions",
    "exprControls",
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

#[test]
fn two_shapes_become_two_named_layers() {
    // oracle: P two 0/1
    let plan = plan_split(
        3,
        &layer(vec![frame(true, false, vec![s("a", true), s("b", true)])]),
        &words(),
    )
    .unwrap();
    assert_eq!(plan.layer_index, 3);
    let got: Vec<&str> = plan.pieces.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(got, ["Layer1 \u{2014} Forme 1", "Layer1 \u{2014} Forme 2"]);
    assert_eq!(plan.pieces[0].frames, vec![pf(true, false, &[0])]);
    assert_eq!(plan.pieces[1].frames, vec![pf(true, false, &[1])]);
    assert!(plan.unstamped.is_empty());
}

#[test]
fn the_split_is_by_position_so_a_texture_copy_and_an_overflow_land_in_the_last_piece() {
    // oracle: P foldTexture 1 (K[1,2,3]) and P overflow 1 (K[1,2,3])
    let mut tex = s("b", false);
    tex.is_brush_texture_copy = true;
    let mut tex_d = s("d", false);
    tex_d.is_brush_texture_copy = true;
    let l = layer(vec![frame(
        true,
        false,
        vec![s("a", true), tex, s("c", true), tex_d],
    )]);
    let plan = plan_split(0, &l, &words()).unwrap();
    assert_eq!(plan.pieces.len(), 2);
    assert_eq!(plan.pieces[0].frames, vec![pf(true, false, &[0])]);
    assert_eq!(plan.pieces[1].frames, vec![pf(true, false, &[1, 2, 3])]);

    let l = layer(vec![
        frame(true, false, vec![s("a", true), s("b", true)]),
        frame(false, false, vec![]),
        frame(
            true,
            false,
            vec![s("a", true), s("b", true), s("c", true), s("d", true)],
        ),
    ]);
    let plan = plan_split(0, &l, &words()).unwrap();
    assert_eq!(plan.pieces[0].frames[2], pf(true, false, &[0]));
    assert_eq!(plan.pieces[1].frames[2], pf(true, false, &[1, 2, 3]));
}

#[test]
fn frames_that_are_neither_key_nor_interpolated_become_blank_and_short_keyframes_leave_gaps() {
    // oracle: P staleBlank (blank frame with stale strokes), P shortKey
    let l = layer(vec![
        frame(true, false, vec![s("a", true), s("b", true), s("c", true)]),
        frame(false, false, vec![s("x", true), s("y", true)]),
        frame(false, true, vec![s("a", true)]),
        frame(true, true, vec![s("a", true), s("b", true)]),
    ]);
    let plan = plan_split(0, &l, &words()).unwrap();
    let blank = pf(false, false, &[]);
    assert_eq!(
        plan.pieces[0].frames,
        vec![
            pf(true, false, &[0]),
            blank.clone(),
            pf(false, true, &[0]),
            pf(true, true, &[0])
        ]
    );
    assert_eq!(
        plan.pieces[1].frames,
        vec![
            pf(true, false, &[1]),
            blank.clone(),
            pf(false, true, &[]),
            pf(true, true, &[1])
        ]
    );
    assert_eq!(
        plan.pieces[2].frames,
        vec![
            pf(true, false, &[2]),
            blank,
            pf(false, true, &[]),
            pf(true, true, &[])
        ]
    );
}

#[test]
fn labels_follow_custom_names_images_and_the_linked_fill() {
    // oracle: E rasterNamed, E linkedFillOwner
    let mut raster = s("b", false);
    raster.is_raster = true;
    let mut l = layer(vec![frame(
        true,
        false,
        vec![s("a", true), raster, s("c", false)],
    )]);
    l.shape_names.insert("a".to_string(), "Eye".to_string());
    l.shape_names.insert("c".to_string(), String::new());
    assert_eq!(
        names(&l),
        [
            "Layer1 \u{2014} Eye",
            "Layer1 \u{2014} Image 2",
            "Layer1 \u{2014} Trait 3"
        ]
    );

    // The owner's label reads its COMPANION's fill, never its own.
    let owner = |id: &str, fill: bool, link: &str| StrokeFacts {
        linked_fill_id: Some(link.to_string()),
        ..s(id, fill)
    };
    let companion = |id: &str, fill: bool, link: &str| StrokeFacts {
        is_linked_fill_companion: true,
        ..owner(id, fill, link)
    };
    let l = layer(vec![frame(
        true,
        false,
        vec![
            owner("a", false, "L1"),
            companion("b", true, "L1"),
            owner("c", true, "L2"),
            companion("d", false, "L2"),
        ],
    )]);
    let els = layer_elements(&l);
    assert_eq!(
        els.iter().map(|e| e.position).collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(element_label(&els[0], 0, &l, &words()), "Forme 1");
    assert_eq!(element_label(&els[1], 1, &l, &words()), "Trait 2");
}

#[test]
fn refusals_and_the_manual_guard() {
    // oracle: O symbolLayer nosym, O oneElement few, O noStrokes few, M rows
    let mut sym = layer(vec![frame(true, false, vec![s("a", true), s("b", true)])]);
    sym.has_symbol = true;
    assert_eq!(plan_split(0, &sym, &words()), Err(SplitRefusal::HasSymbol));
    let one = layer(vec![frame(true, false, vec![s("a", true)])]);
    assert_eq!(
        plan_split(0, &one, &words()),
        Err(SplitRefusal::TooFewElements { count: 1 })
    );
    let none = layer(vec![frame(true, false, vec![])]);
    assert_eq!(
        plan_split(0, &none, &words()),
        Err(SplitRefusal::TooFewElements { count: 0 })
    );
    let two = layer(vec![frame(true, false, vec![s("a", true), s("b", true)])]);
    assert_eq!(
        plan_manual_split(Some("comp"), 0, &two, &words()),
        Err(SplitRefusal::ComponentOpen)
    );
    assert_eq!(
        plan_manual_split(None, 0, &two, &words()),
        plan_split(0, &two, &words())
    );
}

#[test]
fn the_name_falls_back_to_layer() {
    // oracle: P nameVariants / nameUndefined
    for name in [None, Some(String::new())] {
        let mut l = layer(vec![frame(true, false, vec![s("a", true), s("b", true)])]);
        l.name = name;
        assert_eq!(
            names(&l),
            ["Layer \u{2014} Forme 1", "Layer \u{2014} Forme 2"]
        );
    }
}

#[test]
fn strokes_without_an_id_are_listed_for_the_owner_never_minted() {
    // oracle: E noId (u=0,2,3), P noId
    let no_id = || StrokeFacts {
        stroke_id: None,
        ..StrokeFacts::default()
    };
    let companion = StrokeFacts {
        has_fill: true,
        is_linked_fill_companion: true,
        linked_fill_id: Some("L9".to_string()),
        ..no_id()
    };
    let l = layer(vec![frame(
        true,
        false,
        vec![
            StrokeFacts {
                has_fill: true,
                ..no_id()
            },
            s("noId_b", true),
            companion,
            no_id(),
        ],
    )]);
    assert_eq!(unstamped_positions(&l), vec![0, 2, 3]);
    let plan = plan_split(0, &l, &words()).unwrap();
    assert_eq!(plan.unstamped, vec![0, 2, 3]);
    let ids: Vec<Option<&str>> = plan
        .pieces
        .iter()
        .map(|p| p.element_stroke_id.as_deref())
        .collect();
    assert_eq!(ids, [None, Some("noId_b"), None]);
}
