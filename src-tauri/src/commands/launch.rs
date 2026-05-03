use crate::commands::download::read_pack_config;

use super::download::ensure_workspace_runtime;
use super::java::resolve_launch_java_path;
use super::mods::sync_workspace_mods;
use mc_launcher_core::auth::offline::OfflineUser;
use mc_launcher_core::launch::offline::{LaunchConfig, OfflineLauncher};
use mc_launcher_core::launch::version::{
    default_logging_config_path, merge_version_chain, parse_version_metadata,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::Emitter;
use zip::read::ZipArchive;

pub struct PreparedLaunch {
    pub workspace_dir: PathBuf,
    pub program: String,
    pub argfile_path: PathBuf,
    pub libraries_count: usize,
    pub has_fabric_loader: bool,
}

fn mm() -> PathBuf {
    let e = std::env::current_exe().unwrap_or_default();
    e.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}
fn wd(id: &str) -> PathBuf {
    mm().join("workspaces").join(id)
}
fn ps(p: &PathBuf) -> &str {
    p.to_str().unwrap_or("")
}

fn format_arg_for_argfile(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quote = arg.chars().any(|c| c.is_whitespace() || c == '"');
    if !needs_quote {
        return arg.to_string();
    }
    let escaped = arg.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn write_java_argfile(ws: &PathBuf, args: &[String]) -> Result<PathBuf, String> {
    let launch_dir = ws.join("launch");
    std::fs::create_dir_all(&launch_dir).map_err(|e| format!("创建 launch 目录失败: {e}"))?;
    let argfile = launch_dir.join("java.args");
    let content = args
        .iter()
        .map(|a| format_arg_for_argfile(a))
        .collect::<Vec<_>>()
        .join(" ");
    std::fs::write(&argfile, content).map_err(|e| format!("写入 argfile 失败: {e}"))?;
    Ok(argfile)
}

/// Collect all .jar library paths under versions/libraries/ recursively
fn collect_libraries(ws: &PathBuf) -> Vec<PathBuf> {
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
            } else if path.extension().map_or(false, |e| e == "jar") {
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
            } else if path.extension().map_or(false, |e| e == "jar") {
                jars.push(path);
            }
        }
    }
    jars
}

fn is_native_jar(path: &PathBuf) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.contains("-natives-") && name.ends_with(".jar"))
        .unwrap_or(false)
}

fn is_native_lib_file(name: &str) -> bool {
    name.ends_with(".dll") || name.ends_with(".so") || name.ends_with(".dylib")
}

#[derive(Debug, Deserialize)]
struct AssetIndexObjects {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
}

fn prepare_natives_dir(ws: &PathBuf, libraries: &[PathBuf]) -> Result<(), String> {
    let natives_dir = ws.join("natives");
    std::fs::create_dir_all(&natives_dir).map_err(|e| format!("创建 natives 目录失败: {e}"))?;

    for lib in libraries {
        if !is_native_jar(lib) {
            continue;
        }

        let file = std::fs::File::open(lib)
            .map_err(|e| format!("打开 natives jar 失败 ({}): {e}", lib.display()))?;
        let mut zip = ZipArchive::new(file)
            .map_err(|e| format!("读取 natives jar 失败 ({}): {e}", lib.display()))?;

        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .map_err(|e| format!("读取 natives 条目失败 ({}): {e}", lib.display()))?;
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
            let mut out = std::fs::File::create(&out_path)
                .map_err(|e| format!("写入 natives 文件失败 ({}): {e}", out_path.display()))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("解压 natives 文件失败 ({}): {e}", out_path.display()))?;
        }
    }

    Ok(())
}

fn ensure_required_assets(ws: &PathBuf) -> Result<(), String> {
    let asset_index_path = ws.join("versions").join("asset_index.json");
    if !asset_index_path.exists() {
        return Err("缺少 asset_index.json，请先点击“下载/修复 MC 依赖”".into());
    }

    let asset_index_content = std::fs::read_to_string(&asset_index_path)
        .map_err(|e| format!("读取 asset_index.json 失败: {e}"))?;
    let asset_index: AssetIndexObjects = serde_json::from_str(&asset_index_content)
        .map_err(|e| format!("解析 asset_index.json 失败: {e}"))?;

    let assets_objects_dir = mm().join("assets").join("objects");
    let mut missing_count = 0usize;
    let mut sample_hashes = Vec::new();

    for obj in asset_index.objects.values() {
        if obj.hash.len() < 2 {
            continue;
        }
        let asset_path = assets_objects_dir.join(&obj.hash[..2]).join(&obj.hash);
        if !asset_path.is_file() {
            missing_count += 1;
            if sample_hashes.len() < 6 {
                sample_hashes.push(obj.hash.clone());
            }
        }
    }

    if missing_count > 0 {
        return Err(format!(
            "资源文件不完整，缺少 {missing_count} 个 assets，示例哈希: {}。请先点击“下载/修复 MC 依赖”完成补全后再启动。",
            sample_hashes.join(", ")
        ));
    }

    Ok(())
}

/// 同步工作区的mods, 
#[tauri::command]
pub async fn prepare_launch(
    app: &tauri::AppHandle,
    workspace_id: &str,
    player_name: &str,
    java_path: Option<String>,
) -> Result<PreparedLaunch, String> {
    let ws = wd(workspace_id);
    // 同步mod
    sync_workspace_mods(workspace_id.to_string())?;
    let mut pack = read_pack_config(workspace_id).map_err(String::from)?;
    
    let _ = ensure_workspace_runtime(app, workspace_id, &pack.mc_version).await?;
    let cj = ws.join("versions").join("client.jar");
    if !cj.exists() {
        return Err("no client.jar".into());
    }

    // Collect all library JARs into classpath
    let libraries = collect_libraries(&ws);
    if libraries.is_empty() {
        return Err("未检测到 libraries 依赖，请先下载 MC 版本".into());
    }
    let has_fabric_loader = libraries.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("fabric-loader-") && name.ends_with(".jar"))
            .unwrap_or(false)
    });
    ensure_required_assets(&ws)?;
    prepare_natives_dir(&ws, &libraries)?;

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

    let requested_java = java_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let jv = if let Some(path) = requested_java {
        path
    } else {
        resolve_launch_java_path(pack.java_runtime_id.as_deref())?
    };
    let version_json_path = ws.join("versions").join("version.json");
    let version_meta_raw = std::fs::read_to_string(&version_json_path)
        .map_err(|e| format!("读取 version.json 失败: {e}"))?;
    let version_meta = serde_json::from_str::<serde_json::Value>(&version_meta_raw)
        .map_err(|e| format!("解析 version.json 失败: {e}"))?;
    let child_version_metadata = parse_version_metadata(&version_meta_raw)
        .map_err(|e| format!("解析启动元数据失败: {e}"))?;
    let mut version_chain = Vec::new();
    if let Some(parent_id) = child_version_metadata.inherits_from.clone() {
        let parent_version_json_path = ws.join("versions").join(format!("{parent_id}.json"));
        if parent_version_json_path.exists() {
            let parent_raw = std::fs::read_to_string(&parent_version_json_path)
                .map_err(|e| format!("读取父级 version.json 失败: {e}"))?;
            let parent_metadata = parse_version_metadata(&parent_raw)
                .map_err(|e| format!("解析父级启动元数据失败: {e}"))?;
            version_chain.push(parent_metadata);
        }
    }
    version_chain.push(child_version_metadata);
    let parsed_version_metadata =
        merge_version_chain(&version_chain).map_err(|e| format!("合并启动元数据失败: {e}"))?;
    let asset_index_id = version_meta["assetIndex"]["id"].as_str().unwrap_or(&pack.mc_version);


    // assetsDir should point to the root that contains objects/
    let assets_dir = mm().join("assets");
    let library_dir = ws.join("versions").join("libraries");
    let natives_dir = ws.join("natives");
    let logging_config = parsed_version_metadata
        .logging
        .as_ref()
        .and_then(|logging| default_logging_config_path(&ws.join("versions"), logging));

    let mut b = LaunchConfig::builder()
        .java_path(&jv)
        .version_metadata(parsed_version_metadata)
        .minecraft_jar(ps(&cj))
        .game_dir(ps(&ws))
        .assets_dir(ps(&assets_dir))
        .asset_index(asset_index_id)
        .library_dir(&library_dir)
        .natives_dir(&natives_dir)
        .max_mem(&format!("{}M", pack.max_memory_mb))
        .min_mem(&format!("{}M", pack.min_memory_mb))
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
    let built = OfflineLauncher.build_command(&lc, &u);
    let program = built.get_program().to_string_lossy().to_string();
    let args = built
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let argfile = write_java_argfile(&ws, &args)?;

    Ok(PreparedLaunch {
        workspace_dir: ws,
        program,
        argfile_path: argfile,
        libraries_count: libraries.len(),
        has_fabric_loader,
    })
}

#[tauri::command]
pub async fn launch_game(
    app: tauri::AppHandle,
    workspace_id: String,
    player_name: String,
    java_path: Option<String>,
) -> Result<u32, String> {
    let prepared = prepare_launch(&app, &workspace_id, &player_name, java_path).await?;
    let mut cmd = std::process::Command::new(&prepared.program);
    cmd.arg(format!("@{}", prepared.argfile_path.to_string_lossy()));

    // Log command (argfile style)
    app.emit(
        "game-status",
        serde_json::json!({
            "state":"log",
            "message": format!(
                "{} @{} (libraries: {}, fabric_loader: {})",
                prepared.program,
                prepared.argfile_path.display(),
                prepared.libraries_count,
                if prepared.has_fabric_loader { "yes" } else { "no" }
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
    let a2 = app.clone();
    std::thread::spawn(move || {
        use std::io::Read;
        let stdout = ch.stdout.take();
        let stderr = ch.stderr.take();

        let stdout_app = a2.clone();
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
            let stderr_app = a2.clone();
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
        a2.emit("game-status", serde_json::json!({"state":"stopped"}))
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
