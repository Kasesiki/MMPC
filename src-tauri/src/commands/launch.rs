use crate::commands::download::{read_pack_config, TauriProgressReporter};

use super::download::ensure_workspace_runtime;
use super::java::resolve_launch_java_path;
use super::mods::sync_workspace_mods;
use anyhow::{Context, Result};
use mc_launcher_core::auth::offline::OfflineUser;
use mc_launcher_core::launch::offline::{LaunchConfig, OfflineLauncher};
use mc_launcher_core::launch::version::{default_logging_config_path, parse_version_metadata};
use mc_launcher_core::runtime::prepare::{mm, wd};
use mc_launcher_core::runtime::ProgressReporter;
use std::path::{Path, PathBuf};
use tauri::Emitter;
use zip::read::ZipArchive;

pub struct PreparedLaunch {
    pub workspace_dir: PathBuf,
    pub program: String,
    pub argfile_path: PathBuf,
}

/// Collect all .jar library paths under versions/libraries/ recursively
fn collect_libraries(ws: &Path) -> Vec<PathBuf> {
    let libs_dir = ws.join("versions").join("libraries");
    let mut jars = Vec::new();
    if !libs_dir.exists() {
        return jars;
    }
    if let Ok(entries) = std::fs::read_dir(&libs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Recurse into subdirectories (libraries are stored as Maven paths)
                jars.extend(collect_libs_recursive(&path));
            } else if path.extension().is_some_and(|e| e == "jar") {
                jars.push(path);
            }
        }
    }
    jars
}

fn collect_libs_recursive(dir: &PathBuf) -> Vec<PathBuf> {
    let mut jars = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                jars.extend(collect_libs_recursive(&path));
            } else if path.extension().is_some_and(|e| e == "jar") {
                jars.push(path);
            }
        }
    }
    jars
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
    // Collect all library JARs into classpath
    let libraries = collect_libraries(&ws);
    if libraries.is_empty() {
        return Err("未检测到 libraries 依赖，请先下载 MC 版本".into());
    }

    prepare_natives_dir(&ws, &libraries).map_err(|e| e.to_string())?;

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

    let version_meta_raw = std::fs::read_to_string(&runtime_result.version_json_path)
        .map_err(|e| format!("读取 version.json 失败: {e}"))?;
    let child_version_metadata = parse_version_metadata(&version_meta_raw)
        .map_err(|e| format!("解析启动元数据失败: {e}"))?;
    
    let asset_index_id = child_version_metadata.asset_index.as_ref()
    .map(|index| index.id.clone())
    .unwrap_or(pack.mc_version);

    let assets_dir = mm().join("assets");

    let library_dir = runtime_result.version_json_path.join("libraries");
    let natives_dir = ws.join("natives");
    let logging_config = child_version_metadata
        .logging
        .as_ref()
        .and_then(|logging| default_logging_config_path(&ws.join("versions"), logging));

    let mut b = LaunchConfig::builder()
        .java_path(&java_path)
        .version_metadata(child_version_metadata)
        .minecraft_jar(&runtime_result.client_jar_path)
        .game_dir(&ws)
        .assets_dir(&assets_dir)
        .asset_index(asset_index_id)
        .library_dir(&library_dir)
        .natives_dir(&natives_dir)
        .max_mem(format!("{}M", pack.max_memory_mb))
        .min_mem(format!("{}M", pack.min_memory_mb))
        .resolution(pack.window_width, pack.window_height);
    if let Some(logging_config) = logging_config {
        b = b.logging_config(logging_config);
    }
    for a in &pack.jvm_args {
        b = b.add_jvm_arg(a);
    }
    // Add all library JARs to classpath
    for lib in &libraries {
        b = b.add_classpath(lib);
    }
    let lc = b.build();

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
