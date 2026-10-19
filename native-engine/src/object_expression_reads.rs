//! Actual immutable document providers for property expressions. No caller
//! supplies a sampler, control table or context, and no read can mutate state.
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::evaluation::property_expression_lookup::{
    self as lookup, ControlRecord, Fps, KeyRecord, KeyTrack, KeyValue, LayerRecord, LayerTable,
};
use crate::evaluation::property_expression_views::ExpressionContextView;
use crate::object_document::{ObjectLayer, Presence, PropertyHolder, PropertyHolderTarget};
use crate::object_expression_pin::PropertyExpressionRequest;
use crate::object_frame_packet::sample_property_raw;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[derive(Clone)]
pub(super) struct SnapshotReads {
    snapshot: ObjectSnapshot,
    target: PropertyHolderTarget,
    property: String,
    frame: f64,
}

impl SnapshotReads {
    pub(super) fn new(request: &PropertyExpressionRequest) -> Self {
        Self {
            snapshot: request.snapshot().clone(),
            target: request.target().clone(),
            property: request.property().to_owned(),
            frame: request.frame(),
        }
    }
    pub(super) fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
    pub(super) fn target(&self, uid: &str) -> Result<PropertyHolderTarget, &'static str> {
        if uid.is_empty() {
            return Ok(self.target.clone());
        }
        self.layers()?
            .iter()
            .find(|l| l.layer_uid() == uid)
            .ok_or("Unknown layer")?;
        Ok(PropertyHolderTarget {
            context_id: self.target.context_id().to_owned(),
            layer_uid: uid.to_owned(),
            stroke_id: Presence::Missing,
        })
    }
    fn layers(&self) -> Result<&[ObjectLayer], &'static str> {
        ContextSource::of_document(self.snapshot.document())
            .context(self.target.context_id())
            .map(|c| c.layers())
            .map_err(|_| "Unavailable expression context")
    }
    fn holder(&self, target: &PropertyHolderTarget) -> Option<&PropertyHolder> {
        self.snapshot
            .document()
            .property_holders()
            .unwrap_or(&[])
            .iter()
            .find(|h| h.target() == target)
    }
    fn table(&self) -> Result<LayerTable, &'static str> {
        let layers = self
            .layers()?
            .iter()
            .map(|layer| {
                let target = PropertyHolderTarget {
                    context_id: self.target.context_id().to_owned(),
                    layer_uid: layer.layer_uid().to_owned(),
                    stroke_id: Presence::Missing,
                };
                let controls = self
                    .holder(&target)
                    .and_then(|h| h.controls.value())
                    .into_iter()
                    .flatten()
                    .map(|c| ControlRecord {
                        name: Some(c.name.clone()),
                    })
                    .collect();
                LayerRecord {
                    name: layer.name().map(str::to_owned),
                    layer_uid: Some(layer.layer_uid().to_owned()),
                    controls,
                }
            })
            .collect();
        LayerTable::new(layers).map_err(|_| "Expression lookup limit")
    }
    pub(super) fn layer(
        &self,
        reference: &str,
        uid_first: bool,
    ) -> Result<Option<String>, &'static str> {
        let table = self.table()?;
        let uid = if uid_first {
            table
                .find_by_uid(reference)
                .map_err(|_| "Invalid layer reference")?
        } else {
            None
        };
        let index = match uid {
            Some(i) => Some(i),
            None => table
                .find_by_ref(reference)
                .map_err(|_| "Invalid layer reference")?,
        };
        Ok(index.and_then(|i| table.layers()[i].layer_uid.clone()))
    }
    pub(super) fn control(&self, uid: &str, name: &str) -> Result<String, &'static str> {
        // Shape controls belong to its owning layer, not the shape holder.
        let uid = if uid.is_empty() {
            self.target.layer_uid()
        } else {
            uid
        };
        let table = self.table()?;
        let resolution = table
            .layer_control(uid, Some(name))
            .map_err(|_| "Unknown layer")?;
        let index = resolution.control.ok_or("Unknown expression control")?;
        let target = self.target(uid)?;
        self.holder(&target)
            .and_then(|h| h.controls.value())
            .and_then(|c| c.get(index))
            .map(|c| c.key.clone())
            .ok_or("Unknown expression control")
    }
    pub(super) fn raw(
        &self,
        uid: &str,
        property: &str,
        frame: f64,
    ) -> Result<Vec<f64>, &'static str> {
        sample_property_raw(&self.snapshot, &self.target(uid)?, property, frame)
            .map_err(|_| "Unavailable native property")
    }
    pub(super) fn has_expression(&self, target: &PropertyHolderTarget, property: &str) -> bool {
        self.holder(target)
            .and_then(|h| h.properties().iter().find(|r| r.property() == property))
            .and_then(|r| r.expression.value())
            .is_some_and(|e| e.enabled && !e.code.is_empty())
    }
    fn view(&self, uid: &str) -> Result<ExpressionContextView, &'static str> {
        ExpressionContextView::bind(&self.snapshot, &self.target(uid)?, &self.property)
            .map_err(|_| "Unavailable expression context")
    }
    fn fps(&self) -> Result<Fps, &'static str> {
        let view = self.view("")?;
        Fps::new(
            view.comp()
                .map_err(|_| "Unavailable project metadata")?
                .fps(),
        )
        .map_err(|_| "Invalid project frame rate")
    }
    fn keys(&self) -> Result<Option<KeyTrack>, &'static str> {
        self.holder(&self.target)
            .and_then(|h| {
                h.properties()
                    .iter()
                    .find(|r| r.property() == self.property)
            })
            .and_then(|r| r.track.value())
            .map(|track| {
                let keys = track
                    .keys
                    .iter()
                    .map(|k| KeyRecord {
                        frame: k.frame.as_f64().expect("admitted key frame"),
                        value: k
                            .v
                            .iter()
                            .map(|v| v.as_f64().expect("admitted key value"))
                            .collect(),
                    })
                    .collect();
                KeyTrack::new(keys).map_err(|_| "Expression key lookup limit")
            })
            .transpose()
    }
    pub(super) fn info(&self, uid: &str) -> Result<Value, &'static str> {
        let view = self.view(uid)?;
        let Some(me) = view.self_view() else {
            return Ok(Value::Null);
        };
        Ok(
            json!({"name":me.name(), "index":me.index(), "isShape":me.is_shape(),
            "hasParent":me.has_parent(), "property":view.property(),
            "parent":me.parent().map(|p|json!({"name":p.name(),"index":p.index()}))}),
        )
    }
    pub(super) fn parent(&self, uid: &str) -> Result<Value, &'static str> {
        let target = self.target(uid)?;
        let layers = self.layers()?;
        let layer = layers
            .iter()
            .find(|l| l.layer_uid() == target.layer_uid())
            .ok_or("Unknown layer")?;
        let parent = layer
            .parent_layer_uid
            .value()
            .and_then(Option::as_deref)
            .filter(|s| !s.is_empty())
            .and_then(|id| layers.iter().enumerate().find(|(_, l)| l.layer_uid() == id));
        Ok(parent
            .map(|(index, l)| json!({"name":l.name(),"index":index}))
            .unwrap_or(Value::Null))
    }
    pub(super) fn comp(&self) -> Result<Value, &'static str> {
        let view = self.view("")?;
        let c = view.comp().map_err(|_| "Unavailable project metadata")?;
        Ok(
            json!({"width":c.width(),"height":c.height(),"fps":c.fps(),"frames":c.frames(),
            "layers":c.layers(),"name":c.name(),"frameDuration":c.frame_duration(),
            "duration":c.duration(),"numLayers":c.num_layers()}),
        )
    }
    pub(super) fn key(&self, at: f64, nearest: bool) -> Result<Value, &'static str> {
        let keys = self.keys()?;
        let fps = self.fps()?;
        let entry = if nearest {
            lookup::nearest_key_at_frame(keys.as_ref(), at, fps).map_err(|_| "Invalid key frame")?
        } else {
            lookup::key_at_floored(keys.as_ref(), at, fps)
        };
        Ok(entry
            .map(|k| {
                let value = match k.value {
                    KeyValue::Scalar(v) => json!(v),
                    KeyValue::Vector(v) => json!(v),
                };
                json!({"frame":k.frame,"time":k.time,"value":value,"index":k.index})
            })
            .unwrap_or(Value::Null))
    }
    pub(super) fn key_count(&self) -> Result<usize, &'static str> {
        Ok(lookup::num_keys(self.keys()?.as_ref()))
    }
    pub(super) fn frame(&self) -> f64 {
        self.frame
    }
    pub(super) fn property(&self) -> &str {
        &self.property
    }
    pub(super) fn owner_uid(&self) -> &str {
        self.target.layer_uid()
    }
}
