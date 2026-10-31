//! #2373: the "Show transparency" checker, painted by the engine BEHIND the artwork of a viewport witness.
//! Baseline 1065e9d8 (engine-bridge `showAlphaChecker`): the checker REPLACES the artboard background, as scene content under the
//! drawing, in 8 SCREEN px tiles (world tile = 8 / zoom, never more than 4000 tiles) of 222 and 255 grey, the 222 tile at
//! (column + row) even. Only a viewport witness can carry one, so an export (a different witness) can never contain it, and the
//! document background colour stays the export background.
use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneError};
// #2372: the sibling viewport-only display mode, a child here so it reaches the witness' private fields without growing its file.
#[path = "object_viewport_outline.rs"]
pub(crate) mod outline;
// #2373: and the Shadow guides switch, the third.
#[path = "object_viewport_shadow.rs"]
pub(crate) mod shadow;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

const TILE_SCREEN_PX: f64 = 8.;
const MAX_TILES: f64 = 4000.;
const MIN_ZOOM: f64 = 0.001;
const DARK: u8 = 222;

/// The baseline tile grid for a canvas shown at `zoom` screen px per document unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewportChecker {
    width: f64,
    height: f64,
    cols: u32,
    rows: u32,
}

/// The part of the viewport's canonical cache identity that belongs to the checker: another display scale is another picture.
pub fn viewport_checker_quality_suffix(display_scale: f64) -> String {
    format!("+checker:{:016x}", display_scale.to_bits())
}

impl ViewportChecker {
    pub(crate) fn new(canvas: (u32, u32), zoom: f64) -> Option<Self> {
        if canvas.0 == 0 || canvas.1 == 0 || !zoom.is_finite() || zoom <= 0. {
            return None;
        }
        let (width, height) = (f64::from(canvas.0), f64::from(canvas.1));
        let mut tile = TILE_SCREEN_PX / zoom.max(MIN_ZOOM);
        let (ideal_cols, ideal_rows) = (width / tile, height / tile);
        if ideal_cols * ideal_rows > MAX_TILES {
            tile *= ((ideal_cols * ideal_rows) / MAX_TILES).sqrt();
        }
        let count = |extent: f64| (extent / tile).round().clamp(1., 4096.) as u32;
        Some(Self {
            width,
            height,
            cols: count(width),
            rows: count(height),
        })
    }

    #[cfg(test)]
    pub(crate) fn tile_counts(&self) -> (u32, u32) {
        (self.cols, self.rows)
    }

    /// An opaque white artboard, then the darker tiles over it: no seam can show the background through.
    pub(crate) fn draw(&self, scene: &mut vello::Scene, transform: Affine) {
        scene.fill(
            Fill::NonZero,
            transform,
            Color::WHITE,
            None,
            &Rect::new(0., 0., self.width, self.height),
        );
        let (tile_w, tile_h) = (
            self.width / f64::from(self.cols),
            self.height / f64::from(self.rows),
        );
        let dark = Color::from_rgba8(DARK, DARK, DARK, 255);
        for row in 0..self.rows {
            for col in 0..self.cols {
                if (col + row) % 2 != 0 {
                    continue;
                }
                let (x, y) = (f64::from(col) * tile_w, f64::from(row) * tile_h);
                scene.fill(
                    Fill::NonZero,
                    transform,
                    dark,
                    None,
                    &Rect::new(x, y, x + tile_w, y + tile_h),
                );
            }
        }
    }
}

impl ObjectCompositionWitness {
    /// Paint the transparency checker under the artwork of this VIEWPORT witness over the `canvas` artboard (the output extent,
    /// which a project preview pins to the project canvas). `display_scale` is screen px per composition
    /// px (the canvas element width over the output width); the witness' own navigation / camera scale completes the baseline
    /// `view.zoom`, so the tile is 8 SCREEN px at any zoom. An export witness refuses it, and the canonical viewport quality
    /// gains the checker identity, so the scheduler key and the cached frame of the plain viewport can never be mistaken for it.
    pub fn with_viewport_checker(
        mut self,
        canvas: (u32, u32),
        display_scale: f64,
    ) -> Result<Self, ObjectSceneError> {
        let zoom = display_scale * self.camera.map_or(1., |m| m[0].hypot(m[1]));
        let checker =
            ViewportChecker::new(canvas, zoom).ok_or(ObjectSceneError::UnsupportedGeometry {
                source_object_index: 0,
                reason: "viewport checker zoom unavailable",
            })?;
        if !self.viewport {
            return Err(ObjectSceneError::WrongSnapshot);
        }
        let base = self
            .viewport_quality
            .take()
            .ok_or(ObjectSceneError::WrongSnapshot)?;
        self.viewport_quality = Some(format!(
            "{base}{}",
            viewport_checker_quality_suffix(display_scale)
        ));
        self.viewport_checker = Some(checker);
        Ok(self)
    }

    /// The checker as the bottom item of `scene`, under the witness' own camera / navigation (the baseline artboard rect): behind a
    /// viewport reference and the artwork. Nothing for a witness without one, which is every export witness.
    pub(crate) fn paint_viewport_checker(&self, scene: &mut vello::Scene) {
        if let Some(checker) = &self.viewport_checker {
            checker.draw(scene, self.camera.map_or(Affine::IDENTITY, Affine::new));
        }
    }
}
