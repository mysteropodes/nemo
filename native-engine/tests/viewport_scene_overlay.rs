//! CPU contract of the viewport-only overlay gate against the baseline oracle.
//! Expected labels come from the verbatim `buildSceneJson` block run in Node
//! (fixtures/viewport-scene-overlay/oracle.txt); scenario membership, counts
//! and the hand-derived stacks below are independent literals.
use super::*;

const ORACLE: &str = include_str!("fixtures/viewport-scene-overlay/oracle.txt");
const SOURCE: &str = include_str!("../src/viewport_scene_overlay.rs");

pub(super) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split(' ').collect::<Vec<_>>())
        .filter(|row| row[0] == kind)
        .collect()
}

pub(super) fn label(slot: &StackSlot) -> String {
    match slot {
        StackSlot::Artwork(i) => format!("A{i}"),
        StackSlot::Overlay(category) => category.baseline_source().to_owned(),
    }
}

/// Membership is written here, not read from the oracle.
pub(super) fn scenario_set(name: &str) -> ViewportOverlaySet {
    let members: Vec<OverlayCategory> = match name {
        "full" => OVERLAY_CATEGORIES.to_vec(),
        "subset" => vec![
            OverlayCategory::EffectorRings,
            OverlayCategory::EraserCursor,
            OverlayCategory::ImageMeshHandles,
            OverlayCategory::Marquee,
            OverlayCategory::OnionSkin,
        ],
        "pair" => vec![OverlayCategory::GhostAll, OverlayCategory::OnionSkin],
        "guides" => vec![
            OverlayCategory::GuideLayer,
            OverlayCategory::SafetyZones,
            OverlayCategory::SmartGuides,
        ],
        "none" => vec![],
        other => panic!("unknown scenario {other}"),
    };
    let mut set = ViewportOverlaySet::new();
    for category in members {
        set.add_layer(category, 1).unwrap();
    }
    set
}

/// True when `stack` reproduces every oracle `stack` row; returns the row count
/// through `rows_checked` so a vacuous pass is detectable.
pub(super) fn oracle_stack_rows_match<F>(stack: F, rows_checked: &mut usize) -> bool
where
    F: Fn(&ViewportOverlaySet, usize, bool) -> Vec<StackSlot>,
{
    let mut all = true;
    for row in rows("stack") {
        let len: usize = row[2].strip_prefix("len=").unwrap().parse().unwrap();
        let skip = row[3] == "skip_volatile=1";
        let set = if row[4] == "off" {
            ViewportOverlaySet::new()
        } else {
            scenario_set(row[1])
        };
        let got: Vec<String> = stack(&set, len, !skip).iter().map(label).collect();
        all &= got.join(" ") == row[5..].join(" ");
        *rows_checked += 1;
    }
    all
}

/// Source lines that import anything, and whether the production half names
/// `super::`, serialization or interior-shared state.
pub(super) fn production(text: &str) -> &str {
    text.split("\n#[cfg(").next().unwrap()
}
pub(super) fn offending_lines(text: &str) -> Vec<String> {
    let mut found = vec![];
    for line in production(text).lines() {
        let code = line.trim();
        if code.starts_with("//") {
            continue;
        }
        let imports = code.starts_with("use ")
            && code != "use crate::object_render_scene::ObjectCompositionWitness;";
        let forbidden = [
            "super::",
            "serde",
            "Serialize",
            "Mutex",
            "RefCell",
            "thread_local",
            "OnceLock",
        ]
        .iter()
        .any(|word| code.contains(*word));
        if imports || forbidden {
            found.push(code.to_owned());
        }
    }
    found
}

#[test]
fn oracle_facts_and_row_counts_are_pinned() {
    assert!(ORACLE.contains("fact baseline_commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc\n"));
    assert!(ORACLE.contains("fact overlay_block_line 2417\n"));
    assert!(ORACLE.contains("fact overlay_sources 30\n"));
    assert!(ORACLE.contains("fact below_artwork 2\n"));
    assert!(ORACLE.contains("fact volatile 5\n"));
    assert_eq!(rows("eval").len(), 30);
    assert_eq!(rows("stack").len(), 14);
    assert_eq!(rows("flag").len(), 5);
    assert_eq!(rows("callsite").len(), 2);
    assert_eq!(rows("mode").len(), 5);
    assert_eq!(OVERLAY_CATEGORIES.len(), 30);
}

#[test]
fn categories_follow_the_baseline_evaluation_order_placement_and_volatility() {
    let eval = rows("eval");
    assert_eq!(eval.len(), OVERLAY_CATEGORIES.len());
    for (i, row) in eval.iter().enumerate() {
        let category = OVERLAY_CATEGORIES[i];
        assert_eq!(row[1], i.to_string(), "row index");
        assert_eq!(row[2], category.baseline_source(), "source of {i}");
        assert_eq!(
            category as usize, i,
            "declaration order is evaluation order"
        );
        let below = row[3] == "below";
        assert_eq!(
            category.placement(),
            if below {
                OverlayPlacement::BelowArtwork
            } else {
                OverlayPlacement::AboveArtwork
            },
            "placement of {}",
            row[2]
        );
        assert_eq!(category.is_volatile(), row[4] == "volatile", "{}", row[2]);
    }
    for pair in OVERLAY_CATEGORIES.windows(2) {
        assert!(pair[0] < pair[1]);
    }
    let volatile: Vec<_> = OVERLAY_CATEGORIES
        .iter()
        .filter(|c| c.is_volatile())
        .collect();
    assert_eq!(volatile.len(), 5);
    let below: Vec<_> = OVERLAY_CATEGORIES
        .iter()
        .filter(|c| c.placement() == OverlayPlacement::BelowArtwork)
        .collect();
    assert_eq!(
        below,
        [&OverlayCategory::OnionSkin, &OverlayCategory::GhostAll]
    );
}

#[test]
fn every_category_and_display_mode_is_viewport_only() {
    for category in OVERLAY_CATEGORIES {
        assert_eq!(category.disposition(), OverlayDisposition::ViewportOnly);
    }
    for mode in [
        EditorDisplayMode::CurrentFrameOutline,
        EditorDisplayMode::OutlineView,
    ] {
        assert_eq!(mode.disposition(), OverlayDisposition::ViewportOnly);
    }
    assert_eq!(OVERLAY_CATEGORIES.len() + 2, 32);
}

#[test]
fn stack_rows_replay_the_verbatim_baseline_splice_and_push() {
    let mut checked = 0;
    assert!(oracle_stack_rows_match(
        |set, len, volatile| set.stack(len, volatile),
        &mut checked
    ));
    assert_eq!(checked, 14);
}

#[test]
fn stack_literals_are_hand_derived_and_not_read_from_the_oracle() {
    use OverlayCategory::*;
    use StackSlot::{Artwork as A, Overlay as O};
    let mut set = ViewportOverlaySet::new();
    for category in [Marquee, GhostAll, EraserCursor, OnionSkin] {
        set.add_layer(category, 3).unwrap();
    }
    // Zero artwork: both below layers land at index min(1, len) in evaluation order.
    assert_eq!(
        set.stack(0, true),
        [O(OnionSkin), O(GhostAll), O(Marquee), O(EraserCursor)]
    );
    // One artwork layer: each splice goes in right after it, so ghost ends up first.
    assert_eq!(
        set.stack(1, true),
        [A(0), O(GhostAll), O(OnionSkin), O(Marquee), O(EraserCursor)]
    );
    assert_eq!(
        set.stack(3, true),
        [
            A(0),
            O(GhostAll),
            O(OnionSkin),
            A(1),
            A(2),
            O(Marquee),
            O(EraserCursor)
        ]
    );
    // skipVolatile drops only the pointer-following preview.
    assert_eq!(
        set.stack(2, false),
        [A(0), O(GhostAll), O(OnionSkin), A(1), O(Marquee)]
    );
    assert_eq!(ViewportOverlaySet::new().stack(2, true), [A(0), A(1)]);
}

#[test]
fn set_semantics_zero_counts_duplicates_order_and_modes() {
    use OverlayCategory::*;
    let mut set = ViewportOverlaySet::new();
    assert!(set.is_empty());
    set.add_layer(Marquee, 0).unwrap();
    assert!(set.is_empty(), "a zero-item builder pushes no layer");
    set.add_layer(NullLayer, 2).unwrap();
    set.add_layer(NodeHandles, 5).unwrap();
    set.add_layer(OnionSkin, 1).unwrap();
    let order: Vec<_> = set.layers().iter().map(|l| l.category()).collect();
    assert_eq!(order, [OnionSkin, NodeHandles, NullLayer]);
    assert_eq!(set.layers()[1].item_count(), 5);
    let before = set.clone();
    assert_eq!(
        set.add_layer(NodeHandles, 9),
        Err(OverlaySetError::DuplicateCategory(NodeHandles))
    );
    assert_eq!(set, before, "a refused duplicate changes nothing");
    assert_eq!(set.add_layer(NodeHandles, 0), Ok(()));
    assert_eq!(set, before, "an empty duplicate is still a no-op");
    set.enable_mode(EditorDisplayMode::OutlineView);
    set.enable_mode(EditorDisplayMode::CurrentFrameOutline);
    set.enable_mode(EditorDisplayMode::OutlineView);
    assert_eq!(
        set.modes(),
        [
            EditorDisplayMode::CurrentFrameOutline,
            EditorDisplayMode::OutlineView
        ]
    );
    assert!(!set.is_empty());
    let mut modes_only = ViewportOverlaySet::new();
    modes_only.enable_mode(EditorDisplayMode::OutlineView);
    assert!(!modes_only.is_empty());
    assert!(modes_only.layers().is_empty());
}

#[test]
fn flag_default_and_export_callsites_are_the_baseline_facts_the_gate_encodes() {
    let flags: Vec<_> = rows("flag").iter().map(|r| (r[1], r[2])).collect();
    assert_eq!(
        flags,
        [
            ("absent", "1"),
            ("true", "1"),
            ("false", "0"),
            ("undefined", "1"),
            ("null", "1")
        ]
    );
    let sites: Vec<_> = rows("callsite").iter().map(|r| r.join(" ")).collect();
    assert_eq!(
        sites,
        [
            "callsite renderFrameRawPixels forExport=1 includeEditorOverlays=0",
            "callsite buildSceneJsonForFrame forExport=0 includeEditorOverlays=0"
        ]
    );
}

#[test]
fn display_modes_change_the_scene_only_when_the_flag_is_on() {
    let mode = |name: &str, state: &str| -> String {
        rows("mode")
            .into_iter()
            .find(|r| r[1] == name && r[2] == state)
            .unwrap()[3..]
            .join(" ")
    };
    let untouched = r#"{"fillColor":[1,2,3,255],"fillGradient":{"stops":2},"strokeColor":null,"strokeWidth":3}"#;
    assert_eq!(mode("currentFrameOutline", "off"), untouched);
    assert_eq!(
        mode("currentFrameOutline", "on"),
        r#"{"fillColor":null,"strokeColor":[235,235,240,128],"strokeWidth":0.75}"#
    );
    assert_eq!(
        mode("outlineView", "off"),
        r#"[{"fillColor":[1,2,3,255],"strokeColor":null,"strokeWidth":3,"fillGradient":{"stops":2}},{"imageId":"im"}]"#
    );
    assert_eq!(
        mode("outlineView", "on"),
        r#"[{"fillColor":null,"strokeColor":[0,0,0,255],"strokeWidth":1},{"imageId":"im"}]"#
    );
    assert!(ORACLE.contains("mode currentFrameOutline retained_geometry_fast_path_excluded 1\n"));
}

#[test]
fn production_text_imports_only_the_witness_owner_and_holds_no_shared_state() {
    assert_eq!(offending_lines(SOURCE), Vec::<String>::new());
    let used: Vec<_> = production(SOURCE)
        .lines()
        .filter(|l| l.starts_with("use "))
        .collect();
    assert_eq!(
        used,
        ["use crate::object_render_scene::ObjectCompositionWitness;"]
    );
}
