//! A boolean edit replaces its operands by plain islands (`carryBooleanData` of `src/js/tools.js` never carried
//! `linkedFillId` / `isLinkedFillCompanion`), so the half of a pressure-stroke / backdrop pair it did not touch is left
//! claiming a partner that is gone. The baseline tolerated that dangling id; admission here does not (a linked fill is
//! exactly two records, one of them the companion), which refused every eraser bite on a brush stroke drawn with its
//! fill. The surviving half is released instead: it stays what it paints (a ribbon, or a plain fill) without the link.
use crate::object_document::object_group_kernels::object_boolean_ops::stack::Scope;
use crate::object_document::{ObjectDocument, ObjectRecord, Presence};

fn link_of(record: &ObjectRecord) -> Option<&String> {
    record.vector_ext.value()?.linked_fill_id.value()
}

fn companion(record: &ObjectRecord) -> bool {
    record
        .vector_ext
        .value()
        .and_then(|ext| ext.is_linked_fill_companion.value())
        == Some(&true)
}

/// Release every linked-fill claim of the edited cell that no longer names exactly one primary and one companion.
pub(super) fn release_broken_links(document: &mut ObjectDocument, scope: &Scope) {
    let broken: Vec<usize> = (0..document.objects.len())
        .filter(|&at| {
            let record = &document.objects[at];
            if !scope.owns(&record.target) {
                return false;
            }
            let Some(link) = link_of(record) else {
                return false;
            };
            let members: Vec<&ObjectRecord> = document
                .objects
                .iter()
                .filter(|other| scope.owns(&other.target) && link_of(other) == Some(link))
                .collect();
            members.len() != 2 || members.iter().filter(|other| companion(other)).count() != 1
        })
        .collect();
    for at in broken {
        let record = &mut document.objects[at];
        let Presence::Present(ext) = &mut record.vector_ext else {
            continue;
        };
        ext.linked_fill_id = Presence::Missing;
        ext.is_linked_fill_companion = Presence::Missing;
        if ext.is_empty() {
            record.vector_ext = Presence::Missing;
        }
    }
}
