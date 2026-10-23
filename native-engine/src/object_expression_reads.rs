//! Actual immutable document providers for property expressions. No caller
//! supplies a sampler, control table or context, and no read can mutate state.
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::evaluation::property_expression_lookup::{
    self as lookup, ControlRecord, Fps, KeyRecord, KeyTrack, KeyValue, LayerRecord, LayerTable,
};
use crate::evaluation::property_expression_views::ExpressionContextView;
use crate::object_document::{
    ExpressionHolder, ObjectLayer, Presence, PropertyHolder, PropertyHolderTarget,
};

use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_property_holder_targets.rs"]
mod target_tests;

#[derive(Clone)]
pub(crate) struct SnapshotReads {
    snapshot: ObjectSnapshot,
    target: ExpressionHolder,
    property: String,
    frame: f64,
}

impl SnapshotReads {
    pub(crate) fn bind(
        snapshot: &ObjectSnapshot,
        target: &PropertyHolderTarget,
        property: &str,
        frame: f64,
    ) -> Self {
        Self::bind_holder(
            snapshot,
            &ExpressionHolder::Property(target.clone()),
            property,
            frame,
        )
    }
    pub(crate) fn bind_holder(
        snapshot: &ObjectSnapshot,
        target: &ExpressionHolder,
        property: &str,
        frame: f64,
    ) -> Self {
        Self {
            snapshot: snapshot.clone(),
            target: target.clone(),
            property: property.to_owned(),
            frame,
        }
    }
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
    pub(crate) fn target(&self, uid: &str) -> Result<PropertyHolderTarget, &'static str> {
        if uid.is_empty() {
            return self
                .target
                .property_target()
                .cloned()
                .ok_or("Inline animator is not a layer holder");
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
    pub(crate) fn source_target(&self, uid: &str) -> Result<ExpressionHolder, &'static str> {
        if uid.is_empty() {
            Ok(self.target.clone())
        } else {
            self.target(uid).map(ExpressionHolder::Property)
        }
    }
    pub(crate) fn source_has_expression(&self, target: &ExpressionHolder, property: &str) -> bool {
        crate::object_frame_packet::expression_sources::enabled(&self.snapshot, target, property)
            && crate::object_frame_packet::expression_sources::code(
                &self.snapshot,
                target,
                property,
            )
            .is_some_and(|code| !code.is_empty())
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
    pub(crate) fn layer(
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
    pub(crate) fn control(&self, uid: &str, name: &str) -> Result<String, &'static str> {
        // Original resolveHolderLayer does not impersonate an inline animator as a layer.
        if uid.is_empty() && self.target.property_target().is_none() {
            return Err("Inline animator has no layer control holder");
        }
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
    pub(crate) fn raw(
        &self,
        uid: &str,
        property: &str,
        frame: f64,
    ) -> Result<Vec<f64>, &'static str> {
        crate::object_frame_packet::expression_sources::raw(
            &self.snapshot,
            &self.source_target(uid)?,
            property,
            frame,
        )
        .map_err(|_| "Unavailable native property")
    }
    pub(crate) fn has_expression(&self, target: &PropertyHolderTarget, property: &str) -> bool {
        if property == "order" {
            return self
                .snapshot
                .document()
                .property_order(target)
                .and_then(|o| o.expression.value())
                .is_some_and(|e| e.enabled && !e.code.is_empty());
        }
        self.holder(target)
            .and_then(|h| h.properties().iter().find(|r| r.property() == property))
            .and_then(|r| r.expression.value())
            .is_some_and(|e| e.enabled && !e.code.is_empty())
    }
    fn view(&self, uid: &str) -> Result<ExpressionContextView, &'static str> {
        let target = if uid.is_empty() {
            self.target.context_target()
        } else {
            self.target(uid)?
        };
        ExpressionContextView::bind(&self.snapshot, &target, &self.property)
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
        let track = if let Some(target) = self
            .target
            .property_target()
            .filter(|_| self.property == "order")
        {
            self.snapshot
                .document()
                .property_order(target)
                .and_then(|o| o.track.value())
        } else {
            crate::object_frame_packet::expression_sources::record(
                &self.snapshot,
                &self.target,
                &self.property,
            )
            .and_then(|r| r.track.value())
        };
        track
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
    pub(crate) fn info(&self, uid: &str) -> Result<Value, &'static str> {
        if uid.is_empty() && self.target.property_target().is_none() {
            return Ok(
                json!({"name":null,"index":-1,"isShape":false,"hasParent":false,
                "property":self.property,"parent":null}),
            );
        }
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
    pub(crate) fn vertex_index(&self) -> Result<i64, &'static str> {
        match PropertyHolder::vertex_index(&self.property) {
            Some(index) => i64::try_from(index)
                .ok()
                .filter(|i| *i <= i64::from(u32::MAX))
                .ok_or("Unsupported vertex index"),
            None => Ok(-1),
        }
    }
    /// A compact rest-vertex view, not a mesh copy or evaluated pose. Native
    /// raw vtx tracks stay percent offsets added to the separate rest sculpt.
    pub(crate) fn mesh(&self) -> Result<Option<Value>, &'static str> {
        let Some(target) = self.target.property_target() else {
            return Ok(None);
        };
        let Some(mesh) = PropertyHolder::bound_mesh(self.snapshot.document(), target) else {
            return Ok(None);
        };
        let index = self.vertex_index()?;
        let vertex = usize::try_from(index).ok().and_then(|i| mesh.verts.get(i));
        let uv = vertex
            .map(|p| {
                [
                    p[0].as_f64().expect("admitted mesh x"),
                    p[1].as_f64().expect("admitted mesh y"),
                ]
            })
            .unwrap_or([0., 0.]);
        Ok(Some(
            json!({"meshId":target.stroke_id(),"vertexIndex":index,
            "vertexCount":mesh.verts.len(),"outlineVertexCount":mesh.outline.len(),
            "vertexUV":uv,"isOutlineVertex":index>=0 && (index as usize)<mesh.outline.len()}),
        ))
    }
    pub(crate) fn parent(&self, uid: &str) -> Result<Value, &'static str> {
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
    pub(crate) fn comp(&self) -> Result<Value, &'static str> {
        let view = self.view("")?;
        let c = view.comp().map_err(|_| "Unavailable project metadata")?;
        Ok(
            json!({"width":c.width(),"height":c.height(),"fps":c.fps(),"frames":c.frames(),
            "layers":c.layers(),"name":c.name(),"frameDuration":c.frame_duration(),
            "duration":c.duration(),"numLayers":c.num_layers()}),
        )
    }
    pub(crate) fn public_comp(&self) -> Result<Value, &'static str> {
        let comp = self.comp()?;
        if ["frameDuration", "duration"]
            .iter()
            .any(|p| comp[*p].as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("Unavailable project metadata");
        }
        Ok(comp)
    }
    pub(crate) fn key(&self, at: f64, nearest: bool) -> Result<Value, &'static str> {
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
    pub(crate) fn key_count(&self) -> Result<usize, &'static str> {
        Ok(lookup::num_keys(self.keys()?.as_ref()))
    }
    pub(crate) fn frame(&self) -> f64 {
        self.frame
    }
    pub(crate) fn property(&self) -> &str {
        &self.property
    }
    pub(crate) fn owner_uid(&self) -> &str {
        self.target.layer_uid()
    }
}
