use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use bmclapi::bmclapi;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use zip::ZipArchive;

use super::{LoaderKind, ProgressReporter, RuntimeLayout, RuntimeRequest, RuntimeResult};

const MOJANG_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const MOJANG_RESOURCES_BASE: &str = "https://resources.download.minecraft.net";
const MOJANG_LIBRARIES_BASE: &str = "https://libraries.minecraft.net";
const FORGE_MAVEN_BASE: &str = "https://files.minecraftforge.net/maven";
const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases";

#[derive(Debug, Serialize, Deserialize)]
struct VersionManifest {
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionEntry {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionJson {
    id: String,
    downloads: VersionDownloads,
    #[serde(rename = "assetIndex")]
    asset_index: AssetIndex,
    #[serde(default)]
    libraries: Vec<LibraryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VersionDownloads {
    client: DownloadEntry,
    server: Option<DownloadEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DownloadEntry {
    url: String,
    sha1: String,
    size: u64,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AssetIndex {
    id: String,
    url: String,
    sha1: String,
    size: u64,
    #[serde(rename = "totalSize")]
    total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LibraryEntry {
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
struct AssetIndexObjects {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallerProfile {
    version: String,
    #[serde(default)]
    minecraft: String,
    #[serde(default)]
    libraries: Vec<InstallerLibrary>,
    #[serde(default)]
    processors: Vec<InstallerProcessor>,
    #[serde(default)]
    data: HashMap<String, InstallerDataValue>,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallerLibrary {
    name: String,
    downloads: InstallerLibraryDownloads,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallerLibraryDownloads {
    artifact: DownloadEntry,
}

#[derive(Debug, Clone, Deserialize)]
struct InstallerProcessor {
    #[serde(default)]
    sides: Option<Vec<String>>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct InstallerDataValue {
    #[serde(default)]
    client: Option<String>,
    #[serde(default)]
    server: Option<String>,
}

// #[derive(Debug, Deserialize)]
// struct ForgeInstallProfile {
//     #[serde(rename = "versionInfo", default)]
//     version_info: Option<serde_json::Value>,
// }

#[derive(Clone)]
struct DownloadTask {
    urls: String,
    dest: PathBuf,
    sha1: String,
}

#[derive(Clone, Copy)]
struct LoaderInstallerSpec {
    label: &'static str,
    install_arg: &'static str,
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
        libraries_dir: workspace_dir.join("versions").join("libraries"),
        assets_root: root.join("assets"),
        installers_cache_dir: root.join("cache").join("installers"),
        temp_root: root.join("tmp"),
    }
}

// 下载/校验全部依赖
pub async fn prepare_runtime(
    workspace_id: &str,
    request: &RuntimeRequest,
    reporter: &dyn ProgressReporter,
) -> Result<RuntimeResult> {
    // 目录提前创建
    let layout: RuntimeLayout = build_runtime_layout(workspace_id);
    std::fs::create_dir_all(&layout.versions_dir)
        .with_context(|| format!("创建 versions 目录失败: {}", layout.versions_dir.display()))?;
    std::fs::create_dir_all(&layout.assets_root)
        .with_context(|| format!("创建 assets 目录失败: {}", layout.assets_root.display()))?;
    std::fs::create_dir_all(&layout.installers_cache_dir).with_context(|| {
        format!(
            "创建 installer 缓存目录失败: {}",
            layout.installers_cache_dir.display()
        )
    })?;
    std::fs::create_dir_all(&layout.temp_root)
        .with_context(|| format!("创建临时目录失败: {}", layout.temp_root.display()))?;

    let base_value = fetch_vanilla_version_value(&request.mc_version).await?;
    let inherited_version_json_path = layout
        .versions_dir
        .join(format!("{}.json", request.mc_version));
    write_json_pretty(&inherited_version_json_path, &base_value)?;
    let base_version_json: VersionJson =
        serde_json::from_value(base_value.clone()).context("解析基础 version.json 失败")?;

    let client_path = layout.versions_dir.join("client.jar");

    if request.loader == LoaderKind::NeoForge {
        ensure_single_download(
            reporter,
            &base_version_json.downloads.client.url,
            &client_path,
            "下载 client.jar",
            &base_version_json.downloads.client.sha1,
        )
        .await?;
    }

    // download_version_json根源相同, download_version_json通过格式到VersionJson从而会丢失部分信息
    let (download_version_json, launcher_version_json) = match request.loader {
        LoaderKind::Vanilla => {
            // 两者相同
            (base_version_json, base_value)
        }
        LoaderKind::Fabric => {
            // 请求fabric, 合并, 解析为VersionJson, 两者依旧相同，VersionJson并不包含全部参数
            let loader_version = request
                .loader_version
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| anyhow!("Fabric 缺少 loader_version"))?;
            let fabric_value =
                fetch_fabric_version_value(&request.mc_version, loader_version).await?;
            let merged = merge_version_json(&base_value, &fabric_value);
            let version_json: VersionJson =
                serde_json::from_value(merged.clone()).context("解析 Fabric version.json 失败")?;
            (version_json, merged)
        }
        LoaderKind::Forge | LoaderKind::NeoForge => {
            let installed_loader_value =
                ensure_loader_runtime_from_installer(&layout, request, reporter).await?;
            let merged_download = merge_version_json(&base_value, &installed_loader_value);
            let version_json: VersionJson =
                serde_json::from_value(merged_download).context("解析 loader 下载元数据失败")?;
            (version_json, installed_loader_value)
        }
    };

    let version_json_path = layout.versions_dir.join("version.json");
    write_json_pretty(&version_json_path, &launcher_version_json)?;

    // 下载client.jar, 存到工作区的versions文件夹
    ensure_single_download(
        reporter,
        &download_version_json.downloads.client.url,
        &client_path,
        "下载 client.jar",
        &download_version_json.downloads.client.sha1,
    )
    .await?;

    // 下载asset_index.json, 存到工作区的versions文件夹
    let asset_index_path = layout.versions_dir.join("asset_index.json");
    ensure_single_download(
        reporter,
        &download_version_json.asset_index.url,
        &asset_index_path,
        "下载 asset index.json",
        &download_version_json.asset_index.sha1,
    )
    .await?;

    // 下载library, 存到工作区的versions/libraries文件夹
    let library_tasks: Vec<DownloadTask> =
        build_library_tasks(&layout.libraries_dir, &download_version_json)?;
    execute_download_pool(
        reporter,
        "下载 libraries",
        library_tasks,
        request.download_concurrency.max(1),
    )
    .await?;

    let asset_index_content = std::fs::read_to_string(&asset_index_path)
        .with_context(|| format!("读取 asset index 失败: {}", asset_index_path.display()))?;
    let asset_index: AssetIndexObjects =
        serde_json::from_str(&asset_index_content).context("解析 asset index 失败")?;

    let indexes_dir = layout.assets_root.join("indexes");
    let objects_dir = layout.assets_root.join("objects");
    std::fs::create_dir_all(&indexes_dir)
        .with_context(|| format!("创建 indexes 目录失败: {}", indexes_dir.display()))?;
    std::fs::create_dir_all(&objects_dir)
        .with_context(|| format!("创建 objects 目录失败: {}", objects_dir.display()))?;

    // .MMPC/assets/indexes/xx.json, 校验并复制
    let asset_index_dest =
        indexes_dir.join(format!("{}.json", download_version_json.asset_index.id));
    if !check_path_sha1(&asset_index_dest, &download_version_json.asset_index.sha1)? {
        std::fs::copy(&asset_index_path, &asset_index_dest).with_context(|| {
            format!(
                "复制 asset index 到全局目录失败 ({} -> {})",
                asset_index_path.display(),
                asset_index_dest.display()
            )
        })?;
    }

    // 下载全部assets
    let mut asset_tasks = Vec::new();
    for obj in asset_index.objects.values() {
        let hash = &obj.hash;
        if hash.len() < 2 {
            continue;
        }
        let subdir = &hash[..2];
        let asset_path = objects_dir.join(subdir).join(hash);
        let should_download = !file_matches_sha1(&asset_path, hash).unwrap_or(false);
        if should_download {
            asset_tasks.push(DownloadTask {
                urls: asset_url_candidates(hash),
                dest: asset_path,
                sha1: hash.clone(),
            });
        }
    }
    execute_download_pool(
        reporter,
        "下载 assets",
        asset_tasks,
        request.download_concurrency.max(1),
    )
    .await?;

    Ok(RuntimeResult {
        // mc version, 如1.21.1
        version_id: launcher_version_json
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or(&request.mc_version)
            .to_string(),
        // workspace/versions/version.json, mc version.json的缓存
        version_json_path,
        // workspace/versions/{version_id}.json, 如1.21.1.json的缓存，和上个变量内容相同，待测试
        inherited_version_json_path: Some(inherited_version_json_path),
        // workspace/versions/client.jar
        client_jar_path: client_path,
        // workspace/versions/asset_index.json
        asset_index_path,
    })
}

// 将value写入path
fn write_json_pretty(path: &Path, value: &serde_json::Value) -> Result<()> {
    let content = serde_json::to_string_pretty(value)
        .with_context(|| format!("序列化 JSON 失败 ({})", path.display()))?;
    std::fs::write(path, content).with_context(|| format!("写入 JSON 失败 ({})", path.display()))
}

async fn fetch_json_with_candidates(url: &str, _label: &str) -> Result<serde_json::Value> {
    bmclapi::request(url)
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.into())
}

/// 获取原版version.json
async fn fetch_vanilla_version_value(mc_version: &str) -> Result<serde_json::Value> {
    let manifest: VersionManifest = serde_json::from_value(
        fetch_json_with_candidates(MOJANG_MANIFEST_URL, "下载版本清单").await?,
    )
    .context("解析版本清单失败")?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|entry| entry.id == mc_version)
        .ok_or_else(|| anyhow!("未找到 MC 版本 {mc_version}"))?;

    fetch_json_with_candidates(&entry.url, "下载原版 version.json").await
}

async fn fetch_fabric_version_value(
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

fn asset_url_candidates(hash: &str) -> String {
    let subdir = &hash[..2];
    format!("{MOJANG_RESOURCES_BASE}/{subdir}/{hash}")
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

fn merge_version_json(parent: &serde_json::Value, child: &serde_json::Value) -> serde_json::Value {
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

fn build_library_tasks(
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

/// 下载url到dest并显示stage在前端，在下载成功后使用expecred_sha1进行验证
async fn ensure_single_download(
    reporter: &dyn ProgressReporter,
    url: &str,
    dest: &Path,
    stage: &str,
    expected_sha1: &str,
) -> Result<()> {
    if !check_path_sha1(dest, expected_sha1)? {
        reporter.send(stage);
        download_with_sha1(&url, dest, expected_sha1).await?;
    }
    Ok(())
}

/// concurrency是并发数
async fn execute_download_pool(
    reporter: &dyn ProgressReporter,
    stage: &str,
    tasks: Vec<DownloadTask>,
    concurrency: usize,
) -> Result<()> {
    if tasks.is_empty() {
        return Ok(());
    }

    let total = tasks.len();
    let mut completed = 0usize;

    // 将任务转化为future并设置并发数
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

fn parse_artifact_path(spec: &str) -> Option<PathBuf> {
    let trimmed = spec.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(trimmed);
    let (coords, ext) = match inner.rsplit_once('@') {
        Some((coords, ext)) => (coords, ext),
        None => (inner, "jar"),
    };
    let parts = coords.split(':').collect::<Vec<_>>();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let relative = match classifier {
        Some(classifier) => {
            format!("{group}/{artifact}/{version}/{artifact}-{version}-{classifier}.{ext}",)
        }
        None => format!("{group}/{artifact}/{version}/{artifact}-{version}.{ext}"),
    };
    Some(PathBuf::from(relative))
}

fn installer_data_value(
    value: &InstallerDataValue,
    side: &str,
    temp_dir: &Path,
    _installer_path: &Path,
) -> Option<String> {
    let raw = match side {
        "server" => value.server.as_ref().or(value.client.as_ref())?,
        _ => value.client.as_ref().or(value.server.as_ref())?,
    };
    if let Some(rel) = parse_artifact_path(raw) {
        return Some(
            temp_dir
                .join("libraries")
                .join(rel)
                .to_string_lossy()
                .to_string(),
        );
    }
    if let Some(rel) = raw.strip_prefix("/data/") {
        return Some(
            temp_dir
                .join("data")
                .join(rel)
                .to_string_lossy()
                .to_string(),
        );
    }
    if let Some(rel) = raw.strip_prefix('/') {
        return Some(temp_dir.join(rel).to_string_lossy().to_string());
    }
    if raw.starts_with('\'') && raw.ends_with('\'') {
        return Some(raw.trim_matches('\'').to_string());
    }
    Some(raw.to_string())
}

fn extract_installer_data_files(installer_path: &Path, temp_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(installer_path)?;
    let mut zip = ZipArchive::new(file)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if !name.starts_with("data/") {
            continue;
        }
        let rel = name.trim_start_matches("data/");
        if rel.is_empty() {
            continue;
        }
        let out_path = temp_dir.join("data").join(rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

fn read_jar_main_class(jar_path: &Path) -> Result<String> {
    let file = std::fs::File::open(jar_path)
        .with_context(|| format!("打开处理器 jar 失败 ({})", jar_path.display()))?;
    let mut zip = ZipArchive::new(file)
        .with_context(|| format!("读取处理器 jar 失败 ({})", jar_path.display()))?;
    let mut manifest = zip
        .by_name("META-INF/MANIFEST.MF")
        .with_context(|| format!("读取 MANIFEST.MF 失败 ({})", jar_path.display()))?;
    let mut content = String::new();
    manifest
        .read_to_string(&mut content)
        .with_context(|| format!("读取 MANIFEST.MF 内容失败 ({})", jar_path.display()))?;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("Main-Class:") {
            return Ok(value.trim().to_string());
        }
    }
    bail!(
        "处理器 jar 缺少 Main-Class ({}): {}",
        jar_path.display(),
        content.lines().next().unwrap_or("")
    )
}

async fn run_installer_processor(
    java_path: &str,
    processor: &InstallerProcessor,
    temp_dir: &Path,
    installer_path: &Path,
    data_map: &HashMap<String, InstallerDataValue>,
    side: &str,
    token_values: &HashMap<String, String>,
) -> Result<()> {
    let classpath_entries = processor
        .classpath
        .iter()
        .filter_map(|entry| parse_artifact_path(entry))
        .map(|rel| temp_dir.join("libraries").join(rel))
        .collect::<Vec<_>>();
    let classpath = classpath_entries
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { ";" } else { ":" });
    let main_jar = parse_artifact_path(&processor.jar)
        .map(|rel| temp_dir.join("libraries").join(rel))
        .ok_or_else(|| anyhow!("无法解析处理器 jar: {}", processor.jar))?;
    let main_class = read_jar_main_class(&main_jar)?;

    let mut args = Vec::new();
    for arg in &processor.args {
        let mut value = arg.clone();
        for (key, data) in data_map {
            let placeholder = format!("{{{key}}}");
            if let Some(replacement) = installer_data_value(data, side, temp_dir, installer_path) {
                value = value.replace(&placeholder, &replacement);
            }
        }
        for (key, replacement) in token_values {
            let placeholder = format!("{{{key}}}");
            value = value.replace(&placeholder, replacement);
        }
        value = value.replace("{ROOT}", &temp_dir.to_string_lossy());
        value = value.replace("{INSTALLER}", &installer_path.to_string_lossy());
        value = value.replace("{SIDE}", side);
        value = value.replace(
            "{LIBRARY_DIR}",
            &temp_dir.join("libraries").to_string_lossy(),
        );
        // NeoForge processors may pass raw artifact specs (e.g. [group:artifact:version@zip])
        // as direct argument values; rewrite them to local downloaded library paths.
        if let Some(rel) = parse_artifact_path(&value) {
            let candidate = temp_dir.join("libraries").join(rel);
            if candidate.exists() {
                value = candidate.to_string_lossy().to_string();
            }
        }
        args.push(value);
    }

    let cp = if classpath.is_empty() {
        main_jar.to_string_lossy().to_string()
    } else {
        format!(
            "{}{}{}",
            classpath,
            if cfg!(windows) { ";" } else { ":" },
            main_jar.to_string_lossy()
        )
    };

    let java_path = java_path.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(java_path)
            .arg("-cp")
            .arg(cp)
            .arg(main_class)
            .args(args)
            .output()
    })
    .await
    .with_context(|| "执行处理器任务失败".to_string())?
    .with_context(|| "启动处理器失败".to_string())?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = [stdout.trim(), stderr.trim()]
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(anyhow!(
            "处理器执行失败: {}",
            if detail.is_empty() {
                "未返回可用日志".to_string()
            } else {
                detail
            }
        ));
    }

    Ok(())
}

fn extract_installer_json<T: for<'de> Deserialize<'de>>(
    installer_path: &Path,
    file_name: &str,
) -> Result<T> {
    let file = std::fs::File::open(installer_path)?;
    let mut zip = ZipArchive::new(file)?;
    let mut entry = zip.by_name(file_name)?;
    let mut content = String::new();
    entry.read_to_string(&mut content)?;
    serde_json::from_str(&content).map_err(|e| e.into())
}

async fn ensure_neoforge_runtime_from_installer(
    layout: &RuntimeLayout,
    request: &RuntimeRequest,
    reporter: &dyn ProgressReporter,
) -> Result<serde_json::Value> {
    let loader_version = request
        .loader_version
        .as_ref()
        .ok_or_else(|| anyhow!("NeoForge 缺少 loader_version"))?
        .trim();
    let installer_path =
        installer_cache_path(layout, request.loader, &request.mc_version, loader_version);
    let temp_dir = layout
        .temp_root
        .join(format!("installer-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;
    std::fs::create_dir_all(temp_dir.join("libraries"))?;
    if !installer_path.exists() {
        let resp = bmclapi::request(&format!(
            "https://bmclapi2.bangbang93.com/neoforge/version/{}/download/installer.jar",
            loader_version
        ))
        .await
        .context("请求 NeoForge installer 失败")?;

        let mut file = File::create(&installer_path).await?;
        let bytes = resp
            .bytes()
            .await
            .context("读取 NeoForge installer 响应失败")?;
        file.write_all(&bytes).await.with_context(|| {
            format!("写入 NeoForge installer 失败: {}", installer_path.display())
        })?;
        let _ = file.sync_all().await;
        drop(file)
    }
    let profile: InstallerProfile =
        extract_installer_json(&installer_path, "install_profile.json")?;
    let version_json: serde_json::Value = extract_installer_json(&installer_path, "version.json")?;
    extract_installer_data_files(&installer_path, &temp_dir)?;

    let temp_client_path = layout.versions_dir.join("client.jar");
    if !temp_client_path.is_file() {
        bail!("NeoForge 运行前缺少 client.jar")
    }

    let cache_libraries_root = layout
        .installers_cache_dir
        .parent()
        .map(|p| p.join("libraries"))
        .unwrap_or_else(|| layout.installers_cache_dir.join("libraries"));
    std::fs::create_dir_all(&cache_libraries_root)?;

    let mut library_tasks = Vec::new();
    for lib in &profile.libraries {
        let rel = lib
            .downloads
            .artifact
            .path
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("NeoForge 依赖缺少下载路径: {}", lib.name))?;
        let cache_dest = cache_libraries_root.join(&rel);
        if !check_path_sha1(&cache_dest, &lib.downloads.artifact.sha1)? {
            library_tasks.push(DownloadTask {
                urls: lib.downloads.artifact.url.clone(),
                dest: cache_dest.clone(),
                sha1: lib.downloads.artifact.sha1.clone(),
            });
        }
    }
    execute_download_pool(
        reporter,
        "下载 NeoForge 依赖",
        library_tasks,
        request.download_concurrency.max(1),
    )
    .await?;

    for lib in &profile.libraries {
        let rel = lib
            .downloads
            .artifact
            .path
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("NeoForge 依赖缺少下载路径: {}", lib.name))?;
        let cache_src = cache_libraries_root.join(&rel);
        if !cache_src.is_file() {
            bail!("NeoForge 缓存依赖缺失: {}", cache_src.display())
        }
        let dest = temp_dir.join("libraries").join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&cache_src, &dest).with_context(|| {
            format!(
                "复制 NeoForge 缓存依赖失败 ({} -> {})",
                cache_src.display(),
                dest.display()
            )
        })?;
    }

    let side = "client";
    let mut token_values = HashMap::new();
    token_values.insert(
        "MINECRAFT_JAR".to_string(),
        temp_client_path.to_string_lossy().to_string(),
    );
    token_values.insert("MINECRAFT_VERSION".to_string(), request.mc_version.clone());
    token_values.insert("INSTALLER_VERSION".to_string(), profile.version.clone());
    token_values.insert("VERSION".to_string(), profile.version.clone());
    if !profile.minecraft.trim().is_empty() {
        token_values.insert("MC_VERSION".to_string(), profile.minecraft.clone());
    }

    for processor in profile.processors.iter().filter(|processor| {
        processor
            .sides
            .as_ref()
            .map(|sides| sides.iter().any(|s| s == side))
            .unwrap_or(true)
    }) {
        run_installer_processor(
            &request.java_path,
            processor,
            &temp_dir,
            &installer_path,
            &profile.data,
            side,
            &token_values,
        )
        .await?;
    }

    let generated_client =
        generated_client_rel_path(request.loader, &request.mc_version, loader_version)
            .map(|rel| temp_dir.join("libraries").join(rel))
            .ok_or_else(|| anyhow!("无法确定 NeoForge client 产物路径"))?;
    if !generated_client.is_file() {
        return Err(anyhow!(
            "NeoForge 处理完成但缺少生成产物: {}",
            generated_client.display()
        ));
    }

    if layout.libraries_dir.exists() {
        std::fs::remove_dir_all(&layout.libraries_dir).with_context(|| {
            format!(
                "清理旧 libraries 目录失败: {}",
                layout.libraries_dir.display()
            )
        })?;
    }
    let allowed_runtime_paths = collect_runtime_library_paths(
        &temp_dir.join("libraries"),
        &version_json,
        generated_client_rel_path(request.loader, &request.mc_version, loader_version)
            .as_deref()
            .ok_or_else(|| anyhow!("无法确定 NeoForge client 相对路径"))?,
    )?;
    copy_selected_library_paths(
        &temp_dir.join("libraries"),
        &layout.libraries_dir,
        &allowed_runtime_paths,
    )?;

    let version_json_path = layout.versions_dir.join("version.json");
    write_json_pretty(&version_json_path, &version_json)?;
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(version_json)
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

fn file_matches_sha1(path: &Path, expected_sha1: &str) -> Result<bool> {
    if expected_sha1.trim().is_empty() || !path.exists() {
        return Ok(false);
    }
    let actual = compute_file_sha1(path)?;
    Ok(actual.eq_ignore_ascii_case(expected_sha1))
}

/// 判断路径文件sha1是否匹配传入值，如果不同返回true,文件不存在也返回true, sha1为空返回false
fn check_path_sha1(path: &Path, expected_sha1: &str) -> Result<bool> {
    if path.exists() {
        return Ok(file_matches_sha1(path, expected_sha1)?);
    }
    Ok(false)
}

fn loader_installer_spec(loader: LoaderKind) -> Option<LoaderInstallerSpec> {
    match loader {
        LoaderKind::Forge => Some(LoaderInstallerSpec {
            label: "Forge",
            install_arg: "--installClient",
        }),
        LoaderKind::NeoForge => Some(LoaderInstallerSpec {
            label: "NeoForge",
            install_arg: "--install-client",
        }),
        _ => None,
    }
}

fn installer_urls(loader: LoaderKind, mc_version: &str, loader_version: &str) -> Result<String> {
    match loader {
        LoaderKind::Forge => {
            let version = format!("{}-{}", mc_version, loader_version.trim());
            let official = format!(
                "{FORGE_MAVEN_BASE}/net/minecraftforge/forge/{0}/forge-{0}-installer.jar",
                version
            );
            Ok(official)
        }
        LoaderKind::NeoForge => {
            let version = loader_version.trim();
            let official = format!(
                "{NEOFORGE_MAVEN_BASE}/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar",
                version
            );
            Ok(official)
        }
        _ => Err(anyhow!("当前 loader 不支持 installer")),
    }
}

fn generated_client_rel_path(
    loader: LoaderKind,
    mc_version: &str,
    loader_version: &str,
) -> Option<PathBuf> {
    match loader {
        LoaderKind::Forge => {
            let version = format!("{}-{}", mc_version, loader_version.trim());
            Some(PathBuf::from(format!(
                "net/minecraftforge/forge/{0}/forge-{0}-client.jar",
                version
            )))
        }
        LoaderKind::NeoForge => Some(PathBuf::from(format!(
            "net/neoforged/neoforge/{0}/neoforge-{0}-client.jar",
            loader_version.trim()
        ))),
        _ => None,
    }
}

fn known_installer_only_prefixes(loader: LoaderKind) -> &'static [&'static str] {
    match loader {
        LoaderKind::Forge => &[
            "ForgeAutoRenamingTool-",
            "binarypatcher-",
            "installertools-",
            "jarsplitter-",
            "srgutils-0.4.",
            "srgutils-0.5.",
            "client-",
            "asm-9.2",
            "asm-9.6",
            "asm-tree-9.2",
            "asm-tree-9.6",
            "asm-commons-9.2",
            "asm-commons-9.6",
            "asm-analysis-9.2",
        ],
        LoaderKind::NeoForge => &[],
        _ => &[],
    }
}

fn library_dir_has_known_installer_only_jars(
    libraries_dir: &Path,
    loader: LoaderKind,
) -> Result<bool> {
    if !libraries_dir.exists() {
        return Ok(false);
    }
    let prefixes = known_installer_only_prefixes(loader);
    if prefixes.is_empty() {
        return Ok(false);
    }

    for entry in walk_files_recursive(libraries_dir)? {
        let Some(file_name) = entry.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if prefixes.iter().any(|prefix| file_name.starts_with(prefix)) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn walk_files_recursive(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    for entry in
        std::fs::read_dir(root).with_context(|| format!("读取目录失败 ({})", root.display()))?
    {
        let entry = entry.with_context(|| format!("读取目录项失败 ({})", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_files_recursive(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}

fn extract_ignore_list_prefixes(version_json: &serde_json::Value) -> Vec<String> {
    version_json
        .get("arguments")
        .and_then(|value| value.get("jvm"))
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.as_str())
        .find_map(|arg| arg.strip_prefix("-DignoreList=").map(str::to_string))
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty() && !value.contains("${"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn collect_runtime_library_paths(
    libraries_root: &Path,
    version_json: &serde_json::Value,
    generated_client_rel_path: &Path,
) -> Result<HashSet<PathBuf>> {
    let mut allowed = HashSet::new();

    for lib in version_json
        .get("libraries")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(path) = lib
            .get("downloads")
            .and_then(|value| value.get("artifact"))
            .and_then(|value| value.get("path"))
            .and_then(|value| value.as_str())
        {
            allowed.insert(PathBuf::from(path));
        }
    }

    allowed.insert(generated_client_rel_path.to_path_buf());

    let ignore_prefixes = extract_ignore_list_prefixes(version_json);
    if !ignore_prefixes.is_empty() {
        for path in walk_files_recursive(libraries_root)? {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if ignore_prefixes
                .iter()
                .any(|prefix| file_name.starts_with(prefix))
            {
                let rel = path
                    .strip_prefix(libraries_root)
                    .with_context(|| format!("计算运行时库相对路径失败: {}", path.display()))?;
                allowed.insert(rel.to_path_buf());
            }
        }
    }

    Ok(allowed)
}

fn copy_selected_library_paths(
    src_root: &Path,
    dest_root: &Path,
    allowed: &HashSet<PathBuf>,
) -> Result<()> {
    std::fs::create_dir_all(dest_root)
        .with_context(|| format!("创建目录失败 ({})", dest_root.display()))?;
    for rel_path in allowed {
        let src = src_root.join(rel_path);
        if !src.is_file() {
            continue;
        }
        let dest = dest_root.join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录失败 ({})", parent.display()))?;
        }
        std::fs::copy(&src, &dest).with_context(|| {
            format!("复制运行时库失败 ({} -> {})", src.display(), dest.display())
        })?;
    }
    Ok(())
}

fn installer_cache_path(
    layout: &RuntimeLayout,
    loader: LoaderKind,
    mc_version: &str,
    loader_version: &str,
) -> PathBuf {
    let filename = match loader {
        LoaderKind::Forge => format!(
            "forge-{}-{}-installer.jar",
            mc_version,
            loader_version.trim()
        ),
        LoaderKind::NeoForge => format!("neoforge-{}-installer.jar", loader_version.trim()),
        LoaderKind::Fabric => format!(
            "fabric-{}-{}-installer.jar",
            mc_version,
            loader_version.trim()
        ),
        LoaderKind::Vanilla => format!("minecraft-{}-installer.jar", mc_version),
    };
    layout.installers_cache_dir.join(filename)
}

fn find_installed_version_json(install_dir: &Path, expected_id: &str) -> Result<PathBuf> {
    let versions_root = install_dir.join("versions");
    let entries = std::fs::read_dir(&versions_root).with_context(|| {
        format!(
            "读取 installer versions 目录失败 ({})",
            versions_root.display()
        )
    })?;
    let mut fallback = None;
    for entry in entries {
        let entry = entry.context("读取 installer versions 目录项失败")?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(dir_name) = dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let candidate = dir.join(format!("{dir_name}.json"));
        if candidate.is_file() {
            if dir_name == expected_id {
                return Ok(candidate);
            }
            if fallback.is_none() {
                fallback = Some(candidate);
            }
        }
    }
    fallback.ok_or_else(|| anyhow!("installer 未生成版本 json"))
}

async fn ensure_loader_runtime_from_installer(
    layout: &RuntimeLayout,
    request: &RuntimeRequest,
    reporter: &dyn ProgressReporter,
) -> Result<serde_json::Value> {
    let loader_version = request
        .loader_version
        .as_ref()
        .ok_or_else(|| anyhow!("loader 缺少 loader_version"))?
        .trim();

    if request.loader == LoaderKind::NeoForge {
        return ensure_neoforge_runtime_from_installer(layout, request, reporter).await;
    }

    let spec = loader_installer_spec(request.loader)
        .ok_or_else(|| anyhow!("当前 loader 不支持 installer"))?;

    let version_json_path = layout.versions_dir.join("version.json");
    let generated_client_path =
        generated_client_rel_path(request.loader, &request.mc_version, loader_version)
            .map(|rel| layout.libraries_dir.join(rel))
            .ok_or_else(|| anyhow!("无法确定 loader client 产物路径"))?;
    let expected_id = match request.loader {
        LoaderKind::Forge => format!("{}-forge-{}", request.mc_version, loader_version),
        LoaderKind::NeoForge => format!("neoforge-{}", loader_version),
        _ => String::new(),
    };

    if generated_client_path.is_file() && version_json_path.is_file() {
        let cached: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&version_json_path).with_context(|| {
                format!(
                    "读取缓存 version.json 失败: {}",
                    version_json_path.display()
                )
            })?,
        )
        .context("解析缓存 version.json 失败")?;
        if cached
            .get("id")
            .and_then(|value| value.as_str())
            .map(|id| id == expected_id)
            .unwrap_or(false)
            && !library_dir_has_known_installer_only_jars(&layout.libraries_dir, request.loader)?
        {
            return Ok(cached);
        }
    }

    let installer_path =
        installer_cache_path(layout, request.loader, &request.mc_version, loader_version);
    if !installer_path.is_file() {
        let primary = installer_urls(request.loader, &request.mc_version, loader_version)?;
        reporter.emit("下载 loader installer", 0, 1);
        download_with_sha1(&primary, &installer_path, "").await?;
        reporter.emit("下载 loader installer", 1, 1);
    } else {
        reporter.emit("下载 loader installer", 1, 1);
    }

    let temp_dir = layout
        .temp_root
        .join(format!("installer-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)
        .with_context(|| format!("创建临时 installer 目录失败: {}", temp_dir.display()))?;
    std::fs::write(
        temp_dir.join("launcher_profiles.json"),
        r#"{"profiles":{},"settings":{},"version":3}"#,
    )
    .context("写入 launcher_profiles.json 失败")?;

    reporter.emit("生成 loader 运行时", 0, 1);
    let installer_path_clone = installer_path.clone();
    let temp_dir_clone = temp_dir.clone();
    let java_path = request.java_path.clone();
    let install_arg = spec.install_arg.to_string();
    let label = spec.label.to_string();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&java_path)
            .arg("-jar")
            .arg(&installer_path_clone)
            .arg(&install_arg)
            .arg(&temp_dir_clone)
            .output()
    })
    .await
    .with_context(|| format!("执行 {label} installer 任务失败"))?
    .with_context(|| format!("启动 {label} installer 失败"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = [stdout.trim(), stderr.trim()]
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(anyhow!(
            "{label} installer 执行失败: {}",
            if detail.is_empty() {
                "未返回可用日志".to_string()
            } else {
                detail
            }
        ));
    }

    let installed_version_json_path = find_installed_version_json(&temp_dir, &expected_id)?;
    let installed_version_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&installed_version_json_path).with_context(|| {
            format!(
                "读取 installer version.json 失败: {}",
                installed_version_json_path.display()
            )
        })?,
    )
    .context("解析 installer version.json 失败")?;

    if layout.libraries_dir.exists() {
        std::fs::remove_dir_all(&layout.libraries_dir).with_context(|| {
            format!(
                "清理旧 libraries 目录失败: {}",
                layout.libraries_dir.display()
            )
        })?;
    }
    let temp_libraries_dir = temp_dir.join("libraries");
    let allowed_runtime_paths = collect_runtime_library_paths(
        &temp_libraries_dir,
        &installed_version_json,
        generated_client_rel_path(request.loader, &request.mc_version, loader_version)
            .as_deref()
            .ok_or_else(|| anyhow!("无法确定 loader client 相对路径"))?,
    )?;
    copy_selected_library_paths(
        &temp_libraries_dir,
        &layout.libraries_dir,
        &allowed_runtime_paths,
    )?;

    if !generated_client_path.is_file() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(anyhow!(
            "{label} installer 执行完成，但缺少生成产物: {}",
            generated_client_path.display()
        ));
    }

    reporter.emit("生成 loader 运行时", 1, 1);
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(installed_version_json)
}
