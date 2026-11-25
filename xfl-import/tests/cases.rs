mod common;

use common::*;
use nemo_xfl_import::*;
use std::io::Write;

#[test]
fn primitives_become_geometry() {
    let doc = read_fla(&sample()).unwrap();
    let Element::Shape(oval) = &doc.timelines[0].layers[2].frames[2].elements[3] else {
        panic!()
    };
    let c = &oval.paths[0].contours[0];
    assert!(c.closed);
    assert_eq!(c.segments.len(), 8);
    // Every arc end lies on the ellipse centred (30, 30) with radii (20, 10).
    for s in &c.segments {
        let p = s.end();
        let v = ((p.x - 30.0) / 20.0).powi(2) + ((p.y - 30.0) / 10.0).powi(2);
        assert!((v - 1.0).abs() < 1e-12, "{p:?}");
    }
    assert!(doc
        .report
        .skipped_feature("oval primitive arc or inner radius (full ellipse used)")
        .is_some());

    let rect = r##"<DOMRectangleObject x="1" y="2" objectWidth="30" objectHeight="40" topLeftRadius="5">
      <fill><SolidColor color="#00FF00"/></fill>
      <stroke><SolidStroke weight="2"><fill><SolidColor color="#000000"/></fill></SolidStroke></stroke></DOMRectangleObject>"##;
    let doc = shape_doc(rect);
    let s = first_shape(&doc);
    assert_eq!(s.paths.len(), 2);
    assert!((area(&s.paths[0].contours[0]).abs() - 1200.0).abs() < 1e-9);
    assert_eq!(s.paths[1].stroke.as_ref().unwrap().weight, 2.0);
    assert!(doc
        .report
        .skipped_feature("rectangle primitive corner radius (square corners used)")
        .is_some());
}

#[test]
fn same_fill_on_both_sides_is_ignored() {
    let shape = r##"<DOMShape><fills><FillStyle index="1"><SolidColor/></FillStyle></fills><edges>
      <Edge fillStyle1="1" edges="!0 0|2000 0|2000 2000|0 2000|0 0"/>
      <Edge fillStyle0="1" fillStyle1="1" edges="!0 0|2000 2000"/>
    </edges></DOMShape>"##;
    let doc = shape_doc(shape);
    let p = &first_shape(&doc).paths[0];
    assert_eq!(p.contours.len(), 1);
    assert!(p.contours[0].closed);
    assert_eq!(p.contours[0].segments.len(), 4);
}

#[test]
fn touching_loops_at_one_vertex_all_close() {
    // Figure-eight: two squares meet at (100,100); that vertex has two outgoing
    // pieces of the same fill. In a balanced graph a greedy walk can only stop
    // at its start, so every loop closes whatever the pick.
    let shape = r##"<DOMShape><fills><FillStyle index="1"><SolidColor/></FillStyle></fills><edges>
      <Edge fillStyle1="1" edges="!2000 2000|4000 2000|4000 4000|2000 4000|2000 2000"/>
      <Edge fillStyle1="1" edges="!0 0|2000 0|2000 2000|0 2000|0 0"/>
    </edges></DOMShape>"##;
    let doc = shape_doc(shape);
    let p = &first_shape(&doc).paths[0];
    assert!(p.contours.iter().all(|c| c.closed), "{p:?}");
    let total: f64 = p.contours.iter().map(area).sum();
    assert!((total.abs() - 20_000.0).abs() < 1e-9);
    assert!(doc
        .report
        .skipped_feature("fill contour that does not close (kept open)")
        .is_none());
}

#[test]
fn negative_hex_coordinates_end_to_end() {
    // The same square in decimal and in 24-bit hex (-100 px = -2000 twips = #FFF830).
    let shape = r##"<DOMShape><fills><FillStyle index="1"><SolidColor/></FillStyle></fills><edges>
      <Edge fillStyle1="1" edges="!-2000 -2000|0 -2000"/>
      <Edge fillStyle1="1" edges="!#0 #FFF830|#0 #0|#FFF830 #0|#FFF830 #FFF830"/>
    </edges></DOMShape>"##;
    let doc = shape_doc(shape);
    let c = &first_shape(&doc).paths[0].contours;
    assert_eq!(c.len(), 1);
    assert!(c[0].closed);
    assert!((area(&c[0]).abs() - 10_000.0).abs() < 1e-9);
}

#[test]
fn single_closed_quad_stroke_is_closed() {
    let shape = r##"<DOMShape><strokes><StrokeStyle index="1"><SolidStroke><fill><SolidColor/></fill></SolidStroke></StrokeStyle></strokes>
      <edges><Edge strokeStyle="1" edges="!0 0[400 400 0 0"/><Edge cubics="!0 0(1 1 2 2 3 3)"/></edges></DOMShape>"##;
    let doc = shape_doc(shape);
    let c = &first_shape(&doc).paths[0].contours[0];
    assert!(c.closed);
    assert!(doc
        .report
        .skipped_feature("edge stored only as cubics (not read)")
        .is_some());
}

#[test]
fn duplicate_zip_entries_are_refused() {
    let bytes = {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("DOMDocument.xml", o).unwrap();
            w.write_all(b"<DOMDocument/>").unwrap();
            w.start_file("./DOMDocument.xml", o).unwrap();
            w.write_all(b"<DOMDocument/>").unwrap();
            w.finish().unwrap();
        }
        buf.into_inner()
    };
    assert!(matches!(read_fla(&bytes), Err(XflError::NotAnArchive(_))));
}

/// A 2×1 lossless ARGB entry: opaque red, then fully transparent.
/// (Red tells ARGB from BGRA: misreading the order would give blue.)
#[test]
fn library_bitmaps_are_decoded() {
    let doc = read_fla(&sample()).unwrap();
    let Some(BitmapData::Rgba {
        width,
        height,
        pixels,
    }) = &doc.bitmaps[0].data
    else {
        panic!("{:?}", doc.bitmaps[0].data)
    };
    assert_eq!((*width, *height), (2, 1));
    assert_eq!(pixels, &vec![255, 0, 0, 255, 0, 0, 0, 0]);
    assert_eq!(doc.report.converted["decodedBitmaps"], 1);

    let bad = zip_bytes(&[
        ("DOMDocument.xml", doc_xml("").as_bytes()),
        ("bin/M 1 1.dat", b"junk"),
    ]);
    let doc = read_fla(&bad).unwrap();
    assert!(doc.bitmaps[0].data_present && doc.bitmaps[0].data.is_none());
    assert!(doc
        .report
        .skipped_feature("bitmap not decoded: unrecognised bitmap data")
        .is_some());
}

#[test]
fn classic_tween_samples_between_keyframes() {
    let doc = r##"<DOMDocument><timelines><DOMTimeline name="S"><layers><DOMLayer name="L"><frames>
      <DOMFrame index="0" duration="10" tweenType="motion" acceleration="0"><elements>
        <DOMSymbolInstance libraryItemName="Ball" symbolType="graphic"><matrix><Matrix tx="0" ty="0"/></matrix>
          <transformationPoint><Point x="5" y="5"/></transformationPoint></DOMSymbolInstance></elements></DOMFrame>
      <DOMFrame index="10" duration="5" tweenType="motion"><elements>
        <DOMSymbolInstance libraryItemName="Ball" symbolType="graphic"><matrix><Matrix a="2" d="2" tx="100" ty="50"/></matrix>
          <color><Color alphaMultiplier="0"/></color></DOMSymbolInstance></elements></DOMFrame>
      <DOMFrame index="15"><elements>
        <DOMSymbolInstance libraryItemName="Other"/></elements></DOMFrame>
    </frames></DOMLayer></layers></DOMTimeline></timelines></DOMDocument>"##;
    let doc = read_fla(&zip_bytes(&[("DOMDocument.xml", doc.as_bytes())])).unwrap();
    let (tweens, skipped) = classic_tweens(&doc.timelines[0], 0);
    assert_eq!(tweens.len(), 1);
    assert_eq!(skipped, vec![(10, TweenSkip::EndMismatch)]);
    let t = &tweens[0];
    assert_eq!((t.start_frame, t.end_frame), (0, 10));
    // Pivot (5,5): start at (5,5); end matrix puts it at (110,60).
    let (mid, alpha) = t.sample(5);
    assert!(
        (mid.x - 57.5).abs() < 1e-9 && (mid.y - 32.5).abs() < 1e-9,
        "{mid:?}"
    );
    assert!((mid.scale_x - 1.5).abs() < 1e-9 && (alpha - 0.5).abs() < 1e-9);
    let end = t.sample(10).0.to_matrix(t.pivot);
    assert!(
        (end.a - 2.0).abs() < 1e-9 && (end.tx - 100.0).abs() < 1e-9 && (end.ty - 50.0).abs() < 1e-9
    );
}

#[test]
fn classic_tween_scale_hold_and_guide_flagged() {
    let doc = r##"<DOMDocument><timelines><DOMTimeline name="S"><layers>
      <DOMLayer name="G" layerType="guide"><frames><DOMFrame index="0" duration="10"><elements/></DOMFrame></frames></DOMLayer>
      <DOMLayer name="L" parentLayerIndex="0"><frames>
      <DOMFrame index="0" duration="10" tweenType="motion" motionTweenScale="false" motionTweenOrientToPath="true" hasCustomEase="true"><elements>
        <DOMSymbolInstance libraryItemName="Ball"><matrix><Matrix/></matrix></DOMSymbolInstance></elements></DOMFrame>
      <DOMFrame index="10"><elements>
        <DOMSymbolInstance libraryItemName="Ball"><matrix><Matrix a="3" d="3"/></matrix></DOMSymbolInstance></elements></DOMFrame>
    </frames></DOMLayer></layers></DOMTimeline></timelines></DOMDocument>"##;
    let doc = read_fla(&zip_bytes(&[("DOMDocument.xml", doc.as_bytes())])).unwrap();
    let (tweens, _) = classic_tweens(&doc.timelines[0], 1);
    let t = &tweens[0];
    assert!(!t.tween_scale);
    assert!((t.sample(5).0.scale_x - 1.0).abs() < 1e-9);
    assert_eq!(t.approximations.len(), 3, "{:?}", t.approximations);
    // Out-of-range layer index is empty, not a panic.
    assert_eq!(classic_tweens(&doc.timelines[0], 9), (vec![], vec![]));
}

const BALL_AT: &str = r##"<DOMSymbolInstance libraryItemName="Ball"><matrix><Matrix tx="TX" ty="TY"/></matrix>
          <transformationPoint><Point x="5" y="5"/></transformationPoint></DOMSymbolInstance>"##;

fn ball(tx: f64, ty: f64) -> String {
    BALL_AT
        .replace("TX", &tx.to_string())
        .replace("TY", &ty.to_string())
}

#[test]
fn custom_ease_curves_drive_each_property() {
    // `all` is the straight line, `position` is y = u³ over x = u.
    let curves = r##"<tweens>
        <CustomEase target="all"><Point/><Point x="0.25" y="0.25"/><Point x="0.75" y="0.75"/><Point x="1" y="1"/></CustomEase>
        <CustomEase target="position"><Point x="0" y="0"/><Point x="0.3333333333333333" y="0"/><Point x="0.6666666666666666" y="0"/><Point x="1" y="1"/></CustomEase>
      </tweens>"##;
    let frame = |index: u32, attrs: &str, body: &str, tx: f64| {
        format!(
            r##"<DOMFrame index="{index}" duration="10" tweenType="motion" {attrs}>{body}<elements>{}</elements></DOMFrame>"##,
            ball(tx, 0.0)
        )
    };
    let doc = format!(
        r##"<DOMDocument><timelines><DOMTimeline name="S"><layers><DOMLayer name="L"><frames>{}{}{}{}{}
      <DOMFrame index="50"><elements>{}</elements></DOMFrame>
    </frames></DOMLayer></layers></DOMTimeline></timelines></DOMDocument>"##,
        frame(
            0,
            r#"hasCustomEase="true" useSingleEaseCurve="false" acceleration="100""#,
            curves,
            0.0
        ),
        frame(
            10,
            r#"hasCustomEase="true" useSingleEaseCurve="true""#,
            curves,
            100.0
        ),
        // Curve data kept in the file with the custom ease switched off.
        frame(
            20,
            r#"hasCustomEase="false" acceleration="-100""#,
            curves,
            200.0
        ),
        frame(
            30,
            "",
            r#"<tweens><Ease target="all" intensity="-100"/></tweens>"#,
            300.0
        ),
        frame(
            40,
            r#"hasCustomEase="true""#,
            r#"<tweens><CustomEase target="all"><Point/><Point x="1" y="1"/></CustomEase></tweens>"#,
            400.0
        ),
        ball(500.0, 0.0),
    );
    let doc = read_fla(&zip_bytes(&[("DOMDocument.xml", doc.as_bytes())])).unwrap();
    let (tweens, skipped) = classic_tweens(&doc.timelines[0], 0);
    assert!(skipped.is_empty() && tweens.len() == 5);
    // Pivot x at frame 5 of each span: start + 5 + 100·k(0.5).
    let mid_k =
        |i: usize| (tweens[i].sample(i as u32 * 10 + 5).0.x - 5.0 - 100.0 * i as f64) / 100.0;
    assert!((mid_k(0) - 0.125).abs() < 1e-9, "{}", mid_k(0)); // position curve
    assert!((mid_k(1) - 0.5).abs() < 1e-9); // single curve: `all`
    assert!((mid_k(2) - 0.25).abs() < 1e-9); // switched off: acceleration
    assert!((mid_k(3) - 0.25).abs() < 1e-9); // <Ease intensity>
    assert!((mid_k(4) - 0.5).abs() < 1e-9); // malformed: acceleration 0
    for t in &tweens[..4] {
        assert!(t.approximations.is_empty(), "{:?}", t.approximations);
    }
    assert_eq!(
        tweens[4].approximations,
        ["malformed custom ease curve (sampled with the simple ease)"]
    );
    // Rotation, scale and colour of the first tween use `all`, not `position`.
    assert!(matches!(&tweens[0].eases.position, Ease::Custom { points } if points[1].y == 0.0));
    assert_eq!(tweens[0].eases.rotation, tweens[1].eases.position);
    assert_eq!(doc.report.converted["customEases"], 2);
    assert!(doc
        .report
        .skipped_feature("malformed custom ease curve (sampled with the simple ease)")
        .is_some());
}

#[test]
fn motion_guide_moves_along_the_stroke() {
    let guided = |name: &str, edges: &str, attrs: &str, from: (f64, f64), to: (f64, f64)| {
        format!(
            r##"<DOMLayer name="G{name}" layerType="guide"><frames><DOMFrame index="0" duration="11"><elements><DOMShape>
        <strokes><StrokeStyle index="1"><SolidStroke weight="1"><fill><SolidColor color="#000000"/></fill></SolidStroke></StrokeStyle></strokes>
        <edges>{edges}</edges></DOMShape></elements></DOMFrame></frames></DOMLayer>
      <DOMLayer name="{name}" parentLayerIndex="PARENT"><frames>
        <DOMFrame index="0" duration="10" tweenType="motion" motionTweenSnap="true" {attrs}><elements>{}</elements></DOMFrame>
        <DOMFrame index="10"><elements>{}</elements></DOMFrame>
      </frames></DOMLayer>"##,
            ball(from.0, from.1),
            ball(to.0, to.1)
        )
    };
    // Half circle of radius 100 px around (200, 200), over the top, drawn as
    // two strokes that share the top point; then a straight guide; then a
    // guide the instance is nowhere near.
    let arc = r##"<Edge strokeStyle="1" edges="!4000 2000[4828 2000 5414 2586[6000 3172 6000 4000"/>
        <Edge strokeStyle="1" edges="!2000 4000[2000 3172 2586 2586[3172 2000 4000 2000"/>"##;
    let line = r##"<Edge strokeStyle="1" edges="!2000 4000|6000 4000"/>"##;
    let orient = r#"motionTweenOrientToPath="true""#;
    let layers = [
        guided("Arc", arc, orient, (95.0, 195.0), (295.0, 195.0)).replace("PARENT", "0"),
        guided("Line", line, orient, (95.0, 195.0), (295.0, 195.0)).replace("PARENT", "2"),
        guided("Far", line, "", (95.0, 0.0), (295.0, 0.0)).replace("PARENT", "4"),
        // Symbol origin on the arc instead of the transformation point.
        guided("Reg", arc, "", (100.0, 200.0), (300.0, 200.0)).replace("PARENT", "6"),
    ]
    .concat();
    let doc = format!(
        r##"<DOMDocument><timelines><DOMTimeline name="S"><layers>{layers}</layers></DOMTimeline></timelines></DOMDocument>"##
    );
    let doc = read_fla(&zip_bytes(&[("DOMDocument.xml", doc.as_bytes())])).unwrap();
    let tl = &doc.timelines[0];
    assert!(tl.layers[1].frames[0].tween.as_ref().unwrap().snap);
    let near = |a: f64, b: f64| (a - b).abs() < 0.05;

    let arc = &classic_tweens(tl, 1).0[0];
    let g = arc.guide.as_ref().expect("attached to the arc");
    assert_eq!(g.anchor, GuideAnchor::TransformationPoint);
    // Four tangent quads run 0.16 % longer than the true half circle.
    assert!((g.path.length - 100.0 * std::f64::consts::PI).abs() < 1.0);
    let (mid, _) = arc.sample(5);
    assert!(near(mid.x, 200.0) && near(mid.y, 100.0), "{mid:?}");
    // The path turned from straight up to horizontal: a quarter turn.
    let quarter = std::f64::consts::FRAC_PI_2;
    assert!((mid.rotation().unwrap() - quarter).abs() < 1e-3, "{mid:?}");
    let (start, _) = arc.sample(0);
    let (end, _) = arc.sample(10);
    assert!(near(start.x, 100.0) && near(start.y, 200.0) && near(end.x, 300.0));
    assert_eq!(
        arc.approximations,
        ["orient to path (path direction added to the tweened rotation)"]
    );

    // A straight guide is the straight line, and its direction never turns.
    let line = &classic_tweens(tl, 3).0[0];
    assert!(line.guide.is_some());
    for frame in 0..=10 {
        let (d, _) = line.sample(frame);
        let x = 100.0 + 20.0 * frame as f64;
        assert!((d.x - x).abs() < 1e-9 && (d.y - 200.0).abs() < 1e-9);
        assert!(d.rotation().unwrap().abs() < 1e-12);
    }

    // Not on the guide: straight line, flagged.
    let far = &classic_tweens(tl, 5).0[0];
    assert!(far.guide.is_none());
    assert!((far.sample(5).0.y - 5.0).abs() < 1e-9);
    assert_eq!(
        far.approximations,
        ["motion guide (sampled as a straight line)"]
    );

    let reg = &classic_tweens(tl, 7).0[0];
    assert_eq!(
        reg.guide.as_ref().unwrap().anchor,
        GuideAnchor::Registration
    );
    let m = reg.sample(5).0.to_matrix(reg.pivot);
    assert!(near(m.tx, 200.0) && near(m.ty, 100.0), "{m:?}");
}
