//! Original export lifecycle with actual filesystem staging and real Metal PNGs.
use super::*;
use crate::export_job::{JobStatus,StagedArtifactPort};
use crate::object_document::BitmapBrushFields;
use crate::object_frame_packet::{prepare_object_exposure_frame,ObjectFrameSelector};
use crate::application::ObjectBrushRuntime;
use crate::object_render_scene::{ObjectBrushAdmission,prepare_object_composition};
use crate::png_output::ExportArtifact;
use crate::scheduler::{ObjectScheduleScope,FrameScheduler};
use std::{fs,path::PathBuf};

struct Files {root:PathBuf}
impl Files {
    fn new()->Self {
        let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root=std::env::temp_dir().join(format!("native-brush-export-{}-{nonce}",std::process::id()));
        fs::create_dir(&root).unwrap();Self {root}
    }
}
impl Drop for Files {fn drop(&mut self) {let _=fs::remove_dir_all(&self.root);}}
impl StagedArtifactPort for Files {
    fn begin_staging(&mut self,_:&str,target:&str)->Result<(),String> {
        if target!="brush-output" {return Err("unknown output".into());}
        fs::create_dir(self.root.join("stage")).map_err(|e|e.to_string())
    }
    fn write_frame(&mut self,_:&str,name:&str,bytes:&[u8])->Result<(),String> {
        if name.contains('/')||name.contains('\\') {return Err("invalid output name".into());}
        fs::write(self.root.join("stage").join(name),bytes).map_err(|e|e.to_string())
    }
    fn publish(&mut self,_:&str,target:&str,files:&[String])->Result<ExportArtifact,String> {
        fs::rename(self.root.join("stage"),self.root.join("published")).map_err(|e|e.to_string())?;
        Ok(ExportArtifact {target:target.into(),files:files.to_vec()})
    }
    fn cleanup(&mut self,_:&str)->Result<(),String> {
        let path=self.root.join("stage");if path.exists() {fs::remove_dir_all(path).map_err(|e|e.to_string())?;}
        Ok(())
    }
}
fn request(snapshot:&crate::revision::ObjectSnapshot)->ObjectHostExportRequest {
    ObjectHostExportRequest {request_id:"normal-brush-export".into(),target:"brush-output".into(),
        pins:ObjectSchedulePins {instance_id:snapshot.instance_id().into(),document_id:snapshot.document_id().into(),
            content_revision:snapshot.content_revision(),document_snapshot_id:snapshot.snapshot_id().into(),
            context_id:"scene-root".into(),frame:0,scope:ObjectScheduleScope::Authored},
        output:OutputSpec::new("frame","rgba8",64,64,"srgb","straight").unwrap(),maximum_encoded_bytes:65536}
}
fn selector(snapshot:&crate::revision::ObjectSnapshot,frame:u32)->ObjectFrameSelector<'_> {
    ObjectFrameSelector {instance_id:snapshot.instance_id(),document_id:snapshot.document_id(),content_revision:snapshot.content_revision(),
        document_snapshot_id:snapshot.snapshot_id(),context_id:"scene-root",scope_kind:crate::object_document::FrameScopeKind::Authored,frame}
}
fn decode(bytes:&[u8])->Vec<u8> {
    let mut reader=png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
    assert_eq!((reader.info().width,reader.info().height),(64,64));
    assert_eq!(reader.info().color_type,png::ColorType::Rgba);
    let mut output=vec![0;reader.output_buffer_size().unwrap()];let info=reader.next_frame(&mut output).unwrap();
    output.truncate(info.buffer_size());output
}
#[test]
fn gpu_real_brush_materializer_exports_held_source_frames_with_exact_png_and_balanced_leases() {
    let owner=BitmapBrushFields::runtime_test_owner(false);let snapshot=owner.acquire_snapshot(0).unwrap();
    let before=owner.serialize_project_file(0,snapshot.snapshot_id()).unwrap();
    let mut runtime=ObjectBrushRuntime::default();let mut gpu=Compositor::new().expect("real Metal required");
    assert_eq!(gpu.adapter().get_info().backend,wgpu::Backend::Metal);
    let device=gpu.device().clone();let mut files=Files::new();let mut scheduler=FrameScheduler::new();let mut observed=Vec::new();
    let outcome=export_object_frames_with_materializer(&owner,&mut scheduler,&mut files,&mut gpu,request(&snapshot),&[0,1],|frame,gpu| {
        observed.push(frame);let selected=selector(&snapshot,frame);
        let packet=prepare_object_exposure_frame(&snapshot,&selected).map_err(|e|format!("{e:?}"))?;
        let admission=ObjectBrushAdmission::prepare(&packet,&selected).map_err(|e|format!("{e:?}"))?;
        let plan=runtime.materialize(&admission,&mut |ids| gpu.retire_object_brush_resources(ids,(64,64)).is_ok()).map_err(|e|format!("{e:?}"))?;
        assert_eq!(plan.scene_resource_versions().unwrap().len(),1,"brush version exists before scheduler starts");
        prepare_object_composition(&plan,&selected).map_err(|e|format!("{e:?}"))
    });
    let receipt=outcome.receipt.unwrap();assert_eq!(receipt.status,JobStatus::Succeeded);
    assert_eq!(receipt.artifact.unwrap().files,["frame_0001.png","frame_0002.png"]);
    assert_eq!(observed,[0,1]);assert!(outcome.release.cleanup_complete);assert!(!files.root.join("stage").exists());
    let a=decode(&fs::read(files.root.join("published/frame_0001.png")).unwrap());
    let b=decode(&fs::read(files.root.join("published/frame_0002.png")).unwrap());assert_eq!(a,b,"actual held source pixels");
    let pixel=|x:usize,y:usize| -> [u8;4] {a[(y*64+x)*4..(y*64+x+1)*4].try_into().unwrap()};
    assert_eq!(pixel(8,8),[0,255,0,255]);assert_eq!(pixel(13,15),[0,0,255,255]);assert_eq!(pixel(40,40),[255;4]);
    assert_eq!(gpu.device(),&device);assert_eq!(scheduler.lease_counters().live(),0);
    assert_eq!(scheduler.lease_counters().acquired(),scheduler.lease_counters().released());
    assert_eq!(owner.serialize_project_file(0,snapshot.snapshot_id()).unwrap(),before);
}
#[test]
fn gpu_later_materializer_refusal_cleans_real_staging_and_publishes_no_partial_sequence() {
    let owner=BitmapBrushFields::runtime_test_owner(false);let snapshot=owner.acquire_snapshot(0).unwrap();
    let before=owner.serialize_project_file(0,snapshot.snapshot_id()).unwrap();
    let mut runtime=ObjectBrushRuntime::default();let mut gpu=Compositor::new().expect("real Metal required");
    let mut files=Files::new();let mut scheduler=FrameScheduler::new();let mut observed=Vec::new();
    let outcome=export_object_frames_with_materializer(&owner,&mut scheduler,&mut files,&mut gpu,request(&snapshot),&[0,1],|frame,gpu| {
        observed.push(frame);if frame==1 {return Err("injected later resource refusal".into());}
        let selected=selector(&snapshot,frame);let packet=prepare_object_exposure_frame(&snapshot,&selected).map_err(|e|format!("{e:?}"))?;
        let admission=ObjectBrushAdmission::prepare(&packet,&selected).map_err(|e|format!("{e:?}"))?;
        let plan=runtime.materialize(&admission,&mut |ids| gpu.retire_object_brush_resources(ids,(64,64)).is_ok()).map_err(|e|format!("{e:?}"))?;
        prepare_object_composition(&plan,&selected).map_err(|e|format!("{e:?}"))
    });
    assert_eq!(outcome.receipt.unwrap().status,JobStatus::Failed);assert_eq!(observed,[0,1]);
    assert!(outcome.release.cleanup_complete);assert!(!files.root.join("stage").exists());assert!(!files.root.join("published").exists());
    assert_eq!(scheduler.lease_counters().live(),0);assert_eq!(owner.content_revision(),0);
    assert_eq!(owner.serialize_project_file(0,snapshot.snapshot_id()).unwrap(),before);
}
#[test]
fn gpu_invalid_count_range_and_owner_refuse_before_materializer_or_filesystem_staging() {
    let owner=BitmapBrushFields::runtime_test_owner(false);let snapshot=owner.acquire_snapshot(0).unwrap();
    let before=owner.serialize_project_file(0,snapshot.snapshot_id()).unwrap();
    let mut gpu=Compositor::new().expect("real Metal required");
    for (label,frames,foreign) in [("empty",vec![],false),("unsorted",vec![1,0],false),
        ("duplicate",vec![0,0],false),("outside",vec![12],false),("foreign",vec![0],true),("output",vec![0],false)] {
        let mut scheduler=FrameScheduler::new();let mut files=Files::new();let mut input=request(&snapshot);
        input.pins.frame=frames.first().copied().unwrap_or(0);
        if foreign {input.pins.document_id="foreign".into();}
        if label=="output" {input.output=OutputSpec::new("frame","rgba16",64,64,"srgb","straight").unwrap();}
        let mut calls=0;
        let outcome=export_object_frames_with_materializer(&owner,&mut scheduler,&mut files,&mut gpu,input,&frames,|_,_| {
            calls+=1;Err("must not materialize invalid source".into())
        });
        assert!(outcome.receipt.is_err(),"{label}");assert_eq!(calls,0,"{label}");
        assert!(!files.root.join("stage").exists(),"{label}");assert!(!files.root.join("published").exists(),"{label}");
        assert_eq!(scheduler.lease_counters().live(),0);
    }
    assert_eq!(owner.serialize_project_file(0,snapshot.snapshot_id()).unwrap(),before);
}
