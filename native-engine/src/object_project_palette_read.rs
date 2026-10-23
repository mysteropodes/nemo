//! Immutable field-specific palette settings/summary/color pages on the existing snapshot.
//! Private reader and response wrapper; public route/capability admission is separate.
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest, ResponseEnvelope};
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Selector {
    Settings {},
    PalettePage {
        cursor: u64,
        limit: u64,
    },
    ColorPage {
        ordinal: u64,
        cursor: u64,
        limit: u64,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Query {
    command: String,
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    selector: Selector,
}
fn encoded(value: &Value) -> Result<usize, Error> {
    serde_json::to_vec(value)
        .map(|b| b.len())
        .map_err(|_| Error::Internal)
}
fn page_args(cursor: u64, limit: u64) -> bool {
    cursor <= 9_007_199_254_740_991 && (1..=16).contains(&limit)
}
impl ObjectSnapshot {
    pub(crate) fn query_palette_metadata_json(&self, bytes: &[u8]) -> Result<Value, Error> {
        self.palette_read_with_budget(bytes, 4096)
    }
    fn palette_read_with_budget(&self, bytes: &[u8], budget: usize) -> Result<Value, Error> {
        if bytes.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let shape: Value = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if !shape.get("selector").is_some_and(Value::is_object) {
            return Err(Error::InvalidRequest);
        }
        let query: Query = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
        if query.command != "object.project.palette.read" {
            return Err(Error::InvalidRequest);
        }
        if query.instance_id != self.instance_id() {
            return Err(Error::WrongInstance);
        }
        if query.document_id != self.document_id() {
            return Err(Error::WrongDocument);
        }
        if query.at_revision != self.content_revision()
            || query.document_snapshot_id != self.snapshot_id()
        {
            return Err(Error::StaleRevision);
        }
        if query.context_id != "scene-root" {
            return Err(Error::Unavailable);
        }
        match &query.selector {
            Selector::PalettePage { cursor, limit } if !page_args(*cursor, *limit) => {
                return Err(Error::InvalidRequest)
            }
            Selector::ColorPage {
                ordinal,
                cursor,
                limit,
            } if !(1..=9_007_199_254_740_991).contains(ordinal) || !page_args(*cursor, *limit) => {
                return Err(Error::InvalidRequest)
            }
            _ => {}
        }
        let metadata = self.project().ok_or(Error::Unavailable)?.project();
        let (stored, active) = metadata.palette_metadata();
        let palettes = stored.unwrap_or_default();
        let result=match query.selector {
            Selector::Settings{}=>{
                // Same upper-bound/missing selection fallback as the old reader;
                // negative/fractional supplied values stay visible and never persist defaults.
                let effective_count=stored.map_or(1,|p|p.len());
                let effective=active.filter(|v|*v<effective_count as f64).unwrap_or(0.);
                let mut v=json!({"palettesPresent":stored.is_some(),"paletteCount":palettes.len(),"effectiveActivePaletteIdx":effective});
                if let Some(index)=active{v["activePaletteIdx"]=json!(index);}v
            },
            Selector::PalettePage{cursor,limit}=>self.palette_page(
                json!({"kind":"palettePage","palettesPresent":stored.is_some(),"paletteCount":palettes.len(),"cursor":cursor,"items":[],"nextCursor":null}),
                palettes.len(),cursor,limit,budget,|i|{let (id,name,colors)=&palettes[i];json!({"ordinal":i+1,"id":id,"name":name,"colorCount":colors.len()})})?,
            Selector::ColorPage{ordinal,cursor,limit}=>{
                let i=ordinal.checked_sub(1).and_then(|i|usize::try_from(i).ok()).filter(|i|*i<palettes.len()).ok_or(Error::NotFound)?;
                let colors=&palettes[i].2;
                self.palette_page(json!({"kind":"paletteColorPage","paletteOrdinal":ordinal,"colorCount":colors.len(),"cursor":cursor,"items":[],"nextCursor":null}),
                    colors.len(),cursor,limit,budget,|i|json!({"index":i+1,"color":colors[i].as_str()}))?
            },
        };
        let response = self.palette_response(result);
        if encoded(&response)? > budget {
            return Err(Error::Unavailable);
        }
        Ok(response)
    }
    fn palette_page<F: FnMut(usize) -> Value>(
        &self,
        mut page: Value,
        count: usize,
        cursor: u64,
        limit: u64,
        budget: usize,
        mut item: F,
    ) -> Result<Value, Error> {
        let cursor = usize::try_from(cursor).map_err(|_| Error::InvalidRequest)?;
        if cursor > count {
            return Err(Error::InvalidRequest);
        }
        let mut items = Vec::new();
        for at in (cursor..count).take(limit as usize) {
            items.push(item(at));
            page["items"] = json!(items);
            page["nextCursor"] = if at + 1 < count {
                json!(at + 1)
            } else {
                Value::Null
            };
            if encoded(&self.palette_response(page.clone()))? > budget {
                items.pop();
                if items.is_empty() {
                    return Err(Error::Unavailable);
                }
                page["items"] = json!(items);
                page["nextCursor"] = json!(at);
                break;
            }
        }
        Ok(page)
    }
    fn palette_response(&self, result: Value) -> Value {
        json!({"instanceId":self.instance_id(),"documentId":self.document_id(),"atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),"contextId":"scene-root","result":result})
    }
    /// The common dispatcher must admit the request before selecting this wrapper.
    /// It is not currently registered as an available public operation.
    pub(crate) fn dispatch_palette_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, Error> {
        let fields = ["atRevision", "documentSnapshotId", "contextId", "selector"];
        if request
            .payload
            .as_object()
            .is_none_or(|v| v.len() != fields.len() || fields.iter().any(|k| !v.contains_key(*k)))
        {
            return Ok(self.read_failure(
                request,
                Error::InvalidRequest,
                "Invalid palette read payload.",
            ));
        }
        let query = json!({"command":"object.project.palette.read","instanceId":request.instance_id,"documentId":request.document_id,
            "atRevision":request.payload["atRevision"],"documentSnapshotId":request.payload["documentSnapshotId"],"contextId":request.payload["contextId"],"selector":request.payload["selector"]});
        let mut response = ResponseEnvelope {
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(Value::Null),
            error: None,
        };
        let overhead = serde_json::to_vec(&response)
            .map_err(|_| Error::Internal)?
            .len()
            - 4;
        let budget = 4096usize.checked_sub(overhead).ok_or(Error::Unavailable)?;
        match self.palette_read_with_budget(
            &serde_json::to_vec(&query).map_err(|_| Error::InvalidRequest)?,
            budget,
        ) {
            Ok(result) => {
                response.result = Some(result);
                Ok(response)
            }
            Err(code) => Ok(self.read_failure(
                request,
                code,
                "Palette read is invalid, unavailable, stale or not found.",
            )),
        }
    }
}
