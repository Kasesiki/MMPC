use futures_util::FutureExt as _;
use std::path::PathBuf;
use tokio::fs;

use anyhow::anyhow;
use anyhow::{Context, Result};
use futures_util::TryFutureExt;
use mc_launcher_core::runtime::prepare::{
    AssetIndexObjects, DownloadTask, DownloadTrait, MOJANG_RESOURCES_BASE, VersionJson, build_library_tasks, build_runtime_layout, check_path_sha1, execute_download_pool, fetch_fabric_version_value, file_matches_sha1, merge_version_json, write_json_pretty,
};
use mc_launcher_core::runtime::{LoaderKind, ProgressReporter};
use mc_launcher_core::runtime::{RuntimeLayout, RuntimeResult};
use serde_json::Value;
use tauri::Emitter;

use crate::commands::workspace::VersionManifest;

use super::java::resolve_launch_java_path;
use super::settings::load_settings;
use super::workspace::PackConfig;

pub struct TauriProgressReporter {
    app: tauri::AppHandle,
}

impl TauriProgressReporter {
    pub fn new(app: tauri::AppHandle) -> TauriProgressReporter {
        TauriProgressReporter { app }
    }
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

pub async fn fetch_json_with_candidates(url: &str, _label: &str) -> Result<serde_json::Value> {
    bmclapi::request(url)
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.into())
}

const MOJANG_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

/// 获取原版version.json
async fn fetch_vanilla_version_value(mc_version: &str) -> Result<serde_json::Value> {
    let manifest: VersionManifest = serde_json::from_value(
        fetch_json_with_candidates(MOJANG_MANIFEST_URL, "下载版本清单").await?,
    )
    .context("解析版本清单失败")?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|entry| entry.id == mc_version)
        .ok_or_else(|| anyhow!("未找到 MC 版本 {mc_version}"))?;

    fetch_json_with_candidates(&entry.url, "下载原版 version.json").await
}

fn asset_url_candidates(hash: &str) -> String {
    let subdir = &hash[..2];
    format!("{MOJANG_RESOURCES_BASE}/{subdir}/{hash}")
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
    reporter: &TauriProgressReporter,
    workspace_id: &str,
    pack: &PackConfig,
) -> Result<RuntimeResult> {
    reporter.send("正在加载配置文件....");
    let settings = load_settings().unwrap_or_default();

    // prepare_runtime(
    //     workspace_id,
    //     &RuntimeRequest {
    //         mc_version: pack.mc_version.to_string(),
    //         loader: LoaderKind::from_str(&pack.loader_type),
    //         loader_version: pack.loader_version.clone(),
    //         java_path: resolve_workspace_java_path(&pack)?,
    //         download_concurrency: settings.download_pool_size.max(1),
    //     },
    //     reporter,
    // )
    // .await

    let layout: RuntimeLayout = build_runtime_layout(workspace_id);
    std::fs::create_dir_all(&layout.versions_dir)
        .with_context(|| format!("创建 versions 目录失败: {}", layout.versions_dir.display()))?;
    std::fs::create_dir_all(&layout.assets_root)
        .with_context(|| format!("创建 assets 目录失败: {}", layout.assets_root.display()))?;
    std::fs::create_dir_all(&layout.installers_cache_dir).with_context(|| {
        format!(
            "创建 installer 缓存目录失败: {}",
            layout.installers_cache_dir.display()
        )
    })?;
    std::fs::create_dir_all(&layout.temp_root)
        .with_context(|| format!("创建临时目录失败: {}", layout.temp_root.display()))?;

    reporter.send("获取原版版本清单");
    let inherited_version_json_path = layout
        .versions_dir
        .join(format!("{}.json", pack.mc_version));

    let base_value: Value = if let Ok(result) = fs::read_to_string(&inherited_version_json_path)
        .await
        .map_err(|e| anyhow!("{e}"))
        .and_then(|content: String| serde_json::from_str(&content).map_err(|e| anyhow!("{e}"))) {
            result
        } else {
            fetch_vanilla_version_value(&pack.mc_version)
                .await
                .map(|result| {
                    let _ = write_json_pretty(&inherited_version_json_path, &result);
                    result
                })?
        };


    let base_version_json: VersionJson =
        serde_json::from_value(base_value.clone()).context("解析基础 version.json 失败")?;

    let client_path = layout.versions_dir.join("client.jar");

    // download_version_json根源相同, download_version_json通过格式到VersionJson从而会丢失部分信息
    reporter.send("正在请求loader信息....");
    let (download_version_json, launcher_version_json) =
        match LoaderKind::from_str(&pack.loader_type) {
            LoaderKind::Vanilla => {
                // 两者相同
                (base_version_json, base_value)
            }
            LoaderKind::Fabric => {
                // 请求fabric, 合并, 解析为VersionJson, 两者依旧相同，VersionJson并不包含全部参数
                let loader_version = pack
                    .loader_version
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| anyhow!("Fabric 缺少 loader_version"))?;
                let fabric_value =
                    fetch_fabric_version_value(&pack.mc_version, loader_version).await?;

                let merged = merge_version_json(&base_value, &fabric_value);
                let version_json: VersionJson = serde_json::from_value(merged.clone())
                    .context("解析 Fabric version.json 失败")?;
                (version_json, merged)
            }
            LoaderKind::Forge | LoaderKind::NeoForge => {
                todo!()
            }
        };

    let version_json_path = layout.versions_dir.join("version.json");
    write_json_pretty(&version_json_path, &launcher_version_json)?;
    

    // 下载client.jar, 存到工作区的versions文件夹
    reporter.send("下载client.jar...");
    download_version_json.downloads.client.download(&client_path).await;

    let asset_index_path = layout.versions_dir.join("asset_index.json");
    reporter.send("下载asset_index.json...");
    download_version_json.asset_index.download(&asset_index_path);

    // 下载 library，存到全局共享的 .MMPC/libraries 文件夹
    let library_tasks =
        build_library_tasks(&layout.libraries_dir, &download_version_json)?;
    execute_download_pool(
        reporter,
        "下载 libraries",
        library_tasks,
        settings.download_pool_size,
    )
    .await?;

    reporter.send("构建asset index中...");
    let asset_index_content = std::fs::read_to_string(&asset_index_path)
        .with_context(|| format!("读取 asset index 失败: {}", asset_index_path.display()))?;
    let asset_index: AssetIndexObjects =
        serde_json::from_str(&asset_index_content).context("解析 asset index 失败")?;

    let indexes_dir = layout.assets_root.join("indexes");
    let objects_dir = layout.assets_root.join("objects");
    std::fs::create_dir_all(&indexes_dir)
        .with_context(|| format!("创建 indexes 目录失败: {}", indexes_dir.display()))?;
    std::fs::create_dir_all(&objects_dir)
        .with_context(|| format!("创建 objects 目录失败: {}", objects_dir.display()))?;

    // .MMPC/assets/indexes/xx.json, 校验并复制
    let asset_index_dest =
        indexes_dir.join(format!("{}.json", download_version_json.asset_index.id));
    if !check_path_sha1(&asset_index_dest, &download_version_json.asset_index.sha1)? {
        std::fs::copy(&asset_index_path, &asset_index_dest).with_context(|| {
            format!(
                "复制 asset index 到全局目录失败 ({} -> {})",
                asset_index_path.display(),
                asset_index_dest.display()
            )
        })?;
    }

    // 下载全部assets
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
        settings.download_pool_size,
    )
    .await?;

    Ok(RuntimeResult {
        // mc version, 如1.21.1
        version_id: launcher_version_json
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or(&pack.mc_version)
            .to_string(),
        // workspace/versions/version.json, mc version.json的缓存
        version_json_path,
        // workspace/versions/{version_id}.json, 如1.21.1.json的缓存，和上个变量内容相同，待测试
        inherited_version_json_path: Some(inherited_version_json_path),
        // workspace/versions/client.jar
        client_jar_path: client_path,
        // workspace/versions/asset_index.json
        asset_index_path,
    })
}
