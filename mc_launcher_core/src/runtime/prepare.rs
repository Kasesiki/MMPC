use std::collections::{HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use super::{ProgressReporter, RuntimeLayout};

pub const MOJANG_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
pub const MOJANG_RESOURCES_BASE: &str = "https://resources.download.minecraft.net";
pub const MOJANG_LIBRARIES_BASE: &str = "https://libraries.minecraft.net";
pub const FORGE_MAVEN_BASE: &str = "https://files.minecraftforge.net/maven";
pub const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases";


pub trait DownloadTrait {
    fn url(&self) -> &str;
    fn dest(&self) -> Option<&str>;
    fn sha1(&self) -> &str;
    fn download<T: Into<PathBuf> + Send>(
        &self,
        fallback_dest: T,
    ) -> impl std::future::Future<Output = Result<()>> + Send
    where
        Self: Sync,
    {
        async {
            let dest = if let Some(dest) = self.dest() {
                Path::new(dest)
            } else {
                &fallback_dest.into()
            };
            if !check_path_sha1(dest, self.sha1())? {
                download_with_sha1(self.url(), dest, self.sha1()).await?;
            }
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionJson {
    id: String,
    /// client.jar, server.jar ,txt加载url
    pub downloads: VersionDownloads,
    /// 提供了一个链接用于下载assets
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndex,
    /// lib列表
    #[serde(default)]
    pub libraries: Vec<LibraryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: DownloadEntry,
    server: Option<DownloadEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub url: String,
    sha1: String,
    size: u64,
    #[serde(default)]
    path: Option<String>,
}

impl DownloadTrait for DownloadEntry {
    fn url(&self) -> &str {
        &self.url
    }

    fn dest(&self) -> Option<&str> {
        self.path.as_deref()
    }

    fn sha1(&self) -> &str {
        &self.sha1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    url: String,
    pub sha1: String,
    size: u64,
    #[serde(rename = "totalSize")]
    total_size: u64,
}

impl DownloadTrait for AssetIndex {
    fn url(&self) -> &str {
        &self.url
    }

    fn sha1(&self) -> &str {
        &self.sha1
    }

    fn dest(&self) -> Option<&str> {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryEntry {
    name: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    downloads: Option<LibraryDownloads>,
    #[serde(default)]
    rules: Vec<Rule>,
    #[serde(default)]
    natives: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LibraryDownloads {
    artifact: Option<DownloadEntry>,
    classifiers: Option<HashMap<String, DownloadEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Rule {
    action: String,
    os: Option<RuleOs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuleOs {
    name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexObjects {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    size: u64,
}

#[derive(Clone)]
pub struct DownloadTask {
    pub urls: String,
    pub dest: PathBuf,
    pub sha1: String,
}

/// 获取.MMPC路径
pub fn mm() -> PathBuf {
    let e = std::env::current_exe().unwrap_or_default();
    e.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}

/// 获取workspace的路径
pub fn wd(id: &str) -> PathBuf {
    mm().join("workspaces").join(id)
}

pub fn versions_dir(id: &str) -> PathBuf {
    wd(id).join("versions")
}

pub fn build_runtime_layout(workspace_id: &str) -> RuntimeLayout {
    let root = mm();
    let workspace_dir = wd(workspace_id);
    RuntimeLayout {
        workspace_dir: workspace_dir.clone(),
        versions_dir: versions_dir(workspace_id),
        libraries_dir: root.join("libraries"),
        assets_root: root.join("assets"),
        installers_cache_dir: root.join("cache").join("installers"),
        temp_root: root.join("tmp"),
    }
}

pub async fn fetch_json_with_candidates(url: &str, _label: &str) -> Result<serde_json::Value> {
    bmclapi::request(url)
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.into())
}

pub async fn fetch_fabric_version_value(
    mc_version: &str,
    loader_version: &str,
) -> Result<serde_json::Value> {
    let official = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
        mc_version,
        loader_version.trim()
    );
    fetch_json_with_candidates(&official, "下载 Fabric version.json").await
}

fn merge_argument_values(parent: &mut serde_json::Value, child: &serde_json::Value) {
    let Some(parent_obj) = parent.as_object_mut() else {
        *parent = child.clone();
        return;
    };
    let Some(child_obj) = child.as_object() else {
        *parent = child.clone();
        return;
    };

    for key in ["game", "jvm"] {
        if let Some(child_args) = child_obj.get(key).and_then(|v| v.as_array()) {
            let parent_args = parent_obj
                .entry(key.to_string())
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            if let Some(parent_array) = parent_args.as_array_mut() {
                parent_array.extend(child_args.iter().cloned());
            } else {
                *parent_args = serde_json::Value::Array(child_args.clone());
            }
        }
    }

    for (key, value) in child_obj {
        if key == "game" || key == "jvm" {
            continue;
        }
        match parent_obj.get_mut(key) {
            Some(existing) => merge_version_value(existing, value),
            None => {
                parent_obj.insert(key.clone(), value.clone());
            }
        }
    }
}

fn library_merge_key(value: &serde_json::Value) -> Option<String> {
    let name = value.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let (coords, ext) = match name.rsplit_once('@') {
        Some((coords, ext)) => (coords, ext),
        None => (name, "jar"),
    };
    let parts = coords.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        return Some(name.to_string());
    }
    let classifier = parts.get(3).copied().unwrap_or("");
    Some(format!("{}:{}:{}@{}", parts[0], parts[1], classifier, ext))
}

fn merge_library_arrays(
    parent: &[serde_json::Value],
    child: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let mut merged = Vec::new();
    let mut indexes = HashMap::<String, usize>::new();

    for entry in parent.iter().chain(child.iter()) {
        let Some(key) = library_merge_key(entry) else {
            merged.push(entry.clone());
            continue;
        };
        if let Some(index) = indexes.get(&key).copied() {
            merged[index] = entry.clone();
        } else {
            indexes.insert(key, merged.len());
            merged.push(entry.clone());
        }
    }
    merged
}

fn merge_version_value(parent: &mut serde_json::Value, child: &serde_json::Value) {
    match (parent, child) {
        (serde_json::Value::Object(parent_obj), serde_json::Value::Object(child_obj)) => {
            for (key, value) in child_obj {
                match key.as_str() {
                    "libraries" => {
                        let child_array = value.as_array().cloned().unwrap_or_default();
                        let target = parent_obj
                            .entry(key.clone())
                            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
                        if let Some(parent_array) = target.as_array_mut() {
                            *parent_array = merge_library_arrays(parent_array, &child_array);
                        } else {
                            *target = serde_json::Value::Array(child_array);
                        }
                    }
                    "arguments" => {
                        let target = parent_obj
                            .entry(key.clone())
                            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                        merge_argument_values(target, value);
                    }
                    _ => match parent_obj.get_mut(key) {
                        Some(existing) => merge_version_value(existing, value),
                        None => {
                            parent_obj.insert(key.clone(), value.clone());
                        }
                    },
                }
            }
        }
        (parent_value, child_value) => *parent_value = child_value.clone(),
    }
}

pub fn merge_version_json(
    parent: &serde_json::Value,
    child: &serde_json::Value,
) -> serde_json::Value {
    let mut merged = parent.clone();
    merge_version_value(&mut merged, child);
    if let Some(obj) = merged.as_object_mut() {
        obj.remove("inheritsFrom");
    }
    merged
}

fn resolve_download_path(entry: &DownloadEntry) -> Option<PathBuf> {
    entry
        .path
        .as_ref()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn detect_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        other => other,
    }
}

fn evaluate_rules(rules: &[Rule], current_os: &str) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let matches_os = match &rule.os {
            Some(os) => os.name.as_ref().map_or(true, |name| name == current_os),
            None => true,
        };
        if matches_os {
            match rule.action.as_str() {
                "allow" => allowed = true,
                "disallow" => allowed = false,
                _ => {}
            }
        }
    }
    allowed
}

fn get_native_classifier(current_os: &str) -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "64"
    } else {
        "86"
    };
    match current_os {
        "windows" => format!("natives-windows-{}", if arch == "64" { "64" } else { "32" }),
        "osx" => "natives-osx".to_string(),
        "linux" => format!("natives-linux-{arch}"),
        _ => format!("natives-{current_os}"),
    }
}

/// 为forge的library字段修改而补充
fn build_library_download_from_name(name: &str, base_url: Option<&str>) -> Option<DownloadEntry> {
    let (coords, extension) = match name.rsplit_once('@') {
        Some((coords, ext)) => (coords, ext),
        None => (name, "jar"),
    };
    let parts = coords.split(':').collect::<Vec<_>>();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let group = parts[0];
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let relative_path = match classifier {
        Some(classifier) => format!(
            "{}/{}/{}/{}-{}-{}.{}",
            group.replace('.', "/"),
            artifact,
            version,
            artifact,
            version,
            classifier,
            extension
        ),
        None => format!(
            "{}/{}/{}/{}-{}.{}",
            group.replace('.', "/"),
            artifact,
            version,
            artifact,
            version,
            extension
        ),
    };

    let mut base = base_url.unwrap_or(MOJANG_LIBRARIES_BASE);
    if base.is_empty() {
        base = MOJANG_LIBRARIES_BASE;
    }
    let normalized = if base.ends_with('/') {
        base
    } else {
        &format!("{base}/")
    };
    Some(DownloadEntry {
        url: format!("{normalized}{relative_path}"),
        sha1: String::new(),
        size: 0,
        path: Some(relative_path),
    })
}

pub fn build_library_tasks(
    libraries_dir: &Path,
    version_json: &VersionJson,
) -> Result<Vec<DownloadTask>> {
    let mut tasks = Vec::new();
    let current_os = detect_os();

    for lib in &version_json.libraries {
        if !evaluate_rules(&lib.rules, current_os) {
            continue;
        }

        let artifact = lib
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.clone())
            .filter(|artifact| !artifact.url.is_empty())
            .or_else(|| build_library_download_from_name(&lib.name, lib.url.as_deref()))
            .ok_or(anyhow!("version.json格式错误, 无法正确识别library字段格式"))?;

        // 正常情况下的直接提交
        if let Some(rel_path) = resolve_download_path(&artifact) {
            let dest = libraries_dir.join(rel_path);
            if !check_path_sha1(&dest, &artifact.sha1)? {
                tasks.push(DownloadTask {
                    urls: artifact.url.clone(),
                    dest,
                    sha1: artifact.sha1.clone(),
                });
            }
        }

        if let Some(classifiers) = lib
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.classifiers.as_ref())
        {
            let native_classifier = lib
                .natives
                .get(current_os)
                .cloned()
                .unwrap_or_else(|| get_native_classifier(current_os))
                .replace(
                    "${arch}",
                    if cfg!(target_pointer_width = "64") {
                        "64"
                    } else {
                        "32"
                    },
                );
            if let Some(native_entry) = classifiers.get(&native_classifier) {
                if let Some(rel_path) = resolve_download_path(native_entry) {
                    let dest = libraries_dir.join(rel_path);
                    if !check_path_sha1(&dest, &native_entry.sha1)? {
                        tasks.push(DownloadTask {
                            urls: native_entry.url.clone(),
                            dest,
                            sha1: native_entry.sha1.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(tasks)
}

/// concurrency是并发数
pub async fn execute_download_pool(
    reporter: &dyn ProgressReporter,
    stage: &str,
    tasks: Vec<DownloadTask>,
    concurrency: usize,
) -> Result<()> {
    let total = tasks.len();
    let mut completed = 0usize;

    let mut pending = stream::iter(
        tasks
            .into_iter()
            .map(|task| async move { download_task_with_retry(task, 3).await }),
    )
    .buffer_unordered(concurrency.max(1));

    while let Some(result) = pending.next().await {
        completed += 1;
        reporter.emit(stage, completed, total);
        if let Err(e) = result {
            bail!("下载 {stage} 失败, 原因: {e}")
        }
    }
    Ok(())
}

async fn download_task_with_retry(task: DownloadTask, max_retries: u32) -> Result<()> {
    let mut last_err = String::new();
    for attempt in 1..=max_retries {
        match download_with_sha1(&task.urls, &task.dest, &task.sha1).await {
            Ok(()) => return Ok(()),
            Err(e) => last_err = format!("下载失败({attempt}/{max_retries}): {e}"),
        }
    }
    Err(anyhow!("{last_err}"))
}

async fn download_with_sha1(url: &str, dest: &Path, expected_sha1: &str) -> Result<()> {
    let bytes = bmclapi::request(&url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败 ({})", parent.display()))?;
    }
    std::fs::write(dest, &bytes).with_context(|| format!("写入文件失败 ({})", dest.display()))?;
    if expected_sha1.trim().is_empty() || file_matches_sha1(dest, expected_sha1)? {
        return Ok(());
    }
    bail!("SHA1 校验失败")
}


// 将value写入path
pub fn write_json_pretty(path: &Path, value: &serde_json::Value) -> Result<()> {
    let content = serde_json::to_string_pretty(value)
        .with_context(|| format!("序列化 JSON 失败 ({})", path.display()))?;
    std::fs::write(path, content).with_context(|| format!("写入 JSON 失败 ({})", path.display()))
}

fn compute_file_sha1(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("打开文件失败: {}", path.display()))?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)
            .with_context(|| format!("读取文件失败: {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn file_matches_sha1(path: &Path, expected_sha1: &str) -> Result<bool> {
    if expected_sha1.trim().is_empty() || !path.exists() {
        return Ok(false);
    }
    let actual = compute_file_sha1(path)?;
    Ok(actual.eq_ignore_ascii_case(expected_sha1))
}

/// 判断路径文件sha1是否匹配传入值，如果不同返回true,文件不存在也返回true, sha1为空返回false
pub fn check_path_sha1(path: &Path, expected_sha1: &str) -> Result<bool> {
    if path.exists() {
        return Ok(file_matches_sha1(path, expected_sha1)?);
    }
    Ok(false)
}

