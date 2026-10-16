//! PRIVATE source proposal: baseline transport only; no asset identity/owner.
use base64::Engine;
use serde::Deserialize;

pub(super) const IMPORT: &str = "command.document.object.asset.font.import";
pub(super) const QUERY: &str = "query.document.object.asset.font";
const MAX: usize = 16 * 1024 * 1024;

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
            if family.trim().is_empty()
                || family.len() > 255
                || family.contains('\0')
                || !(1..=1000).contains(&weight)
            {
                return Err("invalid font source descriptor".into());
            }
            decoded(&google(family, weight, italic).await?).map(FontBytes::Downloaded)
        }
    }
}

pub(crate) async fn fetch_google_font(
    family: String,
    weight: u32,
    italic: bool,
) -> Result<String, String> {
    use base64::Engine;
    let axis = if italic { "ital,wght@1," } else { "wght@" };
    let css_url = format!(
        "https://fonts.googleapis.com/css2?family={}:{}{}&display=swap",
        family.replace(' ', "+"),
        axis,
        weight
    );
    let client = reqwest::Client::new();
    let css = client
        .get(&css_url)
        .header("User-Agent", "Mozilla/5.0 (Linux; U; Android 2.2)")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
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
    let bytes = client
        .get(&ttf_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
#[cfg(test)]
#[path = "native_font_source_tests.rs"]
mod tests;
