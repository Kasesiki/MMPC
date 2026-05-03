use std::path::PathBuf;

use mc_launcher_core::runtime::{
    prepare::prepare_runtime, LoaderKind, ProgressReporter, RuntimeLayout, RuntimeRequest,
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

fn versions_dir(id: &str) -> PathBuf {
    workspace_dir(id).join("versions")
}

fn pack_config_path(id: &str) -> PathBuf {
    workspace_dir(id).join("pack.json")
}

/// 读取pack.json并返回结构体
pub fn read_pack_config(id: &str) -> Result<PackConfig, String> {
    let content = std::fs::read_to_string(pack_config_path(id))
        .map_err(|e| format!("读取 pack.json 失败: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 pack.json 失败: {e}"))
}

fn resolve_workspace_java_path(pack: &PackConfig) -> Result<String, String> {
    resolve_launch_java_path(pack.java_runtime_id.as_deref())
}

fn build_runtime_layout(workspace_id: &str) -> RuntimeLayout {
    let root = get_mmpc_dir();
    let workspace_dir = workspace_dir(workspace_id);
    RuntimeLayout {
        workspace_dir: workspace_dir.clone(),
        versions_dir: versions_dir(workspace_id),
        libraries_dir: workspace_dir.join("versions").join("libraries"),
        assets_root: root.join("assets"),
        installers_cache_dir: root.join("cache").join("installers"),
        temp_root: root.join("tmp"),
    }
}


pub async fn ensure_workspace_runtime(
    app: &tauri::AppHandle,
    workspace_id: &str,
    mc_version: &str,
) -> Result<String, String> {
    let settings = load_settings().unwrap_or_default();
    let pack = read_pack_config(workspace_id)?;
    let layout = build_runtime_layout(workspace_id);
    let reporter = TauriProgressReporter { app: app.clone() };

    let result = prepare_runtime(
        &layout,
        &RuntimeRequest {
            mc_version: mc_version.to_string(),
            loader: LoaderKind::from_str(&pack.loader_type),
            loader_version: pack.loader_version.clone(),
            java_path: resolve_workspace_java_path(&pack)?,
            download_concurrency: settings.download_pool_size.max(1),
            prefer_bmclapi: true,
        },
        &reporter,
    )
    .await?;

    reporter.emit("完成", 1, 1);
    Ok(format!(
        "MC {} 数据校验完成（version: {}）",
        mc_version, result.version_id
    ))
}
