//! L01-E hand-written pins: the facts of the oracle rows repeated as literals, so neither the generator
//! mapping nor the oracle file is trusted alone (the tests compare them with the decoded native records).

pub(super) struct PressurePin {
    pub(super) id: &'static str,
    /// Per-anchor widths in order; `None` = no centerSegments.
    pub(super) widths: Option<&'static [f64]>,
    /// (t, width) stops; `None` = no widthProfile.
    pub(super) profile: Option<&'static [(f64, f64)]>,
    pub(super) kind: Option<&'static str>,
    pub(super) base: Option<f64>,
}
pub(super) const PRESSURE_PINS: [PressurePin; 7] = [
    PressurePin {
        id: "pressure-nonuniform-bulge",
        widths: Some(&[1.5, 6.0, 3.25, 0.75]),
        profile: Some(&[
            (0.0, 1.5),
            (0.25, 4.5),
            (0.5, 6.0),
            (0.8, 2.25),
            (1.0, 0.75),
        ]),
        kind: Some("bulge"),
        base: Some(6.0),
    },
    PressurePin {
        id: "pressure-profile-without-base",
        widths: Some(&[2.0, 7.5]),
        profile: Some(&[(0.0, 2.0), (0.4, 9.0), (1.0, 7.5)]),
        kind: None,
        base: None,
    },
    PressurePin {
        id: "pressure-flag-only",
        widths: None,
        profile: None,
        kind: None,
        base: None,
    },
    PressurePin {
        id: "pressure-live-stroke-is-dropped",
        widths: Some(&[1.5, 6.0, 3.25, 0.75]),
        profile: Some(&[
            (0.0, 1.5),
            (0.25, 4.5),
            (0.5, 6.0),
            (0.8, 2.25),
            (1.0, 0.75),
        ]),
        kind: None,
        base: None,
    },
    PressurePin {
        id: "pressure-profile-base-zero-dropped",
        widths: Some(&[1.5, 6.0, 3.25, 0.75]),
        profile: Some(&[
            (0.0, 1.5),
            (0.25, 4.5),
            (0.5, 6.0),
            (0.8, 2.25),
            (1.0, 0.75),
        ]),
        kind: Some("even"),
        base: None,
    },
    PressurePin {
        id: "pressure-stationary-pen-equal-t",
        widths: Some(&[2.0, 3.0]),
        profile: Some(&[(0.0, 2.0), (0.5, 3.0), (0.5, 5.0), (1.0, 1.0)]),
        kind: Some("taper-both"),
        base: Some(5.0),
    },
    PressurePin {
        id: "pressure-taper-in-opacity",
        widths: Some(&[1.5, 6.0, 3.25, 0.75]),
        profile: Some(&[
            (0.0, 1.5),
            (0.25, 4.5),
            (0.5, 6.0),
            (0.8, 2.25),
            (1.0, 0.75),
        ]),
        kind: Some("taper-in"),
        base: Some(3.5),
    },
];

pub(super) struct ShapePin {
    pub(super) id: &'static str,
    pub(super) kind: &'static str,
    /// Exactly the kind-specific members the baseline keeps, by wire name.
    pub(super) members: &'static [(&'static str, f64)],
    /// (x1, y1, x2, y2) of `paramShape.box`.
    pub(super) frame: Option<[f64; 4]>,
    pub(super) angle: Option<f64>,
}
pub(super) const SHAPE_PINS: [ShapePin; 11] = [
    ShapePin {
        id: "rect-four-radii-box-angle",
        kind: "rect",
        members: &[("tl", 4.0), ("tr", 8.0), ("br", 12.0), ("bl", 16.0)],
        frame: Some([0.0, 0.0, 100.0, 50.0]),
        angle: Some(15.0),
    },
    ShapePin {
        id: "rect-zero-radius-vs-absent",
        kind: "rect",
        members: &[("tl", 0.0), ("tr", 12.0)],
        frame: None,
        angle: None,
    },
    ShapePin {
        id: "ellipse-arc-donut",
        kind: "ellipse",
        members: &[
            ("startAngle", 30.0),
            ("sweep", 270.0),
            ("innerRadius", 20.0),
        ],
        frame: Some([0.0, 0.0, 100.0, 50.0]),
        angle: None,
    },
    ShapePin {
        id: "ellipse-default-without-box",
        kind: "ellipse",
        members: &[("startAngle", 0.0), ("sweep", 359.9), ("innerRadius", 0.0)],
        frame: None,
        angle: None,
    },
    ShapePin {
        id: "ellipse-sweep-absent",
        kind: "ellipse",
        members: &[("startAngle", 12.5)],
        frame: Some([-10.0, -5.0, 90.0, 45.0]),
        angle: None,
    },
    ShapePin {
        id: "star-five-points",
        kind: "star",
        members: &[
            ("pointCount", 5.0),
            ("innerRatio", 0.5),
            ("cornerRadius", 0.0),
        ],
        frame: Some([0.0, 0.0, 100.0, 50.0]),
        angle: None,
    },
    ShapePin {
        id: "polygon-six-rounded-negative-angle",
        kind: "star",
        members: &[
            ("pointCount", 6.0),
            ("innerRatio", 1.0),
            ("cornerRadius", 3.5),
        ],
        frame: Some([0.0, 0.0, 100.0, 50.0]),
        angle: Some(-30.0),
    },
    ShapePin {
        id: "box-angle-zero-dropped",
        kind: "rect",
        members: &[("tl", 2.0), ("tr", 2.0), ("br", 2.0), ("bl", 2.0)],
        frame: Some([0.0, 0.0, 100.0, 50.0]),
        angle: None,
    },
    ShapePin {
        id: "star-box-translated",
        kind: "star",
        members: &[
            ("pointCount", 7.0),
            ("innerRatio", 0.4),
            ("cornerRadius", 1.0),
        ],
        frame: Some([25.0, -10.0, 125.0, 40.0]),
        angle: None,
    },
    ShapePin {
        id: "ellipse-box-scaled",
        kind: "ellipse",
        members: &[("startAngle", 0.0), ("sweep", 180.0), ("innerRadius", 5.0)],
        frame: Some([20.0, 5.0, 220.0, 30.0]),
        angle: None,
    },
    ShapePin {
        id: "star-box-mirrored-stays-sorted",
        kind: "star",
        members: &[
            ("pointCount", 5.0),
            ("innerRatio", 0.5),
            ("cornerRadius", 0.0),
        ],
        frame: Some([-100.0, 0.0, 0.0, 50.0]),
        angle: None,
    },
];

/// Layer stacks as `type/enabled/p1,p2,p3`: enabled `t`, `f` or `-` (absent); a parameter is a number,
/// `n` (explicit null) or `-` (absent). One entry per word, storage order.
pub(super) const FX_PINS: [(&str, &str); 8] = [
    (
        "order-zigzag-then-roughen",
        "zigzag/t/10,3,1 roughen/t/5,2,7",
    ),
    (
        "order-roughen-then-zigzag",
        "roughen/t/5,2,7 zigzag/t/10,3,1",
    ),
    (
        "enabled-absent-and-null-params",
        "zigzag/-/n,6,- roughen/t/-,-,-",
    ),
    (
        "disabled-entry-keeps-its-slot",
        "zigzag/f/20,5,0 roughen/t/3,9,0",
    ),
    ("all-disabled", "zigzag/f/1,1,0 roughen/f/-,-,-"),
    ("duplicate-types-allowed", "zigzag/t/8,2,0 zigzag/t/16,5,1"),
    (
        "explicit-zero-is-not-absent",
        "zigzag/t/0,1,0 zigzag/t/-,1,-",
    ),
    ("empty-stack", ""),
];
