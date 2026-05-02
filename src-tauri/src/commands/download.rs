use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::Emitter;
use zip::read::ZipArchive;

use super::settings::load_settings;
use super::workspace::{find_version_manifest_entry, PackConfig};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct ForgeInstallProfile {
    #[serde(rename = "versionInfo", default)]
    version_info: Option<serde_json::Value>,
}

fn resolve_download_path(entry: &DownloadEntry) -> Option<PathBuf> {
    entry
        .path
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

fn get_mmpc_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}

fn versions_dir(id: &str) -> PathBuf {
    get_mmpc_dir().join("workspaces").join(id).join("versions")
}

fn workspace_dir(id: &str) -> PathBuf {
    get_mmpc_dir().join("workspaces").join(id)
}

fn pack_config_path(id: &str) -> PathBuf {
    workspace_dir(id).join("pack.json")
}

fn merged_version_json_path(id: &str) -> PathBuf {
    versions_dir(id).join("version.json")
}

fn inherited_version_json_path(id: &str, version_id: &str) -> PathBuf {
    versions_dir(id).join(format!("{version_id}.json"))
}

fn normalize_loader_type(loader_type: &str) -> &str {
    match loader_type.trim().to_lowercase().as_str() {
        "fabric" => "fabric",
        "forge" => "forge",
        "neoforge" => "neoforge",
        _ => "vanilla",
    }
}

fn read_pack_config(id: &str) -> Result<PackConfig, String> {
    let content = std::fs::read_to_string(pack_config_path(id))
        .map_err(|e| format!("读取 pack.json 失败: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 pack.json 失败: {e}"))
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

async fn fetch_json_value(url: &str, label: &str) -> Result<serde_json::Value, String> {
    reqwest::get(url)
        .await
        .map_err(|e| format!("{label} 请求失败: {e}"))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("{label} 解析失败: {e}"))
}

async fn fetch_bytes(url: &str, label: &str) -> Result<Vec<u8>, String> {
    reqwest::get(url)
        .await
        .map_err(|e| format!("{label} 请求失败: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("{label} 读取失败: {e}"))
        .map(|bytes| bytes.to_vec())
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
                            parent_array.extend(child_array);
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
        (parent_value, child_value) => {
            *parent_value = child_value.clone();
        }
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

async fn fetch_vanilla_version_value(mc_version: &str) -> Result<serde_json::Value, String> {
    let entry = find_version_manifest_entry(mc_version).await?;
    fetch_json_value(&entry.url, "下载原版 version.json").await
}

async fn fetch_fabric_version_value(
    mc_version: &str,
    loader_version: &str,
) -> Result<serde_json::Value, String> {
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
        mc_version,
        loader_version.trim()
    );
    fetch_json_value(&url, "下载 Fabric version.json").await
}

fn extract_installer_version_json(bytes: &[u8], label: &str) -> Result<serde_json::Value, String> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut zip =
        ZipArchive::new(cursor).map_err(|e| format!("解析 {label} installer 失败: {e}"))?;

    if let Ok(mut file) = zip.by_name("version.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("读取 {label} version.json 失败: {e}"))?;
        return serde_json::from_str(&content)
            .map_err(|e| format!("解析 {label} version.json 失败: {e}"));
    }

    if let Ok(mut file) = zip.by_name("install_profile.json") {
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| format!("读取 {label} install_profile.json 失败: {e}"))?;
        let profile: ForgeInstallProfile = serde_json::from_str(&content)
            .map_err(|e| format!("解析 {label} install_profile.json 失败: {e}"))?;
        if let Some(version_info) = profile.version_info {
            return Ok(version_info);
        }
    }

    Err(format!("{label} installer 中未找到可用的 version.json"))
}

async fn fetch_forge_version_value(
    mc_version: &str,
    loader_version: &str,
) -> Result<serde_json::Value, String> {
    let forge_version = format!("{}-{}", mc_version, loader_version.trim());
    let url = format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{0}/forge-{0}-installer.jar",
        forge_version
    );
    let bytes = fetch_bytes(&url, "下载 Forge installer").await?;
    extract_installer_version_json(&bytes, "Forge")
}

async fn fetch_neoforge_version_value(loader_version: &str) -> Result<serde_json::Value, String> {
    let version = loader_version.trim();
    let url = format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar",
        version
    );
    let bytes = fetch_bytes(&url, "下载 NeoForge installer").await?;
    extract_installer_version_json(&bytes, "NeoForge")
}

fn write_cached_version_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| format!("序列化工作区 version.json 失败: {e}"))?;
    std::fs::write(path, content).map_err(|e| format!("写入工作区 version.json 失败: {e}"))
}

async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;
    std::fs::write(dest, &bytes).map_err(|e| format!("写入文件失败: {e}"))?;
    Ok(())
}

async fn download_with_sha1(url: &str, dest: &Path, expected_sha1: &str) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
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
    let mut file = std::fs::File::open(path).map_err(|e| format!("打开文件失败: {e}"))?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 8192];
    loop {
        let n =
            std::io::Read::read(&mut file, &mut buf).map_err(|e| format!("读取文件失败: {e}"))?;
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
    let mut pending = stream::iter(
        tasks
            .into_iter()
            .map(|task| async move { download_task_with_retry(task, 3).await }),
    )
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
        Err(format!(
            "{stage} 失败，共 {failed_count} 项，示例: {sample}"
        ))
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

fn detect_os() -> String {
    match std::env::consts::OS {
        "macos" => "osx".to_string(),
        other => other.to_string(),
    }
}

fn evaluate_rules(rules: &[Rule], current_os: &str) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let matches_os = match &rule.os {
            Some(os) => os.name.as_ref().map_or(true, |n| n == current_os),
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
    let base = base_url.unwrap_or("https://libraries.minecraft.net/");
    let normalized_base = if base.ends_with('/') {
        base.to_string()
    } else {
        format!("{base}/")
    };

    Some(DownloadEntry {
        url: format!("{normalized_base}{relative_path}"),
        sha1: String::new(),
        size: 0,
        path: Some(relative_path),
    })
}

async fn resolve_workspace_version_metadata(
    workspace_id: &str,
    mc_version: &str,
) -> Result<(VersionJson, serde_json::Value), String> {
    let pack = read_pack_config(workspace_id)?;
    let loader_type = normalize_loader_type(&pack.loader_type);
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

    let base_value = fetch_vanilla_version_value(mc_version).await?;
    write_cached_version_json(
        &inherited_version_json_path(workspace_id, mc_version),
        &base_value,
    )?;

    let merged_value = match loader_type {
        "fabric" => {
            let loader_version = pack
                .loader_version
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "当前工作区使用 Fabric，但未填写 loader_version".to_string())?;
            let loader_value = fetch_fabric_version_value(mc_version, loader_version).await?;
            merge_version_json(&base_value, &loader_value)
        }
        "forge" => {
            let loader_version = pack
                .loader_version
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "当前工作区使用 Forge，但未填写 loader_version".to_string())?;
            let loader_value = fetch_forge_version_value(mc_version, loader_version).await?;
            merge_version_json(&base_value, &loader_value)
        }
        "neoforge" => {
            let loader_version = pack
                .loader_version
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "当前工作区使用 NeoForge，但未填写 loader_version".to_string())?;
            let loader_value = fetch_neoforge_version_value(loader_version).await?;
            merge_version_json(&base_value, &loader_value)
        }
        _ => base_value,
    };

    write_cached_version_json(&version_json_path, &merged_value)?;

    let version_json: VersionJson = serde_json::from_value(merged_value.clone())
        .map_err(|e| format!("解析工作区 version.json 失败: {e}"))?;
    Ok((version_json, merged_value))
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
    let ver_dir = versions_dir(workspace_id);
    std::fs::create_dir_all(&ver_dir).map_err(|e| format!("创建目录失败: {e}"))?;
    let settings = load_settings().unwrap_or_default();
    let download_pool_size = settings.download_pool_size.max(1);

    let (version_json, merged_version_json) =
        resolve_workspace_version_metadata(workspace_id, mc_version).await?;

    let client_url = &version_json.downloads.client.url;
    let client_path = ver_dir.join("client.jar");
    if should_download_with_sha1(&client_path, &version_json.downloads.client.sha1)? {
        download_single_task(
            app,
            client_url,
            &client_path,
            "下载 client.jar",
            &version_json.downloads.client.sha1,
        )
        .await?;
    } else {
        emit_task_progress(app, "下载 client.jar", 1, 1).ok();
    }

    let asset_index_url = &version_json.asset_index.url;
    let ai_path = ver_dir.join("asset_index.json");
    if should_download_with_sha1(&ai_path, &version_json.asset_index.sha1)? {
        download_single_task(
            app,
            asset_index_url,
            &ai_path,
            "下载 asset index",
            &version_json.asset_index.sha1,
        )
        .await?;
    } else {
        emit_task_progress(app, "下载 asset index", 1, 1).ok();
    }

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

        let artifact_download = downloads
            .artifact
            .clone()
            .or_else(|| build_library_download_from_name(&lib.name, lib.url.as_deref()));
        if let Some(artifact) = artifact_download.as_ref() {
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

        if let Some(classifiers) = downloads.classifiers.as_ref() {
            let native_classifier = lib
                .natives
                .get(&current_os)
                .cloned()
                .unwrap_or_else(|| get_native_classifier(&current_os))
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
    execute_download_pool(app, "下载 libraries", library_tasks, download_pool_size).await?;

    let assets_root = get_mmpc_dir().join("assets");
    let assets_indexes_dir = assets_root.join("indexes");
    let assets_base = assets_root.join("objects");
    std::fs::create_dir_all(&assets_indexes_dir)
        .map_err(|e| format!("创建 assets/indexes 目录失败: {e}"))?;
    let ai_content =
        std::fs::read_to_string(&ai_path).map_err(|e| format!("读取 asset index 失败: {e}"))?;
    let asset_index: AssetIndexObjects =
        serde_json::from_str(&ai_content).map_err(|e| format!("解析 asset index 失败: {e}"))?;
    let asset_index_dest = assets_indexes_dir.join(format!("{}.json", version_json.asset_index.id));
    if should_download_with_sha1(&asset_index_dest, &version_json.asset_index.sha1)? {
        std::fs::copy(&ai_path, &asset_index_dest)
            .map_err(|e| format!("写入 assets index 失败: {e}"))?;
    }

    let mut asset_tasks = Vec::new();
    for obj in asset_index.objects.values() {
        let hash = &obj.hash;
        let sub_dir = &hash[..2];
        let asset_path = assets_base.join(sub_dir).join(hash);
        let should_download = !file_matches_sha1(&asset_path, hash).unwrap_or(false);
        if should_download {
            asset_tasks.push(DownloadTask {
                label: format!("下载 asset {hash}"),
                url: format!(
                    "https://resources.download.minecraft.net/{}/{}",
                    sub_dir, hash
                ),
                dest: asset_path,
                sha1: hash.clone(),
            });
        }
    }
    execute_download_pool(app, "下载 assets", asset_tasks, download_pool_size).await?;

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
    Ok(format!(
        "MC {} 数据校验完成（version: {}）",
        mc_version, version_id
    ))
}
