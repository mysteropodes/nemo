use std::io::Write;

use nemo_xfl_import::*;

const SQUARE_WITH_HOLE: &str = r##"
<DOMShape>
  <fills><FillStyle index="1"><SolidColor color="#FF0000" alpha="0.5"/></FillStyle></fills>
  <strokes><StrokeStyle index="1"><SolidStroke weight="4" caps="none" joints="miter"><fill><SolidColor color="#00ff00"/></fill></SolidStroke></StrokeStyle></strokes>
  <edges>
    <Edge fillStyle1="1" edges="!2000 0|2000 2000"/>
    <Edge fillStyle1="1" strokeStyle="1" edges="!0 0|2000 0"/>
    <Edge fillStyle1="1" edges="!#7D0 #7D0|0 2000!0 2000|0 0"/>
    <Edge fillStyle0="1" edges="!500 500|1500 500|1500 1500|500 1500|500 500"/>
  </edges>
</DOMShape>"##;

fn doc_xml(elements: &str) -> String {
    format!(
        r##"<DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="640" height="360" frameRate="12" backgroundColor="#336699">
  <symbols><Include href="Hero.xml"/><Include href="Missing.xml"/></symbols>
  <media>
    <DOMBitmapItem name="photo.png" href="photo.png" bitmapDataHRef="M 1 1.dat"/>
    <DOMSoundItem name="boom.wav"/>
  </media>
  <timelines><DOMTimeline name="Scene 1"><layers>
    <DOMLayer name="Folder" layerType="folder"/>
    <DOMLayer name="Mask" layerType="mask" parentLayerIndex="0"><frames><DOMFrame index="0" duration="10"><elements/></DOMFrame></frames></DOMLayer>
    <DOMLayer name="Art" parentLayerIndex="1" locked="true"><frames>
      <DOMFrame index="0" duration="2" name="start" tweenType="motion" acceleration="-40" motionTweenRotate="cw" motionTweenRotateTimes="2"><elements>{elements}</elements></DOMFrame>
      <DOMFrame index="2" duration="3" tweenType="shape" hasCustomEase="true"><Actionscript><script><![CDATA[stop();]]></script></Actionscript><elements/></DOMFrame>
      <DOMFrame index="5" keyMode="9728" soundName="boom.wav"><elements>
        <DOMSymbolInstance libraryItemName="Hero" symbolType="graphic" loop="play once" firstFrame="3" name="hero1">
          <matrix><Matrix a="2" d="2" tx="10" ty="20"/></matrix>
          <transformationPoint><Point x="5" y="6"/></transformationPoint>
          <color><Color alphaMultiplier="0.25" tintMultiplier="0.5" tintColor="#FF0000"/></color>
        </DOMSymbolInstance>
        <DOMBitmapInstance libraryItemName="photo.png"/>
        <DOMStaticText><textRuns><DOMTextRun><characters>Hi &amp; bye</characters></DOMTextRun></textRuns></DOMStaticText>
        <DOMOvalObject x="10" y="20" objectWidth="40" objectHeight="20" startAngle="90"><fill><SolidColor color="#0000FF"/></fill></DOMOvalObject>
        <DOMGroup><members>{elements}</members></DOMGroup>
      </elements></DOMFrame>
    </frames></DOMLayer>
    <DOMLayer name="Guide" layerType="guide" visible="false"><frames><DOMFrame index="0"><elements/></DOMFrame></frames></DOMLayer>
  </layers></DOMTimeline></timelines>
</DOMDocument>"##
    )
}

const HERO: &str = r##"<DOMSymbolItem name="Hero" symbolType="graphic"><timeline><DOMTimeline name="Hero"><layers>
  <DOMLayer name="Body"><frames><DOMFrame index="0" duration="4"><elements/></DOMFrame></frames></DOMLayer>
</layers></DOMTimeline></timeline></DOMSymbolItem>"##;

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    buf.into_inner()
}

fn sample() -> Vec<u8> {
    let doc = doc_xml(SQUARE_WITH_HOLE);
    zip_bytes(&[
        ("DOMDocument.xml", doc.as_bytes()),
        ("LIBRARY/Hero.xml", HERO.as_bytes()),
        ("bin/M 1 1.dat", &lossless_2x1()),
    ])
}

/// Signed area of the polygon through the segment end points.
fn area(c: &Contour) -> f64 {
    let mut pts = vec![c.start];
    pts.extend(c.segments.iter().map(|s| s.end()));
    pts.windows(2)
        .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
        .sum::<f64>()
        / 2.0
}

fn first_shape(doc: &XflDocument) -> &Shape {
    match &doc.timelines[0].layers[2].frames[0].elements[0] {
        Element::Shape(s) => s,
        other => panic!("expected a shape, got {other:?}"),
    }
}

#[test]
fn stage_and_structure() {
    let doc = read_fla(&sample()).unwrap();
    assert_eq!(
        (doc.width, doc.height, doc.frame_rate),
        (640.0, 360.0, 12.0)
    );
    assert_eq!(
        doc.background,
        Rgba {
            r: 0x33,
            g: 0x66,
            b: 0x99,
            a: 1.0
        }
    );
    let tl = &doc.timelines[0];
    assert_eq!(tl.name, "Scene 1");
    assert_eq!(tl.frame_count(), 10);
    let kinds: Vec<_> = tl.layers.iter().map(|l| l.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            LayerKind::Folder,
            LayerKind::Mask,
            LayerKind::Normal,
            LayerKind::Guide
        ]
    );
    assert_eq!(tl.layers[1].parent_layer_index, Some(0));
    assert_eq!(tl.layers[2].parent_layer_index, Some(1));
    assert!(tl.layers[2].locked && !tl.layers[3].visible);

    let art = &tl.layers[2];
    let spans: Vec<_> = art.frames.iter().map(|f| (f.index, f.duration)).collect();
    assert_eq!(spans, vec![(0, 2), (2, 3), (5, 1)]);
    assert_eq!(
        art.frames[0].label,
        Some(Label {
            name: "start".into(),
            kind: "name".into()
        })
    );
    let tw = art.frames[0].tween.as_ref().unwrap();
    assert_eq!(
        (
            tw.kind.clone(),
            tw.acceleration,
            tw.rotate.as_deref(),
            tw.rotate_times
        ),
        (TweenKind::Motion, -40.0, Some("cw"), 2)
    );
    assert_eq!(art.frames[1].tween.as_ref().unwrap().kind, TweenKind::Shape);

    let els = &art.frames[2].elements;
    let SymbolInstance {
        library_item,
        symbol_type,
        loop_mode,
        first_frame,
        matrix,
        transformation_point,
        alpha,
        instance_name,
        ..
    } = match &els[0] {
        Element::SymbolInstance(i) => i.clone(),
        e => panic!("{e:?}"),
    };
    assert_eq!(
        (
            library_item.as_str(),
            symbol_type,
            loop_mode.as_str(),
            first_frame
        ),
        ("Hero", SymbolType::Graphic, "play once", 3)
    );
    assert_eq!(
        matrix,
        Matrix {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 2.0,
            tx: 10.0,
            ty: 20.0
        }
    );
    assert_eq!(transformation_point, Point { x: 5.0, y: 6.0 });
    assert_eq!((alpha, instance_name.as_deref()), (0.25, Some("hero1")));
    assert!(
        matches!(&els[1], Element::BitmapInstance { library_item, .. } if library_item == "photo.png")
    );
    assert!(matches!(&els[2], Element::Text { text, .. } if text == "Hi & bye"));
    assert!(matches!(&els[3], Element::Shape(_)));
    assert!(
        matches!(&els[4], Element::Group { members } if matches!(members[0], Element::Shape(_)))
    );

    assert_eq!(doc.symbols.len(), 1);
    assert_eq!(doc.symbols[0].name, "Hero");
    assert_eq!(doc.symbols[0].symbol_type, SymbolType::Graphic);
    assert_eq!(doc.symbols[0].timeline.frame_count(), 4);
    assert_eq!(doc.bitmaps.len(), 1);
    assert!(doc.bitmaps[0].data_present);
}

#[test]
fn report_lists_what_was_left_out() {
    let doc = read_fla(&sample()).unwrap();
    let r = &doc.report;
    for feature in [
        "shape tween (keyframes kept, in-betweens not generated) — v2",
        "custom ease curve",
        "ActionScript",
        "sound on frame — v2",
        "colour transform other than alpha (tint, brightness, advanced)",
        "text (DOMStaticText) — v2",
        "library symbol file missing",
        "sound — v2",
    ] {
        assert!(
            r.skipped_feature(feature).is_some(),
            "missing report entry {feature:?}: {:#?}",
            r.skipped
        );
    }
    assert_eq!(
        r.skipped_feature("ActionScript").unwrap().first_location,
        "Scene 1 › Art › frame 3"
    );
    assert_eq!(r.converted["layers"], 5);
    assert_eq!(r.converted["classicTweens"], 1);
    assert_eq!(r.converted["shapes"], 3);
    assert!(r
        .skipped_feature("fill contour that does not close (kept open)")
        .is_none());
}

#[test]
fn fill_loops_close_with_holes_wound_oppositely() {
    let doc = read_fla(&sample()).unwrap();
    let shape = first_shape(&doc);
    let fill = shape.paths.iter().find(|p| p.fill.is_some()).unwrap();
    assert_eq!(
        fill.fill,
        Some(Paint::Solid {
            color: Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 0.5
            }
        })
    );
    assert_eq!(fill.contours.len(), 2);
    assert!(fill.contours.iter().all(|c| c.closed));
    let areas: Vec<f64> = fill.contours.iter().map(area).collect();
    // 2000 twips = 100 px; hole 1000 twips = 50 px; the hole winds the other way.
    assert!(areas.iter().any(|a| (a.abs() - 10_000.0).abs() < 1e-9));
    assert!(areas.iter().any(|a| (a.abs() - 2_500.0).abs() < 1e-9));
    assert!(areas[0].signum() != areas[1].signum());

    let stroke = shape.paths.iter().find(|p| p.stroke.is_some()).unwrap();
    let s = stroke.stroke.as_ref().unwrap();
    assert_eq!(
        (s.weight, s.caps.as_str(), s.joints.as_str()),
        (4.0, "none", "miter")
    );
    assert_eq!(
        s.paint,
        Paint::Solid {
            color: Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 1.0
            }
        }
    );
    assert_eq!(stroke.contours.len(), 1);
    assert!(!stroke.contours[0].closed);
    assert_eq!(stroke.contours[0].start, Point { x: 0.0, y: 0.0 });
    assert_eq!(
        stroke.contours[0].segments,
        vec![Segment::Line {
            to: Point { x: 100.0, y: 0.0 }
        }]
    );
}

#[test]
fn shared_edge_closes_both_fills_and_quads_elevate_exactly() {
    // Two fills share the vertical edge x=100; the right one has a curved side.
    let shape = r##"<DOMShape>
      <fills><FillStyle index="1"><SolidColor color="#000000"/></FillStyle>
             <FillStyle index="2"><LinearGradient spreadMethod="reflect"><matrix><Matrix a="0.1" tx="50"/></matrix>
               <GradientEntry color="#FFFFFF" ratio="0"/><GradientEntry color="#000000" alpha="0.5" ratio="1"/></LinearGradient></FillStyle></fills>
      <edges>
        <Edge fillStyle0="2" fillStyle1="1" edges="!2000 0|2000 2000"/>
        <Edge fillStyle1="1" edges="!0 0|2000 0!2000 2000|0 2000|0 0"/>
        <Edge fillStyle0="2" edges="!2000 2000|4000 2000[5000 1000 4000 0|2000 0"/>
      </edges></DOMShape>"##;
    let doc = read_fla(&zip_bytes(&[(
        "DOMDocument.xml",
        doc_xml(shape).as_bytes(),
    )]))
    .unwrap();
    let s = first_shape(&doc);
    assert_eq!(s.paths.len(), 2);
    for p in &s.paths {
        assert_eq!(p.contours.len(), 1, "{p:?}");
        assert!(p.contours[0].closed);
    }
    let Some(Paint::Linear {
        stops,
        spread,
        matrix,
    }) = &s.paths[1].fill
    else {
        panic!()
    };
    assert_eq!(
        (stops.len(), *spread, matrix.a, matrix.tx),
        (2, Spread::Reflect, 0.1, 50.0)
    );
    assert_eq!(stops[1].color.hex8(), "#00000080");
    let quad = s.paths[1].contours[0]
        .segments
        .iter()
        .zip(
            std::iter::once(s.paths[1].contours[0].start)
                .chain(s.paths[1].contours[0].segments.iter().map(|g| g.end())),
        )
        .find_map(|(seg, from)| matches!(seg, Segment::Quad { .. }).then_some((*seg, from)))
        .unwrap();
    let (c1, c2, to) = quad.0.to_cubic(quad.1);
    let Segment::Quad { ctrl, .. } = quad.0 else {
        unreachable!()
    };
    // The cubic's handles sit 2/3 of the way to the quadratic control point.
    assert!((c1.x - (quad.1.x + 2.0 / 3.0 * (ctrl.x - quad.1.x))).abs() < 1e-12);
    assert!((c2.y - (to.y + 2.0 / 3.0 * (ctrl.y - to.y))).abs() < 1e-12);
}

#[test]
fn folder_form_reads_like_the_zip() {
    let dir = std::env::temp_dir().join(format!("xfl-import-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("LIBRARY")).unwrap();
    std::fs::create_dir_all(dir.join("bin")).unwrap();
    std::fs::write(dir.join("DOMDocument.xml"), doc_xml(SQUARE_WITH_HOLE)).unwrap();
    std::fs::write(dir.join("LIBRARY/hero.XML"), HERO).unwrap(); // case differs on purpose
    std::fs::write(dir.join("bin/M 1 1.dat"), lossless_2x1()).unwrap();
    let from_dir = read_xfl_dir(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let from_zip = read_fla(&sample()).unwrap();
    assert_eq!(from_dir, from_zip);
}

#[test]
fn refuses_what_it_cannot_read() {
    let ole = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0];
    assert!(matches!(read_fla(&ole), Err(XflError::LegacyBinaryFla)));
    assert!(matches!(read_fla(b"hello"), Err(XflError::NotAnArchive(_))));
    let no_doc = zip_bytes(&[("other.xml", b"<a/>")]);
    assert!(matches!(read_fla(&no_doc), Err(XflError::MissingEntry(_))));
    let wrong_root = zip_bytes(&[("DOMDocument.xml", b"<Nope/>")]);
    assert!(matches!(read_fla(&wrong_root), Err(XflError::Xml { .. })));
    let broken = zip_bytes(&[("DOMDocument.xml", b"<DOMDocument><timelines>")]);
    assert!(matches!(read_fla(&broken), Err(XflError::Xml { .. })));
}

#[test]
fn deep_nesting_is_rejected_not_overflowed() {
    let mut s = String::from("<DOMDocument>");
    for _ in 0..10_000 {
        s.push_str("<a>");
    }
    let bytes = zip_bytes(&[("DOMDocument.xml", s.as_bytes())]);
    assert!(matches!(read_fla(&bytes), Err(XflError::Xml { .. })));
}

#[test]
fn model_serializes_to_json() {
    let doc = read_fla(&sample()).unwrap();
    let json = serde_json::to_value(&doc).unwrap();
    assert_eq!(
        json["timelines"][0]["layers"][2]["frames"][0]["elements"][0]["kind"],
        "shape"
    );
    assert_eq!(
        json["timelines"][0]["layers"][2]["frames"][0]["tween"]["kind"],
        "motion"
    );
}

fn shape_doc(shape: &str) -> XflDocument {
    read_fla(&zip_bytes(&[(
        "DOMDocument.xml",
        doc_xml(shape).as_bytes(),
    )]))
    .unwrap()
}

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

/// A 2×1 lossless ARGB entry: opaque blue, then fully transparent.
fn lossless_2x1() -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&[255, 0, 0, 255, 0, 0, 0, 0]).unwrap();
    let stream = z.finish().unwrap();
    let mut out = vec![0x03, 0x05, 8, 0, 2, 0, 1, 0];
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&[1, 1]);
    out.extend_from_slice(&(stream.len() as u16).to_le_bytes());
    out.extend_from_slice(&stream);
    out.extend_from_slice(&[0, 0]);
    out
}

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
    assert_eq!(pixels, &vec![0, 0, 255, 255, 0, 0, 0, 0]);
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
