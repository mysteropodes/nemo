//! Original team-comment Save/Delete gestures on the existing sealed settings owner.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, Presence, ProjectCommentRecord};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Append {
        comment: ProjectCommentRecord,
    },
    Update {
        ordinal: u64,
        text: String,
        resolved: bool,
    },
    Delete {
        ordinal: u64,
    },
}
struct Payload {
    command: String,
    action: Action,
}
object_deserialize!(Payload {
    command: String,
    action: Action
});
fn index(ordinal: u64) -> Result<usize, Error> {
    if ordinal == 0 || ordinal > 9_007_199_254_740_991 {
        return Err(Error::InvalidRequest);
    }
    usize::try_from(ordinal - 1).map_err(|_| Error::InvalidRequest)
}
// ECMAScript String.trim whitespace, used only by the original fresh-draft guard.
fn blank_draft(text: &str) -> bool {
    text.chars().all(|c| {
        matches!(c,
            '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
            '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
            '\u{205f}' | '\u{3000}' | '\u{feff}'
        )
    })
}
impl ObjectFillOwner {
    pub(crate) fn prepare_comments_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, Error> {
        #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
        {
            let _ = bytes;
            Err(Error::Unavailable)
        }
        #[cfg(all(feature = "history", feature = "project_media_codec"))]
        {
            if bytes.len() > 4096 {
                return Err(Error::InvalidRequest);
            }
            let shape: serde_json::Value =
                serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
            if !shape
                .get("action")
                .is_some_and(serde_json::Value::is_object)
            {
                return Err(Error::InvalidRequest);
            }
            let payload: Payload =
                serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
            if payload.command != "object.project.comments.edit" {
                return Err(Error::InvalidRequest);
            }
            let origin = self.current_snapshot();
            let project = origin.project().ok_or(Error::Unavailable)?;
            let before = project.project();
            let mut comments = before.annotations().1.unwrap_or(&[]).to_vec();
            match payload.action {
                Action::Append { comment } => {
                    if comment.id.missing()
                        || comment.x.missing()
                        || comment.y.missing()
                        || comment.frame.missing()
                        || comment.created_at.missing()
                        || comment.resolved.missing()
                        || comment.text.value().is_none_or(|text| blank_draft(text))
                    {
                        return Err(Error::InvalidRequest);
                    }
                    comments.push(comment);
                }
                Action::Update {
                    ordinal,
                    text,
                    resolved,
                } => {
                    let comment = comments.get_mut(index(ordinal)?).ok_or(Error::NotFound)?;
                    comment.text = Presence::Present(text);
                    comment.resolved = Presence::Present(resolved);
                }
                Action::Delete { ordinal } => {
                    let at = index(ordinal)?;
                    if at >= comments.len() {
                        return Err(Error::NotFound);
                    }
                    comments.remove(at);
                }
            }
            let metadata = before
                .clone()
                .with_comments_metadata(comments)
                .map_err(|_| Error::InvalidRequest)?;
            let changed = &metadata != before;
            if changed {
                ObjectSnapshot::check_candidate_saveable(&project.with_metadata(metadata.clone()))?;
            }
            Ok(PreparedProjectSettings {
                origin,
                metadata,
                changed,
            })
        }
    }
}
