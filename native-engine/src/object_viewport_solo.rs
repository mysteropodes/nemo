//! #2371 U11: the Solo view of a witness. Baseline 1065e9d8 keeps `ld.solo` as a session flag that the canvas AND the export both
//! read (`layerIsEffectivelyVisible`, app.js; export.js exportBuildFrame, the Lottie exporter and the engine scene all resolve
//! solo through it), and never serializes it. A witness therefore takes the set as a view: a viewport witness folds it into its
//! canonical cache identity, so a soloed frame can never be mistaken for the plain one, and an export witness takes the same
//! plan without touching its identity (an export has no cache that outlives the job). Neither changes a document revision.
use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneError};

/// The part of the viewport's canonical cache identity that belongs to a Solo set: the layers it names, order-insensitively.
pub fn viewport_solo_quality_suffix(soloed: &[String]) -> String {
    let mut names: Vec<&str> = soloed.iter().map(String::as_str).collect();
    names.sort_unstable();
    names.dedup();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for name in &names {
        for byte in name.bytes().chain(std::iter::once(0)) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("+solo:{}:{hash:016x}", names.len())
}

impl ObjectCompositionWitness {
    /// Paint only the soloed layers (and the children of a soloed folder layer) in this VIEWPORT witness. An empty set changes
    /// nothing; a set that names no layer of the shown context leaves every item (baseline `anyLayerSoloed` reads that context
    /// only) but still belongs to the quality, since the request asked for it. An export witness refuses this form.
    pub fn with_viewport_solo(mut self, soloed: &[String]) -> Result<Self, ObjectSceneError> {
        if !self.viewport {
            return Err(ObjectSceneError::WrongSnapshot);
        }
        if soloed.is_empty() {
            return Ok(self);
        }
        let base = self
            .viewport_quality
            .take()
            .ok_or(ObjectSceneError::WrongSnapshot)?;
        self.viewport_quality = Some(format!("{base}{}", viewport_solo_quality_suffix(soloed)));
        if let Some(plan) = self.plan.with_solo(soloed) {
            self.plan = plan;
        }
        Ok(self)
    }

    /// The export counterpart: the same layers paint, the identity stays `final`. A viewport witness refuses it.
    pub fn with_export_solo(mut self, soloed: &[String]) -> Result<Self, ObjectSceneError> {
        if self.viewport {
            return Err(ObjectSceneError::WrongSnapshot);
        }
        if let Some(plan) = (!soloed.is_empty()).then(|| self.plan.with_solo(soloed)).flatten() {
            self.plan = plan;
        }
        Ok(self)
    }
}
