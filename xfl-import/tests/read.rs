mod common;

use common::*;
use nemo_xfl_import::*;

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
        "custom ease without curve points (sampled with the simple ease)",
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
