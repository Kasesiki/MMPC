//! Workspace CRUD — reads/writes `.MMPC/workspaces/<id>/pack.json`

use anyhow::anyhow;
use anyhow::{Context, Result as AnyResult, bail};
use chrono::Utc;
use mc_launcher_core::runtime::prepare::versions_dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tokio::sync::OnceCell;

// ─── Data structures ───

/// What gets persisted in pack.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackConfig {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader_type: String,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub description: String,
    #[serde(default, deserialize_with = "deserialize_workspace_mods")]
    pub mods: Vec<WorkspaceMod>,
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub java_runtime_id: Option<String>,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub created_at: String,
    pub last_opened: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceMod {
    pub project_id: String,
    pub version_id: String,
    pub mod_name: String,
    pub mod_version: String,
    pub mc_version: String,
    pub file_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub mod_type: String,
    #[serde(default = "default_mod_enabled")]
    pub enabled: bool,
}

fn default_mod_enabled() -> bool {
    true
}

fn deserialize_workspace_mods<'de, D>(deserializer: D) -> Result<Vec<WorkspaceMod>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<Vec<Value>>::deserialize(deserializer)?.unwrap_or_default();
    let mut mods = Vec::new();

    for item in raw {
        match item {
            Value::String(project_id) => mods.push(WorkspaceMod {
                project_id,
                version_id: String::new(),
                mod_name: String::new(),
                mod_version: String::new(),
                mc_version: String::new(),
                file_name: String::new(),
                title: String::new(),
                mod_type: "unknown".to_string(),
                enabled: true,
            }),
            Value::Object(_) => {
                let parsed = serde_json::from_value::<WorkspaceMod>(item)
                    .map_err(serde::de::Error::custom)?;
                mods.push(parsed);
            }
            _ => {}
        }
    }

    Ok(mods)
}

/// Info sent to the frontend for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader_type: String,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub description: String,
    pub mod_count: usize,
    pub last_opened: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VersionManifest {
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderVersionOption {
    pub version: String,
    #[serde(default)]
    pub stable: bool,
}

static VERSION_MANIFEST_CACHE: OnceCell<VersionManifest> = OnceCell::const_new();

async fn fetch_json(url: &str, label: &str) -> AnyResult<serde_json::Value> {
    let response = match bmclapi::request(url).await {
        Ok(resp) => resp,
        Err(err) => {
            bail!("{label} 获取失败: {err}")
        }
    };
    let response = match response.error_for_status() {
        Ok(resp) => resp,
        Err(err) => {
            bail!("{label} 获取失败: {err}")
        }
    };
    match response.json::<serde_json::Value>().await {
        Ok(value) => Ok(value),
        Err(err) => bail!("{label} 解析失败: {err}"),
    }
}

// ─── Path helpers ───

fn mmpc_root() -> PathBuf {
    let exe = std::env::current_exe().expect("failed to get exe path");
    exe.parent().expect("exe has no parent").join(".MMPC")
}

fn workspaces_dir() -> PathBuf {
    mmpc_root().join("workspaces")
}

fn workspace_dir(id: &str) -> PathBuf {
    workspaces_dir().join(id)
}

fn pack_json_path(id: &str) -> PathBuf {
    workspace_dir(id).join("pack.json")
}

fn natives_dir(id: &str) -> PathBuf {
    workspace_dir(id).join("natives")
}

fn launch_dir(id: &str) -> PathBuf {
    workspace_dir(id).join("launch")
}

fn normalize_loader_type(loader_type: &str) -> String {
    match loader_type.trim().to_lowercase().as_str() {
        "fabric" => "fabric".to_string(),
        "forge" => "forge".to_string(),
        "neoforge" => "neoforge".to_string(),
        _ => "vanilla".to_string(),
    }
}

fn normalize_loader_version(loader_type: &str, loader_version: Option<String>) -> Option<String> {
    if loader_type == "vanilla" {
        return None;
    }

    loader_version
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct McReleaseVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

#[derive(Debug, Clone, Copy)]
struct JavaRequirementRule {
    min: McReleaseVersion,
    max: Option<McReleaseVersion>,
    preferred_java_majors: &'static [u32],
}

const JAVA_REQ_21: &[u32] = &[21];
const JAVA_REQ_17_16: &[u32] = &[17, 16];
const JAVA_REQ_8_11: &[u32] = &[8, 11];
const JAVA_REQ_8: &[u32] = &[8];

const MC_JAVA_REQUIREMENT_RULES: &[JavaRequirementRule] = &[
    JavaRequirementRule {
        min: McReleaseVersion {
            major: 1,
            minor: 20,
            patch: 5,
        },
        max: None,
        preferred_java_majors: JAVA_REQ_21,
    },
    JavaRequirementRule {
        min: McReleaseVersion {
            major: 1,
            minor: 17,
            patch: 0,
        },
        max: Some(McReleaseVersion {
            major: 1,
            minor: 20,
            patch: 4,
        }),
        preferred_java_majors: JAVA_REQ_17_16,
    },
    JavaRequirementRule {
        min: McReleaseVersion {
            major: 1,
            minor: 12,
            patch: 0,
        },
        max: Some(McReleaseVersion {
            major: 1,
            minor: 16,
            patch: 5,
        }),
        preferred_java_majors: JAVA_REQ_8_11,
    },
    JavaRequirementRule {
        min: McReleaseVersion {
            major: 0,
            minor: 0,
            patch: 0,
        },
        max: Some(McReleaseVersion {
            major: 1,
            minor: 11,
            patch: 999,
        }),
        preferred_java_majors: JAVA_REQ_8,
    },
];

fn parse_mc_release_version(mc_version: &str) -> Option<McReleaseVersion> {
    let normalized = mc_version
        .trim()
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .trim();
    if normalized.is_empty() {
        return None;
    }
    let mut parts = normalized.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);

    Some(McReleaseVersion {
        major,
        minor,
        patch,
    })
}

fn preferred_java_majors_for_mc(mc_version: &str) -> Option<&'static [u32]> {
    let parsed = parse_mc_release_version(mc_version)?;
    MC_JAVA_REQUIREMENT_RULES
        .iter()
        .find(|rule| {
            if parsed < rule.min {
                return false;
            }
            match rule.max {
                Some(max) => parsed <= max,
                None => true,
            }
        })
        .map(|rule| rule.preferred_java_majors)
}

fn select_java_runtime_id_for_mc(mc_version: &str) -> Option<String> {
    let preferred = preferred_java_majors_for_mc(mc_version)?;
    let runtimes = super::java::list_java_runtimes().ok()?;

    for major in preferred {
        if let Some(runtime) = runtimes
            .iter()
            .rev()
            .find(|runtime| runtime.major_version == Some(*major))
        {
            return Some(runtime.id.clone());
        }
    }
    None
}

fn clear_workspace_runtime_cache(id: &str) -> AnyResult<()> {
    for path in [versions_dir(id), natives_dir(id), launch_dir(id)] {
        if path.exists() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("清理运行时缓存失败 ({})", path.display()))?;
        }
    }
    Ok(())
}

pub async fn get_cached_version_manifest() -> AnyResult<VersionManifest> {
    VERSION_MANIFEST_CACHE
        .get_or_try_init(|| async {
            serde_json::from_value(
                fetch_json(
                    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
                    "获取版本列表",
                )
                .await?,
            )
            .context("解析版本列表失败")
        })
        .await
        .cloned()
}

// ─── Tauri commands ───

/// List all workspaces by scanning `.MMPC/workspaces/`
#[tauri::command]
pub fn list_workspaces() -> Result<Vec<WorkspaceSummary>, String> {
    let ws_dir = workspaces_dir();
    if !ws_dir.exists() {
        return Ok(vec![]);
    }

    let mut list = Vec::new();
    let entries = fs::read_dir(&ws_dir).map_err(|e| format!("read_dir error: {e}"))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("entry error: {e}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let json_path = path.join("pack.json");
        if !json_path.exists() {
            continue;
        }

        match fs::read_to_string(&json_path) {
            Ok(content) => match serde_json::from_str::<PackConfig>(&content) {
                Ok(cfg) => list.push(WorkspaceSummary {
                    id: cfg.id,
                    name: cfg.name,
                    mc_version: cfg.mc_version,
                    loader_type: cfg.loader_type,
                    loader_version: cfg.loader_version,
                    description: cfg.description,
                    mod_count: cfg.mods.len(),
                    last_opened: cfg.last_opened,
                    created_at: cfg.created_at,
                }),
                Err(e) => {
                    eprintln!("[mmpc] invalid pack.json in {id}: {e}");
                    continue;
                }
            },
            Err(e) => {
                eprintln!("[mmpc] cannot read {id}/pack.json: {e}");
                continue;
            }
        }
    }

    // Sort by last_opened descending
    list.sort_by(|a, b| b.last_opened.cmp(&a.last_opened));
    Ok(list)
}

/// Create a new workspace on disk
#[tauri::command]
pub fn create_workspace(
    name: String,
    mc_version: String,
    description: String,
    loader_type: Option<String>,
    loader_version: Option<String>,
) -> Result<WorkspaceSummary, String> {
    let id = name
        .to_lowercase()
        .replace(char::is_whitespace, "-")
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "");

    if id.is_empty() {
        return Err("Invalid workspace name".to_string());
    }

    let dir = workspace_dir(&id);
    fs::create_dir_all(&dir).map_err(|e| format!("create dir error: {e}"))?;

    let now = Utc::now().to_rfc3339();
    let loader_type = normalize_loader_type(loader_type.as_deref().unwrap_or("vanilla"));
    let loader_version = normalize_loader_version(&loader_type, loader_version);

    let auto_java_runtime_id = select_java_runtime_id_for_mc(&mc_version);

    let cfg = PackConfig {
        id: id.clone(),
        name,
        mc_version,
        loader_type,
        loader_version,
        description,
        mods: vec![],
        jvm_args: vec![],
        java_runtime_id: auto_java_runtime_id,
        min_memory_mb: 1024,
        max_memory_mb: 4096,
        window_width: 1280,
        window_height: 720,
        created_at: now.clone(),
        last_opened: now,
    };

    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("serialize error: {e}"))?;
    fs::write(pack_json_path(&id), &json).map_err(|e| format!("write error: {e}"))?;

    Ok(WorkspaceSummary {
        id: cfg.id,
        name: cfg.name,
        mc_version: cfg.mc_version,
        loader_type: cfg.loader_type,
        loader_version: cfg.loader_version,
        description: cfg.description,
        mod_count: 0,
        last_opened: cfg.last_opened,
        created_at: cfg.created_at,
    })
}

#[tauri::command]
pub async fn list_release_versions() -> Result<Vec<String>, String> {
    let manifest = get_cached_version_manifest()
        .await
        .map_err(|e| e.to_string())?;

    Ok(manifest
        .versions
        .into_iter()
        .filter(|entry| entry.version_type == "release")
        .map(|entry| entry.id)
        .collect())
}

#[tauri::command]
pub async fn list_fabric_loader_versions(
    mc_version: String,
) -> Result<Vec<LoaderVersionOption>, String> {
    let versions = fetch_json(
        &format!("https://meta.fabricmc.net/v2/versions/loader/{mc_version}"),
        "获取 Fabric 版本列表",
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(versions
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            let loader = entry.get("loader")?;
            Some(LoaderVersionOption {
                version: loader.get("version")?.as_str()?.to_string(),
                stable: loader
                    .get("stable")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
            })
        })
        .collect())
}

fn parse_maven_versions(xml: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut start = 0usize;
    let open = "<version>";
    let close = "</version>";

    while let Some(open_idx) = xml[start..].find(open) {
        let value_start = start + open_idx + open.len();
        let Some(close_idx) = xml[value_start..].find(close) else {
            break;
        };
        let value_end = value_start + close_idx;
        let value = xml[value_start..value_end].trim();
        if !value.is_empty() {
            versions.push(value.to_string());
        }
        start = value_end + close.len();
    }

    versions
}

fn mc_version_to_neoforge_prefix(mc_version: &str) -> Option<String> {
    mc_version.strip_prefix("1.").map(|rest| rest.to_string())
}

async fn fetch_maven_metadata_versions(url: &str) -> anyhow::Result<Vec<String>> {
    let response = bmclapi::request(url).await?;
    let response = response.error_for_status()?;
    let xml = response
        .text()
        .await
        .map_err(|e| anyhow!("{}", e.to_string()))?;
    Ok(parse_maven_versions(&xml))
}

#[tauri::command]
pub async fn list_forge_loader_versions(
    mc_version: String,
) -> Result<Vec<LoaderVersionOption>, String> {
    let versions = fetch_maven_metadata_versions(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(versions
        .into_iter()
        .filter_map(|value| {
            let suffix = value.strip_prefix(&format!("{mc_version}-"))?;
            Some(LoaderVersionOption {
                version: suffix.to_string(),
                stable: true,
            })
        })
        .collect())
}

#[tauri::command]
pub async fn list_neoforge_loader_versions(
    mc_version: String,
) -> Result<Vec<LoaderVersionOption>, String> {
    let Some(prefix) = mc_version_to_neoforge_prefix(&mc_version) else {
        return Ok(vec![]);
    };
    let versions = fetch_maven_metadata_versions(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(versions
        .into_iter()
        .filter(|value| value.starts_with(&prefix))
        .map(|value| {
            let stable = !value.contains("beta");
            LoaderVersionOption {
                version: value,
                stable,
            }
        })
        .collect())
}

/// Delete a workspace (recursive)
#[tauri::command]
pub fn delete_workspace(id: String) -> Result<(), String> {
    let dir = workspace_dir(&id);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("delete error: {e}"))?;
    }
    Ok(())
}

/// Get full pack config for a workspace
#[tauri::command]
pub fn get_pack_config(id: String) -> Result<PackConfig, String> {
    let path = pack_json_path(&id);
    let content = fs::read_to_string(&path).map_err(|e| format!("read error: {e}"))?;
    let cfg: PackConfig =
        serde_json::from_str(&content).map_err(|e| format!("parse error: {e}"))?;
    Ok(cfg)
}

/// Save (overwrite) pack config
#[tauri::command]
pub fn save_pack_config(id: String, config: PackConfig) -> Result<(), String> {
    // Update last_opened only if explicitly intended; preserve id
    let mut cfg = config;
    cfg.id = id.clone();
    cfg.loader_type = normalize_loader_type(&cfg.loader_type);
    cfg.loader_version = normalize_loader_version(&cfg.loader_type, cfg.loader_version.clone());

    if let Ok(previous_content) = fs::read_to_string(pack_json_path(&id)) {
        if let Ok(previous_cfg) = serde_json::from_str::<PackConfig>(&previous_content) {
            let previous_loader_type = normalize_loader_type(&previous_cfg.loader_type);
            let previous_loader_version =
                normalize_loader_version(&previous_loader_type, previous_cfg.loader_version);
            cfg.loader_type = previous_loader_type.clone();
            cfg.loader_version = previous_loader_version.clone();
            let version_related_changed = previous_cfg.mc_version != cfg.mc_version
                || previous_loader_type != cfg.loader_type
                || previous_loader_version != cfg.loader_version;

            if version_related_changed {
                clear_workspace_runtime_cache(&id).map_err(|e| e.to_string())?;
            }
        }
    }

    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("serialize error: {e}"))?;
    fs::write(pack_json_path(&id), &json).map_err(|e| format!("write error: {e}"))?;
    Ok(())
}
