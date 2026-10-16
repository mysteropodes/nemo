//! strict prepared native-project input; no owner or file I/O.
use crate::object_codec::project::{
    asset_entry, object, text, Input, Metadata, ProjectWireError, ProjectWireLimits, Reference,
    FORMAT,
};
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, ProjectEnvelope,
    ProjectReferenceState,
};
use crate::reference_image_asset::ImageAssetLimits;
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::Arc,
};

const MAX_ASSETS: usize = 1024;
const MAX_LOCATOR: usize = 4096;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Version {
    format_version: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CatalogInput<'a> {
    format: String,
    format_version: u32,
    project: Metadata,
    #[serde(borrow)]
    drawing: &'a RawValue,
    #[serde(borrow)]
    viewport_reference: &'a RawValue,
    #[serde(borrow)]
    assets: &'a RawValue,
    #[serde(borrow)]
    asset_inventory: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Catalog<'a> {
    #[serde(borrow)]
    asset_ids: &'a RawValue,
    #[serde(borrow)]
    placements: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry<'a> {
    #[serde(borrow)]
    id: &'a RawValue,
    #[serde(borrow)]
    name: &'a RawValue,
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    source: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Embedded<'a> {
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    media_type: &'a RawValue,
    #[serde(borrow)]
    encoding: &'a RawValue,
    #[serde(borrow)]
    data: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Linked<'a> {
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    locator: &'a RawValue,
}

fn array<'a, T: Deserialize<'a>>(
    raw: &'a RawValue,
    limit: usize,
) -> Result<Vec<T>, ProjectWireError> {
    struct Bounded<T>(usize, PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded array")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = seq.next_element()? {
                if items.len() == self.0 {
                    return Err(serde::de::Error::custom("array bound"));
                }
                items
                    .try_reserve(1)
                    .map_err(|_| serde::de::Error::custom("array allocation"))?;
                items.push(item);
            }
            Ok(items)
        }
    }
    serde::Deserializer::deserialize_seq(
        &mut serde_json::Deserializer::from_str(raw.get()),
        Bounded(limit, PhantomData),
    )
    .map_err(|_| ProjectWireError::Invalid)
}

enum Source {
    Png { data: String, length: usize },
    Linked(String),
}
struct Record {
    id: String,
    name: String,
    kind: AssetKind,
    source: Source,
}
struct Viewport {
    index: usize,
    name: String,
    visible: bool,
    opacity: f64,
    offset: i64,
}
pub(crate) struct PreparedProjectWire {
    base: ProjectEnvelope,
    records: Vec<Record>,
    lengths: Vec<usize>,
    catalog: Option<Vec<usize>>,
    viewport: Option<Viewport>,
    png: Option<(usize, ImageAssetLimits)>,
}
pub(crate) struct DecodedProjectWire {
    project: ProjectEnvelope,
    allocations: Vec<Arc<Vec<u8>>>,
}
impl DecodedProjectWire {
    pub(crate) fn allocations(&self) -> &[Arc<Vec<u8>>] {
        &self.allocations
    }
    pub(crate) fn into_project(self) -> ProjectEnvelope {
        self.project
    }
}

fn png_data(raw: &RawValue, cap: usize) -> Result<(String, usize), ProjectWireError> {
    let chars = base64::encoded_len(cap, true).ok_or(ProjectWireError::Budget)?;
    let data = text(raw, chars)?;
    if data.is_empty() || data.len() % 4 != 0 {
        return Err(ProjectWireError::Invalid);
    }
    let padding = data.bytes().rev().take_while(|b| *b == b'=').count();
    if padding > 2 {
        return Err(ProjectWireError::Invalid);
    }
    let length = (data.len() / 4)
        .checked_mul(3)
        .and_then(|n| n.checked_sub(padding))
        .ok_or(ProjectWireError::Budget)?;
    if length == 0 || length > cap {
        return Err(ProjectWireError::Budget);
    }
    Ok((data, length))
}
fn viewport(
    raw: &RawValue,
    ids: &HashMap<String, usize>,
    records: &[Record],
) -> Result<Option<Viewport>, ProjectWireError> {
    if raw.get() == "null" {
        return Ok(None);
    }
    let r: Reference<'_> = object(raw)?;
    if text(r.kind, 11)? != "still-image" {
        return Err(ProjectWireError::Unsupported);
    }
    let id = text(r.asset_id, 255)?;
    let name = text(r.name, 255)?;
    if id.is_empty()
        || id.contains('\0')
        || name.is_empty()
        || name.contains(['/', '\\', '\0'])
        || matches!(name.as_str(), "." | "..")
        || !r.opacity.is_finite()
        || !(0.0..=1.0).contains(&r.opacity)
        || r.offset_frames.unsigned_abs() > 9_007_199_254_740_991
    {
        return Err(ProjectWireError::Invalid);
    }
    let index = *ids.get(&id).ok_or(ProjectWireError::Invalid)?;
    if records[index].kind != AssetKind::Image
        || !matches!(&records[index].source, Source::Png { .. })
    {
        return Err(ProjectWireError::Unsupported);
    }
    Ok(Some(Viewport {
        index,
        name,
        visible: r.visible,
        opacity: r.opacity,
        offset: r.offset_frames,
    }))
}
fn base(
    metadata: Metadata,
    drawing: &RawValue,
    limits: ProjectWireLimits,
) -> Result<ProjectEnvelope, ProjectWireError> {
    if drawing.get().len() > limits.drawing_bytes {
        return Err(ProjectWireError::Budget);
    }
    let metadata = metadata.into_metadata()?;
    let drawing = crate::object_codec::decode_storage_project(drawing.get().as_bytes())
        .map_err(ProjectWireError::Drawing)?;
    Ok(ProjectEnvelope::no_media(metadata, drawing))
}

pub(crate) fn prepare_project_wire(
    bytes: &[u8],
    limits: ProjectWireLimits,
) -> Result<PreparedProjectWire, ProjectWireError> {
    if bytes.len() > limits.envelope_bytes {
        return Err(ProjectWireError::Budget);
    }
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err(ProjectWireError::Invalid);
    }
    let version: Version = serde_json::from_slice(bytes).map_err(|_| ProjectWireError::Invalid)?;
    match version.format_version {
        1 => prepare_legacy(bytes, limits),
        2 => prepare_catalog(bytes, limits),
        _ => Err(ProjectWireError::Unsupported),
    }
}
fn prepare_legacy(
    bytes: &[u8],
    limits: ProjectWireLimits,
) -> Result<PreparedProjectWire, ProjectWireError> {
    let wire: Input<'_> = serde_json::from_slice(bytes).map_err(|_| ProjectWireError::Invalid)?;
    if wire.format != FORMAT || wire.format_version != 1 {
        return Err(ProjectWireError::Unsupported);
    }
    let mut records = Vec::new();
    let asset = asset_entry(wire.assets)?;
    if wire.viewport_reference.get() == "null" && asset.is_some() {
        return Err(ProjectWireError::Unsupported);
    }
    let base = base(wire.project, wire.drawing, limits)?;
    let mut pending_data = None;
    if let Some(raw) = asset {
        limits.png.ok_or(ProjectWireError::Budget)?;
        let a: crate::object_codec::project::Asset<'_> = object(raw)?;
        if text(a.media_type, 9)? != "image/png" || text(a.encoding, 6)? != "base64" {
            return Err(ProjectWireError::Unsupported);
        }
        pending_data = Some(a.data);
        records.push(Record {
            id: text(a.id, 255)?,
            name: String::new(),
            kind: AssetKind::Image,
            source: Source::Png {
                data: String::new(),
                length: 0,
            },
        });
    }
    let ids = records
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.clone(), i))
        .collect();
    let viewport = viewport(wire.viewport_reference, &ids, &records)?;
    if let Some(raw) = pending_data {
        let (data, length) = png_data(raw, limits.png.ok_or(ProjectWireError::Budget)?.0)?;
        records[0].source = Source::Png { data, length };
    }
    Ok(PreparedProjectWire {
        base,
        lengths: records
            .iter()
            .filter_map(|r| match &r.source {
                Source::Png { length, .. } => Some(*length),
                _ => None,
            })
            .collect(),
        records,
        catalog: None,
        viewport,
        png: limits.png,
    })
}
fn prepare_catalog(
    bytes: &[u8],
    limits: ProjectWireLimits,
) -> Result<PreparedProjectWire, ProjectWireError> {
    let wire: CatalogInput<'_> =
        serde_json::from_slice(bytes).map_err(|_| ProjectWireError::Invalid)?;
    if wire.format != FORMAT || wire.format_version != 2 {
        return Err(ProjectWireError::Unsupported);
    }
    let catalog: Catalog<'_> = object(wire.asset_inventory)?;
    if !array::<&RawValue>(catalog.placements, MAX_ASSETS)?.is_empty() {
        return Err(ProjectWireError::Unsupported);
    }
    let mut records = Vec::new();
    let mut ids = HashMap::new();
    for raw in array::<&RawValue>(wire.assets, MAX_ASSETS)? {
        let e: Entry<'_> = object(raw)?;
        let id = text(e.id, 255)?;
        AssetId::new(id.clone()).map_err(|_| ProjectWireError::Invalid)?;
        let name = text(e.name, 255)?;
        if name.trim().is_empty() || name.contains('\0') {
            return Err(ProjectWireError::Invalid);
        }
        let kind =
            AssetKind::from_tag(&text(e.kind, 5)?).map_err(|_| ProjectWireError::Unsupported)?;
        // Strict disjoint source shapes: trying both never admits unknown members.
        let source = if let Ok(source) = object::<Linked<'_>>(e.source) {
            if text(source.kind, 8)? != "linked" {
                return Err(ProjectWireError::Unsupported);
            }
            let locator = text(source.locator, MAX_LOCATOR)?;
            AssetSource::linked(locator.clone(), MAX_LOCATOR)
                .map_err(|_| ProjectWireError::Invalid)?;
            Source::Linked(locator)
        } else {
            let source: Embedded<'_> = object(e.source)?;
            if text(source.kind, 8)? != "embedded"
                || kind != AssetKind::Image
                || text(source.media_type, 9)? != "image/png"
                || text(source.encoding, 6)? != "base64"
            {
                return Err(ProjectWireError::Unsupported);
            }
            let (data, length) =
                png_data(source.data, limits.png.ok_or(ProjectWireError::Budget)?.0)?;
            Source::Png { data, length }
        };
        if ids.insert(id.clone(), records.len()).is_some() {
            return Err(ProjectWireError::Invalid);
        }
        records.push(Record {
            id,
            name,
            kind,
            source,
        });
    }
    let mut order = Vec::new();
    let mut used = HashSet::new();
    for raw in array::<&RawValue>(catalog.asset_ids, MAX_ASSETS)? {
        let id = text(raw, 255)?;
        let index = *ids.get(&id).ok_or(ProjectWireError::Invalid)?;
        if !used.insert(index) {
            return Err(ProjectWireError::Invalid);
        }
        order.push(index);
    }
    let viewport = viewport(wire.viewport_reference, &ids, &records)?;
    if let Some(v) = &viewport {
        used.insert(v.index);
    }
    if used.len() != records.len() {
        return Err(ProjectWireError::Invalid);
    }
    Ok(PreparedProjectWire {
        base: base(wire.project, wire.drawing, limits)?,
        lengths: records
            .iter()
            .filter_map(|r| match &r.source {
                Source::Png { length, .. } => Some(*length),
                _ => None,
            })
            .collect(),
        records,
        catalog: Some(order),
        viewport,
        png: limits.png,
    })
}

impl PreparedProjectWire {
    pub(crate) fn encoded_lengths(&self) -> impl ExactSizeIterator<Item = usize> + '_ {
        self.lengths.iter().copied()
    }
    pub(crate) fn decode(
        self,
        mut allocate: impl FnMut(usize) -> Result<Vec<u8>, ProjectWireError>,
        mut validate: impl FnMut(&[u8], ImageAssetLimits) -> Result<(), ProjectWireError>,
    ) -> Result<DecodedProjectWire, ProjectWireError> {
        let mut allocations = Vec::new();
        let mut sources = Vec::new();
        for record in &self.records {
            let source = match &record.source {
                Source::Linked(locator) => AssetSource::linked(locator.clone(), MAX_LOCATOR)
                    .map_err(|_| ProjectWireError::Invalid)?,
                Source::Png { data, length } => {
                    let (cap, png) = self.png.ok_or(ProjectWireError::Budget)?;
                    let mut encoded = allocate(*length)?;
                    if encoded.len() != *length || encoded.capacity() != *length {
                        return Err(ProjectWireError::Budget);
                    }
                    let mut reader = base64::read::DecoderReader::new(
                        data.as_bytes(),
                        &base64::engine::general_purpose::STANDARD,
                    );
                    std::io::Read::read_exact(&mut reader, &mut encoded)
                        .map_err(|_| ProjectWireError::Invalid)?;
                    if std::io::Read::read(&mut reader, &mut [0])
                        .map_err(|_| ProjectWireError::Invalid)?
                        != 0
                    {
                        return Err(ProjectWireError::Invalid);
                    }
                    validate(&encoded, png)?;
                    let allocation = Arc::new(encoded);
                    let source = AssetSource::admitted_embedded(
                        "image/png".into(),
                        Arc::clone(&allocation),
                        cap,
                    )
                    .map_err(|_| ProjectWireError::Invalid)?;
                    allocations.push(allocation);
                    source
                }
            };
            sources.push(source);
        }
        let mut project = self.base;
        if let Some(v) = self.viewport {
            let r = &self.records[v.index];
            let arc = sources[v.index]
                .encoded_allocation()
                .ok_or(ProjectWireError::Invalid)?;
            let reference = ProjectReferenceState::admitted_still_shared(
                r.id.clone(),
                v.name,
                v.visible,
                v.opacity,
                v.offset,
                Arc::clone(arc),
                self.png.ok_or(ProjectWireError::Budget)?.0,
            )
            .map_err(|_| ProjectWireError::Invalid)?;
            project = project.with_reference(reference);
        }
        if let Some(order) = self.catalog {
            let mut entries = Vec::new();
            for index in order {
                let r = &self.records[index];
                let value = AssetRecord::new(
                    AssetId::new(r.id.clone()).map_err(|_| ProjectWireError::Invalid)?,
                    r.name.clone(),
                    r.kind,
                    sources[index].clone(),
                )
                .map_err(|_| ProjectWireError::Invalid)?;
                entries.push(Arc::new(value));
            }
            let inventory = AssetInventory::new(entries, Vec::new(), MAX_ASSETS, 0)
                .map_err(|_| ProjectWireError::Invalid)?;
            project = project.with_asset_inventory(Some(inventory));
        }
        Ok(DecodedProjectWire {
            project,
            allocations,
        })
    }
}
