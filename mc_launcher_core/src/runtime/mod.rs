pub mod prepare;

use std::{fs::create_dir_all, path::PathBuf, sync::LazyLock};

pub static MMPCDIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let p = std::env::current_dir().map(|p| p.join(".MMPC"))
        .unwrap_or(PathBuf::from(".MMPC"));
    if !p.exists() {
        let _ = create_dir_all(&p);
    }
    p
});

pub static GLOBAL_ASSETS: LazyLock<PathBuf> = LazyLock::new(|| {
    let p = MMPCDIR.join("assets");
    if !p.exists() {
        let _ = create_dir_all(&p);
    }
    p
});

pub static GLOBAL_LIBRARIES: LazyLock<PathBuf> = LazyLock::new(|| {
    let p = MMPCDIR.join("libraries");
    if !p.exists() {
        let _ = create_dir_all(&p);
    }
    p
});

pub static GLOBAL_MODCACHE: LazyLock<PathBuf> = LazyLock::new(|| {
    let p = MMPCDIR.join("modcache");
    if !p.exists() {
        let _ = create_dir_all(&p);
    }
    p
});


#[derive(Debug, Clone)]
pub struct RuntimeLayout {
    /// workspace文件夹
    pub workspace_dir: PathBuf,
    /// workspace/versions
    pub versions_dir: PathBuf,
    /// .MMPC/cache/installers
    pub installers_cache_dir: PathBuf,
    /// .MMPC/tmp
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
    fn send(&self, stage: &str);
}
