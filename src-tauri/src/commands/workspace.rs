//! Workspace CRUD — reads/writes `.MMPC/workspaces/<id>/pack.json`

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};

// ─── Data structures ───

/// What gets persisted in pack.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackConfig {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub description: String,
    pub mods: Vec<String>,
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

/// Info sent to the frontend for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub description: String,
    pub mod_count: usize,
    pub last_opened: String,
    pub created_at: String,
}

// ─── Path helpers ───

fn mmpc_root() -> PathBuf {
    let exe = std::env::current_exe().expect("failed to get exe path");
    exe.parent()
        .expect("exe has no parent")
        .join(".MMPC")
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
pub fn create_workspace(name: String, mc_version: String, description: String) -> Result<WorkspaceSummary, String> {
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
    let cfg = PackConfig {
        id: id.clone(),
        name,
        mc_version,
        description,
        mods: vec![],
        jvm_args: vec![],
        java_runtime_id: None,
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
        description: cfg.description,
        mod_count: 0,
        last_opened: cfg.last_opened,
        created_at: cfg.created_at,
    })
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
    let cfg: PackConfig = serde_json::from_str(&content).map_err(|e| format!("parse error: {e}"))?;
    Ok(cfg)
}

/// Save (overwrite) pack config
#[tauri::command]
pub fn save_pack_config(id: String, config: PackConfig) -> Result<(), String> {
    // Update last_opened only if explicitly intended; preserve id
    let mut cfg = config;
    cfg.id = id.clone();
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("serialize error: {e}"))?;
    fs::write(pack_json_path(&id), &json).map_err(|e| format!("write error: {e}"))?;
    Ok(())
}
