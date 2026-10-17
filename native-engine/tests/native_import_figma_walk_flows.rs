//! Shared access to the oracle `V` flows for the plan and request tests (L10-E, G36).

use super::decode::dec_input;
use super::plan::{plan_import, FigmaImportPlan};
use super::rows::{blocks, Block};

/// The oracle flow `id` and the plan production builds from its input.
pub(crate) fn flow(id: &str) -> (Block, FigmaImportPlan) {
    let block = blocks("V")
        .into_iter()
        .find(|b| b.id == id)
        .unwrap_or_else(|| panic!("no V flow {}", id));
    let plan = plan_import(&dec_input(&block.lines));
    (block, plan)
}

/// The expected `x` lines of a case, in oracle order.
pub(crate) fn x_lines(block: &Block) -> Vec<String> {
    block
        .lines
        .iter()
        .filter(|l| l.starts_with("x "))
        .map(|l| l.to_string())
        .collect()
}

/// `true` when the case holds exactly this line.
pub(crate) fn block_has(block: &Block, line: &str) -> bool {
    block.lines.iter().any(|l| *l == line)
}
