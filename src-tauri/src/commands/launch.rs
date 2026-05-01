use mc_launcher_core::auth::offline::OfflineUser;
use mc_launcher_core::launch::offline::{LaunchConfig, OfflineLauncher};
use std::path::PathBuf;
use tauri::Emitter;
use zip::read::ZipArchive;

fn mm() -> PathBuf {
    let e = std::env::current_exe().unwrap_or_default();
    e.parent().map(|p| p.join(".MMPC")).unwrap_or_else(|| PathBuf::from(".MMPC"))
}
fn wd(id: &str) -> PathBuf { mm().join("workspaces").join(id) }
fn ps(p: &PathBuf) -> &str { p.to_str().unwrap_or("") }

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

fn prepare_natives_dir(ws: &PathBuf, libraries: &[PathBuf]) -> Result<(), String> {
    let natives_dir = ws.join("natives");
    std::fs::create_dir_all(&natives_dir)
        .map_err(|e| format!("创建 natives 目录失败: {e}"))?;

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

#[tauri::command]
pub async fn launch_game(
    app: tauri::AppHandle,
    workspace_id: String,
    player_name: String,
    java_path: Option<String>,
) -> Result<u32, String> {
    let ws = wd(&workspace_id);
    let pk = ws.join("pack.json");
    let c = std::fs::read_to_string(&pk).map_err(|e| format!("read {e}"))?;
    let cfg: serde_json::Value = serde_json::from_str(&c).map_err(|e| format!("parse {e}"))?;
    let cj = ws.join("versions").join("client.jar");
    if !cj.exists() { return Err("no client.jar".into()); }

    // Collect all library JARs into classpath
    let libraries = collect_libraries(&ws);
    if libraries.is_empty() {
        return Err("未检测到 libraries 依赖，请先下载 MC 版本".into());
    }
    prepare_natives_dir(&ws, &libraries)?;

    // Collect JVM args
    let mut ja: Vec<String> = cfg["jvm_args"].as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    ja.extend([
        "--add-modules", "ALL-MODULAR-PATH",
        "--add-opens", "java.base/java.lang=ALL-UNNAMED",
        "--add-opens", "java.base/java.util=ALL-UNNAMED",
        "--add-opens", "java.base/java.lang.reflect=ALL-UNNAMED",
        "--add-opens", "java.base/java.text=ALL-UNNAMED",
        "--add-opens", "java.desktop/java.awt=ALL-UNNAMED",
    ].map(String::from));

    let jv = java_path.unwrap_or_else(|| "java".into());
    let mc_ver = cfg["mc_version"].as_str().unwrap_or("1.21");
    let mx = cfg["max_memory_mb"].as_u64().unwrap_or(4096);
    let mi = cfg["min_memory_mb"].as_u64().unwrap_or(1024);
    let w = cfg["window_width"].as_u64().unwrap_or(1280) as u32;
    let h = cfg["window_height"].as_u64().unwrap_or(720) as u32;

    // assetsDir should point to the root that contains objects/
    let assets_dir = mm().join("assets");

    let mut b = LaunchConfig::builder()
        .java_path(&jv).minecraft_jar(ps(&cj))
        .main_class("net.minecraft.client.main.Main")
        .game_dir(ps(&ws)).assets_dir(ps(&assets_dir))
        .asset_index(mc_ver)
        .max_mem(&format!("{mx}M")).min_mem(&format!("{mi}M"))
        .resolution(w, h);
    for a in &ja { b = b.add_jvm_arg(a); }
    // Add all library JARs to classpath
    for lib in &libraries {
        b = b.add_classpath(lib);
    }
    let lc = b.build();

    let u = OfflineUser::new(&player_name);
    let mut cmd = OfflineLauncher.build_command(&lc, &u);

    // Log command
    let cds: String = std::iter::once(cmd.get_program().to_string_lossy().to_string())
        .chain(cmd.get_args().map(|a| a.to_string_lossy().to_string()))
        .collect::<Vec<_>>().join(" ");
    app.emit("game-status", serde_json::json!({"state":"log","message":cds})).ok();

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.current_dir(&ws);

    let mut ch = cmd.spawn().map_err(|e| format!("spawn {e}"))?;
    let pid = ch.id();
    let a2 = app.clone();
    std::thread::spawn(move || {
        use std::io::Read;
        if let Some(mut se) = ch.stderr.take() {
            let mut bf = [0u8; 4096];
            while let Ok(n) = se.read(&mut bf) {
                if n == 0 { break; }
                let t = String::from_utf8_lossy(&bf[..n]).to_string();
                a2.emit("game-status", serde_json::json!({"state":"stderr","message":t})).ok();
            }
        }
        let _ = ch.wait();
        a2.emit("game-status", serde_json::json!({"state":"stopped"})).ok();
    });
    Ok(pid)
}
