//! Oracle tests for the detached expression context views (D17/#2076).
//!
//! `fixtures/property-expression-views/oracle.txt` (`ORACLE`) was produced by
//! running `COMP_VIEW`, `SELF_VIEW`, `buildAliasComp`, `buildAliasLayer`,
//! `resolveHolderLayer`, `layerSnapshot`, `findLayerIndexByRef`,
//! `findLayerIndexByUid` and `_exprFps`, cut verbatim from `src/js/motion.js`
//! at 8679fb80, over eight fixed documents, every layer and element target
//! and two unresolved ones (generator kept outside the repo as `jsc_v1.js`,
//! sha256 f870801c8483ff76c418e5218428bd49a1289794e9530246086b1c63f283a753).
//!
//! Row: `v1 w,h,fps,frames|layers layerUid,strokeId property comp self alias`.
//! `layers` is `uid:name:parent;...` (`.` for none). Name and parent tokens:
//! `!` absent, `^` null, `~` empty string. Outputs: f64 bits in hex, `s<str>`,
//! `T`/`F`, `-` for null and `!` for undefined; `parent` is `name,index` or
//! `-`. Strings compare exactly and numbers on bits.

use crate::evaluation::property_expression_views::{
    bind_for_test, ExpressionContextView, ParentRef,
};
use crate::object_document::{
    ObjectDocument, ObjectLayer, Presence, ProjectMetadata, PropertyHolder, PropertyHolderTarget,
    OBJECT_DOCUMENT_FORMAT, OBJECT_DOCUMENT_FORMAT_VERSION,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/property-expression-views/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn n(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

fn s(v: Option<&str>) -> String {
    match v {
        None => "!".into(),
        Some("") => "s~".into(),
        Some(v) => format!("s{v}"),
    }
}

fn b(v: bool) -> &'static str {
    if v {
        "T"
    } else {
        "F"
    }
}

fn parent(p: Option<&ParentRef>) -> String {
    p.map_or("-".into(), |p| {
        format!("s{},{}", p.name(), n(p.index() as f64))
    })
}

pub(crate) fn target(layer: &str, stroke: Option<&str>) -> PropertyHolderTarget {
    PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: layer.into(),
        stroke_id: stroke.map_or(Presence::Missing, |s| Presence::Present(s.into())),
    }
}

pub(crate) fn holder(target: PropertyHolderTarget) -> PropertyHolder {
    PropertyHolder {
        target,
        controls: Presence::Missing,
        properties: vec![],
    }
}

fn token(t: &str) -> Presence<String> {
    match t {
        "!" => Presence::Missing,
        "~" => Presence::Present(String::new()),
        t => Presence::Present(t.into()),
    }
}

pub(crate) fn layer(
    uid: &str,
    name: Presence<String>,
    parent: Presence<Option<String>>,
) -> ObjectLayer {
    let mut layer = ObjectLayer::identified(uid.into());
    layer.name = name;
    layer.parent_layer_uid = parent;
    layer
}

pub(crate) fn document(
    total_frames: u32,
    layers: Vec<ObjectLayer>,
    holders: Vec<PropertyHolder>,
) -> ObjectDocument {
    ObjectDocument {
        format: OBJECT_DOCUMENT_FORMAT.into(),
        format_version: OBJECT_DOCUMENT_FORMAT_VERSION,
        total_frames,
        layers,
        objects: vec![],
        property_holders: Presence::Present(holders),
        layer_folders: Presence::Missing,
        composition_order: Presence::Missing,
        image_meshes: Presence::Missing,
        items: Presence::Missing,
        component_tables: Presence::Missing,
        motion_arcs: Presence::Missing,
        audio_tracks: Presence::Missing,
    }
}

/// One parsed row: the document, its metadata, the target and the property.
pub(crate) struct Case {
    pub(crate) document: ObjectDocument,
    pub(crate) metadata: ProjectMetadata,
    pub(crate) target: PropertyHolderTarget,
    pub(crate) property: String,
    pub(crate) expected: [String; 3],
}

pub(crate) fn parse(row: &str) -> Case {
    let f: Vec<&str> = row.split(' ').collect();
    assert_eq!((f.len(), f[0]), (7, "v1"), "row shape: {row}");
    let (meta, layers) = f[1].split_once('|').expect("doc token");
    let m: Vec<&str> = meta.split(',').collect();
    let metadata =
        ProjectMetadata::new(m[0].parse().unwrap(), m[1].parse().unwrap(), hex(m[2])).unwrap();
    let layers: Vec<ObjectLayer> = if layers == "." {
        vec![]
    } else {
        layers
            .split(';')
            .map(|l| {
                let p: Vec<&str> = l.split(':').collect();
                let parent = match p[2] {
                    "!" => Presence::Missing,
                    "^" => Presence::Present(None),
                    t => Presence::Present(token(t).value().cloned()),
                };
                layer(p[0], token(p[1]), parent)
            })
            .collect()
    };
    let (uid, stroke) = f[2].split_once(',').expect("target token");
    let stroke = (stroke != "!").then_some(stroke);
    let target = target(uid, stroke);
    let mut holders = vec![];
    if stroke.is_some() && layers.iter().any(|l| l.layer_uid() == uid) {
        holders.push(holder(target.clone()));
    }
    Case {
        document: document(m[3].parse().unwrap(), layers, holders),
        metadata,
        target,
        property: f[3].into(),
        expected: [f[4].into(), f[5].into(), f[6].into()],
    }
}

/// The three output groups, encoded the way the generator encodes Motion's.
pub(crate) fn encode(view: &ExpressionContextView) -> [String; 3] {
    let c = view.comp().expect("project metadata present");
    let comp = [
        n(c.width().into()),
        n(c.height().into()),
        n(c.fps()),
        n(c.frames().into()),
        n(c.layers() as f64),
        s(Some(c.name())),
        n(c.frame_duration()),
        n(c.duration()),
        n(c.num_layers() as f64),
        s(Some(c.name())),
    ]
    .join(",");
    let me = view.self_view();
    let self_group = format!(
        "{},{},{},{},{},{}",
        s(Some(view.property())),
        me.map_or("-".into(), |v| s(v.name())),
        n(me.map_or(-1.0, |v| v.index() as f64)),
        b(me.is_some_and(|v| v.is_shape())),
        b(me.is_some_and(|v| v.has_parent())),
        parent(me.and_then(|v| v.parent())),
    );
    let alias = me.map_or("-".into(), |v| {
        format!(
            "{},{},{},{}",
            s(v.name()),
            n(v.index() as f64),
            b(v.has_parent()),
            parent(v.parent())
        )
    });
    [comp, self_group, alias]
}

pub(crate) fn run(case: &Case) -> [String; 3] {
    let view = bind_for_test(
        &case.document,
        Some(&case.metadata),
        &case.target,
        &case.property,
    )
    .expect("root context binds");
    encode(&view)
}

#[test]
fn every_oracle_row_matches_motion() {
    let rows: Vec<&str> = ORACLE.lines().collect();
    assert_eq!(rows.len(), 120);
    for row in rows {
        let case = parse(row);
        assert_eq!(run(&case), case.expected, "row: {row}");
    }
}

#[test]
fn oracle_reaches_each_motion_distinction() {
    let has = |needle: &str| ORACLE.lines().any(|r| r.contains(needle));
    // The parent resolves by name: r's parent q is reported at Dup's FIRST index.
    assert!(has("r,! position") && has("sR,4000000000000000,F,T,sDup,0000000000000000"));
    // A stored but vanished parent: hasParent true, parent null.
    assert!(has("t:T:gone") && has("sT,4008000000000000,F,T,-"));
    // An empty parent uid is not a parent; an unnamed parent is null.
    assert!(has("s:S:~") && has("y:Y:x"));
    // frames 0 keeps 0 while duration divides by 1.
    assert!(has(",0|a:A:!"));
    // Unresolved holders, element targets and an absent name.
    assert!(has("missing,st_1") && has("x:!:^"));
    assert!(has("a,st_1 position") && has("sposition,sBall,0000000000000000,T,F,-"));
    // A comp with no layers.
    assert!(has("|. "));
}
