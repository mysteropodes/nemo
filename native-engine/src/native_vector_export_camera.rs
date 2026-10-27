//! Stored camera on the complete accepted SVG paint family, before counting bytes.
//! Applies to absolute anchors and handles, preserving original alpha and path order.
use crate::export_job::native_vector_export::native_vector_export_svg::{
    unsupported_video, Sink, SvgExportError,
};
use crate::object_render_scene::{ordered_entries, ObjectPaintPlan, PaintEntry};
#[derive(Debug, Clone, Copy)]
pub(super) struct Camera {
    matrix: [f64; 6],
    width: u32,
    height: u32,
}
use crate::render_geometry::ClosedCubicPath;

fn unavailable() -> SvgExportError {
    SvgExportError::Unsupported {
        source_object_index: 0,
        reason: "stored camera geometry unavailable",
    }
}
pub(super) fn point(camera: Option<Camera>, p: [f64; 2]) -> Result<[f64; 2], SvgExportError> {
    let value = match camera {
        None => return Ok(p),
        Some(Camera {
            matrix: [a, b, c, d, x, y],
            ..
        }) => [a * p[0] + c * p[1] + x, b * p[0] + d * p[1] + y],
    };
    if value.iter().all(|n| n.is_finite()) {
        Ok(value)
    } else {
        Err(unavailable())
    }
}
fn preflight(camera: Camera, contour: &ClosedCubicPath) -> Result<(), SvgExportError> {
    for segment in contour.segments() {
        point(Some(camera), segment.point)?;
        for handle in [segment.handle_in, segment.handle_out] {
            point(
                Some(camera),
                [segment.point[0] + handle[0], segment.point[1] + handle[1]],
            )?;
        }
    }
    Ok(())
}
pub(super) fn for_plan(plan: &ObjectPaintPlan) -> Result<Option<Camera>, SvgExportError> {
    let Some(matrix) = plan
        .packet()
        .stored_camera_matrix()
        .map_err(|_| unavailable())?
    else {
        return Ok(None);
    };
    let (width, height) = plan
        .packet()
        .snapshot()
        .project()
        .ok_or_else(unavailable)?
        .project()
        .canvas_dimensions();
    let camera = Camera {
        matrix,
        width,
        height,
    };
    // Preflight every accepted path before even the document header is counted.
    // Existing unsupported-family/raster/mask admission still runs unchanged.
    for entry in ordered_entries(plan) {
        match entry {
            PaintEntry::Contour(c) => preflight(camera, c.contour())?,
            PaintEntry::Text(t) => {
                for (contour, _) in t.item.painted() {
                    preflight(camera, &contour.contour)?;
                }
            }
            PaintEntry::Component(c) => {
                if let Some(contour) = c.contour.as_ref() {
                    preflight(camera, contour)?;
                }
            }
            PaintEntry::Duplicator(c) => {
                if let Some(contour) = c.svg_contour.as_ref() {
                    preflight(camera, contour)?;
                }
            }
            PaintEntry::Dab(d) => {
                if let Some(contour) = d.svg_contour.as_ref() {
                    preflight(camera, contour)?;
                }
            }
            PaintEntry::Raster(_) => {}
            PaintEntry::Video(index) => return Err(unsupported_video(plan, index)),
            #[cfg(all(feature = "application", feature = "project_media_codec"))]
            PaintEntry::Brush(_) => {}
        }
    }
    Ok(Some(camera))
}

pub(super) fn begin(sink: &mut Sink, camera: Camera) -> Result<(), SvgExportError> {
    sink.put(&format!("<defs><clipPath id=\"nemo-camera-clip\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/></clipPath></defs>\n<g clip-path=\"url(#nemo-camera-clip)\">\n", camera.width, camera.height))
}
