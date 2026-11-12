//! Typed declarations shared by the closed object edit request and receipt checks.
use super::fill;

#[derive(Clone, Copy)]
pub(super) enum K {
    /// Authored stable identity: non-empty, at most 128 UTF-8 bytes, no control character.
    Uid,
    /// Opaque layer identity: any non-empty string.
    Opaque,
    /// One to 1024 distinct opaque layer identities.
    Opaques,
    Gap,
    Selection,
    Name,
    GroupName,
    Frame,
    /// Stroke identities: at least this many, at most 1024.
    Strokes(usize),
    Combine,
    Boolean,
    EditMode,
    Labels,
    Clip,
    Footprint,
    /// An Alt-drag duplicate's distance: a finite `[dx, dy]` pair, each component at most 1e6.
    Offset,
    /// A multi-layer duplicate's other layers: one to 32 `{layerUid, strokeIds}` with distinct layers and one or more distinct stroke identities each.
    Others,
    Fill(fill::Field),
}

/// Operation suffix, command and typed fields in wire order (the bool marks optional fields).
/// Every operation is a revisioned command; only a
/// duplicate names its minted identity; copy/cut carry a bounded clipboard; other receipts are compact.
pub(super) type Member = (&'static str, K, bool);
pub(super) static SPECS: [(&str, &str, &[Member]); 21] = [
    (
        "layer.delete",
        "object.layer.delete",
        &[
            ("selection", K::Selection, false),
            ("contextId", K::Uid, true),
        ],
    ),
    (
        "layer.duplicate",
        "object.layer.duplicate",
        &[
            ("layerUid", K::Opaque, false),
            ("contextId", K::Uid, true),
        ],
    ),
    (
        "layer.rename",
        "object.layer.rename",
        &[
            ("layerUid", K::Opaque, false),
            ("name", K::Name, false),
        ],
    ),
    (
        "layer.reorder",
        "object.layer.reorder",
        &[
            ("layerUids", K::Opaques, false),
            ("gap", K::Gap, false),
            ("selection", K::Selection, false),
            ("contextId", K::Uid, true),
        ],
    ),
    (
        "group.create",
        "object.group.create",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("name", K::GroupName, false),
            ("strokeIds", K::Strokes(1), false),
        ],
    ),
    (
        "group.ungroup",
        "object.group.ungroup",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("strokeIds", K::Strokes(1), false),
        ],
    ),
    (
        "group.combine",
        "object.group.combine",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("mode", K::Combine, false),
            ("strokeIds", K::Strokes(1), false),
        ],
    ),
    (
        "group.mode.set",
        "object.group.mode.set",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("mode", K::Combine, false),
        ],
    ),
    (
        "group.rename",
        "object.group.rename",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("groupId", K::Uid, false),
            ("name", K::GroupName, false),
        ],
    ),
    (
        "group.member.remove",
        "object.group.member.remove",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("strokeId", K::Uid, false),
        ],
    ),
    (
        "selection.delete",
        "object.selection.delete",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
        ],
    ),
    (
        "selection.duplicate",
        "object.selection.duplicate",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
            ("offset", K::Offset, true),
            ("others", K::Others, true),
        ],
    ),
    (
        "layer.split",
        "object.layer.split",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("labels", K::Labels, false),
        ],
    ),
    (
        "clipboard.paste",
        "object.clipboard.paste",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("clip", K::Clip, false),
        ],
    ),
    (
        "boolean.apply",
        "object.boolean.apply",
        &[
            ("mode", K::Boolean, false),
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("strokeIds", K::Strokes(2), false),
        ],
    ),
    (
        "boolean.erase",
        "object.boolean.erase",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("strokeId", K::Uid, false),
            ("footprint", K::Footprint, false),
            ("joinHistory", K::Fill(fill::Field::Flag), true),
        ],
    ),
    (
        "boolean.flatten",
        "object.boolean.flatten",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("groupId", K::Uid, false),
        ],
    ),
    (
        "fill.trace",
        "object.fill.trace",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("point", K::Fill(fill::Field::Point), false),
            ("color", K::Fill(fill::Field::Color), false),
            ("opacity", K::Fill(fill::Field::Opacity), false),
            ("remove", K::Fill(fill::Field::Flag), false),
        ],
    ),
    (
        "fill.brush",
        "object.fill.brush",
        &[
            ("layerUid", K::Uid, false),
            ("contextId", K::Uid, false),
            ("frame", K::Frame, false),
            ("outline", K::Fill(fill::Field::Outline), false),
            ("color", K::Fill(fill::Field::Color), false),
            ("opacity", K::Fill(fill::Field::Opacity), false),
            ("placement", K::Fill(fill::Field::Placement), false),
            ("mask", K::Fill(fill::Field::Mask), false),
            ("copies", K::Fill(fill::Field::Copies), false),
            ("stamps", K::Fill(fill::Field::Stamps), false),
        ],
    ),
    (
        "clipboard.copy",
        "object.clipboard.copy",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
        ],
    ),
    (
        "clipboard.cut",
        "object.clipboard.cut",
        &[
            ("contextId", K::Uid, false),
            ("layerUid", K::Uid, false),
            ("frame", K::Frame, false),
            ("mode", K::EditMode, false),
            ("strokeIds", K::Strokes(0), false),
        ],
    ),
];
