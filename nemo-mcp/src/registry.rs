//! Per-user discovery records. Connection secrets never enter MCP results or diagnostics.
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub instance_id: String,
    pub port: u16,
    pub secret: String,
    pub build_id: String,
}

pub fn registry_root() -> io::Result<PathBuf> {
    if let Some(root) = std::env::var_os("NEMO_MCP_REGISTRY") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(io::Error::other("NEMO_MCP_REGISTRY must be absolute"));
        }
        return Ok(root);
    }
    if let Some(root) = std::env::var_os("NEMO_TAURI_DATA_DIR") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(io::Error::other("NEMO_TAURI_DATA_DIR must be absolute"));
        }
        return Ok(root.join("mcp"));
    }
    dirs::data_local_dir()
        .map(|root| root.join("com.strokemotion.app").join("mcp"))
        .ok_or_else(|| io::Error::other("local application data directory unavailable"))
}

pub fn read_endpoints(root: &Path) -> io::Result<Vec<Endpoint>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.len() > 4096 {
            continue;
        }
        if let Ok(record) = serde_json::from_slice::<Endpoint>(&fs::read(path)?) {
            if uuid::Uuid::parse_str(&record.instance_id).is_ok() && record.secret.len() == 36 {
                records.push(record);
            }
        }
    }
    records.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
    Ok(records)
}

pub struct Registration {
    path: PathBuf,
}

impl Registration {
    pub fn create(root: &Path, endpoint: &Endpoint) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        }
        let path = root.join(format!("{}.json", endpoint.instance_id));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        use io::Write;
        file.write_all(&serde_json::to_vec(endpoint)?)?;
        Ok(Self { path })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
