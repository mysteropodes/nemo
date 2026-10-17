//! Negative and mutation controls of the nested-context resolver (L04-J, F14).
//!
//! Each deviation is a plausible wrong resolver: it resolves an oracle row by a different rule and
//! is replayed over EVERY settled state of both scripts (rows `V`). The honest resolver misses
//! none. Every deviation must miss at least the number of rows the same deviation misses when
//! written in JavaScript over the baseline state (`X` rows of the oracle, measured by the
//! generator), and always more than zero, so a control cannot pass vacuously.
use super::sequence_tests::replay;
use super::world::{fill_hex, inventory, rows, step_rows, World};
use super::*;

const INITIAL_ORDER: [&str; 2] = ["ly_comp_ia", "ly_comp_ib"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    Honest,
    /// Looks the stroke id up across all components, ignoring context, layer and frame.
    GlobalStroke,
    /// Addresses the inner layer by its position in the initial order.
    PositionalLayer,
    /// Matches context, layer and stroke but takes the first drawing whatever the frame.
    IgnoreFrame,
}

/// The fill the rule finds for an oracle row `[sym, layer, frame, stroke, fill]`.
fn resolve(rule: Rule, world: &World, row: &[String]) -> Option<String> {
    let (symbol, layer, frame, stroke) =
        (&row[0], &row[1], row[2].parse::<u32>().unwrap(), &row[3]);
    let all = inventory(&world.set);
    match rule {
        Rule::Honest => {
            let source = world.source();
            let context = source.context(symbol).ok()?;
            let identity = ElementIdentity::new(symbol, layer, stroke).ok()?;
            let hit = context.element_at(&identity, frame).ok()?;
            Some(fill_hex(hit.record))
        }
        Rule::GlobalStroke => all.iter().find(|r| &r[3] == stroke).map(|r| r[4].clone()),
        Rule::PositionalLayer => {
            let position = INITIAL_ORDER
                .iter()
                .position(|uid| *uid == layer.as_str())?;
            let component = world.set.component(symbol)?;
            let uid = component.layers().get(position)?.layer_uid().to_string();
            all.iter()
                .find(|r| &r[0] == symbol && r[1] == uid && r[2] == row[2] && &r[3] == stroke)
                .map(|r| r[4].clone())
        }
        Rule::IgnoreFrame => all
            .iter()
            .find(|r| &r[0] == symbol && &r[1] == layer && &r[3] == stroke)
            .map(|r| r[4].clone()),
    }
}

/// Rows of every settled state of both scripts that the rule does not reproduce.
fn misses(rule: Rule) -> (usize, usize) {
    let mut missed = 0;
    let mut total = 0;
    for scenario in ["edits", "twoContexts"] {
        replay(scenario, |step, session| {
            for row in step_rows(scenario, step) {
                total += 1;
                if resolve(rule, &session.world, &row).as_deref() != Some(row[4].as_str()) {
                    missed += 1;
                }
            }
        });
    }
    (missed, total)
}

fn measured(name: &str) -> usize {
    rows("X")
        .iter()
        .find(|fields| fields[1] == name)
        .map(|fields| fields[2].parse().unwrap())
        .expect("measured deviation")
}

#[test]
fn the_honest_resolver_reproduces_every_settled_row() {
    let (missed, total) = misses(Rule::Honest);
    assert_eq!(missed, 0);
    // edits: 26 strokes at step 0 and 27 at each of steps 7, 8 and 10 (two added, one deleted);
    // twoContexts: 26 strokes at each of its four settled steps.
    assert_eq!(total, 26 + 3 * 27 + 4 * 26);
}

#[test]
fn a_global_stroke_lookup_serves_the_look_alike_of_another_component() {
    let (missed, _) = misses(Rule::GlobalStroke);
    assert!(missed > 0);
    assert!(
        missed >= measured("globalStroke"),
        "{missed} < {}",
        measured("globalStroke")
    );
}

#[test]
fn a_positional_layer_address_breaks_when_the_inner_layers_are_reordered() {
    let (missed, _) = misses(Rule::PositionalLayer);
    assert!(missed > 0);
    assert!(
        missed >= measured("positionalLayer"),
        "{missed} < {}",
        measured("positionalLayer")
    );
}

#[test]
fn ignoring_the_frame_serves_the_wrong_keyed_drawing_of_a_spanning_element() {
    let (missed, _) = misses(Rule::IgnoreFrame);
    assert!(missed > 0);
    assert!(
        missed >= measured("ignoreFrame"),
        "{missed} < {}",
        measured("ignoreFrame")
    );
}

#[test]
fn the_measured_deviations_are_not_vacuous() {
    for name in ["globalStroke", "positionalLayer", "ignoreFrame"] {
        assert!(measured(name) > 0, "{name}");
    }
}

#[test]
fn resolving_never_changes_the_document_view() {
    let mut session = super::sequence_tests::Session::new();
    for row in rows("T").iter().filter(|row| row[1] == "edits").take(5) {
        session.apply(&row[4..]);
    }
    let before = session.world.set.clone();
    let layers = session.world.layers.clone();
    {
        let source = session.world.source();
        for identity in source.context("sym_comp").unwrap().identities() {
            for frame in 0..14 {
                let _ = source
                    .context("sym_comp")
                    .unwrap()
                    .element_at(&identity, frame);
            }
        }
        let _ = source.context("sym_comp_b").unwrap().exposed_targets(
            &source
                .context("sym_comp_b")
                .unwrap()
                .component()
                .unwrap()
                .exposed_props()[0],
        );
    }
    assert_eq!(session.world.set, before);
    assert_eq!(session.world.layers, layers);
}

#[test]
fn the_production_files_are_read_only_and_reach_siblings_by_absolute_paths() {
    let resolver = include_str!("../src/object_context_resolver.rs");
    let admission = include_str!("../src/object_context_admission.rs");
    let selection = include_str!("../src/object_context_selection.rs");
    for (name, text) in [
        ("resolver", resolver),
        ("admission", admission),
        ("selection", selection),
    ] {
        let production = text.split("#[cfg(test)]").next().unwrap();
        assert!(
            !production.contains("super::"),
            "{name}: R5, no super:: in production"
        );
        assert!(
            !production.contains("&mut "),
            "{name}: the kernel only reads"
        );
        assert!(!production.contains("unsafe"), "{name}");
        assert!(
            !production.contains("convert"),
            "{name}: conversion is an explicit request of L04-M"
        );
        assert!(
            !production.contains("symbol_id ="),
            "{name}: no symbol id is written or minted"
        );
    }
}
