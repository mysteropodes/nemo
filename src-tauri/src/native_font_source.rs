//! PRIVATE source proposal: baseline transport only; no asset identity/owner.
use base64::Engine;
use serde::Deserialize;
use std::{future::Future, time::Duration};

pub(super) const IMPORT: &str = "command.document.object.asset.font.import";
pub(super) const QUERY: &str = "query.document.object.asset.font";
const MAX: usize = 16 * 1024 * 1024;
const FETCH_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum FontSource {
    Bundled {
        #[serde(rename = "fontKey")]
        font_key: String,
    },
    Google {
        family: String,
        weight: u32,
        italic: bool,
    },
}
pub(super) enum FontBytes {
    Bundled(&'static [u8]),
    Downloaded(Vec<u8>),
}
impl AsRef<[u8]> for FontBytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Bundled(v) => v,
            Self::Downloaded(v) => v,
        }
    }
}
fn bundled(key: &str) -> Result<&'static [u8], String> {
    Ok(match key {
        "Roboto-Regular" => include_bytes!("../../src/fonts/Roboto-Regular.ttf"),
        "Roboto-Bold" => include_bytes!("../../src/fonts/Roboto-Bold.ttf"),
        "DejaVuSans-Regular" => include_bytes!("../../src/fonts/DejaVuSans.ttf"),
        "DejaVuSans-Bold" => include_bytes!("../../src/fonts/DejaVuSans-Bold.ttf"),
        "DejaVuSerif-Regular" => include_bytes!("../../src/fonts/DejaVuSerif.ttf"),
        "DejaVuSerif-Bold" => include_bytes!("../../src/fonts/DejaVuSerif-Bold.ttf"),
        "Manrope-Regular" => include_bytes!("../../src/fonts/Manrope-Regular.ttf"),
        "Inter-Regular" => include_bytes!("../../src/fonts/Inter-Regular.ttf"),
        _ => return Err("unknown bundled font".into()),
    })
}
fn decoded(encoded: &str) -> Result<Vec<u8>, String> {
    // Refuse before a decoded transport allocation. Persistent allocation is
    // separately reserved and bound by the original native owner, not here.
    if encoded.len() > MAX.div_ceil(3) * 4 {
        return Err("font source exceeds byte admission limit".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "invalid baseline font transport encoding")?;
    if bytes.is_empty() || bytes.len() > MAX {
        return Err("font source exceeds byte admission limit".into());
    }
    Ok(bytes)
}
pub(super) async fn resolve(source: FontSource) -> Result<FontBytes, String> {
    resolve_using(source, |family, weight, italic| {
        fetch_google_font(family, weight, italic)
    })
    .await
}
async fn resolve_using<F, Fut>(source: FontSource, google: F) -> Result<FontBytes, String>
where
    F: FnOnce(String, u32, bool) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    match source {
        FontSource::Bundled { font_key } => bundled(&font_key).map(FontBytes::Bundled),
        FontSource::Google {
            family,
            weight,
            italic,
        } => {
            validate_google_descriptor(&family, weight)?;
            decoded(&google(family, weight, italic).await?).map(FontBytes::Downloaded)
        }
    }
}

fn validate_google_descriptor(family: &str, weight: u32) -> Result<(), String> {
    if family.trim().is_empty()
        || family.len() > 255
        || family.contains('\0')
        || !(1..=1000).contains(&weight)
    {
        return Err("invalid font source descriptor".into());
    }
    Ok(())
}

fn check_body_length(length: Option<u64>) -> Result<(), String> {
    if length.is_some_and(|length| length > MAX as u64) {
        return Err("font source exceeds byte admission limit".into());
    }
    Ok(())
}

fn append_bounded_body(bytes: &mut Vec<u8>, chunk: &[u8]) -> Result<(), String> {
    if bytes
        .len()
        .checked_add(chunk.len())
        .is_none_or(|length| length > MAX)
    {
        return Err("font source exceeds byte admission limit".into());
    }
    // Check each accumulated length before reserving or copying transport bytes.
    bytes
        .try_reserve_exact(chunk.len())
        .map_err(|_| "font source allocation unavailable")?;
    bytes.extend_from_slice(chunk);
    Ok(())
}

async fn read_bounded_body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    check_body_length(response.content_length())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        append_bounded_body(&mut bytes, &chunk)?;
    }
    Ok(bytes)
}

async fn read_bounded_css(response: reqwest::Response) -> Result<String, String> {
    let headers = response.headers().clone();
    let bytes = read_bounded_body(response).await?;
    // Preserve reqwest's existing charset/BOM decoding on an already bounded body.
    let mut bounded = tauri::http::Response::new(bytes);
    *bounded.headers_mut() = headers;
    reqwest::Response::from(bounded)
        .text()
        .await
        .map_err(|error| error.to_string())
}

async fn fetch_before_deadline<F>(fetch: F, deadline: Duration) -> Result<String, String>
where
    F: Future<Output = Result<String, String>>,
{
    tokio::time::timeout(deadline, fetch)
        .await
        .map_err(|_| "font source fetch deadline exceeded".to_owned())?
}

pub(crate) async fn fetch_google_font(
    family: String,
    weight: u32,
    italic: bool,
) -> Result<String, String> {
    fetch_before_deadline(
        fetch_google_font_body(family, weight, italic),
        FETCH_DEADLINE,
    )
    .await
}

async fn fetch_google_font_body(
    family: String,
    weight: u32,
    italic: bool,
) -> Result<String, String> {
    validate_google_descriptor(&family, weight)?;
    let axis = if italic { "ital,wght@1," } else { "wght@" };
    let css_url = format!(
        "https://fonts.googleapis.com/css2?family={}:{}{}&display=swap",
        family.replace(' ', "+"),
        axis,
        weight
    );
    let client = reqwest::Client::new();
    let response = client
        .get(&css_url)
        .header("User-Agent", "Mozilla/5.0 (Linux; U; Android 2.2)")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let css = read_bounded_css(response).await?;
    // The response is one @font-face block per requested weight/style —
    // just one here since exactly one weight/italic combo was asked for.
    // Extract the URL inside its first src: url(...) rather than a full
    // CSS parse — this endpoint's own output shape is stable/simple enough
    // (confirmed via curl) that a real parser would be pure overhead.
    let ttf_url = css
        .split("url(")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .ok_or_else(|| {
            format!(
                "no font URL found for '{}' — family name may not exist on Google Fonts",
                family
            )
        })?
        .to_string();
    let response = client
        .get(&ttf_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let bytes = read_bounded_body(response).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
#[cfg(test)]
#[path = "native_font_source_tests.rs"]
mod tests;
