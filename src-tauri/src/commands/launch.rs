use crate::commands::download::{read_pack_config, TauriProgressReporter};

use super::download::ensure_workspace_runtime;
use super::java::resolve_launch_java_path;
use super::mods::sync_workspace_mods;
use anyhow::{Context, Result};
use mc_launcher_core::auth::offline::OfflineUser;
use mc_launcher_core::launch::offline::{LaunchConfig, OfflineLauncher};
use mc_launcher_core::launch::version::{
    Library,
    default_logging_config_path,
    evaluate_rules,
    parse_version_metadata,
};
use mc_launcher_core::runtime::prepare::{mm, wd};
use mc_launcher_core::runtime::ProgressReporter;
use std::path::{Path, PathBuf};
use tauri::Emitter;
use zip::read::ZipArchive;

fn shared_libraries_dir() -> PathBuf {
    mm().join("libraries")
}

pub struct PreparedLaunch {
    pub workspace_dir: PathBuf,
    pub program: String,
    pub argfile_path: PathBuf,
}

fn resolve_download_path(path: &Option<String>) -> Option<PathBuf> {
    path.as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn native_classifier_for_current_os() -> String {
    let arch = std::env::consts::ARCH;
    match mc_launcher_core::launch::version::detect_os_name() {
        "windows" => {
            if arch == "x86_64" {
                "natives-windows-64".to_string()
            } else {
                "natives-windows-32".to_string()
            }
        }
        "osx" => "natives-osx".to_string(),
        "linux" => format!("natives-linux-{arch}"),
        other => format!("natives-{other}"),
    }
}

fn collect_libraries_from_metadata(
    library_dir: &Path,
    libraries: &[Library],
) -> Vec<PathBuf> {
    let current_os = mc_launcher_core::launch::version::detect_os_name();
    let current_arch = std::env::consts::ARCH;
    let feature_flags = std::collections::HashMap::new();
    let mut jars = Vec::new();

    for lib in libraries {
        if !evaluate_rules(&lib.rules, current_os, current_arch, &feature_flags) {
            continue;
        }

        if let Some(rel_path) = lib
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
            .and_then(|artifact| resolve_download_path(&artifact.path))
        {
            let full_path = library_dir.join(rel_path);
            if full_path.is_file() {
                jars.push(full_path);
            }
        }
    }

    jars
}

fn collect_native_libraries_from_metadata(
    library_dir: &Path,
    libraries: &[Library],
) -> Vec<PathBuf> {
    let current_os = mc_launcher_core::launch::version::detect_os_name();
    let current_arch = std::env::consts::ARCH;
    let feature_flags = std::collections::HashMap::new();
    let fallback_classifier = native_classifier_for_current_os();
    let mut jars = Vec::new();

    for lib in libraries {
        if !evaluate_rules(&lib.rules, current_os, current_arch, &feature_flags) {
            continue;
        }

        let native_classifier = lib
            .natives
            .get(current_os)
            .cloned()
            .unwrap_or_else(|| fallback_classifier.clone())
            .replace(
                "${arch}",
                if cfg!(target_pointer_width = "64") { "64" } else { "32" },
            );

        if let Some(rel_path) = lib
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.classifiers.as_ref())
            .and_then(|classifiers| classifiers.get(&native_classifier))
            .and_then(|entry| resolve_download_path(&entry.path))
        {
            let full_path = library_dir.join(rel_path);
            if full_path.is_file() {
                jars.push(full_path);
            }
        }
    }

    jars
}

fn load_effective_version_metadata(
    version_json_path: &Path,
    inherited_version_json_path: Option<&Path>,
) -> Result<mc_launcher_core::launch::version::VersionMetadata, String> {
    let child_raw = std::fs::read_to_string(version_json_path)
        .map_err(|e| format!("读取 version.json 失败: {e}"))?;
    let child = parse_version_metadata(&child_raw)
        .map_err(|e| format!("解析启动元数据失败: {e}"))?;

    if child.inherits_from.is_none() {
        return Ok(child);
    }

    let Some(parent_path) = inherited_version_json_path else {
        return Err("version.json 需要继承父版本，但缺少父版本元数据".to_string());
    };
    let parent_raw = std::fs::read_to_string(parent_path)
        .map_err(|e| format!("读取继承版本元数据失败: {e}"))?;
    let parent = parse_version_metadata(&parent_raw)
        .map_err(|e| format!("解析继承版本元数据失败: {e}"))?;
    Ok(mc_launcher_core::launch::version::merge_version_metadata(&parent, &child))
}

fn is_native_jar(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.contains("-natives-") && name.ends_with(".jar"))
        .unwrap_or(false)
}

fn is_native_lib_file(name: &str) -> bool {
    name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib")
}

fn prepare_natives_dir(ws: &Path, libraries: &[PathBuf]) -> Result<()> {
    let natives_dir = ws.join("natives");
    std::fs::create_dir_all(&natives_dir)
        .with_context(|| format!("创建 natives 目录失败: {}", natives_dir.display()))?;

    for lib in libraries {
        if !is_native_jar(lib) {
            continue;
        }

        let file = std::fs::File::open(lib)
            .with_context(|| format!("打开 natives jar 失败 ({})", lib.display()))?;
        let mut zip = ZipArchive::new(file)
            .with_context(|| format!("读取 natives jar 失败 ({})", lib.display()))?;

        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .with_context(|| format!("读取 natives 条目失败 ({})", lib.display()))?;
            if !entry.is_file() {
                continue;
            }
            let Some(name) = entry.enclosed_name() else {
                continue;
            };
            let file_name = name.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !is_native_lib_file(file_name) {
                continue;
            }
            let out_path = natives_dir.join(file_name);
            if out_path.exists() {
                continue;
            }
            let mut out = std::fs::File::create(&out_path)
                .with_context(|| format!("写入 natives 文件失败 ({})", out_path.display()))?;
            std::io::copy(&mut entry, &mut out)
                .with_context(|| format!("解压 natives 文件失败 ({})", out_path.display()))?;
        }
    }

    Ok(())
}

/// 启动用
#[tauri::command]
pub async fn prepare_launch(
    app: &tauri::AppHandle,
    workspace_id: &str,
    player_name: &str,
) -> Result<PreparedLaunch, String> {
    let reporter = TauriProgressReporter::new(app.clone());

    let ws = wd(workspace_id);
    // 同步mod
    reporter.send("同步mod中....");
    sync_workspace_mods(workspace_id.to_string())?;
    let mut pack = read_pack_config(workspace_id).map_err(|e| e.to_string())?;

    // 下载部分全于该部分进行
    let runtime_result: mc_launcher_core::runtime::RuntimeResult =
        ensure_workspace_runtime(&reporter, workspace_id, &pack)
            .await
            .map_err(|e| e.to_string())?;
    
    reporter.send("环境准备完毕——");        
    let library_dir = shared_libraries_dir();
    let child_version_metadata = load_effective_version_metadata(
        &runtime_result.version_json_path,
        runtime_result.inherited_version_json_path.as_deref(),
    )?;

    let libraries = collect_libraries_from_metadata(&library_dir, &child_version_metadata.libraries);
    if libraries.is_empty() {
        return Err("未检测到 libraries 依赖，请先下载 MC 版本".into());
    }

    let native_libraries = collect_native_libraries_from_metadata(&library_dir, &child_version_metadata.libraries);
    prepare_natives_dir(&ws, &native_libraries).map_err(|e| e.to_string())?;

    // Collect JVM args
    pack.jvm_args.extend(
        [
            "--add-modules",
            "ALL-MODULE-PATH",
            "--add-opens",
            "java.base/java.lang=ALL-UNNAMED",
            "--add-opens",
            "java.base/java.util=ALL-UNNAMED",
            "--add-opens",
            "java.base/java.lang.reflect=ALL-UNNAMED",
            "--add-opens",
            "java.base/java.text=ALL-UNNAMED",
            "--add-opens",
            "java.desktop/java.awt=ALL-UNNAMED",
        ]
        .map(String::from),
    );

    let java_path = resolve_launch_java_path(pack.java_runtime_id.as_deref())?;

    let asset_index_id = child_version_metadata.asset_index.as_ref()
    .map(|index| index.id.clone())
    .unwrap_or(pack.mc_version);

    let assets_dir = mm().join("assets");

    let natives_dir = ws.join("natives");
    let logging_config = child_version_metadata
        .logging
        .as_ref()
        .and_then(|logging| default_logging_config_path(&ws.join("versions"), logging));

    let lc = LaunchConfig {
        java_path: java_path.into(),
        version_metadata: child_version_metadata,
        version_jar: runtime_result.client_jar_path,
        classpath: libraries,
        main_class: None,
        game_dir: ws.clone(),
        assets_dir,
        asset_index: asset_index_id,
        library_dir,
        natives_dir,
        logging_config,
        max_mem: format!("{}M", pack.max_memory_mb),
        min_mem: Some(format!("{}M", pack.min_memory_mb)),
        extra_jvm_args: pack.jvm_args,
        extra_game_args: Vec::new(),
        demo: false,
        width: Some(pack.window_width),
        height: Some(pack.window_height),
        launcher_name: "mmpc".into(),
        launcher_version: env!("CARGO_PKG_VERSION").into(),
    };

    let u = OfflineUser::new(player_name);
    let built = OfflineLauncher.build_command(&lc, &u).map_err(|e| e.to_string())?;

    Ok(PreparedLaunch {
        workspace_dir: ws,
        program: built.0.get_program().to_string_lossy().to_string(),
        argfile_path: built.1,
    })
}

#[tauri::command]
pub async fn launch_game(
    app: tauri::AppHandle,
    workspace_id: String,
    player_name: String,
) -> Result<u32, String> {
    let prepared = prepare_launch(&app, &workspace_id, &player_name).await?;
    let mut cmd = std::process::Command::new(&prepared.program);
    cmd.arg(format!("@{}", prepared.argfile_path.to_string_lossy()));

    // 后续代码均为启动后管理代码，主体代码在prepare_launch完成构建并导出argfile_path最后执行
    // Log command (argfile style)
    app.emit(
        "game-status",
        serde_json::json!({
            "state":"log",
            "message": format!(
                "{} @{}",
                prepared.program,
                prepared.argfile_path.display(),
            )
        }),
    )
    .ok();

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.current_dir(&prepared.workspace_dir);

    let mut ch = cmd
        .spawn()
        .map_err(|e| format!("启动游戏进程失败 (java: {}): {e}", prepared.program))?;
    let pid = ch.id();
    std::thread::spawn(move || {
        use std::io::Read;
        let stdout = ch.stdout.take();
        let stderr = ch.stderr.take();

        let stdout_app = app.clone();
        let stdout_thread = stdout.map(|mut so| {
            std::thread::spawn(move || {
                let mut bf = [0u8; 4096];
                while let Ok(n) = so.read(&mut bf) {
                    if n == 0 {
                        break;
                    }
                    let t = String::from_utf8_lossy(&bf[..n]).to_string();
                    stdout_app
                        .emit(
                            "game-status",
                            serde_json::json!({"state":"stdout","message":t}),
                        )
                        .ok();
                }
            })
        });

        let stderr_thread = stderr.map(|mut se| {
            let stderr_app = app.clone();
            std::thread::spawn(move || {
                let mut bf = [0u8; 4096];
                while let Ok(n) = se.read(&mut bf) {
                    if n == 0 {
                        break;
                    }
                    let t = String::from_utf8_lossy(&bf[..n]).to_string();
                    stderr_app
                        .emit(
                            "game-status",
                            serde_json::json!({"state":"stderr","message":t}),
                        )
                        .ok();
                }
            })
        });

        let _ = ch.wait();
        if let Some(handle) = stdout_thread {
            let _ = handle.join();
        }
        if let Some(handle) = stderr_thread {
            let _ = handle.join();
        }
        app.emit("game-status", serde_json::json!({"state":"stopped"}))
            .ok();
    });
    Ok(pid)
}

#[tauri::command]
pub fn stop_game(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        let status = std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .status()
            .map_err(|e| format!("执行 kill 失败: {e}"))?;
        if status.success() {
            return Ok(());
        }
        return Err(format!("终止游戏进程失败，退出码: {:?}", status.code()));
    }

    #[cfg(windows)]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .map_err(|e| format!("执行 taskkill 失败: {e}"))?;
        if status.success() {
            return Ok(());
        }
        return Err(format!("终止游戏进程失败，退出码: {:?}", status.code()));
    }

    #[allow(unreachable_code)]
    Err("当前平台暂不支持关闭游戏进程".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_launcher_core::launch::version::{DownloadFile, LibraryDownloads};

    #[test]
    fn collects_only_libraries_declared_in_metadata() {
        let temp = std::env::temp_dir().join(format!("mmpc-launch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(temp.join("com/example/keep/1.0")).unwrap();
        std::fs::create_dir_all(temp.join("com/example/skip/2.0")).unwrap();
        std::fs::write(temp.join("com/example/keep/1.0/keep-1.0.jar"), b"jar").unwrap();
        std::fs::write(temp.join("com/example/skip/2.0/skip-2.0.jar"), b"jar").unwrap();

        let libraries = vec![Library {
            name: "com.example:keep:1.0".to_string(),
            downloads: Some(LibraryDownloads {
                artifact: Some(DownloadFile {
                    id: None,
                    sha1: None,
                    size: None,
                    url: None,
                    path: Some("com/example/keep/1.0/keep-1.0.jar".to_string()),
                }),
                classifiers: None,
            }),
            ..Default::default()
        }];

        let resolved = collect_libraries_from_metadata(&temp, &libraries);
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].ends_with("com/example/keep/1.0/keep-1.0.jar"));
        assert!(!resolved.iter().any(|path| path.ends_with("skip-2.0.jar")));

        let _ = std::fs::remove_dir_all(&temp);
    }
}
