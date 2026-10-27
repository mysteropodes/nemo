//! D16-K2 (#2072): the D16-K1 marker API over actual immutable native marker records.
//!
//! Mirrors where `src/js/motion.js` takes each list from: `thisComp.marker` and the
//! alias `comp.marker` read `state.markers`, which is the root composition's list
//! (a component keeps reading it), and a layer's `marker` reads that layer's stored
//! `ld.markers`, natively its `range.markers`. A missing list reads empty, as
//! `_markerApi(list || [])` does. `_exprFps` is `state.fps || 24` over the project fps.
//!
//! Nothing is copied into a second store: every list is rebuilt from the pinned
//! snapshot it is asked about. A bare drawing has no composition metadata, so its
//! composition list and fps are refused instead of inventing Motion's defaults.

use super::{Fps, LookupRefusal, MarkerList, MarkerRecord};
use crate::evaluation::object_component_kernels::object_context_resolver::ContextSource;
use crate::object_document::ObjectLayer;
use crate::revision::ObjectSnapshot;

/// Why a native marker list cannot be bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkerBindingError {
    Refused(LookupRefusal),
    /// The snapshot is a bare drawing: no composition markers or fps exist.
    NoProject,
    /// The context id names no context of the pinned document.
    UnknownContext,
    /// No layer of that context has this uid.
    UnknownLayer,
}

impl From<LookupRefusal> for MarkerBindingError {
    fn from(r: LookupRefusal) -> Self {
        Self::Refused(r)
    }
}

/// `_exprFps()`: `state.fps || 24`, so a zero or NaN project fps reads 24.
pub(crate) fn snapshot_fps(snapshot: &ObjectSnapshot) -> Result<Fps, MarkerBindingError> {
    let project = snapshot.project().ok_or(MarkerBindingError::NoProject)?;
    let fps = project.project().fps();
    let fps = if fps == 0.0 || fps.is_nan() {
        24.0
    } else {
        fps
    };
    Ok(Fps::new(fps)?)
}

/// `_markerApi(state.markers)`: the root composition's stored markers, in stored order.
pub(crate) fn composition_markers(
    snapshot: &ObjectSnapshot,
) -> Result<MarkerList, MarkerBindingError> {
    let project = snapshot.project().ok_or(MarkerBindingError::NoProject)?;
    let records = project
        .project()
        .composition_markers()
        .unwrap_or(&[])
        .iter()
        .map(|(frame, name, color)| MarkerRecord {
            frame: *frame,
            name: name.clone(),
            color: color.clone(),
        })
        .collect();
    Ok(MarkerList::new(records)?)
}

/// `_markerApi(ld.markers)` for one stored layer.
pub(crate) fn layer_markers(layer: &ObjectLayer) -> Result<MarkerList, LookupRefusal> {
    let records = layer
        .range
        .value()
        .and_then(|range| range.markers.value())
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|m| MarkerRecord {
            frame: m.frame as f64,
            name: m.name.value().cloned(),
            color: m.color.value().cloned(),
        })
        .collect();
    MarkerList::new(records)
}

/// [`layer_markers`] for the layer `layer_uid` of context `context_id` in the pin.
pub(crate) fn snapshot_layer_markers(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uid: &str,
) -> Result<MarkerList, MarkerBindingError> {
    let context = ContextSource::of_document(snapshot.document())
        .context(context_id)
        .map_err(|_| MarkerBindingError::UnknownContext)?;
    let layer = context
        .layers()
        .iter()
        .find(|l| l.layer_uid() == layer_uid)
        .ok_or(MarkerBindingError::UnknownLayer)?;
    Ok(layer_markers(layer)?)
}

/// D16-K2: the marker API over actual pinned native records, not supplied tables.
#[cfg(all(test, feature = "history"))]
mod tests {
    use super::super::{Fps, MarkerEntry, MarkerQuery as Q, MAX_MARKERS};
    use super::{
        composition_markers, snapshot_fps, snapshot_layer_markers, MarkerBindingError as E,
    };
    use crate::history::NativeObjectHistory;
    use crate::object_snapshot::ObjectSnapshot;
    use serde_json::{json, Value};

    fn owner(fps: f64, markers: Option<Value>, layers: Value) -> NativeObjectHistory {
        let mut metadata = json!({"canvasW":320,"canvasH":180,"fps":fps});
        if let Some(markers) = markers {
            metadata["markers"] = markers;
        }
        let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":metadata, "drawing":{"format":"nemo.native-object-document",
            "formatVersion":1,"totalFrames":40,"layers":layers,"objects":[]},
            "viewportReference":null,"assets":[]});
        NativeObjectHistory::from_project_file("marker-binding", wire.to_string().as_bytes())
            .unwrap()
    }
    fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
        o.acquire_snapshot(o.content_revision()).unwrap()
    }
    fn entry(frame: f64, fps: f64, name: &str, color: Option<&str>, index: usize) -> MarkerEntry {
        MarkerEntry {
            frame,
            time: frame / fps,
            name: name.into(),
            color: color.map(str::to_owned),
            index,
        }
    }

    #[test]
    fn composition_markers_keep_stored_order_ties_and_raw_names() {
        let o = owner(
            30.0,
            Some(json!([{"frame":30.5},{"frame":-2,"name":"","color":""},
                {"frame":30.5,"name":"X","color":"blue"},{"frame":0,"name":"X"}])),
            json!([{"layerUid":"L"}]),
        );
        let p = pin(&o);
        let fps = snapshot_fps(&p).unwrap();
        assert_eq!(fps, Fps::new(30.0).unwrap());
        let list = composition_markers(&p).unwrap();
        assert_eq!(list.count(), 4);
        // The first marker named exactly "X"; an empty color reads null.
        assert_eq!(
            list.at(Q::Name("X"), fps),
            Ok(Some(entry(30.5, 30.0, "X", Some("blue"), 3)))
        );
        assert_eq!(
            list.at(Q::Number(2.9), fps),
            Ok(Some(entry(-2.0, 30.0, "", None, 2)))
        );
        assert_eq!(
            list.at(Q::Name(""), fps),
            Ok(Some(entry(30.5, 30.0, "", None, 1)))
        );
        assert_eq!(list.at(Q::Number(5.0), fps), Ok(None));
        assert_eq!(list.at(Q::Number(f64::NAN), fps), Ok(None));
        // Equal distance: the first stored marker wins, sorted or not.
        assert_eq!(
            list.nearest(30.5, fps),
            Some(entry(30.5, 30.0, "", None, 1))
        );
        assert_eq!(
            list.nearest(-1.0, fps),
            Some(entry(-2.0, 30.0, "", None, 2))
        );
        assert_eq!(
            list.nearest_key_seconds(0.01, fps),
            Some(entry(0.0, 30.0, "X", None, 4))
        );
    }

    #[test]
    fn composition_binding_agrees_with_the_pinned_composition_read() {
        let o = owner(
            24.0,
            Some(
                json!([{"frame":12,"name":"in","color":"#5FA875"},{"frame":3},
                {"frame":12,"name":"in"}]),
            ),
            json!([{"layerUid":"L"}]),
        );
        let p = pin(&o);
        let (fps, list) = (snapshot_fps(&p).unwrap(), composition_markers(&p).unwrap());
        let read = |selector: Value| {
            let q = json!({"command":"object.project.composition.read",
                "instanceId":p.instance_id(),"documentId":p.document_id(),
                "atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
                "contextId":"scene-root","selector":selector});
            p.query_composition_metadata_json(q.to_string().as_bytes())
                .unwrap()["result"]
                .clone()
        };
        let as_json = |m: MarkerEntry| json!({"index":m.index,"frame":m.frame,"time":m.time,"name":m.name,"color":m.color});
        for (selector, got) in [
            (
                json!({"kind":"markerAt","index":2}),
                list.at(Q::Number(2.0), fps),
            ),
            (
                json!({"kind":"markerNamed","name":"in"}),
                list.at(Q::Name("in"), fps),
            ),
            (
                json!({"kind":"markerNearest","frame":7.5}),
                Ok(list.nearest(7.5, fps)),
            ),
            (
                json!({"kind":"markerNearest","frame":11.0}),
                Ok(list.nearest(11.0, fps)),
            ),
        ] {
            assert_eq!(read(selector), as_json(got.unwrap().unwrap()));
        }
    }

    #[test]
    fn layer_markers_come_from_each_layers_stored_range() {
        let o = owner(
            24.0,
            Some(json!([{"frame":1,"name":"comp"}])),
            json!([
                {"layerUid":"a","name":"Ink","range":{"inPoint":2,"outPoint":20,
                    "markers":[{"frame":12,"name":"α"},{"frame":3,"color":"#e8b64c"},
                    {"frame":12,"name":"β","color":""}]}},
                {"layerUid":"b","name":"Bare"},
                {"layerUid":"c","range":{"inPoint":1}}
            ]),
        );
        let p = pin(&o);
        let fps = snapshot_fps(&p).unwrap();
        let a = snapshot_layer_markers(&p, "scene-root", "a").unwrap();
        assert_eq!(a.count(), 3);
        assert_eq!(
            a.at(Q::Name("α"), fps),
            Ok(Some(entry(12.0, 24.0, "α", None, 1)))
        );
        assert_eq!(
            a.at(Q::Number(2.0), fps),
            Ok(Some(entry(3.0, 24.0, "", Some("#e8b64c"), 2)))
        );
        assert_eq!(a.nearest(12.0, fps), Some(entry(12.0, 24.0, "α", None, 1)));
        assert_eq!(a.at(Q::Name("comp"), fps), Ok(None));
        // No range, or a range without markers, reads an empty list.
        for uid in ["b", "c"] {
            let list = snapshot_layer_markers(&p, "scene-root", uid).unwrap();
            assert_eq!(list.count(), 0);
            assert_eq!(list.nearest(0.0, fps), None);
        }
        assert_eq!(
            snapshot_layer_markers(&p, "scene-root", "z").unwrap_err(),
            E::UnknownLayer
        );
        assert_eq!(
            snapshot_layer_markers(&p, "no-such-context", "a").unwrap_err(),
            E::UnknownContext
        );
    }

    #[test]
    fn missing_composition_list_reads_empty_and_bounds_refuse() {
        let o = owner(12.0, None, json!([{"layerUid":"L"}]));
        let p = pin(&o);
        let list = composition_markers(&p).unwrap();
        assert_eq!(list.count(), 0);
        assert_eq!(list.at(Q::Number(1.0), snapshot_fps(&p).unwrap()), Ok(None));
        // The project codec admits exactly the lookup bound and refuses one more, so
        // the binding's own TooManyMarkers refusal is unreachable from a real pin.
        let list = |n: usize| -> Value { (0..n).map(|f| json!({"frame":f})).collect() };
        let o = owner(12.0, Some(list(MAX_MARKERS)), json!([{"layerUid":"L"}]));
        assert_eq!(composition_markers(&pin(&o)).unwrap().count(), MAX_MARKERS);
        let mut metadata = json!({"canvasW":320,"canvasH":180,"fps":12});
        metadata["markers"] = list(MAX_MARKERS + 1);
        let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":metadata, "drawing":{"format":"nemo.native-object-document",
            "formatVersion":1,"totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},
            "viewportReference":null,"assets":[]});
        assert!(
            NativeObjectHistory::from_project_file("over", wire.to_string().as_bytes()).is_err()
        );
    }
}
