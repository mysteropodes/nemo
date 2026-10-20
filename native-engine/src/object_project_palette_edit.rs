//! Field-specific palette intents on the existing sealed project settings owner.
//! Private preparation only; public contract/read/caller admission is separate.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, Presence, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag="kind",rename_all="camelCase",deny_unknown_fields)]
enum Action {
    Create {id:String,name:String,#[serde(default)] color:Presence<String>},
    Rename {ordinal:u64,name:String},
    Duplicate {ordinal:u64,id:String,name:String},
    Remove {ordinal:u64},
    Select {ordinal:u64},
    AppendColor {ordinal:u64,color:String},
    RemoveColor {ordinal:u64,index:u64},
    MoveColor {ordinal:u64,from:u64,to:u64},
}
struct Payload {command:String,action:Action}
object_deserialize!(Payload {command:String,action:Action});
fn text(s:&str)->bool {s.len()<=1024&&!s.contains('\0')}
fn ordinal(n:u64)->bool {(1..=9_007_199_254_740_991).contains(&n)}
fn valid(action:&Action)->bool {
    match action {
        Action::Create{id,name,color}=>text(id)&&text(name)&&color.value().is_none_or(|s|text(s)),
        Action::Rename{ordinal:n,name}=>ordinal(*n)&&text(name),
        Action::Duplicate{ordinal:n,id,name}=>ordinal(*n)&&text(id)&&text(name),
        Action::Remove{ordinal:n}|Action::Select{ordinal:n}=>ordinal(*n),
        Action::AppendColor{ordinal:n,color}=>ordinal(*n)&&text(color),
        Action::RemoveColor{ordinal:n,index}=>ordinal(*n)&&ordinal(*index),
        Action::MoveColor{ordinal:n,from,to}=>ordinal(*n)&&ordinal(*from)&&ordinal(*to),
    }
}
fn at(n:u64,count:usize)->Result<usize,Error> {
    n.checked_sub(1).and_then(|i|usize::try_from(i).ok()).filter(|i|*i<count).ok_or(Error::NotFound)
}
fn change(before:&ProjectMetadata,action:Action)->Result<ProjectMetadata,Error> {
    let (source,mut active)=before.palette_metadata();
    let mut palettes=source.unwrap_or_default().to_vec();
    match action {
        Action::Create{id,name,color}=> {
            // An explicit create may materialize the collection. Reads never do.
            palettes.push((id,name,color.value().cloned().into_iter().collect()));
            active=Some((palettes.len()-1)as f64);
        },
        Action::Rename{ordinal,name}=> {let i=at(ordinal,palettes.len())?;palettes[i].1=name;},
        Action::Duplicate{ordinal,id,name}=> {
            let i=at(ordinal,palettes.len())?;let colors=palettes[i].2.clone();
            palettes.insert(i+1,(id,name,colors));active=Some((i+1)as f64);
        },
        Action::Remove{ordinal}=> {
            let i=at(ordinal,palettes.len())?;
            if palettes.len()<=1{return Err(Error::InvalidRequest);}
            palettes.remove(i);
            // Match the original delete control; unrelated raw fractional/negative
            // selection values are not normalized during storage or other edits.
            if active.is_some_and(|n|n>=palettes.len()as f64){active=Some((palettes.len()-1)as f64);}
        },
        Action::Select{ordinal}=> {let i=at(ordinal,palettes.len())?;active=Some(i as f64);},
        Action::AppendColor{ordinal,color}=> {
            let i=at(ordinal,palettes.len())?;
            if palettes[i].2.contains(&color){return Ok(before.clone());}
            palettes[i].2.push(color);
        },
        Action::RemoveColor{ordinal,index}=> {
            let i=at(ordinal,palettes.len())?;let j=at(index,palettes[i].2.len())?;palettes[i].2.remove(j);
        },
        Action::MoveColor{ordinal,from,to}=> {
            let i=at(ordinal,palettes.len())?;let colors=&mut palettes[i].2;
            let source=at(from,colors.len())?;let target=at(to,colors.len())?;
            let color=colors.remove(source);colors.insert(target,color);
        },
    }
    before.clone().with_palette_metadata(Some(palettes),active).map_err(|_|Error::InvalidRequest)
}
impl ObjectFillOwner {
    pub(crate) fn prepare_palette_delta_json(&self,bytes:&[u8])->Result<PreparedProjectSettings,Error> {
        #[cfg(not(all(feature="history",feature="project_media_codec")))]
        {let _=bytes;return Err(Error::Unavailable);}
        #[cfg(all(feature="history",feature="project_media_codec"))]
        {
            if bytes.len()>4096{return Err(Error::InvalidRequest);}
            let shape:serde_json::Value=serde_json::from_slice(bytes).map_err(|_|Error::InvalidRequest)?;
            if !shape.get("action").is_some_and(serde_json::Value::is_object){return Err(Error::InvalidRequest);}
            let payload:Payload=serde_json::from_slice(bytes).map_err(|_|Error::InvalidRequest)?;
            if payload.command!="object.project.palette.edit"||!valid(&payload.action){return Err(Error::InvalidRequest);}
            let origin=self.current_snapshot();let project=origin.project().ok_or(Error::Unavailable)?;
            let before=project.project();let metadata=change(before,payload.action)?;let changed=&metadata!=before;
            if changed {ObjectSnapshot::check_candidate_saveable(&project.with_metadata(metadata.clone()))?;}
            Ok(PreparedProjectSettings{origin,metadata,changed})
        }
    }
}
