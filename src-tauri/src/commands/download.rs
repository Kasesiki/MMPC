use std::collections::HashMap;
use std::path::{Path, PathBuf};

use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::Emitter;

use super::settings::load_settings;
use super::workspace::PackConfig;

// ─── Version manifest (from launchermeta) ───

#[derive(Debug, Serialize, Deserialize)]
struct VersionManifest {
    latest: Latest,
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Latest {
    release: String,
    snapshot: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VersionEntry {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
    url: String,
    time: String,
    #[serde(rename = "releaseTime")]
    release_time: String,
}

// ─── Version JSON (per-version metadata) ───

#[derive(Debug, Serialize, Deserialize)]
struct VersionJson {
    downloads: VersionDownloads,
    #[serde(rename = "assetIndex")]
    asset_index: AssetIndex,
    #[serde(default)]
    libraries: Vec<LibraryEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VersionDownloads {
    client: DownloadEntry,
    server: Option<DownloadEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DownloadEntry {
    url: String,
    sha1: String,
    size: u64,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssetIndex {
    id: String,
    url: String,
    sha1: String,
    size: u64,
    #[serde(rename = "totalSize")]
    total_size: u64,
}

// ─── Library entry (from version.json libraries[]) ───

#[derive(Debug, Serialize, Deserialize)]
struct LibraryEntry {
    name: String,
    #[serde(default)]
    downloads: Option<LibraryDownloads>,
    #[serde(default)]
    rules: Vec<Rule>,
    #[serde(default)]
    natives: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LibraryDownloads {
    artifact: Option<DownloadEntry>,
    classifiers: Option<HashMap<String, DownloadEntry>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Rule {
    action: String,
    os: Option<RuleOs>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RuleOs {
    name: Option<String>,
}

// ─── Asset index (from assets/<version>.json) ───

#[derive(Debug, Serialize, Deserialize)]
struct AssetIndexObjects {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Clone)]
struct DownloadTask {
    label: String,
    url: String,
    dest: PathBuf,
    sha1: String,
}

fn resolve_download_path(entry: &DownloadEntry) -> Option<PathBuf> {
    entry.path.as_ref().filter(|p| !p.is_empty()).map(PathBuf::from)
}

fn get_mmpc_dir() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent().map(|p| p.join(".MMPC")).unwrap_or_else(|| std::path::PathBuf::from(".MMPC"))
}

fn versions_dir(id: &str) -> std::path::PathBuf {
    get_mmpc_dir().join("workspaces").join(id).join("versions")
}

fn workspace_dir(id: &str) -> std::path::PathBuf {
    get_mmpc_dir().join("workspaces").join(id)
}

fn pack_config_path(id: &str) -> PathBuf {
    workspace_dir(id).join("pack.json")
}

fn merged_version_json_path(id: &str) -> PathBuf {
    versions_dir(id).join("version.json")
}

fn read_pack_config(id: &str) -> Result<PackConfig, String> {
    let content = std::fs::read_to_string(pack_config_path(id))
        .map_err(|e| format!("读取 pack.json 失败: {e}"))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("解析 pack.json 失败: {e}"))
}

fn emit_task_progress(
    app: &tauri::AppHandle,
    stage: &str,
    current: usize,
    total: usize,
) -> Result<(), String> {
    app.emit(
        "download-progress",
        serde_json::json!({
            "stage": stage,
            "current": current,
            "total": total,
        }),
    )
    .map_err(|e| format!("发送事件失败: {e}"))
}

// ─── Helper: simple file download ───

async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;
    std::fs::write(dest, &bytes)
        .map_err(|e| format!("写入文件失败: {}", e))?;
    Ok(())
}

async fn download_with_sha1(url: &str, dest: &Path, expected_sha1: &str) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败: {e}"))?;
    }
    download_file(url, dest).await?;
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
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("打开文件失败: {}", e))?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)
            .map_err(|e| format!("读取文件失败: {}", e))?;
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

async fn download_task_with_retry(task: DownloadTask, max_retries: u32) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 1..=max_retries {
        match download_with_sha1(&task.url, &task.dest, &task.sha1).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = format!("{} 尝试 {attempt}/{max_retries} 失败: {e}", task.label);
            }
        }
    }
    Err(last_err)
}

async fn execute_download_pool(
    app: &tauri::AppHandle,
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
    let mut pending = stream::iter(tasks.into_iter().map(|task| async move {
        download_task_with_retry(task, 3).await
    }))
    .buffer_unordered(concurrency.max(1));

    while let Some(result) = pending.next().await {
        completed += 1;
        emit_task_progress(app, stage, completed, total).ok();

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

async fn download_single_task(
    app: &tauri::AppHandle,
    url: &str,
    dest: &Path,
    stage: &str,
    expected_sha1: &str,
) -> Result<(), String> {
    emit_task_progress(app, stage, 0, 1)?;
    download_with_sha1(url, dest, expected_sha1)
        .await
        .map_err(|e| format!("{stage} 失败: {e}"))?;
    emit_task_progress(app, stage, 1, 1)?;
    Ok(())
}

// ─── OS helpers for library rule evaluation ───

/// Detect current OS name matching Mojang's convention
fn detect_os() -> String {
    match std::env::consts::OS {
        "macos" => "osx".to_string(),
        other => other.to_string(),
    }
}

/// Evaluate a list of rules to determine if a library should be included.
/// Returns true if the library should be used on this OS.
fn evaluate_rules(rules: &[Rule], current_os: &str) -> bool {
    if rules.is_empty() {
        return true; // no rules = always included
    }
    // Default is disallow; rules grant or deny access
    let mut allowed = false;
    for rule in rules {
        let matches_os = match &rule.os {
            Some(os) => os.name.as_ref().map_or(true, |n| n == current_os),
            None => true, // no OS filter means applies to all
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

/// Get the native classifier string for the current OS
fn get_native_classifier(current_os: &str) -> String {
    let arch = if cfg!(target_arch = "x86_64") { "64" } else { "86" };
    match current_os {
        "windows" => format!("natives-windows-{}", if arch == "64" { "64" } else { "32" }),
        "osx" => "natives-osx".to_string(),
        "linux" => format!("natives-linux-{}", arch),
        _ => format!("natives-{}", current_os),
    }
}

async fn resolve_workspace_version_metadata(
    workspace_id: &str,
    mc_version: &str,
) -> Result<(VersionJson, serde_json::Value), String> {
    let pack = read_pack_config(workspace_id)?;
    let loader_type = pack.loader_type.trim().to_lowercase();
    let version_json_path = merged_version_json_path(workspace_id);
    if version_json_path.exists() {
        let cached = std::fs::read_to_string(&version_json_path)
            .map_err(|e| format!("读取缓存 version.json 失败: {e}"))?;
        let cached_value: serde_json::Value = serde_json::from_str(&cached)
            .map_err(|e| format!("解析缓存 version.json 失败: {e}"))?;
        let version_json: VersionJson = serde_json::from_value(cached_value.clone())
            .map_err(|e| format!("解析缓存启动元数据失败: {e}"))?;
        return Ok((version_json, cached_value));
    }

    if loader_type != "vanilla" {
        return Err(format!(
            "当前工作区使用 {}，但还没有缓存的 version.json。请先为该工作区写入对应 loader 的 version.json 缓存后再启动。",
            loader_type
        ));
    }

    let manifest_url = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
    let manifest: VersionManifest = reqwest::get(manifest_url)
        .await
        .map_err(|e| format!("获取版本清单失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析版本清单失败: {}", e))?;
    let entry = manifest.versions.iter()
        .find(|v| v.id == mc_version)
        .ok_or_else(|| format!("未找到 MC 版本 {}", mc_version))?;

    let base_version_content = reqwest::get(&entry.url)
        .await
        .map_err(|e| format!("下载原版 version.json 失败: {}", e))?
        .text()
        .await
        .map_err(|e| format!("读取原版 version.json 失败: {}", e))?;
    let base_value: serde_json::Value = serde_json::from_str(&base_version_content)
        .map_err(|e| format!("解析原版 version.json 失败: {}", e))?;
    let merged_content = serde_json::to_string_pretty(&base_value)
        .map_err(|e| format!("序列化工作区 version.json 失败: {e}"))?;
    std::fs::write(&version_json_path, &merged_content)
        .map_err(|e| format!("写入工作区 version.json 失败: {e}"))?;

    let version_json: VersionJson = serde_json::from_value(base_value.clone())
        .map_err(|e| format!("解析工作区 version.json 失败: {}", e))?;
    Ok((version_json, base_value))
}

#[tauri::command]
pub async fn download_mc_version(
    app: tauri::AppHandle,
    workspace_id: String,
    mc_version: String,
) -> Result<String, String> {
    ensure_workspace_runtime(&app, &workspace_id, &mc_version).await
}

pub async fn ensure_workspace_runtime(
    app: &tauri::AppHandle,
    workspace_id: &str,
    mc_version: &str,
) -> Result<String, String> {
    let ver_dir = versions_dir(&workspace_id);
    std::fs::create_dir_all(&ver_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    let settings = load_settings().unwrap_or_default();
    let download_pool_size = settings.download_pool_size.max(1);

    let (version_json, merged_version_json) = resolve_workspace_version_metadata(workspace_id, mc_version).await?;
    // 5. Download client.jar
    let client_url = &version_json.downloads.client.url;
    let client_path = ver_dir.join("client.jar");
    if should_download_with_sha1(&client_path, &version_json.downloads.client.sha1)? {
        download_single_task(app, client_url, &client_path, "下载 client.jar", &version_json.downloads.client.sha1).await?;
    } else {
        emit_task_progress(app, "下载 client.jar", 1, 1).ok();
    }

    // 6. Download & extract asset index
    let asset_index_url = &version_json.asset_index.url;
    let ai_path = ver_dir.join("asset_index.json");
    if should_download_with_sha1(&ai_path, &version_json.asset_index.sha1)? {
        download_single_task(app, asset_index_url, &ai_path, "下载 asset index", &version_json.asset_index.sha1).await?;
    } else {
        emit_task_progress(app, "下载 asset index", 1, 1).ok();
    }

    // 7. Download libraries
    let libs_dir = ver_dir.join("libraries");
    let current_os = detect_os();
    let mut library_tasks = Vec::new();

    for lib in &version_json.libraries {
        if !evaluate_rules(&lib.rules, &current_os) {
            continue;
        }

        let Some(downloads) = lib.downloads.as_ref() else {
            continue;
        };

        // Main artifact (the library JAR)
        if let Some(artifact) = downloads.artifact.as_ref() {
            if let Some(rel_path) = resolve_download_path(artifact) {
                let lib_path = libs_dir.join(rel_path);
                if should_download_with_sha1(&lib_path, &artifact.sha1)? {
                    library_tasks.push(DownloadTask {
                        label: format!("下载库 {}", lib.name),
                        url: artifact.url.clone(),
                        dest: lib_path,
                        sha1: artifact.sha1.clone(),
                    });
                }
            }
        }

        // Classifiers (natives for current OS)
        if let Some(classifiers) = downloads.classifiers.as_ref() {
            let native_classifier = lib
                .natives
                .get(&current_os)
                .cloned()
                .unwrap_or_else(|| get_native_classifier(&current_os))
                .replace("${arch}", if cfg!(target_pointer_width = "64") { "64" } else { "32" });
            if let Some(native_entry) = classifiers.get(&native_classifier) {
                if let Some(rel_path) = resolve_download_path(native_entry) {
                    let native_path = libs_dir.join(rel_path);
                    if should_download_with_sha1(&native_path, &native_entry.sha1)? {
                        library_tasks.push(DownloadTask {
                            label: format!("下载 natives {}", lib.name),
                            url: native_entry.url.clone(),
                            dest: native_path,
                            sha1: native_entry.sha1.clone(),
                        });
                    }
                }
            }
        }
    }
    execute_download_pool(&app, "下载 libraries", library_tasks, download_pool_size).await?;

    // 8. Download assets
    let assets_root = get_mmpc_dir().join("assets");
    let assets_indexes_dir = assets_root.join("indexes");
    let assets_base = assets_root.join("objects");
    std::fs::create_dir_all(&assets_indexes_dir)
        .map_err(|e| format!("创建 assets/indexes 目录失败: {}", e))?;
    let ai_content = std::fs::read_to_string(&ai_path)
        .map_err(|e| format!("读取 asset index 失败: {}", e))?;
    let asset_index: AssetIndexObjects = serde_json::from_str(&ai_content)
        .map_err(|e| format!("解析 asset index 失败: {}", e))?;
    let asset_index_dest = assets_indexes_dir.join(format!("{}.json", version_json.asset_index.id));
    if should_download_with_sha1(&asset_index_dest, &version_json.asset_index.sha1)? {
        std::fs::copy(&ai_path, &asset_index_dest)
            .map_err(|e| format!("写入 assets index 失败: {}", e))?;
    }

    let mut asset_tasks = Vec::new();

    for (_, obj) in &asset_index.objects {
        let hash = &obj.hash;
        let sub_dir = &hash[..2];
        let asset_path = assets_base.join(sub_dir).join(hash);
        let should_download = !file_matches_sha1(&asset_path, hash).unwrap_or(false);

        if should_download {
            asset_tasks.push(DownloadTask {
                label: format!("下载 asset {}", hash),
                url: format!("https://resources.download.minecraft.net/{}/{}", sub_dir, hash),
                dest: asset_path,
                sha1: hash.clone(),
            });
        }
    }
    execute_download_pool(&app, "下载 assets", asset_tasks, download_pool_size).await?;

    // 9. Create assets symlink in workspace versions dir (best effort)
    let ws_assets_link = ver_dir.join("assets");
    if !ws_assets_link.exists() {
        #[cfg(unix)]
        if let Err(e) = std::os::unix::fs::symlink(&assets_root, &ws_assets_link) {
            eprintln!("[mmpc] 创建 assets 链接失败: {e}");
        }
        #[cfg(windows)]
        if let Err(e) = std::os::windows::fs::symlink_dir(&assets_root, &ws_assets_link) {
            eprintln!("[mmpc] 创建 assets 链接失败: {e}");
        }
    }

    emit_task_progress(app, "完成", 1, 1).ok();

    let version_id = merged_version_json["id"].as_str().unwrap_or(mc_version);
    Ok(format!("MC {} 数据校验完成（version: {}）", mc_version, version_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that the version manifest can be fetched and parsed
    #[tokio::test]
    async fn test_fetch_manifest() {
        let url = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
        let resp = reqwest::get(url).await.expect("HTTP request failed");
        assert!(resp.status().is_success(), "HTTP {}", resp.status());

        let manifest: VersionManifest = resp.json().await.expect("Deserialize manifest");
        assert!(!manifest.versions.is_empty(), "No versions in manifest");
        assert!(!manifest.latest.release.is_empty());

        // Verify 1.21 exists
        let v121 = manifest.versions.iter().find(|v| v.id == "1.21");
        assert!(v121.is_some(), "1.21 not found in manifest");
        assert!(!v121.unwrap().url.is_empty(), "version URL is empty");
    }

    /// Test that a version JSON (1.21) can be fetched and decoded
    #[tokio::test]
    async fn test_fetch_version_json() {
        // First get manifest to find the URL
        let manifest: VersionManifest = reqwest::get(
            "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
        )
        .await
        .expect("manifest request")
        .json()
        .await
        .expect("manifest parse");

        let entry = manifest.versions.iter().find(|v| v.id == "1.21")
            .expect("1.21 not found");

        // Fetch version JSON
        let vjson: VersionJson = reqwest::get(&entry.url)
            .await
            .expect("version JSON request")
            .json()
            .await
            .expect("version JSON parse");

        assert!(!vjson.downloads.client.url.is_empty(), "client URL empty");
        assert!(vjson.downloads.client.size > 0, "client size is 0");
        assert!(!vjson.asset_index.url.is_empty(), "asset index URL empty");
        assert!(vjson.asset_index.total_size > 0, "total_size is 0");
    }

    /// Test that a snapshot version also works
    #[tokio::test]
    async fn test_fetch_snapshot() {
        let manifest: VersionManifest = reqwest::get(
            "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
        )
        .await
        .expect("manifest request")
        .json()
        .await
        .expect("manifest parse");

        // Find a snapshot version
        let snapshot = manifest.versions.iter()
            .find(|v| v.version_type == "snapshot");

        if let Some(entry) = snapshot {
            let vjson: VersionJson = reqwest::get(&entry.url)
                .await
                .expect("snapshot JSON request")
                .json()
                .await
                .expect("snapshot JSON parse");

            assert!(!vjson.downloads.client.url.is_empty());
            assert!(vjson.asset_index.total_size > 0);
        }
        // If no snapshot, just pass (between releases)
    }
}
