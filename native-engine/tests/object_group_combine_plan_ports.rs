//! Provider-port controls for the combine operand plan (G24/G19, L02-B).
//!
//! The geometry provider is outside this crate (lead ruling R3), so the kernel
//! is exercised against a hand-built boolean over disjoint 1-D pieces. That
//! port is a conformance probe, not a stand-in for geometry: it proves the
//! kernel hands the provider EVERY operand in group order in ONE call, that a
//! provider which drops the middle piece is detectable, and that
//! unavailability and failure stay typed.

use super::tests::{grp, lay, Lay, Rec};
use super::{
    execute_flatten, plan_flatten, render_combines, CombineError, CombineFold, CombineMode as M,
    FoldError, UnavailableFold,
};

type Pieces = Vec<(i64, i64)>;

/// Unite keeps every operand's pieces in operand order; islands are the pieces.
struct Disjoint {
    ids: Vec<String>,
    seen: Vec<usize>,
    keep_all: bool,
}

impl Disjoint {
    fn for_layer(layer: &Lay, keep_all: bool) -> Disjoint {
        Disjoint {
            ids: layer.ids.clone(),
            seen: Vec::new(),
            keep_all,
        }
    }
}

impl CombineFold for Disjoint {
    type Shape = Pieces;

    fn member_shape(
        &mut self,
        item: usize,
        _companion: Option<usize>,
    ) -> Result<Pieces, FoldError> {
        let at = (item as i64) * 10;
        Ok(vec![(at, at + 1)])
    }

    fn fold(&mut self, _mode: M, operands: &[Pieces]) -> Result<Pieces, FoldError> {
        self.seen.push(operands.len());
        // The broken variant is the pairwise-accumulator bug: only the ends survive.
        let last = operands.len() - 1;
        Ok(operands
            .iter()
            .enumerate()
            .filter(|(at, _)| self.keep_all || *at == 0 || *at == last)
            .flat_map(|(_, piece)| piece.clone())
            .collect())
    }

    fn islands(&mut self, shape: &Pieces) -> Result<Vec<Pieces>, FoldError> {
        Ok(shape.iter().map(|piece| vec![*piece]).collect())
    }
}

fn three() -> Lay {
    lay(
        &["a", "b", "c"],
        vec![grp("g", Some(M::Unite), None, &["a", "b", "c"])],
    )
}

fn pieces(out: &super::CombineOutcome<Pieces>) -> Pieces {
    out.paths.iter().flat_map(|p| p.shape.clone()).collect()
}

#[test]
fn three_disjoint_operands_all_reach_the_provider_in_one_call() {
    let layer = three();
    let mut port = Disjoint::for_layer(&layer, true);
    let out = render_combines(&layer.view(), &mut port).unwrap();
    assert_eq!(port.seen, vec![3]);
    // a=item 0, b=item 1, c=item 2: the middle piece b survives
    assert_eq!(pieces(&out), vec![(0, 1), (10, 11), (20, 21)]);
    assert_eq!(out.paths.len(), 3);
}

#[test]
fn a_provider_that_drops_the_middle_piece_is_caught() {
    let layer = three();
    let mut broken = Disjoint::for_layer(&layer, false);
    let out = render_combines(&layer.view(), &mut broken).unwrap();
    // same kernel, same call count: only the provider differs, and the loss shows
    assert_eq!(broken.seen, vec![3]);
    assert_eq!(pieces(&out), vec![(0, 1), (20, 21)]);
    assert_ne!(pieces(&out), vec![(0, 1), (10, 11), (20, 21)]);
}

#[test]
fn operands_follow_group_order_not_layer_order() {
    let layer = lay(
        &["a", "b", "c"],
        vec![grp("g", Some(M::Unite), None, &["c", "a", "b"])],
    );
    let mut port = Disjoint::for_layer(&layer, true);
    let out = render_combines(&layer.view(), &mut port).unwrap();
    assert_eq!(pieces(&out), vec![(20, 21), (0, 1), (10, 11)]);
}

#[test]
fn a_nested_result_stays_one_operand_with_all_its_pieces() {
    let layer = lay(
        &["a", "b", "c", "d"],
        vec![
            grp("P", Some(M::Unite), None, &["a", "C", "d"]),
            grp("C", Some(M::Unite), Some("P"), &["b", "c"]),
        ],
    );
    let mut port = Disjoint::for_layer(&layer, true);
    let out = render_combines(&layer.view(), &mut port).unwrap();
    // C folds b,c (2 operands) then P folds a, C's result, d (3 operands)
    assert_eq!(port.seen, vec![2, 3]);
    assert_eq!(pieces(&out), vec![(0, 1), (10, 11), (20, 21), (30, 31)]);
}

#[test]
fn an_unavailable_provider_yields_typed_unavailability_and_no_outcome() {
    let layer = three();
    assert_eq!(
        render_combines(&layer.view(), &mut UnavailableFold).err(),
        Some(CombineError::FoldUnavailable)
    );
    // nested: unavailability is not swallowed by the per-group catch
    let nested = lay(
        &["a", "b", "c"],
        vec![
            grp("P", Some(M::Unite), None, &["a", "C"]),
            grp("C", Some(M::Unite), Some("P"), &["b", "c"]),
        ],
    );
    assert_eq!(
        render_combines(&nested.view(), &mut UnavailableFold).err(),
        Some(CombineError::FoldUnavailable)
    );
    // nothing to fold: the port is never asked, so even an unavailable one succeeds
    let plain = lay(
        &["a", "b"],
        vec![grp("g", Some(M::None), None, &["a", "b"])],
    );
    let out = render_combines(&plain.view(), &mut UnavailableFold).unwrap();
    assert!(out.suppress.is_empty() && out.paths.is_empty());
}

struct FailsMembers;

impl CombineFold for FailsMembers {
    type Shape = ();
    fn member_shape(&mut self, _item: usize, _c: Option<usize>) -> Result<(), FoldError> {
        Err(FoldError::Failed("bad member".to_string()))
    }
    fn fold(&mut self, _mode: M, _operands: &[()]) -> Result<(), FoldError> {
        unreachable!("no operand shape exists")
    }
    fn islands(&mut self, _shape: &()) -> Result<Vec<()>, FoldError> {
        unreachable!("no fold result exists")
    }
}

#[test]
fn a_failed_member_shape_degrades_like_a_failed_fold() {
    let layer = three();
    let out = render_combines(&layer.view(), &mut FailsMembers).unwrap();
    assert!(out.suppress.is_empty() && out.paths.is_empty());
}

#[test]
fn flatten_refuses_whole_on_unavailable_or_failed_providers() {
    let layer = three();
    let plan = plan_flatten(&layer.view(), "g").unwrap();
    assert_eq!(
        execute_flatten(&plan, &mut UnavailableFold).err(),
        Some(CombineError::FoldUnavailable)
    );
    assert_eq!(
        execute_flatten(&plan, &mut FailsMembers).err(),
        Some(CombineError::FoldFailed("bad member".to_string()))
    );
    let mut port: Rec = layer.port();
    port.fail = vec!["unite(a,b,c)".to_string()];
    assert_eq!(
        execute_flatten(&plan, &mut port).err(),
        Some(CombineError::FoldFailed("unite(a,b,c)".to_string()))
    );
    // the plan is a plain value: a refused execution left it intact and reusable
    let mut again = layer.port();
    let islands = execute_flatten(&plan, &mut again).unwrap();
    assert_eq!(islands.len(), 1);
    assert_eq!(islands[0].desc, "unite(a,b,c)#0");
}
