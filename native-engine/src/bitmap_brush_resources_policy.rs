//! Pure policy of the charged bitmap-brush store (L07-D / G06): decoded cost,
//! soft byte budget and least-recently-used eviction planning.
//!
//! Ported from `src/js/application/render/image-budget.js` at fbe4b8a3
//! (`noteRegistered` bytes = `w * h * 4`, `setBudgetBytes` =
//! `Math.max(1, Math.floor(n))`, `endBuild` candidate order). `StoreRules`
//! carries the baseline constants so the controls can mutate each one and prove
//! a named test notices; `BASELINE` is the only value production callers use.

/// One resident entry offered to the eviction planner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) id: String,
    pub(crate) last_used: u64,
    pub(crate) bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoreRules {
    /// Decoded RGBA bytes per pixel (`w * h * 4`).
    pub(crate) bytes_per_pixel: usize,
    /// `total <= budget` fits (the baseline `total > budget` keeps evicting).
    pub(crate) budget_inclusive: bool,
    /// Ids emitted by the current build are never candidates.
    pub(crate) protect_current_build: bool,
    /// Entries still held by a scene/lease (outstanding clones) are never candidates.
    pub(crate) protect_leased: bool,
    /// A replacement publishes a new version number (the fence stale holders see).
    pub(crate) new_version_on_replace: bool,
}

impl StoreRules {
    pub(crate) const BASELINE: StoreRules = StoreRules {
        bytes_per_pixel: 4,
        budget_inclusive: true,
        protect_current_build: true,
        protect_leased: true,
        new_version_on_replace: true,
    };

    /// `w * h * 4` as a checked usize; `None` on overflow.
    pub(crate) fn cost(&self, width: u32, height: u32) -> Option<usize> {
        (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(self.bytes_per_pixel)
    }

    /// Whether a running total is within the soft budget.
    pub(crate) fn fits(&self, total: u64, budget: u64) -> bool {
        if self.budget_inclusive {
            total <= budget
        } else {
            total < budget
        }
    }

    /// `endBuild` drop list: candidates oldest use first, dropped only while the
    /// running total still exceeds the budget. Empty when the total already fits.
    pub(crate) fn plan_end_build_eviction(
        &self,
        mut candidates: Vec<Candidate>,
        total: u64,
        budget: u64,
    ) -> Vec<String> {
        if self.fits(total, budget) {
            return Vec::new();
        }
        candidates.sort_by_key(|candidate| candidate.last_used);
        let mut running = total;
        let mut drop = Vec::new();
        for candidate in candidates {
            if self.fits(running, budget) {
                break;
            }
            running = running.saturating_sub(candidate.bytes);
            drop.push(candidate.id);
        }
        drop
    }
}

/// `setBudgetBytes(n)` = `Math.max(1, Math.floor(n))` for a JS number. NaN makes
/// the baseline compare false everywhere, so its loop never drops anything: it is
/// the unlimited budget here, as is any value past `u64::MAX` (the cast saturates).
pub(crate) fn budget_bytes_from_number(n: f64) -> u64 {
    if n.is_nan() {
        return u64::MAX;
    }
    let floored = n.floor();
    if floored < 1.0 {
        1
    } else {
        floored as u64
    }
}
