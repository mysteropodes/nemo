//! Drop/paste router kernel (L10-C, lane 10 / #2116; IDs G37, I46).
//!
//! Pure port of the routing decisions of the agreed desktop baseline
//! (`fbe4b8a3`): `handleDrop` of `src/js/drop-import.js` (file classification,
//! SVG peel-off, group execution order, toast text), `isDraggingFiles`,
//! `isSvgFile`, `looksLikeSvg` and `onPaste` of `src/js/svg-import.js` (paste routing and
//! markup extraction) and `importPath` of
//! `src/js/adapters/native-object-reference.js` (the PNG-only native reference
//! route). Nothing here imports, decodes or applies a file: the kernel decides
//! WHICH provider receives WHICH input index, in WHICH order, and reports every
//! input that no provider can take as typed data. The paste router, the
//! JavaScript text rules and the reference pick live in the child [`paste`].
//!
//! Baseline facts the kernel keeps on purpose:
//! - the groups are four filters over the dropped list, not a partition: a file
//!   named `*.svg` whose MIME is `audio/*` or `video/*` is in BOTH its media group
//!   and the SVG group (the baseline imports it twice). [`ImportBatchPlan`]
//!   keeps both entries; every input still appears at least once, none is
//!   dropped, and [`ImportBatchPlan::appearances`] exposes the overlap;
//! - execution order (audio, SVG, image, video, other) differs from the toast
//!   order (audio, image, SVG, video, other);
//! - the baseline skips a group silently when its importer global is absent
//!   (`window.SMAudio` and so on) while the toast still lists it. The native
//!   kernel never does that: a group whose provider is missing makes the whole
//!   drop [`DropRoute::Unavailable`] and nothing is applied. The
//!   `window.SMSvgImport &&` guard of the SVG filter is the same kind of guard
//!   (without the importer the baseline would send an SVG file to the raster
//!   pile, the behaviour its feedback #749 comment calls wrong): native
//!   classification always peels SVG, so a missing vector provider is
//!   `Unavailable`, never a raster import;
//! - `otherFiles` (neither audio, image, video nor SVG) go to the reference
//!   importer, which under a native project only toasts
//!   [`NATIVE_REFERENCE_DROP_TOAST`] and imports nothing; the legacy path would
//!   only toast [`LEGACY_UNRECOGNIZED_TOAST_KEY`], because that group can contain
//!   no `image/*` or `video/*` file. Either way it is NEVER success, so the
//!   native route reports it as [`BlockReason`] and applies nothing.
//!
//! Strings are Rust UTF-8 text; the baseline's UTF-16 strings differ only for
//! lone surrogates, which a Rust `String` cannot hold. MIME comparisons are
//! case-sensitive like `indexOf`; `/\.svg$/i`, `/<svg[\s>]/i` and the
//! `.png` suffix are ASCII case-insensitive (non-ASCII never folds to ASCII in
//! a non-Unicode JS regexp or in `toLowerCase` for these letters).

/// MIME type compared by `isSvgFile`.
pub(crate) const SVG_MIME: &str = "image/svg+xml";
/// Start of every drop toast (`'Import : '`).
pub(crate) const TOAST_PREFIX: &str = "Import : ";
/// `reference-bridge.js` `importFiles` toast under `nativeReference()`.
pub(crate) const NATIVE_REFERENCE_DROP_TOAST: &str =
    "Use Import Reference to choose a PNG file for this native project.";
/// i18n key the legacy `importFiles` toasts when it finds no image and no video.
pub(crate) const LEGACY_UNRECOGNIZED_TOAST_KEY: &str = "toastFormatNotRecognizedVideoImages";

/// One dropped file as the baseline reads it: `File.type` and `File.name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DropFile {
    pub(crate) mime: String,
    pub(crate) name: String,
}

impl DropFile {
    pub(crate) fn new(mime: &str, name: &str) -> DropFile {
        DropFile {
            mime: mime.to_string(),
            name: name.to_string(),
        }
    }
}

/// ASCII case-insensitive `endsWith` over bytes (the suffixes are ASCII).
fn ends_with_ignore_ascii_case(text: &str, suffix: &str) -> bool {
    let (t, s) = (text.as_bytes(), suffix.as_bytes());
    t.len() >= s.len() && t[t.len() - s.len()..].eq_ignore_ascii_case(s)
}

fn starts_with_ci(bytes: &[u8], at: usize, pattern: &[u8]) -> bool {
    bytes.len() >= at + pattern.len() && bytes[at..at + pattern.len()].eq_ignore_ascii_case(pattern)
}

/// `isSvgFile`: `type === 'image/svg+xml' || /\.svg$/i.test(name)`.
pub(crate) fn is_svg_file(file: &DropFile) -> bool {
    file.mime == SVG_MIME || ends_with_ignore_ascii_case(&file.name, ".svg")
}

/// `isDraggingFiles`: `e.dataTransfer && indexOf.call(types || [], 'Files') >= 0`;
/// `None` stands for a missing `dataTransfer` or a missing `types` list. Only
/// the exact, case-sensitive `Files` type opens the drop gate.
pub(crate) fn is_dragging_files(types: Option<&[&str]>) -> bool {
    types.map_or(false, |t| t.contains(&"Files"))
}

/// The five import groups of a drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropGroup {
    Audio,
    Svg,
    Image,
    Video,
    Other,
}

/// What would take a group's files natively (a delegated target, not behaviour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    /// L09 audio tracks (G30).
    AudioTracks,
    /// L10 vector import transaction (G35).
    VectorImport,
    /// L06 raster/media asset attach (G02, G27).
    RasterAssets,
    /// L09 video media (G28, G42).
    VideoMedia,
    /// The existing PNG-only native reference owner.
    NativeReference,
}

impl Provider {
    pub(crate) fn task_ids(self) -> &'static [&'static str] {
        match self {
            Provider::AudioTracks => &["G30"],
            Provider::VectorImport => &["G35"],
            Provider::RasterAssets => &["G02", "G27"],
            Provider::VideoMedia => &["G28", "G42"],
            Provider::NativeReference => &[],
        }
    }
}

impl DropGroup {
    /// The order `handleDrop` calls the importers in.
    pub(crate) const EXECUTION_ORDER: [DropGroup; 5] = [
        DropGroup::Audio,
        DropGroup::Svg,
        DropGroup::Image,
        DropGroup::Video,
        DropGroup::Other,
    ];
    /// The order `handleDrop` lists the groups in its toast.
    pub(crate) const TOAST_ORDER: [DropGroup; 5] = [
        DropGroup::Audio,
        DropGroup::Image,
        DropGroup::Svg,
        DropGroup::Video,
        DropGroup::Other,
    ];

    pub(crate) fn provider(self) -> Provider {
        match self {
            DropGroup::Audio => Provider::AudioTracks,
            DropGroup::Svg => Provider::VectorImport,
            DropGroup::Image => Provider::RasterAssets,
            DropGroup::Video => Provider::VideoMedia,
            DropGroup::Other => Provider::NativeReference,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            DropGroup::Audio => "audio",
            DropGroup::Svg => "svg",
            DropGroup::Image => "image",
            DropGroup::Video => "video",
            DropGroup::Other => "other",
        }
    }

    /// The toast fragment of a non-empty group (`référence` has no count).
    fn toast_part(self, count: usize) -> String {
        match self {
            DropGroup::Audio => format!("{} piste(s) audio", count),
            DropGroup::Image => format!("{} image(s)", count),
            DropGroup::Svg => format!("{} SVG (vecteurs)", count),
            DropGroup::Video => format!("{} vidéo(s)", count),
            DropGroup::Other => "référence".to_string(),
        }
    }
}

/// Which of the baseline's five filters accept one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Memberships {
    pub(crate) audio: bool,
    pub(crate) svg: bool,
    pub(crate) image: bool,
    pub(crate) video: bool,
    pub(crate) other: bool,
}

impl Memberships {
    pub(crate) fn has(&self, group: DropGroup) -> bool {
        match group {
            DropGroup::Audio => self.audio,
            DropGroup::Svg => self.svg,
            DropGroup::Image => self.image,
            DropGroup::Video => self.video,
            DropGroup::Other => self.other,
        }
    }
}

/// The five `files.filter(...)` predicates of `handleDrop`, with SVG peeled
/// before the image pile and excluded from `otherFiles`.
pub(crate) fn memberships(file: &DropFile) -> Memberships {
    let svg = is_svg_file(file);
    let audio = file.mime.starts_with("audio/");
    let image_prefix = file.mime.starts_with("image/");
    let video = file.mime.starts_with("video/");
    Memberships {
        audio,
        svg,
        image: image_prefix && !svg,
        video,
        other: !audio && !image_prefix && !video && !svg,
    }
}

/// One non-empty group: input indices in dropped order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupPlan {
    pub(crate) group: DropGroup,
    pub(crate) file_indices: Vec<usize>,
}

/// The ordered groups of one drop; empty groups are absent, as `if (xFiles.length ...)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportBatchPlan {
    /// In [`DropGroup::EXECUTION_ORDER`].
    pub(crate) groups: Vec<GroupPlan>,
    /// Number of dropped files the plan was built from.
    pub(crate) file_count: usize,
}

impl ImportBatchPlan {
    pub(crate) fn indices(&self, group: DropGroup) -> &[usize] {
        self.groups
            .iter()
            .find(|g| g.group == group)
            .map(|g| g.file_indices.as_slice())
            .unwrap_or(&[])
    }

    /// How many groups hold input `index` (2 for the baseline double-routed files).
    pub(crate) fn appearances(&self, index: usize) -> usize {
        self.groups
            .iter()
            .filter(|g| g.file_indices.contains(&index))
            .count()
    }

    /// True when every input index below `file_count` is in at least one group.
    pub(crate) fn covers_every_file(&self) -> bool {
        (0..self.file_count).all(|i| self.appearances(i) > 0)
    }

    /// `'Import : ' + parts.join(' + ')` in toast order.
    pub(crate) fn toast_text(&self) -> String {
        let parts: Vec<String> = DropGroup::TOAST_ORDER
            .iter()
            .filter_map(|&g| {
                let n = self.indices(g).len();
                if n == 0 {
                    None
                } else {
                    Some(g.toast_part(n))
                }
            })
            .collect();
        format!("{}{}", TOAST_PREFIX, parts.join(" + "))
    }
}

/// Group a drop. `None` for an empty drop (the baseline returns before doing
/// anything) and, defensively, when no group holds a file (unreachable: every
/// file belongs to at least one group).
pub(crate) fn plan_groups(files: &[DropFile]) -> Option<ImportBatchPlan> {
    if files.is_empty() {
        return None;
    }
    let all: Vec<Memberships> = files.iter().map(memberships).collect();
    let mut groups = Vec::new();
    for group in DropGroup::EXECUTION_ORDER {
        let file_indices: Vec<usize> = (0..files.len()).filter(|&i| all[i].has(group)).collect();
        if !file_indices.is_empty() {
            groups.push(GroupPlan {
                group,
                file_indices,
            });
        }
    }
    if groups.is_empty() {
        return None;
    }
    Some(ImportBatchPlan {
        groups,
        file_count: files.len(),
    })
}

/// Whether a delegated provider can take its group today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderState {
    Available,
    Unavailable,
}

/// The native reference owner the `other` group would go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReferenceOwner {
    /// The accepted PNG-only owner exists.
    NativePngOnly,
    Absent,
}

/// What the caller says it can take; no provider is invented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DropPorts {
    pub(crate) audio: ProviderState,
    pub(crate) svg: ProviderState,
    pub(crate) image: ProviderState,
    pub(crate) video: ProviderState,
    pub(crate) reference: ReferenceOwner,
}

impl DropPorts {
    /// The state at the lane-10 base: no group has a native provider.
    pub(crate) const NONE_AVAILABLE: DropPorts = DropPorts {
        audio: ProviderState::Unavailable,
        svg: ProviderState::Unavailable,
        image: ProviderState::Unavailable,
        video: ProviderState::Unavailable,
        reference: ReferenceOwner::NativePngOnly,
    };
}

/// Why one group cannot be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockReason {
    /// The group's delegated provider does not exist yet.
    ProviderUnavailable,
    /// Native reference: a drop imports nothing; the user must use Import Reference (PNG).
    UseImportReference,
    /// No native reference owner at all.
    ReferenceOwnerAbsent,
}

impl BlockReason {
    /// The baseline toast of the native-reference case.
    pub(crate) fn baseline_toast(self) -> Option<&'static str> {
        match self {
            BlockReason::UseImportReference => Some(NATIVE_REFERENCE_DROP_TOAST),
            _ => None,
        }
    }
}

/// One blocked group of a drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blocked {
    pub(crate) group: DropGroup,
    pub(crate) provider: Provider,
    pub(crate) reason: BlockReason,
    pub(crate) file_indices: Vec<usize>,
}

/// A drop nothing may apply; it carries no executable plan and no success toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnavailableDrop {
    pub(crate) file_count: usize,
    /// In execution order.
    pub(crate) blocked: Vec<Blocked>,
}

/// Outcome of routing one drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DropRoute {
    /// No files: the baseline does nothing.
    Empty,
    /// Every non-empty group has its provider; the whole plan may be prepared.
    Ready(ImportBatchPlan),
    /// At least one group cannot be taken; nothing is applied (all or nothing).
    Unavailable(UnavailableDrop),
}

fn block_reason(group: DropGroup, ports: &DropPorts) -> Option<BlockReason> {
    let state = match group {
        DropGroup::Audio => ports.audio,
        DropGroup::Svg => ports.svg,
        DropGroup::Image => ports.image,
        DropGroup::Video => ports.video,
        DropGroup::Other => {
            return Some(match ports.reference {
                ReferenceOwner::NativePngOnly => BlockReason::UseImportReference,
                ReferenceOwner::Absent => BlockReason::ReferenceOwnerAbsent,
            })
        }
    };
    match state {
        ProviderState::Available => None,
        ProviderState::Unavailable => Some(BlockReason::ProviderUnavailable),
    }
}

/// Group a drop and check every group's provider before anything can run.
pub(crate) fn route_drop(files: &[DropFile], ports: &DropPorts) -> DropRoute {
    let plan = match plan_groups(files) {
        Some(plan) => plan,
        None => return DropRoute::Empty,
    };
    let blocked: Vec<Blocked> = plan
        .groups
        .iter()
        .filter_map(|g| {
            block_reason(g.group, ports).map(|reason| Blocked {
                group: g.group,
                provider: g.group.provider(),
                reason,
                file_indices: g.file_indices.clone(),
            })
        })
        .collect();
    if blocked.is_empty() {
        DropRoute::Ready(plan)
    } else {
        DropRoute::Unavailable(UnavailableDrop {
            file_count: plan.file_count,
            blocked,
        })
    }
}

#[path = "native_import_paste_route.rs"]
pub(crate) mod paste;

#[cfg(test)]
#[path = "../tests/native_import_drop_route_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/native_import_drop_route.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/native_import_drop_route_ports.rs"]
mod ports_tests;

#[cfg(test)]
#[path = "../tests/native_import_drop_route_controls.rs"]
mod controls;
