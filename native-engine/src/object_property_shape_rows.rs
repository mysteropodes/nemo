//! A dynamic shape's own Motion rows (baseline `propsFor`: `paramShape` kind rect / ellipse / star), keyable per element.

pub(crate) fn shape_rows(kind: Option<&str>) -> &'static [&'static str] {
    match kind {
        Some("rect") => &["cornerTL", "cornerTR", "cornerBR", "cornerBL"],
        Some("ellipse") => &["arcStart", "arcSweep", "arcInner"],
        Some("star") => &["starInner", "starCorner"],
        _ => &[],
    }
}
