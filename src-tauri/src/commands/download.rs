use std::path::PathBuf;

use anyhow::{Context, Result};
use mc_launcher_core::runtime::RuntimeResult;
use mc_launcher_core::runtime::{
    prepare::prepare_runtime, LoaderKind, ProgressReporter, RuntimeRequest,
};
use tauri::Emitter;

use super::java::resolve_launch_java_path;
use super::settings::load_settings;
use super::workspace::PackConfig;

struct TauriProgressReporter {
    app: tauri::AppHandle,
}

impl ProgressReporter for TauriProgressReporter {
    fn emit(&self, stage: &str, current: usize, total: usize) {
        let _ = self.app.emit(
            "download-progress",
            serde_json::json!({
                "stage": stage,
                "current": current,
                "total": total,
            }),
        );
    }

    fn send(&self, stage: &str) {
        let _ = self.app.emit(
            "download-progress",
            serde_json::json!({
                "stage": stage,
            }),
        );
    }
}

fn get_mmpc_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}

fn workspace_dir(id: &str) -> PathBuf {
    get_mmpc_dir().join("workspaces").join(id)
}

fn pack_config_path(id: &str) -> PathBuf {
    workspace_dir(id).join("pack.json")
}

/// 读取pack.json并返回结构体
pub fn read_pack_config(id: &str) -> Result<PackConfig> {
    let path = pack_config_path(id);
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("读取 pack.json 失败: {}", path.display()))?;
    serde_json::from_str(&content).context("解析 pack.json 失败")
}

fn resolve_workspace_java_path(pack: &PackConfig) -> Result<String> {
    resolve_launch_java_path(pack.java_runtime_id.as_deref()).map_err(anyhow::Error::msg)
}

pub async fn ensure_workspace_runtime(
    app: &tauri::AppHandle,
    workspace_id: &str,
    mc_version: &str,
) -> Result<RuntimeResult> {
    let settings = load_settings().unwrap_or_default();
    let pack = read_pack_config(workspace_id)?;
    let reporter = TauriProgressReporter { app: app.clone() };

    let _result = prepare_runtime(
        workspace_id,
        &RuntimeRequest {
            mc_version: mc_version.to_string(),
            loader: LoaderKind::from_str(&pack.loader_type),
            loader_version: pack.loader_version.clone(),
            java_path: resolve_workspace_java_path(&pack)?,
            download_concurrency: settings.download_pool_size.max(1),
        },
        &reporter,
    )
    .await?;
    Ok(_result)
}
