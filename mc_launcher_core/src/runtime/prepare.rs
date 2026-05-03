use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use bmclapi::bmclapi::replace;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use uuid::Uuid;

use super::{LoaderKind, ProgressReporter, RuntimeLayout, RuntimeRequest, RuntimeResult};

const MOJANG_MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const MOJANG_RESOURCES_BASE: &str = "https://resources.download.minecraft.net";
const MOJANG_LIBRARIES_BASE: &str = "https://libraries.minecraft.net";
const FORGE_MAVEN_BASE: &str = "https://maven.minecraftforge.net";
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


// #[derive(Debug, Deserialize)]
// struct ForgeInstallProfile {
//     #[serde(rename = "versionInfo", default)]
//     version_info: Option<serde_json::Value>,
// }

#[derive(Clone)]
struct DownloadTask {
    label: String,
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


pub async fn prepare_runtime(
    workspace_id: &str,
    request: &RuntimeRequest,
    reporter: &dyn ProgressReporter,
) -> Result<RuntimeResult, String> {

    // 目录提前创建
    let layout: RuntimeLayout = build_runtime_layout(workspace_id);
    std::fs::create_dir_all(&layout.versions_dir).map_err(|e| format!("创建 versions 目录失败: {e}"))?;
    std::fs::create_dir_all(&layout.assets_root).map_err(|e| format!("创建 assets 目录失败: {e}"))?;
    std::fs::create_dir_all(&layout.installers_cache_dir)
        .map_err(|e| format!("创建 installer 缓存目录失败: {e}"))?;
    std::fs::create_dir_all(&layout.temp_root).map_err(|e| format!("创建临时目录失败: {e}"))?;

    let base_value = fetch_vanilla_version_value(&request.mc_version).await?;
    let inherited_version_json_path = layout.versions_dir.join(format!("{}.json", request.mc_version));
    write_json_pretty(&inherited_version_json_path, &base_value)?;

    let (download_version_json, launcher_version_json) = match request.loader {
        LoaderKind::Vanilla => {
            let version_json: VersionJson = serde_json::from_value(base_value.clone())
                .map_err(|e| format!("解析原版 version.json 失败: {e}"))?;
            (version_json, base_value)
        }
        LoaderKind::Fabric => {
            let loader_version = request
                .loader_version
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "Fabric 缺少 loader_version".to_string())?;
            let fabric_value = fetch_fabric_version_value(&request.mc_version, loader_version).await?;
            let merged = merge_version_json(&base_value, &fabric_value);
            let version_json: VersionJson = serde_json::from_value(merged.clone())
                .map_err(|e| format!("解析 Fabric version.json 失败: {e}"))?;
            (version_json, merged)
        }
        LoaderKind::Forge | LoaderKind::NeoForge => {
            let installed_loader_value = ensure_loader_runtime_from_installer(&layout, request, reporter).await?;
            let merged_download = merge_version_json(&base_value, &installed_loader_value);
            let version_json: VersionJson = serde_json::from_value(merged_download)
                .map_err(|e| format!("解析 loader 下载元数据失败: {e}"))?;
            (version_json, installed_loader_value)
        }
    };

    let version_json_path = layout.versions_dir.join("version.json");
    write_json_pretty(&version_json_path, &launcher_version_json)?;

    let client_path = layout.versions_dir.join("client.jar");
    ensure_single_download(
        reporter,
        &download_version_json.downloads.client.url,
        &client_path,
        "下载 client.jar",
        &download_version_json.downloads.client.sha1,
    )
    .await?;

    let asset_index_path = layout.versions_dir.join("asset_index.json");
    ensure_single_download(
        reporter,
        &download_version_json.asset_index.url,
        &asset_index_path,
        "下载 asset index",
        &download_version_json.asset_index.sha1,
    )
    .await?;

    let library_tasks = build_library_tasks(
        &layout.libraries_dir,
        &download_version_json,
    )?;
    execute_download_pool(
        reporter,
        "下载 libraries",
        library_tasks,
        request.download_concurrency.max(1),
    )
    .await?;

    let asset_index_content = std::fs::read_to_string(&asset_index_path)
        .map_err(|e| format!("读取 asset index 失败: {e}"))?;
    let asset_index: AssetIndexObjects = serde_json::from_str(&asset_index_content)
        .map_err(|e| format!("解析 asset index 失败: {e}"))?;

    let indexes_dir = layout.assets_root.join("indexes");
    let objects_dir = layout.assets_root.join("objects");
    std::fs::create_dir_all(&indexes_dir).map_err(|e| format!("创建 indexes 目录失败: {e}"))?;
    std::fs::create_dir_all(&objects_dir).map_err(|e| format!("创建 objects 目录失败: {e}"))?;

    let asset_index_dest = indexes_dir.join(format!("{}.json", download_version_json.asset_index.id));
    if should_download_with_sha1(&asset_index_dest, &download_version_json.asset_index.sha1)? {
        std::fs::copy(&asset_index_path, &asset_index_dest)
            .map_err(|e| format!("复制 asset index 到全局目录失败: {e}"))?;
    }

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
                label: format!("下载 asset {hash}"),
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
        version_id: launcher_version_json
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or(&request.mc_version)
            .to_string(),
        version_json_path,
        inherited_version_json_path: Some(inherited_version_json_path),
        client_jar_path: client_path,
        asset_index_path,
    })
}


// 将value写入path
fn write_json_pretty(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| format!("序列化 JSON 失败 ({}): {e}", path.display()))?;
    std::fs::write(path, content).map_err(|e| format!("写入 JSON 失败 ({}): {e}", path.display()))
}

// async fn fetch_json_value_with_fallback(primary: &str, fallback: &str, label: &str) -> Result<serde_json::Value, String> {
//     match reqwest::get(primary).await {
//         Ok(response) => match response.error_for_status() {
//             Ok(success) => success
//                 .json::<serde_json::Value>()
//                 .await
//                 .map_err(|e| format!("{label} 解析失败: {e}")),
//             Err(_) => fetch_json_value(fallback, label).await,
//         },
//         Err(_) => fetch_json_value(fallback, label).await,
//     }
// }

// async fn fetch_json_value(url: &str, label: &str) -> Result<serde_json::Value, String> {
//     reqwest::get(url)
//         .await
//         .map_err(|e| format!("{label} 请求失败: {e}"))?
//         .error_for_status()
//         .map_err(|e| format!("{label} 状态异常: {e}"))?
//         .json::<serde_json::Value>()
//         .await
//         .map_err(|e| format!("{label} 解析失败: {e}"))
// }

async fn fetch_vanilla_version_value(mc_version: &str) -> Result<serde_json::Value, String> {

    let manifest: VersionManifest = serde_json::from_value(bmclapi::bmclapi::fetch_json_value(MOJANG_MANIFEST_URL).await
    .map_err(|e| e.to_string())?)
    .map_err(|e| format!("解析版本清单失败: {e}"))?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|entry| entry.id == mc_version)
        .ok_or_else(|| format!("未找到 MC 版本 {mc_version}"))?;

    bmclapi::bmclapi::fetch_json_value(&entry.url).await.map_err(|e| e.to_string())
}

async fn fetch_fabric_version_value(mc_version: &str, loader_version: &str) -> Result<serde_json::Value, String> {
    let official = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
        mc_version,
        loader_version.trim()
    );
    bmclapi::bmclapi::fetch_json_value(&official).await.map_err(|e| e.to_string())
}

// fn mirror_asset_url(hash: &str, prefer_bmclapi: bool) -> String {
//     let subdir = &hash[..2];
//     if prefer_bmclapi {
//         format!("{BMCLAPI_RESOURCES_BASE}/{subdir}/{hash}")
//     } else {
//         format!("{MOJANG_RESOURCES_BASE}/{subdir}/{hash}")
//     }
// }

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

fn merge_library_arrays(parent: &[serde_json::Value], child: &[serde_json::Value]) -> Vec<serde_json::Value> {
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
    entry.path.as_ref().filter(|v| !v.is_empty()).map(PathBuf::from)
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
    let arch = if cfg!(target_arch = "x86_64") { "64" } else { "86" };
    match current_os {
        "windows" => format!("natives-windows-{}", if arch == "64" { "64" } else { "32" }),
        "osx" => "natives-osx".to_string(),
        "linux" => format!("natives-linux-{arch}"),
        _ => format!("natives-{current_os}"),
    }
}

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

    let mut base = base_url.unwrap_or(MOJANG_LIBRARIES_BASE).trim().to_string();
    if base.is_empty() {
        base = MOJANG_LIBRARIES_BASE.to_string();
    }
    let normalized = if base.ends_with('/') { base } else { format!("{base}/") };
    Some(DownloadEntry {
        url: format!("{normalized}{relative_path}"),
        sha1: String::new(),
        size: 0,
        path: Some(relative_path),
    })
}

fn build_library_tasks(libraries_dir: &Path, version_json: &VersionJson) -> Result<Vec<DownloadTask>, String> {
    let mut tasks = Vec::new();
    let current_os = detect_os();

    for lib in &version_json.libraries {
        if !evaluate_rules(&lib.rules, current_os) {
            continue;
        }

        let artifact_download = lib
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.clone())
            .filter(|artifact| !artifact.url.trim().is_empty())
            .or_else(|| build_library_download_from_name(&lib.name, lib.url.as_deref()));

        if let Some(artifact) = artifact_download.as_ref() {
            if let Some(rel_path) = resolve_download_path(artifact) {
                let dest = libraries_dir.join(rel_path);
                if should_download_with_sha1(&dest, &artifact.sha1)? {
                    tasks.push(DownloadTask {
                        label: format!("下载库 {}", lib.name),
                        urls: artifact.url.clone(),
                        dest,
                        sha1: artifact.sha1.clone(),
                    });
                }
            }
        }

        if let Some(classifiers) = lib.downloads.as_ref().and_then(|downloads| downloads.classifiers.as_ref()) {
            let native_classifier = lib
                .natives
                .get(current_os)
                .cloned()
                .unwrap_or_else(|| get_native_classifier(current_os))
                .replace("${arch}", if cfg!(target_pointer_width = "64") { "64" } else { "32" });
            if let Some(native_entry) = classifiers.get(&native_classifier) {
                if let Some(rel_path) = resolve_download_path(native_entry) {
                    let dest = libraries_dir.join(rel_path);
                    if should_download_with_sha1(&dest, &native_entry.sha1)? {
                        tasks.push(DownloadTask {
                            label: format!("下载 natives {}", lib.name),
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

async fn ensure_single_download(
    reporter: &dyn ProgressReporter,
    url: &str,
    dest: &Path,
    stage: &str,
    expected_sha1: &str,
) -> Result<(), String> {
    if should_download_with_sha1(dest, expected_sha1)? {
        reporter.emit(stage, 0, 1);
        download_with_sha1(&url, dest, expected_sha1).await?;
        reporter.emit(stage, 1, 1);
    } else {
        reporter.emit(stage, 1, 1);
    }
    Ok(())
}

async fn execute_download_pool(
    reporter: &dyn ProgressReporter,
    stage: &str,
    tasks: Vec<DownloadTask>,
    concurrency: usize,
) -> Result<(), String> {
    if tasks.is_empty() {
        return Ok(());
    }

    let total = tasks.len();
    let mut completed = 0usize;
    let mut failed = Vec::new();
    let mut pending = stream::iter(
        tasks.into_iter().map(|task| async move { download_task_with_retry(task, 3).await }),
    )
    .buffer_unordered(concurrency.max(1));

    while let Some(result) = pending.next().await {
        completed += 1;
        reporter.emit(stage, completed, total);
        if let Err(e) = result {
            failed.push(e);
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        let failed_count = failed.len();
        let sample = failed.into_iter().take(6).collect::<Vec<_>>().join(" | ");
        Err(format!("{stage} 失败，共 {failed_count} 项，示例: {sample}"))
    }
}

async fn download_task_with_retry(task: DownloadTask, max_retries: u32) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 1..=max_retries {
        match download_with_sha1(&task.urls, &task.dest, &task.sha1).await {
            Ok(()) => return Ok(()),
            Err(e) => last_err = format!("{} 尝试 {attempt}/{max_retries} 失败: {e}", task.label),
        }
    }
    Err(last_err)
}

async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("状态异常: HTTP {}", status.as_u16()));
    }
    let bytes = response.bytes().await.map_err(|e| format!("读取响应失败: {e}"))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    std::fs::write(dest, &bytes).map_err(|e| format!("写入文件失败: {e}"))?;
    Ok(())
}

async fn download_with_sha1(url: &str, dest: &Path, expected_sha1: &str) -> Result<(), String> {
    let url = replace(url);
    download_file(&url, dest).await?;
    if expected_sha1.trim().is_empty() {
        return Ok(());
    }
    match file_matches_sha1(dest, expected_sha1) {
        Ok(true) => Ok(()),
        Ok(false) => Err("SHA1 校验失败".into()),
        Err(e) => Err(e),
    }
}

fn compute_file_sha1(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("打开文件失败: {e}"))?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf).map_err(|e| format!("读取文件失败: {e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn file_matches_sha1(path: &Path, expected_sha1: &str) -> Result<bool, String> {
    if expected_sha1.trim().is_empty() || !path.exists() {
        return Ok(false);
    }
    let actual = compute_file_sha1(path)?;
    Ok(actual.eq_ignore_ascii_case(expected_sha1))
}

fn should_download_with_sha1(path: &Path, expected_sha1: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(true);
    }
    if expected_sha1.trim().is_empty() {
        return Ok(false);
    }
    Ok(!file_matches_sha1(path, expected_sha1)?)
}

// fn extract_installer_version_json(bytes: &[u8], label: &str) -> Result<serde_json::Value, String> {
//     let cursor = Cursor::new(bytes.to_vec());
//     let mut zip = ZipArchive::new(cursor).map_err(|e| format!("解析 {label} installer 失败: {e}"))?;

//     if let Ok(mut file) = zip.by_name("version.json") {
//         let mut content = String::new();
//         file.read_to_string(&mut content)
//             .map_err(|e| format!("读取 {label} version.json 失败: {e}"))?;
//         return serde_json::from_str(&content).map_err(|e| format!("解析 {label} version.json 失败: {e}"));
//     }

//     if let Ok(mut file) = zip.by_name("install_profile.json") {
//         let mut content = String::new();
//         file.read_to_string(&mut content)
//             .map_err(|e| format!("读取 {label} install_profile.json 失败: {e}"))?;
//         let profile: ForgeInstallProfile = serde_json::from_str(&content)
//             .map_err(|e| format!("解析 {label} install_profile.json 失败: {e}"))?;
//         if let Some(version_info) = profile.version_info {
//             return Ok(version_info);
//         }
//     }
//     Err(format!("{label} installer 中未找到可用的 version.json"))
// }

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

fn installer_urls(loader: LoaderKind, mc_version: &str, loader_version: &str) -> Result<String, String> {
    match loader {
        LoaderKind::Forge => {
            let version = format!("{}-{}", mc_version, loader_version.trim());
            let official = format!("{FORGE_MAVEN_BASE}/net/minecraftforge/forge/{0}/forge-{0}-installer.jar", version);
            Ok(official)
        }
        LoaderKind::NeoForge => {
            let version = loader_version.trim();
            let official = format!("{NEOFORGE_MAVEN_BASE}/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar", version);
            Ok(official)
        }
        _ => Err("当前 loader 不支持 installer".into()),
    }
}

fn generated_client_rel_path(loader: LoaderKind, mc_version: &str, loader_version: &str) -> Option<PathBuf> {
    match loader {
        LoaderKind::Forge => {
            let version = format!("{}-{}", mc_version, loader_version.trim());
            Some(PathBuf::from(format!("net/minecraftforge/forge/{0}/forge-{0}-client.jar", version)))
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
) -> Result<bool, String> {
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

fn walk_files_recursive(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(root)
        .map_err(|e| format!("读取目录失败 ({}): {e}", root.display()))?
    {
        let entry = entry.map_err(|e| format!("读取目录项失败 ({}): {e}", root.display()))?;
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
) -> Result<HashSet<PathBuf>, String> {
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
                    .map_err(|e| format!("计算运行时库相对路径失败: {e}"))?;
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
) -> Result<(), String> {
    std::fs::create_dir_all(dest_root)
        .map_err(|e| format!("创建目录失败 ({}): {e}", dest_root.display()))?;
    for rel_path in allowed {
        let src = src_root.join(rel_path);
        if !src.is_file() {
            continue;
        }
        let dest = dest_root.join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录失败 ({}): {e}", parent.display()))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| {
            format!(
                "复制运行时库失败 ({} -> {}): {e}",
                src.display(),
                dest.display()
            )
        })?;
    }
    Ok(())
}

fn installer_cache_path(layout: &RuntimeLayout, loader: LoaderKind, mc_version: &str, loader_version: &str) -> PathBuf {
    let filename = match loader {
        LoaderKind::Forge => format!("forge-{}-{}-installer.jar", mc_version, loader_version.trim()),
        LoaderKind::NeoForge => format!("neoforge-{}-installer.jar", loader_version.trim()),
        LoaderKind::Fabric => format!("fabric-{}-{}-installer.jar", mc_version, loader_version.trim()),
        LoaderKind::Vanilla => format!("minecraft-{}-installer.jar", mc_version),
    };
    layout.installers_cache_dir.join(filename)
}

fn find_installed_version_json(
    install_dir: &Path,
    expected_id: &str,
) -> Result<PathBuf, String> {
    let versions_root = install_dir.join("versions");
    let entries = std::fs::read_dir(&versions_root)
        .map_err(|e| format!("读取 installer versions 目录失败 ({}): {e}", versions_root.display()))?;
    let mut fallback = None;
    for entry in entries {
        let entry = entry.map_err(|e| format!("读取 installer versions 目录项失败: {e}"))?;
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
    fallback.ok_or_else(|| "installer 未生成版本 json".into())
}

async fn ensure_loader_runtime_from_installer(
    layout: &RuntimeLayout,
    request: &RuntimeRequest,
    reporter: &dyn ProgressReporter,
) -> Result<serde_json::Value, String> {
    let loader_version = request
        .loader_version.as_ref().ok_or("err")?.trim();

    let spec = loader_installer_spec(request.loader).ok_or_else(|| "当前 loader 不支持 installer".to_string())?;

    let version_json_path = layout.versions_dir.join("version.json");
    let generated_client_path = generated_client_rel_path(request.loader, &request.mc_version, loader_version)
        .map(|rel| layout.libraries_dir.join(rel))
        .ok_or_else(|| "无法确定 loader client 产物路径".to_string())?;
    let expected_id = match request.loader {
        LoaderKind::Forge => format!("{}-forge-{}", request.mc_version, loader_version),
        LoaderKind::NeoForge => format!("neoforge-{}", loader_version),
        _ => String::new(),
    };

    if generated_client_path.is_file() && version_json_path.is_file() {
        let cached: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&version_json_path)
                .map_err(|e| format!("读取缓存 version.json 失败: {e}"))?,
        )
        .map_err(|e| format!("解析缓存 version.json 失败: {e}"))?;
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

    let installer_path = installer_cache_path(layout, request.loader, &request.mc_version, loader_version);
    if !installer_path.is_file() {
        let primary = installer_urls(request.loader, &request.mc_version, loader_version)?;
        reporter.emit("下载 loader installer", 0, 1);
        download_with_sha1(&primary, &installer_path, "").await?;
        reporter.emit("下载 loader installer", 1, 1);
    } else {
        reporter.emit("下载 loader installer", 1, 1);
    }

    let temp_dir = layout.temp_root.join(format!("installer-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("创建临时 installer 目录失败: {e}"))?;
    std::fs::write(temp_dir.join("launcher_profiles.json"), r#"{"profiles":{},"settings":{},"version":3}"#)
        .map_err(|e| format!("写入 launcher_profiles.json 失败: {e}"))?;

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
    .map_err(|e| format!("执行 {label} installer 任务失败: {e}"))?
    .map_err(|e| format!("启动 {label} installer 失败: {e}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = [stdout.trim(), stderr.trim()]
            .into_iter()
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(format!("{label} installer 执行失败: {}", if detail.is_empty() { "未返回可用日志".to_string() } else { detail }));
    }

    let installed_version_json_path = find_installed_version_json(&temp_dir, &expected_id)?;
    let installed_version_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&installed_version_json_path)
            .map_err(|e| format!("读取 installer version.json 失败: {e}"))?,
    )
    .map_err(|e| format!("解析 installer version.json 失败: {e}"))?;

    if layout.libraries_dir.exists() {
        std::fs::remove_dir_all(&layout.libraries_dir)
            .map_err(|e| format!("清理旧 libraries 目录失败: {e}"))?;
    }
    let temp_libraries_dir = temp_dir.join("libraries");
    let allowed_runtime_paths = collect_runtime_library_paths(
        &temp_libraries_dir,
        &installed_version_json,
        generated_client_rel_path(request.loader, &request.mc_version, loader_version)
            .as_deref()
            .ok_or_else(|| "无法确定 loader client 相对路径".to_string())?,
    )?;
    copy_selected_library_paths(
        &temp_libraries_dir,
        &layout.libraries_dir,
        &allowed_runtime_paths,
    )?;

    if !generated_client_path.is_file() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(format!("{label} installer 执行完成，但缺少生成产物: {}", generated_client_path.display()));
    }

    reporter.emit("生成 loader 运行时", 1, 1);
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(installed_version_json)
}
