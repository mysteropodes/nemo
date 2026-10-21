use super::*;
use crate::evaluation::object_scene_style_kernels::{
    object_scene_producer_brush::{BrushInk, InkSource, InkState, TrimSetup},
    object_scene_producer_brush_anchor::{AnchorRestore, Remembered},
    object_scene_producer_pin::ProducerEntry,
};
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame,ObjectFrameSelector};
use crate::revision::ObjectSnapshot;

fn anchor(closed:bool)->TrimAnchor {
    let points=if closed {vec![[0.,0.],[10.,0.],[10.,10.],[0.,10.]]} else {vec![[0.,0.],[100.,0.]]};
    TrimAnchor {segments:points.into_iter().map(segment).collect(),closed}
}
fn packet()->ObjectFramePacket {
    let doc=decode_storage_project(BitmapBrushFields::test_drawing().to_string().as_bytes()).unwrap();
    let pin=ObjectSnapshot::new("native-brush-baker",doc).unwrap();
    prepare_object_frame(&pin,&ObjectFrameSelector {instance_id:pin.instance_id(),document_id:pin.document_id(),
        content_revision:0,document_snapshot_id:pin.snapshot_id(),context_id:"scene-root",scope_kind:FrameScopeKind::Authored,frame:0}).unwrap()
}
fn source(fields:&BitmapBrushFields,win:TrimWindow)->BrushStrokeSource {
    let s=&fields.bitmap_brush_spec;
    BrushStrokeSource {layer_uid:"L".into(),stroke_id:"anchor".into(),group_id:fields.brush_group_id.clone(),
        restore:AnchorRestore {pre_stroke:Remembered::Set(None),pre_opacity:Remembered::Unset},
        ink:InkSource::Refused("trim must not look up untrimmed PNG"),ink_opacity:1.,
        trim:Some(TrimSetup {spec:StampSpec {size:s.size.as_f64().unwrap(),spacing:s.spacing.as_f64().unwrap(),scatter:s.scatter.as_f64().unwrap(),
            seed:s.seed.as_f64().unwrap(),opacity:s.opacity.as_f64().unwrap(),pressure:s.pressure},
            pressure_profile:fields.bitmap_pressure_profile.value().map(|p|p.iter().map(|a|[a[0].as_f64().unwrap(),a[1].as_f64().unwrap(),a[2].as_f64().unwrap()]).collect()),window:Some(win)})}
}
fn run(packet:&ObjectFramePacket,fields:&BitmapBrushFields,win:TrimWindow,store:&mut BrushBitmapStore,slots:&mut TrimSlots)->BrushProduction {
    produce_native(packet,&source(fields,win),fields,store,slots,None).unwrap()
}
fn ink(output:&BrushProduction)->&BrushInk {
    match &output.output.entries[0] {
        ProducerEntry::Item {payload,..}=>match &payload.ink {InkState::Ink(ink)=>ink,_=>panic!("must have real pixels")},
        other=>panic!("native bake refused: {other:?}"),
    }
}
#[test]
fn original_open_trim_clips_exact_endpoint_and_full_window_preserves_cubic_handles() {
    let a=anchor(false);let out=trim(&a,&TrimWindow {start:25.,end:100.,offset:0.});
    assert_eq!(out.segments.first().unwrap().point,[25.,0.]);assert_eq!(out.segments.last().unwrap().point,[100.,0.]);
    assert!(!out.closed);assert!(out.segments.iter().all(|s|s.handle_in==Some([0.;2])&&s.handle_out==Some([0.;2])));
    assert_eq!(trim(&a,&TrimWindow {start:0.,end:100.,offset:73.}),original(&a));
    assert!(trim(&a,&TrimWindow {start:100.,end:120.,offset:0.}).segments.is_empty());
    assert!(trim(&a,&TrimWindow {start:70.,end:20.,offset:0.}).segments.is_empty());
}
#[test]
fn original_closed_trim_wraps_across_seam_and_never_closes_the_revealed_polyline() {
    let a=anchor(true);let out=trim(&a,&TrimWindow {start:75.,end:125.,offset:0.});
    assert_eq!(out.segments.first().unwrap().point,[0.,10.]);assert_eq!(out.segments.last().unwrap().point,[10.,0.]);
    assert!(!out.closed);assert!(out.segments.iter().any(|s|s.point==[0.,0.]));
}
#[test]
fn concrete_tip_recipes_preserve_alpha_rules_and_custom_mask_is_unavailable() {
    assert_eq!(tip_shape("hard",1.).unwrap().alpha(32.,32.),1.);
    assert_eq!(tip_shape("hard",1.).unwrap().alpha(0.,0.),0.);
    assert_eq!(tip_shape("soft",1.).unwrap().alpha(32.,32.),1.);
    for (x,y) in [(0.,0.),(20.,32.),(32.,32.),(63.,63.)] {
        assert_eq!(tip_shape("grain",17.).unwrap().alpha(x,y),tip_shape("soft",17.).unwrap().alpha(x,y),
            "black source-atop grain cannot change destination-in alpha");
    }
    for tip in crate::object_document::bitmap_brush::PROCEDURAL_TIPS {
        let shape=tip_shape(tip,17.).unwrap();
        assert!((0..64).all(|i|(0..64).all(|j|(0.0..=1.0).contains(&shape.alpha(i as f64+0.5,j as f64+0.5)))));
    }
    assert!(tip_shape("abr:missing mask",17.).is_err());
    assert_ne!(tip_pixels(&tip_shape("stipple",17.).unwrap()),tip_pixels(&tip_shape("stipple",18.).unwrap()));
}
#[test]
fn real_producer_publishes_charged_pixels_and_rounded_window_hit_keeps_exact_version() {
    let p=packet();let f=BitmapBrushFields::test_fields();let budget=ImageResourceBudget::isolated(8<<20).unwrap();
    let mut store=BrushBitmapStore::with_budget(budget.clone());let mut slots=TrimSlots::new();
    let first=run(&p,&f,TrimWindow {start:0.,end:50.,offset:0.},&mut store,&mut slots);
    let bitmap=ink(&first).bitmap.clone();assert!(bitmap.rgba8().iter().any(|b|*b!=0));
    assert_eq!(budget.used().unwrap(),bitmap.cost(),"no tip/raw scratch charge escapes scoped publish");
    let again=run(&p,&f,TrimWindow {start:0.,end:50.01,offset:0.},&mut store,&mut slots);
    assert!(bitmap.same_allocation_as(&ink(&again).bitmap));
    assert_eq!(slots.len(),1);assert_eq!(store.stats().count,1);
    assert_eq!(first.output.resources,again.output.resources);
}
#[test]
fn changed_tip_or_tint_replaces_one_slot_and_retained_old_scene_keeps_original_pixels() {
    let p=packet();let mut f=BitmapBrushFields::test_fields();let budget=ImageResourceBudget::isolated(16<<20).unwrap();
    let mut store=BrushBitmapStore::with_budget(budget.clone());let mut slots=TrimSlots::new();let w=TrimWindow {start:0.,end:50.,offset:0.};
    let first=run(&p,&f,w,&mut store,&mut slots);let old=ink(&first).bitmap.clone();let bytes=old.rgba8().to_vec();
    f.bitmap_brush_spec.color=String::from("#ff0000ff");let red=run(&p,&f,w,&mut store,&mut slots);
    assert!(ink(&red).bitmap.version()>old.version());assert_ne!(ink(&red).bitmap.rgba8(),bytes);
    assert_eq!(old.rgba8(),bytes);assert_eq!(slots.len(),1);assert_eq!(store.stats().count,1);
    f.bitmap_brush_spec.tip=String::from("soft");let soft=run(&p,&f,w,&mut store,&mut slots);
    assert!(ink(&soft).bitmap.version()>ink(&red).bitmap.version());assert_eq!(old.rgba8(),bytes);
    assert_eq!(budget.used().unwrap(),old.cost()+ink(&red).bitmap.cost()+ink(&soft).bitmap.cost());
}
#[test]
fn collapse_and_refused_budget_or_custom_tip_never_silently_drop_or_replace_old_ink() {
    let p=packet();let f=BitmapBrushFields::test_fields();let budget=ImageResourceBudget::isolated(16<<20).unwrap();
    let mut store=BrushBitmapStore::with_budget(budget.clone());let mut slots=TrimSlots::new();let w=TrimWindow {start:0.,end:50.,offset:0.};
    let first=run(&p,&f,w,&mut store,&mut slots);let before=budget.used().unwrap();let old=ink(&first).bitmap.clone();
    let mut custom=f.clone();custom.bitmap_brush_spec.tip="abr:missing".into();
    let refused=run(&p,&custom,w,&mut store,&mut slots);
    assert!(matches!(&refused.output.entries[0],ProducerEntry::Unavailable {..}));
    assert_eq!(budget.used().unwrap(),before);let retry=run(&p,&f,w,&mut store,&mut slots);assert!(old.same_allocation_as(&ink(&retry).bitmap));
    let tiny=ImageResourceBudget::isolated(1).unwrap();let mut small=BrushBitmapStore::with_budget(tiny.clone());let mut empty=TrimSlots::new();
    let refused=run(&p,&f,w,&mut small,&mut empty);assert!(matches!(&refused.output.entries[0],ProducerEntry::Unavailable {..}));
    assert_eq!(tiny.used().unwrap(),0);assert_eq!(empty.len(),0);assert_eq!(small.stats().count,0);
    let collapsed=run(&p,&f,TrimWindow {start:25.,end:25.,offset:0.},&mut small,&mut empty);
    assert!(matches!(&collapsed.output.entries[0],ProducerEntry::Item {payload,..} if matches!(&payload.ink,InkState::Collapsed)));
    assert_eq!(tiny.used().unwrap(),0);assert!(collapsed.output.resources.is_empty());
}

#[test]
fn unaffordable_replacement_retains_old_slot_pixels_and_all_charges() {
    let p=packet();let f=BitmapBrushFields::test_fields();let budget=ImageResourceBudget::isolated(128<<10).unwrap();
    let mut store=BrushBitmapStore::with_budget(budget.clone());let mut slots=TrimSlots::new();
    let w=TrimWindow {start:0.,end:50.,offset:0.};let first=run(&p,&f,w,&mut store,&mut slots);
    let old=ink(&first).bitmap.clone();let old_bytes=old.rgba8().to_vec();let before=budget.used().unwrap();
    let mut large=f.clone();large.bitmap_brush_spec.size=serde_json::Number::from(256);
    let refused=run(&p,&large,w,&mut store,&mut slots);
    assert!(matches!(&refused.output.entries[0],ProducerEntry::Unavailable {..}));
    assert_eq!(budget.used().unwrap(),before);assert_eq!(slots.len(),1);assert_eq!(store.stats().count,1);
    let retry=run(&p,&f,w,&mut store,&mut slots);assert!(old.same_allocation_as(&ink(&retry).bitmap));
    assert_eq!(old.rgba8(),old_bytes);
}
