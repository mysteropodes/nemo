use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use crate::render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint};
use vello::kurbo::PathEl;

fn segments() -> Vec<CubicSegment> {
    vec![
        CubicSegment {
            point: [40.0, 40.0],
            handle_in: [0.0, 0.0],
            handle_out: [0.0, 40.0],
        },
        CubicSegment {
            point: [80.0, 40.0],
            handle_in: [0.0, 40.0],
            handle_out: [0.0, 0.0],
        },
    ]
}

pub(crate) fn cap(offset: f64) -> GeometryPaintInput {
    GeometryPaintInput::new(
        "geometry/r08",
        "v1",
        vec![LayerGeometry::closed_path(
            "r08_curve_layer",
            ClosedCubicPath::new(segments()).unwrap(),
            [1.0, 0.0, 0.0, 1.0, offset, 0.0],
            OpaqueSrgbPaint::new(255, 0, 0),
        )
        .unwrap()],
    )
    .unwrap()
}

pub(crate) fn assert_cap_pixels(bytes: &[u8], opacity: u8, offset: usize) {
    assert_eq!(bytes.len(), 320 * 180 * 4);
    let pixel =
        |x: usize, y: usize| &bytes[((y * 320 + x + offset) * 4)..((y * 320 + x + offset) * 4 + 4)];
    for (x, y) in [(60, 50), (60, 65)] {
        let p = pixel(x, y);
        assert_eq!((p[0], p[3]), (255, 255));
        assert!((f64::from(p[1]) - 255.0 * (1.0 - f64::from(opacity) / 100.0)).abs() <= 1.0);
        assert_eq!(p[1], p[2]);
    }
    for (x, y) in [(45, 65), (60, 75), (30, 50)] {
        assert_eq!(pixel(x, y), [255, 255, 255, 255], "outside cap at {x},{y}");
    }
}

#[test]
fn hand_authored_closed_commands_and_hull() {
    let path = ClosedCubicPath::new(segments()).unwrap();
    assert_eq!(path.segments(), segments());
    assert_eq!(path.control_bounds(), [40.0, 40.0, 80.0, 80.0]);
    assert_eq!(
        path.commands(),
        &[
            PathEl::MoveTo((40.0, 40.0).into()),
            PathEl::CurveTo(
                (40.0, 80.0).into(),
                (80.0, 80.0).into(),
                (80.0, 40.0).into()
            ),
            PathEl::CurveTo(
                (80.0, 40.0).into(),
                (40.0, 40.0).into(),
                (40.0, 40.0).into()
            ),
            PathEl::ClosePath,
        ]
    );
    assert_eq!(
        path.transformed_envelope([1.0, 0.0, 0.0, 1.0, 64.0, 0.0])
            .unwrap(),
        [104.0, 40.0, 144.0, 80.0]
    );
}

#[test]
fn malformed_count_controls_hull_and_transform_are_rejected() {
    for count in [0, 1, 257] {
        assert!(ClosedCubicPath::new(vec![segments()[0]; count]).is_err());
    }
    assert!(ClosedCubicPath::new(segments().into_iter().cycle().take(256).collect()).is_ok());
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for component in 0..6 {
            let mut input = segments();
            let values = match component / 2 {
                0 => &mut input[0].point,
                1 => &mut input[0].handle_in,
                _ => &mut input[0].handle_out,
            };
            values[component % 2] = bad;
            assert!(ClosedCubicPath::new(input).is_err());
        }
    }
    let mut overflow = segments();
    overflow[0].point[0] = f64::MAX;
    overflow[0].handle_in[0] = f64::MAX;
    assert!(ClosedCubicPath::new(overflow).is_err());
    for points in [
        [[0.0, 0.0], [0.0, 0.0]],
        [[0.0, 0.0], [1.0, 0.0]],
        [[0.0, 0.0], [1.0, 1.0]],
    ] {
        assert!(ClosedCubicPath::new(
            points
                .map(|point| CubicSegment {
                    point,
                    handle_in: [0.0; 2],
                    handle_out: [0.0; 2]
                })
                .to_vec()
        )
        .is_err());
    }
    for transform in [
        [0.0; 6],
        [1.0, 0.0, 0.0, 1.0, f64::NAN, 0.0],
        [f64::MAX, 0.0, 0.0, 1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0, 1.0, f64::MAX, 0.0],
    ] {
        assert!(LayerGeometry::closed_path(
            "layer",
            ClosedCubicPath::new(segments()).unwrap(),
            transform,
            OpaqueSrgbPaint::new(255, 0, 0)
        )
        .is_err());
    }
}

// Observe the exact production composition read by the export job. This wrapper
// adds no GPU owner, pixel generator or alternate composition route.
#[cfg(feature = "test-export_job")]
pub(crate) struct ObservedCompositor {
    pub(crate) gpu: crate::compositor::Compositor,
    pub(crate) readbacks: std::rc::Rc<std::cell::RefCell<Vec<crate::png_output::ExportReadback>>>,
}

#[cfg(feature = "test-export_job")]
pub(crate) fn assert_cap_exports(
    decoded: &[(u32, u32, Vec<u8>)],
    readbacks: &[crate::png_output::ExportReadback],
    snapshot_id: &str,
) {
    assert_eq!(decoded.len(), 3);
    assert_eq!(readbacks.len(), 3);
    for ((png, pixels), (frame, opacity)) in
        decoded
            .iter()
            .zip(readbacks)
            .zip([(0, 20), (10, 50), (20, 80)])
    {
        assert_eq!(png, &(pixels.width, pixels.height, pixels.bytes.clone()));
        assert_eq!(pixels.document_snapshot_id, snapshot_id);
        assert_eq!((pixels.content_revision, pixels.source_frame), (0, frame));
        assert_cap_pixels(&pixels.bytes, opacity, 64);
    }
}

#[cfg(feature = "test-export_job")]
impl crate::png_output::ExportCompositor for ObservedCompositor {
    type Composition = crate::compositor::CompositionResult;
    fn compose(
        &mut self,
        scene: &crate::render_scene::RenderScene,
    ) -> Result<Self::Composition, String> {
        self.gpu.compose(scene).map_err(|e| e.to_string())
    }
    fn readback_rgba8(
        &self,
        result: &Self::Composition,
    ) -> Result<crate::png_output::ExportReadback, String> {
        let pixels = crate::png_output::ExportCompositor::readback_rgba8(&self.gpu, result)?;
        self.readbacks.borrow_mut().push(pixels.clone());
        Ok(pixels)
    }
}
