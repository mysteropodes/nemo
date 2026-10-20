//! Authored storage values, not derived UI defaults or a second state owner.
use super::*;
fn metadata() -> ProjectMetadata { ProjectMetadata::new(320,180,24.0).unwrap() }

#[test]
fn palette_and_shadow_metadata_preserve_presence_order_duplicates_and_spelling() {
    let before=metadata();assert_eq!(before.palette_metadata(),(None,None));
    let palettes=vec![("p".into(),"".into(),vec!["#AbC".into(),"".into()]),
        ("p".into(),"same id".into(),vec!["rgba(1,2,3,.5)".into()])];
    let value=before.clone().with_palette_metadata(Some(palettes.clone()),Some(-0.5)).unwrap()
        .with_shadow_palette_metadata(Some(vec![("s".into(),"#AA00cc".into())]),Some("missing-id".into())).unwrap();
    assert_eq!(value.palette_metadata(),(Some(palettes.as_slice()),Some(-0.5)));
    assert_eq!(value.shadow_palette_metadata().1,Some("missing-id"));
    assert_eq!(before.palette_metadata(),(None,None));
    let empty=before.with_palette_metadata(Some(vec![]),None).unwrap();
    assert_eq!(empty.palette_metadata(),(Some([].as_slice()),None));
}

#[test]
fn guides_and_perspective_preserve_missing_null_empty_fractional_and_locked_presence() {
    let before=metadata();assert_eq!(before.guide_metadata(),None);assert_eq!(before.perspective_metadata(),(None,None,None,None));
    let value=before.clone().with_guide_metadata(Some((None,Some(vec![-0.5,4.25,-0.5])))).unwrap()
        .with_perspective_metadata(Some(false),Some("fisheye".into()),Some(-3.5),Some(None)).unwrap();
    assert_eq!(value.guide_metadata(),Some((None,Some([-0.5,4.25,-0.5].as_slice()))));
    assert_eq!(value.perspective_metadata(),(Some(false),Some("fisheye"),Some(-3.5),Some(None)));
    let empty=value.clone().with_perspective_metadata(None,None,None,Some(Some(vec![]))).unwrap();
    assert_eq!(empty.perspective_metadata().3,Some(Some([].as_slice())));
    let points=vec![(-4.5,200.25,None),(10.0,-9.0,Some(false))];
    let supplied=value.with_perspective_metadata(None,None,None,Some(Some(points.clone()))).unwrap();
    assert_eq!(supplied.perspective_metadata().3,Some(Some(points.as_slice())));
    assert_eq!(before.guide_metadata(),None);
}

#[test]
fn unrelated_marker_allocation_survives_visual_edits_and_invalid_values_refuse() {
    let before=metadata().with_composition_metadata(Some(vec![(1.5,None,None)]),Some(132.0),None,None).unwrap();
    let ptr=before.composition_markers().unwrap().as_ptr();
    let edited=before.clone().with_guide_metadata(Some((Some(vec![0.0]),None))).unwrap();
    assert_eq!(edited.composition_markers().unwrap().as_ptr(),ptr);
    assert_eq!(edited.composition_bpm(),(Some(132.0),None,None));
    for value in [f64::INFINITY,f64::NEG_INFINITY,f64::NAN] {
        assert_eq!(before.clone().with_palette_metadata(None,Some(value)),Err(invalid()));
        assert_eq!(before.clone().with_guide_metadata(Some((Some(vec![value]),None))),Err(invalid()));
        let bad=before.clone().with_perspective_metadata(None,None,None,Some(Some(vec![(0.0,value,None)])));
        assert_eq!(bad,Err(invalid()));
    }
    assert_eq!(before.clone().with_shadow_palette_metadata(None,Some("bad\0id".into())),Err(invalid()));
    assert_eq!(before.with_palette_metadata(Some(vec![("p".into(),"é".repeat(513),vec![])]),None),Err(invalid()));
}

#[cfg(all(feature="history",feature="project_media_codec"))]
mod wire {
    use crate::history::NativeObjectHistory;
    use serde_json::{json,Value};
    fn input(version:u32,fields:Value) -> Vec<u8> {
        let mut project=json!({"canvasW":320,"canvasH":180,"fps":24});
        project.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone());
        let mut value=json!({"format":"nemo.native-project-document","formatVersion":version,"project":project,
            "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},
            "viewportReference":null,"assets":[]});
        if version==2 { value["assetInventory"]=json!({"assetIds":[],"placements":[]}); }
        serde_json::to_vec(&value).unwrap()
    }
    fn save(owner:&NativeObjectHistory) -> Vec<u8> {
        let pin=owner.acquire_snapshot(owner.content_revision()).unwrap();
        owner.serialize_project_file(pin.content_revision(),pin.snapshot_id()).unwrap()
    }
    fn project(owner:&NativeObjectHistory) -> Value { serde_json::from_slice::<Value>(&save(owner)).unwrap()["project"].clone() }
    const FIELDS:[&str;9]=["palettes","activePaletteIdx","shadowPalette","shadowActiveId","guides",
        "perspectiveEnabled","perspectiveMode","perspectiveDensity","perspectiveVPs"];

    #[test]
    fn visual_metadata_both_wire_versions_survive_history_and_save_fresh_reopen() {
        let fields=json!({"palettes":[{"id":"p","name":"","colors":["#AbC",""]},{"id":"p","name":"X","colors":[]}],
            "activePaletteIdx":-0.5,"shadowPalette":[{"id":"s","color":"#AA00cc"}],"shadowActiveId":"missing-id",
            "guides":{"v":[-0.5,4.25,-0.5]},"perspectiveEnabled":false,"perspectiveMode":"fisheye",
            "perspectiveDensity":-3.5,"perspectiveVPs":null});
        for version in [1,2] {
            let mut owner=NativeObjectHistory::from_project_file("visual-fields",&input(version,fields.clone())).unwrap();
            let before=save(&owner);let raw=project(&owner);
            for field in FIELDS { assert_eq!(raw.get(field),fields.get(field),"{version}:{field}"); }
            let candidate=owner.prepare_composition_delta_json(br#"{"command":"object.project.composition.edit","action":{"kind":"setBpm","bpm":132}}"#).unwrap();
            owner.commit_project_settings(candidate).unwrap();assert_eq!(owner.history_depths(),(1,0));
            let after=save(&owner);let changed=project(&owner);
            for field in FIELDS { assert_eq!(changed.get(field),fields.get(field),"tempo edit must retain {field}"); }
            let reopened=NativeObjectHistory::from_project_file("visual-fresh",&after).unwrap();assert_eq!(save(&reopened),after);
            owner.undo(1).unwrap();assert_eq!(save(&owner),before);
            owner.redo(2).unwrap();assert_eq!(save(&owner),after);
        }
    }

    #[test]
    fn optional_visual_missing_null_empty_axes_and_point_lock_presence_remain_distinct() {
        for fields in [json!({}),json!({"palettes":[],"shadowPalette":[],"guides":{},"perspectiveVPs":[]}),
            json!({"guides":{"h":[],"v":[]},"perspectiveVPs":null}),
            json!({"perspectiveVPs":[{"x":-0.25,"y":4.5},{"x":8.0,"y":-3.0,"locked":false}]})] {
            let owner=NativeObjectHistory::from_project_file("visual-presence",&input(1,fields.clone())).unwrap();
            let actual=project(&owner);for field in FIELDS { assert_eq!(actual.get(field),fields.get(field),"{field}"); }
            let reopened=NativeObjectHistory::from_project_file("visual-presence-fresh",&save(&owner)).unwrap();
            assert_eq!(save(&reopened),save(&owner));
        }
    }

    #[test]
    fn visual_nested_records_are_object_only_and_reject_null_unknown_and_duplicate_fields() {
        for fields in [json!({"palettes":null}),json!({"palettes":[["p","P",[]]]}),
            json!({"palettes":[{"id":"p","name":"P","colors":[],"other":0}]}),
            json!({"guides":[]}),json!({"guides":{"h":null}}),json!({"guides":{"h":[],"depth":1}}),
            json!({"perspectiveVPs":[{"x":null,"y":0}]}),json!({"perspectiveVPs":[{"x":0,"y":0,"locked":null}]}),
            json!({"perspectiveVPs":[[0,0,false]]}),json!({"perspectiveEnabled":null})] {
            assert!(NativeObjectHistory::from_project_file("visual-invalid",&input(1,fields)).is_err());
        }
        let raw=String::from_utf8(input(1,json!({"palettes":[{"id":"p","name":"P","colors":[]}]}))).unwrap();
        let duplicate=raw.replace("\"id\":\"p\"","\"id\":\"p\",\"id\":\"other\"");assert_ne!(duplicate,raw);
        assert!(NativeObjectHistory::from_project_file("visual-duplicate",duplicate.as_bytes()).is_err());
    }
}
