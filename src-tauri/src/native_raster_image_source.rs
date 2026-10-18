//! Actual scoped native file input for L06-M2. No page bytes, inferred File.path, image decode or writer.
use serde::Deserialize;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) type ImageReadPermission = Arc<dyn Fn(&Path) -> bool + Send + Sync>;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum ImageSource {
    File { path: String },
}

fn read(source: ImageSource, allowed: &dyn Fn(&Path) -> bool) -> Result<Vec<u8>, String> {
    let ImageSource::File { path } = source;
    if path.contains('\0') || !Path::new(&path).is_absolute() {
        return Err("invalid native image source path".into());
    }
    let resolved = PathBuf::from(path)
        .canonicalize()
        .map_err(|_| "native image source unavailable")?;
    if !allowed(&resolved) {
        return Err("native image source outside filesystem scope".into());
    }
    let file = File::open(&resolved).map_err(|_| "native image source unavailable")?;
    let metadata = file
        .metadata()
        .map_err(|_| "native image source unavailable")?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_BYTES {
        return Err("native image source exceeds admission bounds".into());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(metadata.len() as usize)
        .map_err(|_| "native image source allocation unavailable")?;
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "native image source read failed")?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_BYTES {
        return Err("native image source exceeds admission bounds".into());
    }
    Ok(bytes)
}

pub(super) async fn resolve(
    source: ImageSource,
    allowed: ImageReadPermission,
) -> Result<Vec<u8>, String> {
    tokio::task::spawn_blocking(move || read(source, allowed.as_ref()))
        .await
        .map_err(|_| "native image source worker unavailable".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_requires_a_scoped_regular_file_and_never_infers_a_dom_handle() {
        let directory =
            std::env::temp_dir().join(format!("nemo-image-source-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("still.png");
        std::fs::write(&path, b"bounded host bytes").unwrap();
        let source = ImageSource::File {
            path: path.to_str().unwrap().to_owned(),
        };
        assert_eq!(
            read(source.clone(), &|resolved| resolved
                == path.canonicalize().unwrap())
            .unwrap(),
            b"bounded host bytes"
        );
        assert!(read(source, &|_| false).is_err());
        assert!(read(
            ImageSource::File {
                path: "relative.png".into()
            },
            &|_| true
        )
        .is_err());
        assert!(read(
            ImageSource::File {
                path: directory.to_str().unwrap().into()
            },
            &|_| true
        )
        .is_err());
        assert!(serde_json::from_value::<ImageSource>(
            serde_json::json!({"kind":"file","path":"/still.png","bytes":"page bytes"})
        )
        .is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
