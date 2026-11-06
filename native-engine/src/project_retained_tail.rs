//! Cells past the project length that the Frames field keeps for the next grow. Baseline `setTotalFrames`
//! (src/js/timeline.js at 1065e9d8) never truncates `ld.frames`, so 120 -> 30 -> 120 restores frames 30-119.
//! The live document holds nothing past `totalFrames` (every reader stays inert there by construction); the cut
//! cells wait here, as frames relative to the end the timeline had when they were cut, and travel with the
//! project metadata so undo, redo and save/reopen carry them in the same revision as the length.
use crate::object_document::timing::TimingTail;
use crate::object_document::{
    object_deserialize, ObjectRecord, OrderScope, RasterEntry, TextEntry,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The longest timeline (the Frames field clamps to 999); a cell relative to the end can never need more.
pub(crate) const MAX_RELATIVE: u32 = 999;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TailLayer {
    pub(crate) layer_uid: String,
    pub(crate) timing: TimingTail,
}
object_deserialize!(TailLayer {
    layer_uid: String,
    timing: TimingTail
});

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetainedTail {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) layers: Vec<TailLayer>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) objects: Vec<ObjectRecord>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) raster: Vec<RasterEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) text: Vec<TextEntry>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) scopes: Vec<OrderScope>,
}
object_deserialize!(RetainedTail {
    #[serde(default)] layers: Vec<TailLayer>,
    #[serde(default)] objects: Vec<ObjectRecord>,
    #[serde(default)] raster: Vec<RasterEntry>,
    #[serde(default)] text: Vec<TextEntry>,
    #[serde(default)] scopes: Vec<OrderScope>
});

fn drop_after<T>(list: &mut Vec<T>, frame: impl Fn(&mut T) -> &mut u32, by: u32) {
    list.retain_mut(|entry| {
        let at = frame(entry);
        match at.checked_add(by) {
            Some(next) if next < MAX_RELATIVE => {
                *at = next;
                true
            }
            _ => false,
        }
    });
}

impl RetainedTail {
    pub(crate) fn is_empty(&self) -> bool {
        self.layers.is_empty()
            && self.objects.is_empty()
            && self.raster.is_empty()
            && self.text.is_empty()
            && self.scopes.is_empty()
    }
    /// Layers unique and non-empty, frames inside the longest timeline, timing entries ordered. A tail that fails
    /// this is refused when a project is read, so a restore only ever sees tidy input.
    pub(crate) fn well_formed(&self) -> bool {
        let mut seen = HashSet::new();
        self.layers.iter().all(|layer| {
            !layer.layer_uid.is_empty()
                && seen.insert(layer.layer_uid.as_str())
                && layer.timing.well_formed(MAX_RELATIVE)
        }) && self
            .objects
            .iter()
            .all(|r| r.target.frame_scope.frame < MAX_RELATIVE)
            && self
                .raster
                .iter()
                .all(|e| e.target.frame_scope.frame < MAX_RELATIVE)
            && self
                .text
                .iter()
                .all(|e| e.target.frame_scope.frame < MAX_RELATIVE)
            && self
                .scopes
                .iter()
                .all(|s| s.frame_scope.frame < MAX_RELATIVE)
    }
    /// Every cell moves `by` frames further from the end; what would pass the longest timeline is dropped.
    pub(crate) fn shift(&mut self, by: u32) {
        for layer in &mut self.layers {
            layer.timing.shift(by, MAX_RELATIVE);
        }
        self.layers.retain(|layer| !layer.timing.is_empty());
        drop_after(&mut self.objects, |r| &mut r.target.frame_scope.frame, by);
        drop_after(&mut self.raster, |e| &mut e.target.frame_scope.frame, by);
        drop_after(&mut self.text, |e| &mut e.target.frame_scope.frame, by);
        drop_after(&mut self.scopes, |s| &mut s.frame_scope.frame, by);
    }
    /// The cells nearer than `k` frames (to be laid back) and the rest, re-based so `k` is the new first frame.
    pub(crate) fn split_front(self, k: u32) -> (Self, Self) {
        let mut front = Self::default();
        let mut rest = Self::default();
        for layer in self.layers {
            let (a, b) = layer.timing.split_front(k);
            if !a.is_empty() {
                front.layers.push(TailLayer {
                    layer_uid: layer.layer_uid.clone(),
                    timing: a,
                });
            }
            if !b.is_empty() {
                rest.layers.push(TailLayer {
                    layer_uid: layer.layer_uid,
                    timing: b,
                });
            }
        }
        fn part<T>(all: Vec<T>, frame: impl Fn(&mut T) -> &mut u32, k: u32) -> (Vec<T>, Vec<T>) {
            let (mut front, mut rest) = (Vec::new(), Vec::new());
            for mut entry in all {
                if *frame(&mut entry) < k {
                    front.push(entry);
                } else {
                    *frame(&mut entry) -= k;
                    rest.push(entry);
                }
            }
            (front, rest)
        }
        (front.objects, rest.objects) = part(self.objects, |r| &mut r.target.frame_scope.frame, k);
        (front.raster, rest.raster) = part(self.raster, |e| &mut e.target.frame_scope.frame, k);
        (front.text, rest.text) = part(self.text, |e| &mut e.target.frame_scope.frame, k);
        (front.scopes, rest.scopes) = part(self.scopes, |s| &mut s.frame_scope.frame, k);
        (front, rest)
    }
    /// The cells of `front` precede this tail; this tail has already been shifted clear of them.
    pub(crate) fn prepend(&mut self, front: Self) {
        for mut layer in front.layers {
            match self
                .layers
                .iter_mut()
                .find(|own| own.layer_uid == layer.layer_uid)
            {
                Some(own) => own.timing.prepend(layer.timing),
                None => self.layers.insert(0, layer),
            }
        }
        let mut objects = front.objects;
        objects.append(&mut self.objects);
        self.objects = objects;
        let mut raster = front.raster;
        raster.append(&mut self.raster);
        self.raster = raster;
        let mut text = front.text;
        text.append(&mut self.text);
        self.text = text;
        let mut scopes = front.scopes;
        scopes.append(&mut self.scopes);
        self.scopes = scopes;
    }
}

/// Missing differs from an empty tail only on the wire; the metadata stores `None` for both.
pub(crate) fn canonical(tail: Option<RetainedTail>) -> Option<RetainedTail> {
    tail.filter(|tail| !tail.is_empty())
}
