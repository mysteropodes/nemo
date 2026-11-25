//! Fixtures shared by the integration tests.
#![allow(dead_code)]

use std::io::Write;

use nemo_xfl_import::*;

pub const SQUARE_WITH_HOLE: &str = r##"
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

pub fn doc_xml(elements: &str) -> String {
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

pub const HERO: &str = r##"<DOMSymbolItem name="Hero" symbolType="graphic"><timeline><DOMTimeline name="Hero"><layers>
  <DOMLayer name="Body"><frames><DOMFrame index="0" duration="4"><elements/></DOMFrame></frames></DOMLayer>
</layers></DOMTimeline></timeline></DOMSymbolItem>"##;

pub fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
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

pub fn sample() -> Vec<u8> {
    let doc = doc_xml(SQUARE_WITH_HOLE);
    zip_bytes(&[
        ("DOMDocument.xml", doc.as_bytes()),
        ("LIBRARY/Hero.xml", HERO.as_bytes()),
        ("bin/M 1 1.dat", &lossless_2x1()),
    ])
}

/// Signed area of the polygon through the segment end points.
pub fn area(c: &Contour) -> f64 {
    let mut pts = vec![c.start];
    pts.extend(c.segments.iter().map(|s| s.end()));
    pts.windows(2)
        .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
        .sum::<f64>()
        / 2.0
}

pub fn first_shape(doc: &XflDocument) -> &Shape {
    match &doc.timelines[0].layers[2].frames[0].elements[0] {
        Element::Shape(s) => s,
        other => panic!("expected a shape, got {other:?}"),
    }
}

pub fn shape_doc(shape: &str) -> XflDocument {
    read_fla(&zip_bytes(&[(
        "DOMDocument.xml",
        doc_xml(shape).as_bytes(),
    )]))
    .unwrap()
}

pub fn lossless_2x1() -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&[255, 255, 0, 0, 0, 0, 0, 0]).unwrap();
    let stream = z.finish().unwrap();
    let mut out = vec![0x03, 0x05, 8, 0, 2, 0, 1, 0];
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&[1, 1]);
    out.extend_from_slice(&(stream.len() as u16).to_le_bytes());
    out.extend_from_slice(&stream);
    out.extend_from_slice(&[0, 0]);
    out
}
