use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEFAULT_DOWNLOAD_POOL_SIZE: usize = 16;
const MAX_DOWNLOAD_POOL_SIZE: usize = 64;
const DEFAULT_THEME: &str = "dark";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub download_pool_size: usize,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            download_pool_size: DEFAULT_DOWNLOAD_POOL_SIZE,
            theme: default_theme(),
        }
    }
}

fn mmpc_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}

fn settings_path() -> PathBuf {
    mmpc_root().join("settings.json")
}

pub fn normalize_download_pool_size(size: usize) -> usize {
    size.clamp(1, MAX_DOWNLOAD_POOL_SIZE)
}

fn normalize_theme(theme: &str) -> String {
    match theme.trim() {
        "cupcake" => "cupcake".to_string(),
        _ => DEFAULT_THEME.to_string(),
    }
}

pub fn load_settings() -> Result<AppSettings, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("读取 settings.json 失败: {e}"))?;
    let mut settings: AppSettings =
        serde_json::from_str(&content).map_err(|e| format!("解析 settings.json 失败: {e}"))?;
    settings.download_pool_size = normalize_download_pool_size(settings.download_pool_size);
    settings.theme = normalize_theme(&settings.theme);
    Ok(settings)
}

fn persist_settings(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建设置目录失败: {e}"))?;
    }
    let content =
        serde_json::to_string_pretty(settings).map_err(|e| format!("序列化设置失败: {e}"))?;
    std::fs::write(path, content).map_err(|e| format!("写入 settings.json 失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    let settings = load_settings()?;
    persist_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn save_settings(mut settings: AppSettings) -> Result<AppSettings, String> {
    settings.download_pool_size = normalize_download_pool_size(settings.download_pool_size);
    settings.theme = normalize_theme(&settings.theme);
    persist_settings(&settings)?;
    Ok(settings)
}
