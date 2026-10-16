//! Literal pin tables for the serP/desP oracle (G14/G22/G13, leaf L01-C), part 2: pressure ribbons, paramShape,
//! pathFx and stroke ids. Same generator, row shape and literal grammar as `object_vector_oracle_tables.rs`.

use super::tables::Pin;

pub(crate) const PRESSURE: &[Pin] = &[
    ("vb-basic", "in.strokeColor", "\"#123456\""),
    ("vb-basic", "in.hasRealStroke", "!"),
    ("vb-basic", "paper.strokeColor", "^"),
    (
        "vb-basic",
        "paper.dataKeys",
        concat!(
            "[\"isVectorBrush\",\"centerSegments\",\"widthProfile\",\"strokeProfile\"",
            ",\"profileBase\"]"
        ),
    ),
    ("vb-basic", "out.strokeColor", "^"),
    ("vb-basic", "out.hasRealStroke", "F"),
    ("vb-basic", "out.isVectorBrush", "T"),
    ("vb-basic", "out.isFillShape", "!"),
    ("vb-basic", "out.fillColor", "\"#000000\""),
    (
        "vb-basic",
        "out.segments",
        concat!(
            "[{\"point\":[1,1.5],\"handleIn\":[0,0],\"handleOut\":[2.5,0]},{\"point\"",
            ":[60.123,5.5],\"handleIn\":[-2.5,0],\"handleOut\":[3.75,0]},{\"point\"",
            ":[120.4,2],\"handleIn\":[-3.75,0],\"handleOut\":[0,0]},{\"point\":[60.",
            "8,12],\"handleIn\":[2,0],\"handleOut\":[-2,0]}]"
        ),
    ),
    (
        "vb-basic",
        "out.centerSegments",
        concat!(
            "[{\"point\":[1.23456789,2.34567891],\"handleIn\":[0,0],\"handleOut\":[",
            "3.1111111,0.2222222],\"width\":6.5},{\"point\":[50.987654321,40.1234",
            "56789],\"handleIn\":[-3.1111111,0.2222222],\"handleOut\":[4.4444444,",
            "-1.5555555],\"width\":9.25},{\"point\":[120.5,41.75],\"handleIn\":[-4.",
            "4444444,1.5555555],\"handleOut\":[0,0],\"width\":2}]"
        ),
    ),
    (
        "vb-basic",
        "out.widthProfile",
        concat!(
            "[{\"t\":0,\"width\":2},{\"t\":0.25,\"width\":4.82842712474619},{\"t\":0.5,",
            "\"width\":6},{\"t\":0.75,\"width\":4.82842712474619},{\"t\":1,\"width\":2.",
            "0000000000000004}]"
        ),
    ),
    ("vb-basic", "out.strokeProfile", "\"taper-both\""),
    ("vb-basic", "out.profileBase", "12"),
    ("vb-basic", "paper.centerSegmentsSameRef", "T"),
    ("vb-basic", "paper.widthProfileSameRef", "T"),
    ("vb-has-real-stroke-true", "in.hasRealStroke", "T"),
    ("vb-has-real-stroke-true", "in.strokeColor", "\"#ff00ff\""),
    ("vb-has-real-stroke-true", "paper.strokeColor", "^"),
    ("vb-has-real-stroke-true", "out.strokeColor", "^"),
    ("vb-has-real-stroke-true", "out.hasRealStroke", "F"),
    ("vb-has-real-stroke-true", "out.strokeProfile", "!"),
    ("vb-has-real-stroke-true", "out.profileBase", "!"),
    ("vb-fill-shape", "in.strokeColor", "\"#00ff00\""),
    ("vb-fill-shape", "paper.strokeColor", "\"#00ff00\""),
    (
        "vb-fill-shape",
        "paper.dataKeys",
        concat!(
            "[\"isVectorBrush\",\"centerSegments\",\"widthProfile\",\"strokeProfile\"",
            ",\"profileBase\",\"isFillShape\"]"
        ),
    ),
    ("vb-fill-shape", "out.strokeColor", "^"),
    ("vb-fill-shape", "out.hasRealStroke", "F"),
    ("vb-fill-shape", "out.isVectorBrush", "T"),
    ("vb-fill-shape", "out.isFillShape", "T"),
    ("vb-fill-shape", "out.strokeProfile", "\"bulge\""),
    ("vb-fill-shape", "out.profileBase", "5"),
    (
        "vb-no-center-no-profile",
        "paper.dataKeys",
        "[\"isVectorBrush\"]",
    ),
    ("vb-no-center-no-profile", "out.isVectorBrush", "T"),
    ("vb-no-center-no-profile", "out.centerSegments", "!"),
    ("vb-no-center-no-profile", "out.widthProfile", "!"),
    ("vb-no-center-no-profile", "out.strokeProfile", "!"),
    ("vb-no-center-no-profile", "out.profileBase", "!"),
    ("vb-no-center-no-profile", "out.strokeColor", "^"),
    ("vb-no-center-no-profile", "out.hasRealStroke", "F"),
    ("vb-stroke-color-absent", "in.strokeColor", "!"),
    ("vb-stroke-color-absent", "in.hasRealStroke", "!"),
    ("vb-stroke-color-absent", "paper.strokeColor", "^"),
    ("vb-stroke-color-absent", "out.strokeColor", "^"),
    ("vb-stroke-color-absent", "out.hasRealStroke", "F"),
    ("vb-empty-arrays-zero-base", "in.profileBase", "0"),
    ("vb-empty-arrays-zero-base", "in.strokeProfile", "\"\""),
    (
        "vb-empty-arrays-zero-base",
        "paper.dataKeys",
        "[\"isVectorBrush\",\"centerSegments\",\"widthProfile\"]",
    ),
    ("vb-empty-arrays-zero-base", "out.centerSegments", "[]"),
    ("vb-empty-arrays-zero-base", "out.widthProfile", "[]"),
    ("vb-empty-arrays-zero-base", "out.strokeProfile", "!"),
    ("vb-empty-arrays-zero-base", "out.profileBase", "!"),
    ("non-vb-with-pressure-fields", "in.isFillShape", "T"),
    ("non-vb-with-pressure-fields", "paper.dataKeys", "[]"),
    ("non-vb-with-pressure-fields", "out.isVectorBrush", "!"),
    ("non-vb-with-pressure-fields", "out.isFillShape", "!"),
    ("non-vb-with-pressure-fields", "out.centerSegments", "!"),
    ("non-vb-with-pressure-fields", "out.widthProfile", "!"),
    ("non-vb-with-pressure-fields", "out.strokeProfile", "!"),
    ("non-vb-with-pressure-fields", "out.profileBase", "!"),
    (
        "non-vb-with-pressure-fields",
        "out.strokeColor",
        "\"#112233\"",
    ),
    ("non-vb-with-pressure-fields", "out.hasRealStroke", "T"),
    (
        "bitmap-pressure-profile-carried",
        "paper.dataKeys",
        "[\"bitmapBrushSpec\",\"bitmapPressureProfile\"]",
    ),
    ("bitmap-pressure-profile-carried", "out.strokeColor", "^"),
    ("bitmap-pressure-profile-carried", "out.hasRealStroke", "F"),
    (
        "bitmap-pressure-profile-carried",
        "out.bitmapBrushSpec",
        "{\"preset\":\"chalk\"}",
    ),
    (
        "bitmap-pressure-profile-carried",
        "out.bitmapPressureProfile",
        "[[1,2,0.5],[3,4,1]]",
    ),
    ("bitmap-pressure-profile-carried", "out.isVectorBrush", "!"),
];

pub(crate) const PARAM_FX_ID: &[Pin] = &[
    (
        "param-rect-with-box-angle",
        "in.paramShape",
        concat!(
            "{\"kind\":\"rect\",\"tl\":5,\"tr\":0,\"br\":0,\"bl\":10,\"box\":{\"x1\":0,\"y1\":0",
            ",\"x2\":100,\"y2\":50}}"
        ),
    ),
    ("param-rect-with-box-angle", "in.boxAngle", "30"),
    (
        "param-rect-with-box-angle",
        "paper.dataKeys",
        "[\"boxAngle\",\"paramShape\"]",
    ),
    ("param-rect-with-box-angle", "paper.paramShapeSameRef", "T"),
    (
        "param-rect-with-box-angle",
        "out.paramShape",
        concat!(
            "{\"kind\":\"rect\",\"tl\":5,\"tr\":0,\"br\":0,\"bl\":10,\"box\":{\"x1\":0,\"y1\":0",
            ",\"x2\":100,\"y2\":50}}"
        ),
    ),
    ("param-rect-with-box-angle", "out.boxAngle", "30"),
    (
        "param-ellipse",
        "out.paramShape",
        concat!(
            "{\"kind\":\"ellipse\",\"startAngle\":0,\"sweep\":359.9,\"innerRadius\":0,\"",
            "box\":{\"x1\":0,\"y1\":0,\"x2\":100,\"y2\":50}}"
        ),
    ),
    ("param-ellipse", "paper.paramShapeSameRef", "T"),
    (
        "param-star",
        "out.paramShape",
        concat!(
            "{\"kind\":\"star\",\"pointCount\":5,\"innerRatio\":0.5,\"cornerRadius\":0,",
            "\"box\":{\"x1\":0,\"y1\":0,\"x2\":100,\"y2\":50}}"
        ),
    ),
    ("param-star", "paper.paramShapeSameRef", "T"),
    ("param-null", "in.paramShape", "^"),
    ("param-null", "paper.dataKeys", "[]"),
    ("param-null", "out.paramShape", "!"),
    ("param-absent", "in.paramShape", "!"),
    ("param-absent", "paper.dataKeys", "[]"),
    ("param-absent", "out.paramShape", "!"),
    (
        "path-fx-key-on-stroke-dict-dropped",
        "in.pathFx",
        "[{\"type\":\"zigzag\",\"enabled\":T,\"p1\":4,\"p2\":2}]",
    ),
    (
        "path-fx-key-on-stroke-dict-dropped",
        "paper.dataKeys",
        "[\"effects\"]",
    ),
    ("path-fx-key-on-stroke-dict-dropped", "out.pathFx", "!"),
    (
        "path-fx-key-on-stroke-dict-dropped",
        "out.effects",
        "[{\"type\":\"blur\",\"enabled\":T}]",
    ),
    (
        "stroke-id-kept",
        "paper.dataKeys",
        "[\"strokeId\",\"groupId\"]",
    ),
    ("stroke-id-kept", "out.strokeId", "\"st_01\""),
    ("stroke-id-kept", "out.groupId", "\"g_9\""),
    ("stroke-id-empty", "in.strokeId", "\"\""),
    ("stroke-id-empty", "paper.dataKeys", "[]"),
    ("stroke-id-empty", "out.strokeId", "!"),
];
