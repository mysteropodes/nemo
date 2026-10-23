//! PSD import plan: per-layer eligibility, placement, opacity, naming, canvas resize and the
//! explicit skip report (L10-F, G36/G37).
//!
//! Pure port of the decisions of `importChildren` and of the post-parse part of `importFile` in
//! `src/js/psd-import-bridge.js` at the agreed desktop baseline `fbe4b8a3`. ag-psd (vendored,
//! untouched) stays the parser: the adapter reads the file and hands over the top-level layers as a
//! [`PsdInput`]. Pixel bytes never enter this kernel; each planned layer carries a
//! [`RasterAssetRequest`] that the raster-asset provider (lane 6, G27) resolves.
//!
//! Baseline facts the kernel keeps on purpose:
//! - a child is imported only when it has a canvas and is not hidden (`!layer.canvas || layer.hidden`
//!   skips it); a group has no canvas of its own and its children are NOT flattened;
//! - the layer name is `layer.name || ('Calque PSD ' + (imported + 1))`, `imported` counting the
//!   layers imported so far (a skipped child does not advance it), through
//!   `object_import_remap_ids::psd_layer_name`; the layer is made visible;
//! - the Raster is sized `canvas.width x canvas.height` and centred on
//!   `(left + w / 2, top + h / 2)` with `left = layer.left || 0` (undefined, null, NaN and 0 are all
//!   0), and its opacity is `layer.opacity !== undefined ? layer.opacity : 1` (a stored 0 stays 0);
//! - the canvas is resized only when `psd.width && psd.height` are both truthy and either differs
//!   from the current canvas, before any layer is created;
//! - nothing imported is an error carrying the baseline message [`NO_PIXELS_MESSAGE`];
//! - the file-picker toasts are `n calque(s)` + the i18n suffix + `(s)` + ` depuis le PSD` and the
//!   i18n failure prefix + `err.message` (or `erreur inconnue`); the i18n strings are parameters.
//!
//! Deliberate, contract-mandated differences from the baseline (each pinned by a test):
//! - the skipped children are REPORTED ([`SkippedLayer`], with their reason); the baseline skipped
//!   them silently and documented that as a scope limit. The report does not change what is imported;
//! - the baseline resized the canvas BEFORE it found there was nothing to import and then threw, so a
//!   groups-only PSD left the canvas resized; the plan is all-or-nothing, so the error carries no
//!   resize and nothing is published;
//! - a non-finite `left`, `top` or `opacity` of an imported layer is refused ([`PsdPlanError::NonFinite`])
//!   instead of being placed (ag-psd never produces one).
//!
//! The plan is a plan, not an import: [`decide_psd_import`] answers [`PsdImportDecision::Unavailable`]
//! until the raster-asset provider exists, and never a mock success.

use crate::history::object_history::object_shot_kernels::native_import_drop_route::ProviderState;
use crate::history::object_history::object_shot_kernels::object_import_remap_ids::{
    psd_layer_key, psd_layer_name,
};

/// The baseline error message when no layer has pixels (French, as shipped).
pub(crate) const NO_PIXELS_MESSAGE: &str = "Aucun calque avec des pixels trouv\u{e9} (fichier vide, ou uniquement des groupes/calques masqu\u{e9}s)";
/// `err.message` fallback of the failure toast.
pub(crate) const UNKNOWN_ERROR: &str = "erreur inconnue";
/// `layer.canvas.toDataURL('image/png')`: the encoding the baseline stored the pixels in.
pub(crate) const RASTER_MIME: &str = "image/png";

/// One top-level child of the parsed PSD (`psd.children[i]`), as the adapter reads it from ag-psd.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PsdLayerInput {
    /// `layer.name`; `None` is undefined or null.
    pub(crate) name: Option<String>,
    /// `layer.hidden` (truthy).
    pub(crate) hidden: bool,
    /// `layer.canvas` is present (a group has none).
    pub(crate) has_canvas: bool,
    /// `layer.left`; `None` is undefined or null.
    pub(crate) left: Option<f64>,
    pub(crate) top: Option<f64>,
    /// `layer.canvas.width` / `.height`; `0` when there is no canvas.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// `layer.opacity`; `None` is undefined.
    pub(crate) opacity: Option<f64>,
}

/// The parsed PSD: `psd.width`, `psd.height` (`None` is undefined) and `psd.children || []`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PsdInput {
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) children: Vec<PsdLayerInput>,
}

/// The current canvas, `state.canvasW` / `state.canvasH`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasSize {
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// `window.SM.setCanvasSize(w, h)`, to be published before the layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CanvasResize {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// `r.size` and `r.position` of the Raster: centre and size, in canvas units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacementRect {
    pub(crate) center_x: f64,
    pub(crate) center_y: f64,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// What the raster-asset provider (lane 6, G27) is asked for: the pixels of one PSD layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RasterAssetRequest {
    /// Asset source key, `object_import_remap_ids::psd_layer_key(source_index)`.
    pub(crate) key: String,
    pub(crate) source_index: usize,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) mime: &'static str,
}

/// One layer to create, in `psd.children` order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PsdLayerRow {
    pub(crate) source_index: usize,
    pub(crate) layer_name: String,
    /// `state.layers[idx].visible = true`.
    pub(crate) visible: bool,
    pub(crate) rect: PlacementRect,
    pub(crate) opacity: f64,
    pub(crate) raster: RasterAssetRequest,
}

/// Why a child is not imported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SkipReason {
    /// No canvas: a group (not flattened) or a layer without pixels; `hidden` is carried too.
    NoCanvas { hidden: bool },
    /// A hidden layer that has pixels.
    Hidden,
}

/// A child that was not imported. The baseline skipped these silently.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SkippedLayer {
    pub(crate) source_index: usize,
    pub(crate) name: Option<String>,
    pub(crate) reason: SkipReason,
}

/// The whole import, to be published once or not at all.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PsdImportPlan {
    /// Published before the layers.
    pub(crate) resize: Option<CanvasResize>,
    pub(crate) rows: Vec<PsdLayerRow>,
    pub(crate) skipped: Vec<SkippedLayer>,
}

impl PsdImportPlan {
    /// The `count` the baseline returned (and toasted).
    pub(crate) fn imported_count(&self) -> usize {
        self.rows.len()
    }

    /// Every child is exactly one of a row or a skipped entry.
    pub(crate) fn child_count(&self) -> usize {
        self.rows.len() + self.skipped.len()
    }
}

/// Why no plan exists.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PsdPlanError {
    /// No child was importable; carries what was skipped.
    NoPixels {
        child_count: usize,
        skipped: Vec<SkippedLayer>,
    },
    /// An imported layer has a non-finite `left`, `top` or `opacity`.
    NonFinite {
        source_index: usize,
        field: &'static str,
    },
}

impl PsdPlanError {
    /// The text a toast shows (`err.message`).
    pub(crate) fn message(&self) -> String {
        match self {
            PsdPlanError::NoPixels { .. } => NO_PIXELS_MESSAGE.to_string(),
            PsdPlanError::NonFinite {
                source_index,
                field,
            } => format!("PSD layer {source_index}: {field} is not a finite number"),
        }
    }
}

/// `x || 0`: undefined, null, NaN and zero (either sign) are all `+0`.
fn or_zero(value: Option<f64>) -> f64 {
    match value {
        Some(v) if v != 0.0 && !v.is_nan() => v,
        _ => 0.0,
    }
}

fn resize_request(psd: &PsdInput, canvas: &CanvasSize) -> Option<CanvasResize> {
    let width = psd.width.filter(|v| *v != 0)?;
    let height = psd.height.filter(|v| *v != 0)?;
    if f64::from(width) != canvas.width || f64::from(height) != canvas.height {
        Some(CanvasResize { width, height })
    } else {
        None
    }
}

/// Plan the import of one parsed PSD, in the baseline order: eligibility and rows, then the error
/// when nothing is importable, then the canvas resize.
pub(crate) fn plan_psd_import(
    psd: &PsdInput,
    canvas: &CanvasSize,
) -> Result<PsdImportPlan, PsdPlanError> {
    let mut rows: Vec<PsdLayerRow> = Vec::new();
    let mut skipped: Vec<SkippedLayer> = Vec::new();
    for (source_index, layer) in psd.children.iter().enumerate() {
        let reason = if !layer.has_canvas {
            Some(SkipReason::NoCanvas {
                hidden: layer.hidden,
            })
        } else if layer.hidden {
            Some(SkipReason::Hidden)
        } else {
            None
        };
        if let Some(reason) = reason {
            skipped.push(SkippedLayer {
                source_index,
                name: layer.name.clone(),
                reason,
            });
            continue;
        }
        let left = or_zero(layer.left);
        let top = or_zero(layer.top);
        let opacity = layer.opacity.unwrap_or(1.0);
        for (field, value) in [("left", left), ("top", top), ("opacity", opacity)] {
            if !value.is_finite() {
                return Err(PsdPlanError::NonFinite {
                    source_index,
                    field,
                });
            }
        }
        rows.push(PsdLayerRow {
            source_index,
            layer_name: psd_layer_name(layer.name.as_deref(), rows.len()),
            visible: true,
            rect: PlacementRect {
                center_x: left + f64::from(layer.width) / 2.0,
                center_y: top + f64::from(layer.height) / 2.0,
                width: layer.width,
                height: layer.height,
            },
            opacity,
            raster: RasterAssetRequest {
                key: psd_layer_key(source_index),
                source_index,
                width: layer.width,
                height: layer.height,
                mime: RASTER_MIME,
            },
        });
    }
    if rows.is_empty() {
        return Err(PsdPlanError::NoPixels {
            child_count: psd.children.len(),
            skipped,
        });
    }
    Ok(PsdImportPlan {
        resize: resize_request(psd, canvas),
        rows,
        skipped,
    })
}

/// What the caller says it can take; no provider is invented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PsdPorts {
    /// The raster-asset provider (lane 6, G27) that turns a [`RasterAssetRequest`] into an asset.
    pub(crate) raster_assets: ProviderState,
}

impl PsdPorts {
    /// The state at the lane-10 base: no raster-asset provider exists.
    pub(crate) const NONE_AVAILABLE: PsdPorts = PsdPorts {
        raster_assets: ProviderState::Unavailable,
    };
}

/// Why a valid plan cannot be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PsdBlockReason {
    RasterAssetProviderUnavailable,
}

/// A valid import nothing may apply; it carries no executable plan and no success toast.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UnavailablePsdImport {
    pub(crate) reason: PsdBlockReason,
    /// How many layers the import would have created.
    pub(crate) eligible_count: usize,
    pub(crate) skipped: Vec<SkippedLayer>,
}

/// Outcome of deciding one PSD import.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PsdImportDecision {
    /// The provider exists; the plan may be prepared and published once.
    Ready(PsdImportPlan),
    /// The plan is valid but cannot run; nothing is applied.
    Unavailable(UnavailablePsdImport),
}

/// Plan, then check the raster-asset provider before anything can run. The plan errors (and their
/// baseline message) are the same whether or not the provider exists.
pub(crate) fn decide_psd_import(
    psd: &PsdInput,
    canvas: &CanvasSize,
    ports: &PsdPorts,
) -> Result<PsdImportDecision, PsdPlanError> {
    let plan = plan_psd_import(psd, canvas)?;
    Ok(match ports.raster_assets {
        ProviderState::Available => PsdImportDecision::Ready(plan),
        ProviderState::Unavailable => PsdImportDecision::Unavailable(UnavailablePsdImport {
            reason: PsdBlockReason::RasterAssetProviderUnavailable,
            eligible_count: plan.rows.len(),
            skipped: plan.skipped,
        }),
    })
}

/// The success toast: `count + ' calque' + (count > 1 ? 's' : '') + SM.t('toastImportedSuffix') +
/// (count > 1 ? 's' : '') + ' depuis le PSD'`.
pub(crate) fn success_toast(count: usize, imported_suffix: &str) -> String {
    let plural = if count > 1 { "s" } else { "" };
    format!("{count} calque{plural}{imported_suffix}{plural} depuis le PSD")
}

/// The failure toast: `SM.t('toastPsdImportFailedSuffix') + (err && err.message ? err.message :
/// 'erreur inconnue')`; `message` is `err.message` (`None` when there is none), empty counts as none.
pub(crate) fn failure_toast(failed_prefix: &str, message: Option<&str>) -> String {
    let text = match message {
        Some(m) if !m.is_empty() => m,
        _ => UNKNOWN_ERROR,
    };
    format!("{failed_prefix}{text}")
}

#[cfg(test)]
#[path = "../tests/native_import_psd_plan_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/native_import_psd_plan.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/native_import_psd_plan_oracle.rs"]
mod oracle;

#[cfg(test)]
#[path = "../tests/native_import_psd_plan_controls.rs"]
mod controls;
