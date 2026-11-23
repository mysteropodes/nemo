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
