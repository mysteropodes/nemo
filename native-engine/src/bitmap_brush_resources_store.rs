//! Charged bitmap-brush bitmap store (L07-D / G06): retained RGBA pixels whose
//! quota is the shared image-resource domain, with a baseline-exact LRU.
//!
//! Baseline: `src/js/application/render/image-budget.js` at fbe4b8a3
//! (`noteRegistered` / `touch` / `beginBuild` / `endBuild(retire)` /
//! `setBudgetBytes`) bookkeeping the baked brush canvases that
//! `bitmap-brush.js` uploads through `registerImagePixels`. The policy is
//! unchanged: least recently USED (last emitted into a scene), never anything
//! the current build emitted, dropped only until the total fits, and a refused
//! retire forgets nothing. What native adds, because the quota is hard:
//! - a published `BrushBitmap` owns its `ImageResourceCharge` (cost `w*h*4`);
//!   its pixels are allocated THROUGH that charge and drop before it, so the
//!   quota becomes reusable only after the storage is gone, exactly when the
//!   last clone (store entry or scene/lease holder) drops;
//! - two-phase publication (`begin` -> `fill` -> `commit`): cancel or any
//!   failure before commit releases the charge and the id mark, and a second
//!   `begin` for an id with a live mark is refused (no duplicate charge);
//! - replacement fence: committing an id installs the new version atomically;
//!   the previous `BrushBitmap` stays alive, immutable and charged for every
//!   holder, and a holder can ask `is_current` (versions only ever rise);
//! - pressure: a reserve the hard quota refuses evicts the oldest UNPINNED
//!   resident (not in the current build, not held by a scene) one at a time
//!   until it fits; with only pinned residents left the result is
//!   `Budget(Capacity)`. Like the baseline, eviction needs a retire port
//!   (`None` is inert) and a refused retire evicts nothing more.
//!
//! The process-wide quota is reached only by `BrushBitmapStore::shared()`;
//! tests use isolated budgets. `StoreRules` (policy module) carries the baseline
//! constants so the controls can mutate each one.

use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_bitmap::{
    release_mark, Marks,
};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_policy::{
    budget_bytes_from_number, Candidate,
};
use crate::revision::{
    shared_image_resource_budget, ImageResourceBudget, ImageResourceCharge, ImageResourceError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

pub(crate) use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_bitmap::{
    BrushBitmap, BrushStoreError, PendingBrush,
};
pub(crate) use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_policy::StoreRules;

/// `NemoImageBudget.create` default: `384 * 1024 * 1024`.
const DEFAULT_SOFT_BUDGET: u64 = 384 * 1024 * 1024;
/// Port to whoever mirrors the store (the engine image table): `true` when the
/// ids were retired. Anything else, or no port at all, evicts nothing.
pub(crate) type RetirePort<'a> = Option<&'a mut dyn FnMut(&[String]) -> bool>;

#[derive(Debug)]
struct Entry {
    brush: BrushBitmap,
    bytes: u64,
    last_used: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoreStats {
    pub(crate) total_bytes: u64,
    pub(crate) budget_bytes: u64,
    pub(crate) evictions: u64,
    pub(crate) pressure_evictions: u64,
    pub(crate) count: usize,
}

#[derive(Debug)]
pub(crate) struct BrushBitmapStore {
    budget: ImageResourceBudget,
    rules: StoreRules,
    entries: BTreeMap<String, Entry>,
    marks: Marks,
    current_build: Option<BTreeSet<String>>,
    tick: u64,
    generation: u64,
    soft_budget: u64,
    evictions: u64,
    pressure_evictions: u64,
}

fn ask(retire: &mut RetirePort<'_>, ids: &[String]) -> Option<bool> {
    retire.as_mut().map(|port| (**port)(ids))
}

impl BrushBitmapStore {
    /// Production constructor: the process-wide image-resource quota.
    pub(crate) fn shared() -> Self {
        Self::with_budget(shared_image_resource_budget().clone())
    }
    pub(crate) fn with_budget(budget: ImageResourceBudget) -> Self {
        Self::with_rules(budget, StoreRules::BASELINE)
    }
    pub(crate) fn with_rules(budget: ImageResourceBudget, rules: StoreRules) -> Self {
        Self {
            budget,
            rules,
            entries: BTreeMap::new(),
            marks: Arc::new(Mutex::new(BTreeSet::new())),
            current_build: None,
            tick: 0,
            generation: 0,
            soft_budget: DEFAULT_SOFT_BUDGET,
            evictions: 0,
            pressure_evictions: 0,
        }
    }

    /// The quota domain this store charges. A decode feeding the store (L07-G) reserves
    /// from this same budget, never from another one.
    pub(crate) fn budget(&self) -> &ImageResourceBudget {
        &self.budget
    }

    /// `setBudgetBytes(n)`: `Math.max(1, Math.floor(n))`; returns the budget.
    pub(crate) fn set_budget_bytes(&mut self, n: f64) -> u64 {
        self.soft_budget = budget_bytes_from_number(n);
        self.soft_budget
    }
    pub(crate) fn begin_build(&mut self) {
        self.current_build = Some(BTreeSet::new());
    }
    pub(crate) fn building(&self) -> bool {
        self.current_build.is_some()
    }
    /// `touch(id)`: ages the LRU and pins the id for the open build, resident or not.
    pub(crate) fn touch(&mut self, id: &str) {
        self.tick = self.tick.saturating_add(1);
        let tick = self.tick;
        if let Some(entry) = self.entries.get_mut(id) {
            entry.last_used = tick;
        }
        if let Some(set) = self.current_build.as_mut() {
            set.insert(id.to_string());
        }
    }
    /// A scene's hold on a brush: touches it, then hands out an alias that pins
    /// it against eviction until dropped.
    pub(crate) fn lease(&mut self, id: &str) -> Option<BrushBitmap> {
        self.touch(id);
        self.entries.get(id).map(|entry| entry.brush.clone())
    }
    /// A read that neither ages the LRU nor pins the id (the host inspect/scene reads).
    pub(crate) fn peek(&self, id: &str) -> Option<&BrushBitmap> {
        self.entries.get(id).map(|entry| &entry.brush)
    }
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }
    pub(crate) fn ids(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }
    /// Whether `bitmap` is still the version the store serves for its id.
    pub(crate) fn is_current(&self, bitmap: &BrushBitmap) -> bool {
        self.entries
            .get(bitmap.id())
            .is_some_and(|entry| entry.brush.version() == bitmap.version())
    }
    pub(crate) fn total_bytes(&self) -> u64 {
        self.entries
            .values()
            .fold(0u64, |sum, entry| sum.saturating_add(entry.bytes))
    }
    pub(crate) fn stats(&self) -> StoreStats {
        StoreStats {
            total_bytes: self.total_bytes(),
            budget_bytes: self.soft_budget,
            evictions: self.evictions,
            pressure_evictions: self.pressure_evictions,
            count: self.entries.len(),
        }
    }

    fn checked_cost(&self, id: &str, width: u32, height: u32) -> Result<usize, BrushStoreError> {
        if id.is_empty() {
            return Err(BrushStoreError::InvalidId);
        }
        if width == 0 || height == 0 {
            return Err(BrushStoreError::InvalidDimensions);
        }
        self.rules
            .cost(width, height)
            .ok_or(BrushStoreError::CostOverflow)
    }

    /// Entries a build or a reserve may drop: not `exclude`, not emitted by the
    /// open build, not held by a scene.
    fn victims(&self, exclude: &str) -> Vec<Candidate> {
        let build = self.current_build.as_ref();
        self.entries
            .iter()
            .filter(|(id, _)| id.as_str() != exclude)
            .filter(|(id, _)| {
                !(self.rules.protect_current_build && build.is_some_and(|set| set.contains(*id)))
            })
            .filter(|(_, entry)| !(self.rules.protect_leased && entry.brush.holders() > 1))
            .map(|(id, entry)| Candidate {
                id: id.clone(),
                last_used: entry.last_used,
                bytes: entry.bytes,
            })
            .collect()
    }

    /// Phase 1: validate, mark the id, reserve (evicting unpinned residents when
    /// the hard quota refuses) and allocate the charged pixel storage.
    pub(crate) fn begin(
        &mut self,
        id: &str,
        width: u32,
        height: u32,
        mut retire: RetirePort<'_>,
    ) -> Result<PendingBrush, BrushStoreError> {
        let cost = self.checked_cost(id, width, height)?;
        {
            let mut marks = self
                .marks
                .lock()
                .map_err(|_| BrushStoreError::Unavailable)?;
            if !marks.insert(id.to_string()) {
                return Err(BrushStoreError::PendingConflict);
            }
        }
        let prepared = self
            .reserve_with_pressure(id, cost, &mut retire)
            .and_then(|mut charge| match charge.allocate() {
                Ok(pixels) => Ok((pixels, charge)),
                Err(error) => Err(BrushStoreError::Budget(error)),
            });
        match prepared {
            Ok(parts) => Ok(PendingBrush::new(parts, id, (width, height), &self.marks)),
            Err(error) => {
                release_mark(&self.marks, id);
                Err(error)
            }
        }
    }

    fn reserve_with_pressure(
        &mut self,
        id: &str,
        cost: usize,
        retire: &mut RetirePort<'_>,
    ) -> Result<ImageResourceCharge, BrushStoreError> {
        match self.budget.reserve(cost) {
            Ok(charge) => return Ok(charge),
            Err(ImageResourceError::Capacity) => {}
            Err(error) => return Err(BrushStoreError::Budget(error)),
        }
        let refused = BrushStoreError::Budget(ImageResourceError::Capacity);
        if retire.is_none() {
            return Err(refused);
        }
        // Evict only when it can help: free quota must already cover what even
        // evicting every unpinned resident cannot supply (probe, released at once).
        let evictable = self
            .victims(id)
            .iter()
            .fold(0usize, |sum, c| sum.saturating_add(c.bytes as usize));
        let shortfall = cost.saturating_sub(evictable);
        if shortfall > 0 {
            let probe = self
                .budget
                .reserve(shortfall)
                .map_err(BrushStoreError::Budget)?;
            drop(probe);
        }
        loop {
            let oldest = self.victims(id).into_iter().min_by_key(|c| c.last_used);
            let Some(victim) = oldest else {
                return Err(refused);
            };
            if ask(retire, std::slice::from_ref(&victim.id)) != Some(true) {
                return Err(refused);
            }
            self.entries.remove(&victim.id);
            self.pressure_evictions += 1;
            match self.budget.reserve(cost) {
                Ok(charge) => return Ok(charge),
                Err(ImageResourceError::Capacity) => {}
                Err(error) => return Err(BrushStoreError::Budget(error)),
            }
        }
    }

    /// Phase 2: install atomically. A previous version of the id stays alive for
    /// every holder; the store only drops its own alias.
    pub(crate) fn commit(&mut self, pending: PendingBrush) -> Result<BrushBitmap, BrushStoreError> {
        if !pending.belongs_to(&self.marks) {
            return Err(BrushStoreError::ForeignPending);
        }
        let id = pending.id().to_string();
        let cost = pending.cost() as u64;
        let next = self.generation.saturating_add(1);
        let version = match self.entries.get(&id) {
            Some(old) if !self.rules.new_version_on_replace => old.brush.version(),
            _ => next,
        };
        let brush = pending.into_bitmap(version)?;
        self.generation = next;
        self.tick = self.tick.saturating_add(1);
        if let Some(set) = self.current_build.as_mut() {
            set.insert(id.clone());
        }
        let entry = Entry {
            brush: brush.clone(),
            bytes: cost,
            last_used: self.tick,
        };
        self.entries.insert(id, entry);
        Ok(brush)
    }

    /// `noteRegistered` + upload in one call; any failure leaves nothing behind.
    pub(crate) fn publish(
        &mut self,
        id: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
        retire: RetirePort<'_>,
    ) -> Result<BrushBitmap, BrushStoreError> {
        let cost = self.checked_cost(id, width, height)?;
        if rgba.len() != cost {
            return Err(BrushStoreError::PixelLength {
                expected: cost,
                actual: rgba.len(),
            });
        }
        let mut pending = self.begin(id, width, height, retire)?;
        pending.fill(rgba)?;
        self.commit(pending)
    }

    /// `endBuild(retire)`: closes the build set on every path and returns the
    /// ids dropped. Oldest unpinned first, only while the total exceeds the soft
    /// budget; no port or a refused port drops (and forgets) nothing.
    pub(crate) fn end_build(&mut self, mut retire: RetirePort<'_>) -> Vec<String> {
        let candidates = self.victims("");
        self.current_build = None;
        if retire.is_none() {
            return Vec::new();
        }
        let dropped =
            self.rules
                .plan_end_build_eviction(candidates, self.total_bytes(), self.soft_budget);
        if dropped.is_empty() || ask(&mut retire, &dropped) != Some(true) {
            return Vec::new();
        }
        for id in &dropped {
            self.entries.remove(id);
        }
        self.evictions += dropped.len() as u64;
        dropped
    }
}

#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_store_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_store_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_store.rs"]
mod tests;
