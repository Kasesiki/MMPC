use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::Emitter;

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

async fn download_asset_with_retry(
    url: &str,
    dest: &Path,
    expected_size: u64,
    expected_sha1: &str,
    max_retries: u32,
) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 1..=max_retries {
        if let Err(e) = download_file(url, dest).await {
            last_err = format!("尝试 {attempt}/{max_retries} 失败: {e}");
            continue;
        }
        if !expected_sha1.is_empty() {
            match file_matches_sha1(dest, expected_sha1) {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    last_err = format!("尝试 {attempt}/{max_retries} SHA1 校验失败");
                    continue;
                }
                Err(e) => {
                    last_err = format!("尝试 {attempt}/{max_retries} SHA1 校验失败: {e}");
                    continue;
                }
            }
        }
        match std::fs::metadata(dest) {
            Ok(meta) if meta.len() == expected_size => return Ok(()),
            Ok(meta) => {
                last_err = format!(
                    "尝试 {attempt}/{max_retries} 文件大小不匹配，期望 {} 实际 {}",
                    expected_size,
                    meta.len()
                );
            }
            Err(e) => {
                last_err = format!("尝试 {attempt}/{max_retries} 校验文件失败: {e}");
            }
        }
    }
    Err(last_err)
}

// ─── Helper: download with progress events ───

async fn download_file_with_progress(
    app: &tauri::AppHandle,
    url: &str,
    dest: &Path,
    stage: &str,
) -> Result<(), String> {
    use futures_util::StreamExt;

    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("{} 请求失败: {}", stage, e))?;

    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut buffer = Vec::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("{} 读取失败: {}", stage, e))?;
        downloaded += chunk.len() as u64;
        buffer.extend_from_slice(&chunk);

        if total_size > 0 {
            let pct = (downloaded as f64 / total_size as f64 * 100.0) as u32;
            app.emit("download-progress", serde_json::json!({
                "stage": stage,
                "progress": pct,
                "downloaded": downloaded,
                "total": total_size,
            })).ok();
        }
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败: {}", e))?;
    }
    std::fs::write(dest, &buffer)
        .map_err(|e| format!("{} 保存失败: {}", stage, e))?;
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

#[tauri::command]
pub async fn download_mc_version(
    app: tauri::AppHandle,
    workspace_id: String,
    mc_version: String,
) -> Result<String, String> {
    let ver_dir = versions_dir(&workspace_id);
    std::fs::create_dir_all(&ver_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    // 1. Fetch version manifest
    let manifest_url = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
    let manifest: VersionManifest = reqwest::get(manifest_url)
        .await
        .map_err(|e| format!("获取版本清单失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析版本清单失败: {}", e))?;

    // 2. Find matching version
    let entry = manifest.versions.iter()
        .find(|v| v.id == mc_version)
        .ok_or_else(|| format!("未找到 MC 版本 {}", mc_version))?;

    // 3. Download version JSON
    let version_json: VersionJson = reqwest::get(&entry.url)
        .await
        .map_err(|e| format!("获取版本 JSON 失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("解析版本 JSON 失败: {}", e))?;

    // 4. Save version JSON
    let vjson_path = ver_dir.join("version.json");
    let vjson_bytes = reqwest::get(&entry.url)
        .await
        .map_err(|e| format!("下载 version.json 失败: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("读取 version.json 失败: {}", e))?;
    std::fs::write(&vjson_path, &vjson_bytes)
        .map_err(|e| format!("保存 version.json 失败: {}", e))?;

    // 5. Download client.jar
    app.emit("download-progress", serde_json::json!({"stage": "下载 client.jar", "progress": 0}))
        .map_err(|e| format!("发送事件失败: {}", e))?;

    let client_url = &version_json.downloads.client.url;
    let client_path = ver_dir.join("client.jar");
    if should_download_with_sha1(&client_path, &version_json.downloads.client.sha1)? {
        download_file_with_progress(&app, client_url, &client_path, "下载 client.jar").await?;
    } else {
        app.emit("download-progress", serde_json::json!({"stage": "下载 client.jar", "progress": 100}))
            .ok();
    }

    // 6. Download & extract asset index
    let asset_index_url = &version_json.asset_index.url;
    let ai_path = ver_dir.join("asset_index.json");
    if should_download_with_sha1(&ai_path, &version_json.asset_index.sha1)? {
        download_file_with_progress(&app, asset_index_url, &ai_path, "下载 asset index").await?;
    } else {
        app.emit("download-progress", serde_json::json!({"stage": "下载 asset index", "progress": 100}))
            .ok();
    }

    // 7. Download libraries
    app.emit("download-progress", serde_json::json!({"stage": "下载 libraries", "progress": 0}))
        .map_err(|e| format!("发送事件失败: {}", e))?;

    let libs_dir = ver_dir.join("libraries");
    let current_os = detect_os();
    let lib_count = version_json
        .libraries
        .iter()
        .filter(|lib| evaluate_rules(&lib.rules, &current_os))
        .count() as u64;
    let mut downloaded_libs = 0u64;

    for lib in &version_json.libraries {
        // Check rules (e.g. exclude based on OS)
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
                    if let Some(parent) = lib_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|e| format!("创建库目录失败: {}", e))?;
                    }
                    download_file(&artifact.url, &lib_path).await
                        .map_err(|e| format!("下载库 {} 失败: {}", lib.name, e))?;
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
                        if let Some(parent) = native_path.parent() {
                            std::fs::create_dir_all(parent)
                                .map_err(|e| format!("创建 natives 目录失败: {}", e))?;
                        }
                        download_file(&native_entry.url, &native_path).await
                            .map_err(|e| format!("下载 natives {} 失败: {}", lib.name, e))?;
                    }
                }
            }
        }

        downloaded_libs += 1;
        if lib_count > 0 {
            let pct = (downloaded_libs as f64 / lib_count as f64 * 100.0) as u32;
            app.emit("download-progress", serde_json::json!({"stage": "下载 libraries", "progress": pct, "current": downloaded_libs, "total": lib_count}))
                .ok();
        }
    }
    if lib_count == 0 {
        app.emit("download-progress", serde_json::json!({"stage": "下载 libraries", "progress": 100, "current": 0, "total": 0}))
            .ok();
    }

    // 8. Download assets
    app.emit("download-progress", serde_json::json!({"stage": "下载 assets", "progress": 0}))
        .map_err(|e| format!("发送事件失败: {}", e))?;

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

    let total_objects = asset_index.objects.len();
    let mut downloaded_assets = 0u64;
    let mut failed_assets: Vec<String> = Vec::new();

    for (_, obj) in &asset_index.objects {
        let hash = &obj.hash;
        let sub_dir = &hash[..2];
        let asset_path = assets_base.join(sub_dir).join(hash);
        let should_download = !file_matches_sha1(&asset_path, hash).unwrap_or(false);

        if should_download {
            let url = format!("https://resources.download.minecraft.net/{}/{}", sub_dir, hash);
            if let Some(parent) = asset_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建 asset 目录失败: {}", e))?;
            }
            if let Err(e) = download_asset_with_retry(&url, &asset_path, obj.size, hash, 3).await {
                eprintln!("[mmpc] 下载 asset {} 失败: {}", hash, e);
                failed_assets.push(hash.to_string());
            }
        }

        downloaded_assets += 1;
        if downloaded_assets % 50 == 0 || downloaded_assets == total_objects as u64 {
            let pct = (downloaded_assets as f64 / total_objects as f64 * 100.0) as u32;
            app.emit("download-progress", serde_json::json!({"stage": "下载 assets", "progress": pct, "current": downloaded_assets, "total": total_objects}))
                .ok();
        }
    }

    if !failed_assets.is_empty() {
        let sample = failed_assets.iter().take(8).cloned().collect::<Vec<_>>().join(", ");
        return Err(format!(
            "assets 下载不完整，失败 {}/{}，示例哈希: {}",
            failed_assets.len(),
            total_objects,
            sample
        ));
    }

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

    app.emit("download-progress", serde_json::json!({"stage": "完成", "progress": 100}))
        .ok();

    Ok(format!("MC {} 数据下载完成（包含 libraries、assets）", mc_version))
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
