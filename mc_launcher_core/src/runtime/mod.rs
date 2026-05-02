pub mod prepare;

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RuntimeLayout {
    pub workspace_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub libraries_dir: PathBuf,
    pub assets_root: PathBuf,
    pub installers_cache_dir: PathBuf,
    pub temp_root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub fn from_str(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "fabric" => Self::Fabric,
            "forge" => Self::Forge,
            "neoforge" => Self::NeoForge,
            _ => Self::Vanilla,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeRequest {
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub java_path: String,
    pub download_concurrency: usize,
    pub prefer_bmclapi: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimeResult {
    pub version_id: String,
    pub version_json_path: PathBuf,
    pub inherited_version_json_path: Option<PathBuf>,
    pub client_jar_path: PathBuf,
    pub asset_index_path: PathBuf,
}

pub trait ProgressReporter: Send + Sync {
    fn emit(&self, stage: &str, current: usize, total: usize);
}
