//! Raster item geometry (G01 / L06-C): the two baseline readings of a live Paper
//! Raster at fbe4b8a3, as pure data in and out.
//!
//! * [`engine_image_rect`] is `rasterImageRect` (`src/js/engine-bridge.js`): the
//!   rect the engine draws. Unrotated it is `bounds` (top-left origin); rotated it
//!   is the un-rotated natural size times `|scaling|`, centred on `position`,
//!   with `matrix.rotation` (degrees) reported separately.
//! * [`capture_geometry`] is the geometry block of `serR` (`src/js/app.js`): the
//!   stored dict keeps the raster CENTRE (`position`) as `x`/`y`, the display size
//!   as `width`/`height`, and `rotation` only when non-zero. An undecoded raster
//!   (`!loaded` with a stashed `_pendingGeom`) is stored from that stash.
//!
//! Deliberate differences from the baseline, all refusals before any output: a
//! non-finite rotation or a non-finite result is `NonFinite` (the baseline lets
//! `NaN` fall through `|| 0` and propagate). Mirror flips stay unrepresentable as
//! the baseline documents: scaling is `abs()`-ed and Paper reports a mirror as a
//! 180 degree rotation plus a negative scale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RasterItemError {
    /// Not a strict raster item object (unknown, duplicate, missing or mistyped member, trailing data).
    Malformed,
    /// The asset id is not a valid `AssetId`.
    AssetId,
    /// A coordinate, size, rotation or opacity is not a finite number.
    NonFinite,
    /// Width or height is not positive.
    Extent,
    /// Opacity is outside `0..=1`.
    Opacity,
    /// A reserved passthrough id is empty, longer than 255 bytes or contains NUL.
    Tag,
    /// The referenced asset is not in the project inventory.
    MissingAsset,
    /// The referenced asset is not an image.
    NotImage,
}

impl std::fmt::Display for RasterItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Malformed => "malformed raster item",
            Self::AssetId => "invalid raster asset id",
            Self::NonFinite => "non-finite raster item number",
            Self::Extent => "raster width and height must be positive",
            Self::Opacity => "raster opacity must be within 0..=1",
            Self::Tag => "invalid raster passthrough id",
            Self::MissingAsset => "raster asset is not in the inventory",
            Self::NotImage => "raster asset is not an image",
        })
    }
}

/// `{x, y, width, height, rotation}` as the engine image item takes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ImageRect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) rotation: f64,
}

/// `raster.bounds`: the axis-aligned envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SurfaceBounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

/// `raster.data._pendingGeom`: the intended geometry `desR` stashes before decode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PendingGeometry {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) rotation: f64,
}

/// The observed state of one live Raster, copied out by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RasterSurface {
    /// `raster.matrix.rotation` in degrees; `None` when there is no matrix.
    pub(crate) matrix_rotation: Option<f64>,
    pub(crate) scaling_x: f64,
    pub(crate) scaling_y: f64,
    /// Natural (pixel) size, `raster.width` / `raster.height`.
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) position_x: f64,
    pub(crate) position_y: f64,
    pub(crate) bounds: SurfaceBounds,
    pub(crate) loaded: bool,
    pub(crate) pending: Option<PendingGeometry>,
    /// `raster.opacity`; `None` is the baseline's `undefined`.
    pub(crate) opacity: Option<f64>,
}

/// The geometry members `serR` writes; `rotation` is `None` when `serR` omits it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CapturedGeometry {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) rotation: Option<f64>,
}

/// `(c.matrix && c.matrix.rotation) || 0`, with non-finite refused and -0 read as 0.
fn read_rotation(value: Option<f64>) -> Result<f64, RasterItemError> {
    match value {
        None => Ok(0.0),
        Some(v) if !v.is_finite() => Err(RasterItemError::NonFinite),
        Some(v) if v == 0.0 => Ok(0.0),
        Some(v) => Ok(v),
    }
}

fn all_finite(values: [f64; 4]) -> Result<(), RasterItemError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(RasterItemError::NonFinite)
    }
}

/// `rasterImageRect(c)` of `engine-bridge.js`.
pub(crate) fn engine_image_rect(surface: &RasterSurface) -> Result<ImageRect, RasterItemError> {
    let rot = read_rotation(surface.matrix_rotation)?;
    let rect = if rot == 0.0 {
        let b = surface.bounds;
        ImageRect {
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
            rotation: 0.0,
        }
    } else {
        let sw = surface.scaling_x.abs() * surface.width;
        let sh = surface.scaling_y.abs() * surface.height;
        ImageRect {
            x: surface.position_x - sw / 2.0,
            y: surface.position_y - sh / 2.0,
            width: sw,
            height: sh,
            rotation: rot,
        }
    };
    all_finite([rect.x, rect.y, rect.width, rect.height])?;
    Ok(rect)
}

/// The geometry block of `serR(r)` in `app.js`.
pub(crate) fn capture_geometry(
    surface: &RasterSurface,
) -> Result<CapturedGeometry, RasterItemError> {
    let (x, y, width, height, rot) = match (surface.loaded, surface.pending) {
        // `!r.loaded && pending`: the stash wins over the placeholder geometry.
        (false, Some(p)) => (
            p.x,
            p.y,
            p.width,
            p.height,
            read_rotation(Some(p.rotation))?,
        ),
        _ => {
            let rot = read_rotation(surface.matrix_rotation)?;
            let width = if rot != 0.0 {
                surface.scaling_x.abs() * surface.width
            } else {
                surface.bounds.width
            };
            let height = if rot != 0.0 {
                surface.scaling_y.abs() * surface.height
            } else {
                surface.bounds.height
            };
            (surface.position_x, surface.position_y, width, height, rot)
        }
    };
    all_finite([x, y, width, height])?;
    Ok(CapturedGeometry {
        x,
        y,
        width,
        height,
        rotation: if rot != 0.0 { Some(rot) } else { None },
    })
}
