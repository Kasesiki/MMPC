use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::workspace::{PackConfig, WorkspaceMod};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthVersionSummary {
    pub version_id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub file_name: String,
    pub download_url: String,
    pub size: u64,
    pub sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthProjectHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthProjectHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    icon_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    id: String,
    version_number: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    filename: String,
    url: String,
    size: u64,
    #[serde(default)]
    hashes: ModrinthHashes,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ModrinthHashes {
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModUsageType {
    ClientOnly,
    ServerOnly,
    ClientAndServer,
    DevelopmentOnly,
    #[default]
    Unknown,
}

impl ModUsageType {
    fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "client_only" => Some(Self::ClientOnly),
            "server_only" => Some(Self::ServerOnly),
            "client_and_server" => Some(Self::ClientAndServer),
            "development_only" => Some(Self::DevelopmentOnly),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ClientOnly => "client_only",
            Self::ServerOnly => "server_only",
            Self::ClientAndServer => "client_and_server",
            Self::DevelopmentOnly => "development_only",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Mod {
    name: String,
    mod_type: ModUsageType,
    #[serde(flatten)]
    project: Map<String, Value>,
}

fn mmpc_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}

fn workspace_dir(id: &str) -> PathBuf {
    mmpc_root().join("workspaces").join(id)
}

fn pack_json_path(id: &str) -> PathBuf {
    workspace_dir(id).join("pack.json")
}

fn modcache_dir() -> PathBuf {
    mmpc_root().join("modcache")
}

fn workspace_mods_dir(id: &str) -> PathBuf {
    workspace_dir(id).join("mods")
}

fn mods_registry_path() -> PathBuf {
    mmpc_root().join("mods.json")
}

fn read_pack_config(id: &str) -> Result<PackConfig, String> {
    let content = std::fs::read_to_string(pack_json_path(id))
        .map_err(|e| format!("读取 pack.json 失败: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 pack.json 失败: {e}"))
}

fn write_pack_config(id: &str, config: &PackConfig) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化 pack.json 失败: {e}"))?;
    std::fs::write(pack_json_path(id), content).map_err(|e| format!("写入 pack.json 失败: {e}"))
}

fn read_mod_registry() -> Vec<Mod> {
    let path = mods_registry_path();
    if !path.exists() {
        return Vec::new();
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str::<Vec<Mod>>(&content).unwrap_or(vec![])
}

fn write_mod_registry(registry: &[Mod]) -> Result<(), String> {
    let path = mods_registry_path();
    let content = serde_json::to_string_pretty(registry)
        .map_err(|e| format!("序列化 mods.json 失败: {e}"))?;
    std::fs::write(path, content).map_err(|e| format!("写入 mods.json 失败: {e}"))
}

fn sanitize_filename_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn build_cached_mod_filename(mod_name: &str, mod_version: &str, mc_version: &str) -> String {
    format!(
        "{}_{}_{}.jar",
        sanitize_filename_component(mod_name),
        sanitize_filename_component(mod_version),
        sanitize_filename_component(mc_version)
    )
}

fn ensure_symlink_or_copy(src: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        let metadata = std::fs::symlink_metadata(dest)
            .map_err(|e| format!("读取 mods 链接失败 ({}): {e}", dest.display()))?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            std::fs::remove_file(dest)
                .map_err(|e| format!("移除旧 mods 文件失败 ({}): {e}", dest.display()))?;
        } else if metadata.is_dir() {
            std::fs::remove_dir_all(dest)
                .map_err(|e| format!("移除旧 mods 目录失败 ({}): {e}", dest.display()))?;
        }
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dest).map_err(|e| {
            format!(
                "创建模组软链接失败 ({} -> {}): {e}",
                src.display(),
                dest.display()
            )
        })?;
        return Ok(());
    }

    #[cfg(windows)]
    {
        match std::os::windows::fs::symlink_file(src, dest) {
            Ok(()) => return Ok(()),
            Err(_) => {
                std::fs::copy(src, dest).map_err(|e| {
                    format!(
                        "创建软链接失败且复制模组失败 ({} -> {}): {e}",
                        src.display(),
                        dest.display()
                    )
                })?;
                return Ok(());
            }
        }
    }

    #[allow(unreachable_code)]
    {
        std::fs::copy(src, dest).map_err(|e| {
            format!(
                "复制模组失败 ({} -> {}): {e}",
                src.display(),
                dest.display()
            )
        })?;
        Ok(())
    }
}


/// Pass 8/10, 验证，清理，连接工作区的mods，保证工作区的mods与传入参数对齐
fn sync_workspace_mod_links(workspace_id: &str, mods: &[WorkspaceMod]) -> Result<(), String> {
    let mods_dir = workspace_mods_dir(workspace_id);
    std::fs::create_dir_all(&mods_dir).map_err(|e| format!("创建 mods 目录失败: {e}"))?;

    let mut expected = std::collections::HashSet::new();
    for mod_entry in mods {
        if ModUsageType::from_str(&mod_entry.mod_type) == Some(ModUsageType::ServerOnly) {
            continue;
        }
        if mod_entry.file_name.trim().is_empty() {
            continue;
        }
        let cache_path = modcache_dir().join(&mod_entry.file_name);
        if !cache_path.is_file() {
            continue;
        }
        let link_path = mods_dir.join(&mod_entry.file_name);
        ensure_symlink_or_copy(&cache_path, &link_path)?;
        expected.insert(mod_entry.file_name.clone());
    }

    for entry in std::fs::read_dir(&mods_dir).map_err(|e| format!("读取 mods 目录失败: {e}"))?
    {
        let entry = entry.map_err(|e| format!("读取 mods 目录项失败: {e}"))?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if expected.contains(&file_name) {
            continue;
        }
        if path.is_file()
            || std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
        {
            let _ = std::fs::remove_file(&path);
        }
    }

    Ok(())
}

fn normalize_loader_for_modrinth(loader_type: &str) -> Option<&'static str> {
    match loader_type.trim().to_lowercase().as_str() {
        "fabric" => Some("fabric"),
        "forge" => Some("forge"),
        "neoforge" => Some("neoforge"),
        _ => None,
    }
}

fn mod_registry_key(title: &str, mod_name: &str, project_id: &str) -> String {
    if !title.trim().is_empty() {
        title.trim().to_string()
    } else if !mod_name.trim().is_empty() {
        mod_name.trim().to_string()
    } else {
        project_id.to_string()
    }
}

fn mod_registry_matches(entry: &Mod, name: &str, project_id: &str) -> bool {
    if entry.name == name {
        return true;
    }
    entry
        .project
        .get("id")
        .and_then(|value| value.as_str())
        .map(|value| value == project_id)
        .unwrap_or(false)
}

fn side_supported(value: Option<&str>) -> bool {
    matches!(value, Some("required" | "optional"))
}

fn infer_mod_usage_type(project: &Value) -> ModUsageType {
    let client_side = project.get("client_side").and_then(|value| value.as_str());
    let server_side = project.get("server_side").and_then(|value| value.as_str());
    let is_library = project
        .get("categories")
        .and_then(|value| value.as_array())
        .map(|values| values.iter().any(|entry| entry.as_str() == Some("library")))
        .unwrap_or(false);

    let client_supported = side_supported(client_side);
    let server_supported = side_supported(server_side);

    if client_supported && server_supported {
        ModUsageType::ClientAndServer
    } else if client_supported {
        ModUsageType::ClientOnly
    } else if server_supported {
        ModUsageType::ServerOnly
    } else if is_library
        || (client_side == Some("unsupported") && server_side == Some("unsupported"))
    {
        ModUsageType::DevelopmentOnly
    } else {
        ModUsageType::Unknown
    }
}

fn upsert_mod_registry_entry(
    registry_key: &str,
    project: &Value,
    mod_type: ModUsageType,
) -> Result<(), String> {
    let mut registry = read_mod_registry();
    let mut object = project
        .as_object()
        .cloned()
        .ok_or_else(|| "Modrinth 项目详情不是对象结构".to_string())?;
    for key in [
        "versions",
        "game_versions",
        "discord_url",
        "body",
        "followers",
        "gallery",
        "donation_urls",
    ] {
        object.remove(key);
    }
    let project_id = object
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();

    if let Some(existing) = registry
        .iter_mut()
        .find(|entry| mod_registry_matches(entry, registry_key, &project_id))
    {
        existing.name = registry_key.to_string();
        existing.mod_type = mod_type;
        existing.project = object;
    } else {
        registry.push(Mod {
            name: registry_key.to_string(),
            mod_type,
            project: object,
        });
    }
    write_mod_registry(&registry)
}

fn update_mod_registry_type(registry_key: &str, mod_type: ModUsageType) -> Result<(), String> {
    let mut registry = read_mod_registry();
    let entry = registry
        .iter_mut()
        .find(|entry| entry.name == registry_key)
        .ok_or_else(|| format!("mods.json 中未找到模组 {registry_key}"))?;
    entry.mod_type = mod_type;
    write_mod_registry(&registry)
}

async fn fetch_latest_version(
    project_id: &str,
    mc_version: &str,
    loader: Option<&str>,
) -> Result<Option<ModrinthVersionSummary>, String> {
    let mut url = format!(
        "https://api.modrinth.com/v2/project/{project_id}/version?game_versions=[\"{mc_version}\"]"
    );
    if let Some(loader) = loader {
        url.push_str(&format!("&loaders=[\"{loader}\"]"));
    }

    let versions: Vec<ModrinthVersion> = reqwest::get(&url)
        .await
        .map_err(|e| format!("请求 Modrinth 版本失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Modrinth 版本状态异常: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 Modrinth 版本失败: {e}"))?;

    let Some(version) = versions.into_iter().next() else {
        return Ok(None);
    };
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| "该模组版本没有可下载文件".to_string())?;

    Ok(Some(ModrinthVersionSummary {
        version_id: version.id,
        version_number: version.version_number,
        game_versions: version.game_versions,
        loaders: version.loaders,
        file_name: file.filename.clone(),
        download_url: file.url.clone(),
        size: file.size,
        sha1: file.hashes.sha1.clone(),
    }))
}

#[tauri::command]
pub async fn search_modrinth_mods(
    workspace_id: String,
    query: String,
) -> Result<Vec<ModrinthProjectHit>, String> {
    let pack = read_pack_config(&workspace_id)?;
    let facets = if let Some(loader) = normalize_loader_for_modrinth(&pack.loader_type) {
        format!(
            "[[\"project_type:mod\"],[\"versions:{}\"],[\"categories:{}\"]]",
            pack.mc_version, loader
        )
    } else {
        format!(
            "[[\"project_type:mod\"],[\"versions:{}\"]]",
            pack.mc_version
        )
    };
    let query = urlencoding::encode(query.trim());
    let url = format!(
        "https://api.modrinth.com/v2/search?query={query}&limit=20&index=relevance&facets={facets}"
    );

    let response: ModrinthSearchResponse = reqwest::get(&url)
        .await
        .map_err(|e| format!("请求 Modrinth 搜索失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Modrinth 搜索状态异常: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 Modrinth 搜索结果失败: {e}"))?;

    Ok(response.hits)
}

#[tauri::command]
pub async fn install_modrinth_mod(
    workspace_id: String,
    project_id: String,
) -> Result<WorkspaceMod, String> {
    let mut pack = read_pack_config(&workspace_id)?;
    let loader = normalize_loader_for_modrinth(&pack.loader_type);
    let version_summary = fetch_latest_version(&project_id, &pack.mc_version, loader)
        .await?
        .ok_or_else(|| "未找到匹配当前工作区版本/加载器的模组版本".to_string())?;

    let version_id = version_summary.version_id;
    let version_url = format!("https://api.modrinth.com/v2/version/{version_id}");
    let version: ModrinthVersion = reqwest::get(&version_url)
        .await
        .map_err(|e| format!("请求 Modrinth 版本详情失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Modrinth 版本详情状态异常: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 Modrinth 版本详情失败: {e}"))?;

    if !version
        .game_versions
        .iter()
        .any(|value| value == &pack.mc_version)
    {
        return Err(format!(
            "模组版本 {} 不支持 MC {}",
            version.version_number, pack.mc_version
        ));
    }
    if let Some(loader) = loader {
        if !version.loaders.iter().any(|value| value == loader) {
            return Err(format!(
                "模组版本 {} 不支持加载器 {}",
                version.version_number, loader
            ));
        }
    }

    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| "该模组版本没有可下载文件".to_string())?;

    let project_url = format!("https://api.modrinth.com/v2/project/{project_id}");
    let project: serde_json::Value = reqwest::get(&project_url)
        .await
        .map_err(|e| format!("请求 Modrinth 项目详情失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Modrinth 项目详情状态异常: {e}"))?
        .json()
        .await
        .map_err(|e| format!("解析 Modrinth 项目详情失败: {e}"))?;
    let mod_name = project
        .get("slug")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| project.get("title").and_then(|value| value.as_str()))
        .unwrap_or(&project_id)
        .to_string();
    let title = project
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or(&mod_name)
        .to_string();
    let registry_key = mod_registry_key(&title, &mod_name, &project_id);
    let existing_mod_type = read_mod_registry()
        .into_iter()
        .find(|entry| mod_registry_matches(entry, &registry_key, &project_id))
        .map(|entry| entry.mod_type);
    let mod_type = existing_mod_type.unwrap_or_else(|| infer_mod_usage_type(&project));

    let cached_file_name =
        build_cached_mod_filename(&mod_name, &version.version_number, &pack.mc_version);
    let cache_path = modcache_dir().join(&cached_file_name);
    std::fs::create_dir_all(modcache_dir()).map_err(|e| format!("创建 modcache 目录失败: {e}"))?;

    if !cache_path.is_file() {
        let bytes = reqwest::get(&file.url)
            .await
            .map_err(|e| format!("下载模组文件失败: {e}"))?
            .error_for_status()
            .map_err(|e| format!("模组文件下载状态异常: {e}"))?
            .bytes()
            .await
            .map_err(|e| format!("读取模组文件失败: {e}"))?;
        std::fs::write(&cache_path, &bytes).map_err(|e| format!("写入模组缓存失败: {e}"))?;
    }

    let mod_entry = WorkspaceMod {
        project_id,
        version_id: version.id,
        mod_name,
        mod_version: version.version_number,
        mc_version: pack.mc_version.clone(),
        file_name: cached_file_name,
        title,
        mod_type: mod_type.as_str().to_string(),
    };

    upsert_mod_registry_entry(&registry_key, &project, mod_type)?;

    //为了不重复
    pack.mods
        .retain(|item| item.project_id != mod_entry.project_id);
    pack.mods.push(mod_entry.clone());
    sync_workspace_mod_links(&workspace_id, &pack.mods)?;
    write_pack_config(&workspace_id, &pack)?;
    Ok(mod_entry)
}

#[tauri::command]
pub fn remove_workspace_mod(workspace_id: String, project_id: String) -> Result<(), String> {
    let mut pack = read_pack_config(&workspace_id)?;
    pack.mods.retain(|item| item.project_id != project_id);
    write_pack_config(&workspace_id, &pack)
}

#[tauri::command]
pub fn sync_workspace_mods(workspace_id: String) -> Result<(), String> {
    let pack = read_pack_config(&workspace_id)?;
    sync_workspace_mod_links(&workspace_id, &pack.mods)
}

#[tauri::command]
pub fn update_workspace_mod_type(
    workspace_id: String,
    project_id: String,
    mod_type: String,
) -> Result<WorkspaceMod, String> {
    let parsed_mod_type =
        ModUsageType::from_str(&mod_type).ok_or_else(|| format!("无效的模组类型: {mod_type}"))?;
    let mut pack = read_pack_config(&workspace_id)?;
    let mod_entry = pack
        .mods
        .iter_mut()
        .find(|item| item.project_id == project_id)
        .ok_or_else(|| format!("工作区中未找到模组 {project_id}"))?;
    mod_entry.mod_type = parsed_mod_type.as_str().to_string();

    let registry_key =
        mod_registry_key(&mod_entry.title, &mod_entry.mod_name, &mod_entry.project_id);
    update_mod_registry_type(&registry_key, parsed_mod_type)?;
    let updated = mod_entry.clone();

    write_pack_config(&workspace_id, &pack)?;
    Ok(updated)
}
