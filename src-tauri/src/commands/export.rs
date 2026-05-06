use std::path::{Path, PathBuf};

use bmclapi::bmclapi;
use mc_launcher_core::runtime::LoaderKind;
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

use super::java::resolve_launch_java_path;
use super::launch::prepare_launch;
use super::workspace::{PackConfig, WorkspaceMod};

const MOJANG_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
const FORGE_MAVEN_BASE: &str = "https://files.minecraftforge.net/maven";
const NEOFORGE_MAVEN_BASE: &str = "https://maven.neoforged.net/releases";
const FABRIC_META_BASE: &str = "https://meta.fabricmc.net";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Client,
    Server,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub workspace_id: String,
    pub export_kind: ExportKind,
    #[serde(default)]
    pub include_java: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub export_dir: String,
    pub copied_mods: usize,
    pub included_java: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ExportProgressPayload {
    stage: String,
    current: usize,
    total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionManifest {
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionEntry {
    id: String,
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaVersionJson {
    downloads: VanillaVersionDownloads,
}

#[derive(Debug, Clone, Deserialize)]
struct VanillaVersionDownloads {
    server: Option<DownloadEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadEntry {
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FabricInstallerVersion {
    version: String,
    stable: bool,
}

fn mmpc_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}

fn shared_libraries_dir() -> PathBuf {
    mmpc_root().join("libraries")
}

fn workspace_dir(id: &str) -> PathBuf {
    mmpc_root().join("workspaces").join(id)
}

fn read_pack_config(id: &str) -> Result<PackConfig, String> {
    let path = workspace_dir(id).join("pack.json");
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 pack.json 失败 ({}): {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 pack.json 失败: {e}"))
}

fn emit_export_progress(
    app: &tauri::AppHandle,
    stage: impl Into<String>,
    current: usize,
    total: usize,
    message: Option<String>,
) {
    let _ = app.emit(
        "export-progress",
        ExportProgressPayload {
            stage: stage.into(),
            current,
            total,
            message,
        },
    );
}

fn copy_file(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败 ({}): {e}", parent.display()))?;
    }
    if src.is_symlink() {
        let src = src.read_link().map_err(|e| e.to_string())?;
        std::fs::copy(&src, dest).map_err(|e| {
            format!(
                "复制文件失败 ({} -> {}): {e}",
                src.display(),
                dest.display()
            )
        })?;
    } else {
        std::fs::copy(src, dest).map_err(|e| {
            format!(
                "复制文件失败 ({} -> {}): {e}",
                src.display(),
                dest.display()
            )
        })?;
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("创建目录失败 ({}): {e}", dest.display()))?;
    for entry in
        std::fs::read_dir(src).map_err(|e| format!("读取目录失败 ({}): {e}", src.display()))?
    {
        let entry = entry.map_err(|e| format!("读取目录项失败 ({}): {e}", src.display()))?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            copy_file(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path)
            .map_err(|e| format!("清理目录失败 ({}): {e}", path.display()))?;
    }
    Ok(())
}

fn should_include_mod(export_kind: ExportKind, mod_type: &str) -> bool {
    match export_kind {
        ExportKind::Client => matches!(mod_type, "client_only" | "client_and_server" | "unknown"),
        ExportKind::Server => matches!(mod_type, "server_only" | "client_and_server" | "unknown"),
        ExportKind::Full => true,
    }
}

fn filter_mods<'a>(mods: &'a [WorkspaceMod], export_kind: ExportKind) -> Vec<&'a WorkspaceMod> {
    mods.iter()
        .filter(|mod_entry| should_include_mod(export_kind, &mod_entry.mod_type))
        .collect()
}

fn rewrite_argfile_paths(
    content: &str,
    source_workspace_dir: &Path,
    source_assets_dir: &Path,
    source_libraries_dir: &Path,
    export_dir: &Path,
) -> String {
    let source = source_workspace_dir.to_string_lossy();
    let target = export_dir.to_string_lossy();
    let source_assets = source_assets_dir.to_string_lossy();
    let target_assets = export_dir.join("assets");
    let target_assets = target_assets.to_string_lossy();
    let source_libraries = source_libraries_dir.to_string_lossy();
    let target_libraries = export_dir.join("libraries");
    let target_libraries = target_libraries.to_string_lossy();
    content
        .replace(source.as_ref(), target.as_ref())
        .replace(source_libraries.as_ref(), target_libraries.as_ref())
        .replace(source_assets.as_ref(), target_assets.as_ref())
}

fn write_launch_scripts(export_dir: &Path, java_program: &str) -> Result<(), String> {
    let sh = export_dir.join("launch.sh");
    let bat = export_dir.join("launch.bat");
    std::fs::write(
        &sh,
        format!(
            "#!/usr/bin/env bash\nset -e\ncd \"$(dirname \"$0\")\"\n{} @launch/java.args\n",
            java_program
        ),
    )
    .map_err(|e| format!("写入 launch.sh 失败: {e}"))?;
    std::fs::write(
        &bat,
        format!(
            "@echo off\r\ncd /d \"%~dp0\"\r\n{} @launch\\java.args\r\n",
            java_program.replace('/', "\\")
        ),
    )
    .map_err(|e| format!("写入 launch.bat 失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&sh)
            .map_err(|e| format!("读取 launch.sh 权限失败: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&sh, perms)
            .map_err(|e| format!("设置 launch.sh 权限失败: {e}"))?;
    }
    Ok(())
}

fn open_dir(app: &tauri::AppHandle, dir: &Path) -> Result<(), String> {
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("打开导出目录失败: {e}"))
}

fn loader_kind(pack: &PackConfig) -> LoaderKind {
    LoaderKind::from_str(&pack.loader_type)
}

fn workspace_java_path(pack: &PackConfig) -> Result<String, String> {
    resolve_launch_java_path(pack.java_runtime_id.as_deref())
}

async fn fetch_json(url: &str, label: &str) -> Result<serde_json::Value, String> {
    let response = bmclapi::request(url)
        .await
        .map_err(|e| format!("{label} 请求失败: {e}"))?;
    let response = response
        .error_for_status()
        .map_err(|e| format!("{label} 状态异常: {e}"))?;
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("{label} 解析失败: {e}"))
}

async fn download_file(url: &str, dest: &Path) -> Result<(), String> {
    let response = bmclapi::request(url)
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let response = response
        .error_for_status()
        .map_err(|e| format!("状态异常: {e}"))?;
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目录失败 ({}): {e}", parent.display()))?;
    }
    std::fs::write(dest, bytes).map_err(|e| format!("写入文件失败 ({}): {e}", dest.display()))
}

async fn fetch_vanilla_server_download(mc_version: &str) -> Result<String, String> {
    let manifest_value = fetch_json(MOJANG_MANIFEST_URL, "获取版本清单").await?;
    let manifest: VersionManifest =
        serde_json::from_value(manifest_value).map_err(|e| format!("解析版本清单失败: {e}"))?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|entry| entry.id == mc_version)
        .ok_or_else(|| format!("未找到 MC 版本 {mc_version}"))?;

    let version_value = bmclapi::fetch_json_value(&entry.url)
        .await
        .map_err(|e| e.to_string())?;
    let version_json: VanillaVersionJson = serde_json::from_value(version_value)
        .map_err(|e| format!("解析 version.json 失败: {e}"))?;
    let server = version_json
        .downloads
        .server
        .ok_or_else(|| format!("MC {mc_version} 没有可用的服务端下载"))?;
    Ok(server.url)
}

async fn fetch_latest_fabric_installer_version() -> Result<String, String> {
    let value = fetch_json(
        &format!("{FABRIC_META_BASE}/v2/versions/installer"),
        "获取 Fabric installer 版本",
    )
    .await?;
    let versions: Vec<FabricInstallerVersion> = serde_json::from_value(value)
        .map_err(|e| format!("解析 Fabric installer 版本失败: {e}"))?;
    let mut stable = None;
    let mut first = None;
    for item in versions {
        if first.is_none() {
            first = Some(item.clone());
        }
        if item.stable {
            stable = Some(item);
            break;
        }
    }
    stable
        .or(first)
        .map(|item| item.version)
        .ok_or_else(|| "Fabric installer 版本列表为空".to_string())
}

fn installer_path_for_export(
    loader: LoaderKind,
    mc_version: &str,
    loader_version: &str,
) -> PathBuf {
    let root = mmpc_root().join("cache").join("installers");
    let filename = match loader {
        LoaderKind::Forge => format!(
            "forge-{}-{}-installer.jar",
            mc_version,
            loader_version.trim()
        ),
        LoaderKind::NeoForge => format!("neoforge-{}-installer.jar", loader_version.trim()),
        LoaderKind::Fabric => format!(
            "fabric-{}-{}-installer.jar",
            mc_version,
            loader_version.trim()
        ),
        LoaderKind::Vanilla => format!("minecraft-{}-installer.jar", mc_version),
    };
    root.join(filename)
}

fn forge_installer_urls(
    loader: LoaderKind,
    mc_version: &str,
    loader_version: &str,
) -> Result<String, String> {
    match loader {
        LoaderKind::Forge => {
            let version = format!("{}-{}", mc_version, loader_version.trim());
            Ok(format!(
                "{FORGE_MAVEN_BASE}/net/minecraftforge/forge/{0}/forge-{0}-installer.jar",
                version
            ))
        }
        LoaderKind::NeoForge => {
            let version = loader_version.trim();
            Ok(format!(
                "{NEOFORGE_MAVEN_BASE}/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar",
                version
            ))
        }
        _ => Err("当前 loader 不支持 installer".to_string()),
    }
}

fn java_bin_relative(include_java: bool) -> &'static str {
    if include_java {
        if cfg!(windows) {
            ".\\java\\bin\\java.exe"
        } else {
            "./java/bin/java"
        }
    } else {
        "java"
    }
}

fn replace_script_java(script: &str, java_program: &str) -> String {
    let mut output = script.replace(
        "java @user_jvm_args.txt",
        &format!("{java_program} @user_jvm_args.txt"),
    );
    output = output.replace("java -jar", &format!("{java_program} -jar"));
    output
}

fn rewrite_server_script_file(path: &Path, java_program: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let script = std::fs::read_to_string(path)
        .map_err(|e| format!("读取脚本失败 ({}): {e}", path.display()))?;
    let rewritten = replace_script_java(&script, java_program);
    std::fs::write(path, rewritten)
        .map_err(|e| format!("写入脚本失败 ({}): {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| format!("读取脚本权限失败 ({}): {e}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)
            .map_err(|e| format!("设置脚本权限失败 ({}): {e}", path.display()))?;
    }
    Ok(())
}

fn ensure_custom_user_jvm_args(export_dir: &Path, pack: &PackConfig) -> Result<(), String> {
    let user_jvm_args = export_dir.join("user_jvm_args.txt");
    if !user_jvm_args.exists() {
        return Ok(());
    }
    let mut lines = std::fs::read_to_string(&user_jvm_args)
        .map_err(|e| format!("读取 user_jvm_args.txt 失败: {e}"))?
        .lines()
        .map(str::to_string)
        .collect::<Vec<_>>();
    lines.retain(|line| {
        let trimmed = line.trim();
        !(trimmed.starts_with("-Xmx") || trimmed.starts_with("-Xms"))
    });
    lines.push(format!("-Xms{}M", pack.min_memory_mb.max(256)));
    lines.push(format!(
        "-Xmx{}M",
        pack.max_memory_mb.max(pack.min_memory_mb)
    ));
    let content = lines.join("\n");
    std::fs::write(&user_jvm_args, content)
        .map_err(|e| format!("写入 user_jvm_args.txt 失败: {e}"))?;
    Ok(())
}

fn prepare_export_dir(_request: &ExportRequest) -> Result<PathBuf, String> {
    let tmp_root = mmpc_root().join("tmp");
    std::fs::create_dir_all(&tmp_root).map_err(|e| format!("创建 tmp 目录失败: {e}"))?;
    let export_dir = tmp_root.join(Uuid::new_v4().to_string());
    remove_dir_if_exists(&export_dir)?;
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("创建导出目录失败 ({}): {e}", export_dir.display()))?;
    Ok(export_dir)
}

fn copy_filtered_mods(
    source_workspace_dir: &Path,
    export_dir: &Path,
    filtered_mods: &[&WorkspaceMod],
) -> Result<(), String> {
    std::fs::create_dir_all(export_dir.join("mods"))
        .map_err(|e| format!("创建导出 mods 目录失败: {e}"))?;
    for mod_entry in filtered_mods {
        let src = source_workspace_dir.join("mods").join(&mod_entry.file_name);
        let resolved = std::fs::canonicalize(&src).unwrap_or(src);
        copy_file(
            &resolved,
            &export_dir.join("mods").join(&mod_entry.file_name),
        )?;
    }
    Ok(())
}

fn copy_export_java(
    pack: &PackConfig,
    export_dir: &Path,
    include_java: bool,
) -> Result<String, String> {
    if include_java {
        let java_path = resolve_launch_java_path(pack.java_runtime_id.as_deref())?;
        let java_bin = PathBuf::from(&java_path);
        let java_home = java_bin
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| format!("无法确定 Java Home: {java_path}"))?;
        copy_dir_recursive(java_home, &export_dir.join("java"))?;
    }
    Ok(java_bin_relative(include_java).to_string())
}

async fn export_client_runtime(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    pack: &PackConfig,
    export_dir: &Path,
) -> Result<usize, String> {
    emit_export_progress(app, "准备客户端运行时", 0, 5, None);
    let player_name = pack.name.clone();
    let prepared = prepare_launch(app, &request.workspace_id, &player_name).await?;
    let source_workspace_dir = workspace_dir(&request.workspace_id);
    let source_assets_dir = mmpc_root().join("assets");
    let source_libraries_dir = shared_libraries_dir();

    emit_export_progress(app, "复制版本文件", 1, 5, None);
    copy_dir_recursive(
        &source_workspace_dir.join("versions"),
        &export_dir.join("versions"),
    )?;

    emit_export_progress(app, "复制本地库与 natives", 2, 5, None);
    copy_dir_recursive(&source_libraries_dir, &export_dir.join("libraries"))?;
    copy_dir_recursive(
        &source_workspace_dir.join("natives"),
        &export_dir.join("natives"),
    )?;

    emit_export_progress(app, "复制 assets", 3, 5, None);
    copy_dir_recursive(&source_assets_dir, &export_dir.join("assets"))?;

    let filtered_mods = filter_mods(&pack.mods, request.export_kind);
    emit_export_progress(
        app,
        "复制模组",
        4,
        5,
        Some(format!("{} 个模组", filtered_mods.len())),
    );
    copy_filtered_mods(&source_workspace_dir, export_dir, &filtered_mods)?;

    let java_program = copy_export_java(pack, export_dir, request.include_java)?;
    let argfile_content = std::fs::read_to_string(&prepared.argfile_path)
        .map_err(|e| format!("读取 java.args 失败: {e}"))?;
    let rewritten = rewrite_argfile_paths(
        &argfile_content,
        &source_workspace_dir,
        &source_assets_dir,
        &source_libraries_dir,
        export_dir,
    );
    let export_argfile = export_dir.join("launch").join("java.args");
    if let Some(parent) = export_argfile.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建导出 launch 目录失败: {e}"))?;
    }
    std::fs::write(&export_argfile, rewritten)
        .map_err(|e| format!("写入导出 java.args 失败: {e}"))?;
    write_launch_scripts(export_dir, &java_program)?;
    emit_export_progress(app, "完成客户端导出", 5, 5, None);

    Ok(filtered_mods.len())
}

async fn export_vanilla_server(
    app: &tauri::AppHandle,
    pack: &PackConfig,
    export_dir: &Path,
    include_java: bool,
) -> Result<(), String> {
    emit_export_progress(app, "获取原版服务端", 0, 3, None);
    let server_url = fetch_vanilla_server_download(&pack.mc_version).await?;
    let server_jar = export_dir.join(format!("server-{}.jar", pack.mc_version));
    download_file(&server_url, &server_jar).await?;

    emit_export_progress(app, "写入启动脚本", 1, 3, None);
    let java_program = copy_export_java(pack, export_dir, include_java)?;
    let sh = export_dir.join("run.sh");
    let bat = export_dir.join("run.bat");
    std::fs::write(
        &sh,
        format!(
            "#!/usr/bin/env bash\nset -e\ncd \"$(dirname \"$0\")\"\n{} -Xms{}M -Xmx{}M -jar \"{}\" nogui \"$@\"\n",
            java_program,
            pack.min_memory_mb.max(256),
            pack.max_memory_mb.max(pack.min_memory_mb),
            server_jar
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("server.jar")
        ),
    )
    .map_err(|e| format!("写入 run.sh 失败: {e}"))?;
    std::fs::write(
        &bat,
        format!(
            "@echo off\r\ncd /d \"%~dp0\"\r\n{} -Xms{}M -Xmx{}M -jar \"{}\" nogui %*\r\n",
            java_program.replace('/', "\\"),
            pack.min_memory_mb.max(256),
            pack.max_memory_mb.max(pack.min_memory_mb),
            server_jar
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("server.jar")
        ),
    )
    .map_err(|e| format!("写入 run.bat 失败: {e}"))?;
    rewrite_server_script_file(&sh, &java_program)?;
    rewrite_server_script_file(&bat, &java_program)?;
    emit_export_progress(app, "完成原版服务端导出", 2, 3, None);
    Ok(())
}

async fn export_fabric_server(
    app: &tauri::AppHandle,
    pack: &PackConfig,
    export_dir: &Path,
    include_java: bool,
) -> Result<(), String> {
    let loader_version = pack
        .loader_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Fabric 服务端导出缺少 loader_version".to_string())?;
    emit_export_progress(app, "获取 Fabric 服务端版本", 0, 4, None);
    let installer_version = fetch_latest_fabric_installer_version().await?;
    let server_jar = export_dir.join(format!(
        "fabric-server-mc.{}-loader.{}-launcher.{}.jar",
        pack.mc_version, loader_version, installer_version
    ));
    let server_jar_url = format!(
        "{FABRIC_META_BASE}/v2/versions/loader/{}/{}/{}/server/jar",
        pack.mc_version, loader_version, installer_version
    );
    emit_export_progress(app, "下载 Fabric 服务端启动器", 1, 4, None);
    download_file(&server_jar_url, &server_jar).await?;

    let vanilla_server_url = fetch_vanilla_server_download(&pack.mc_version).await?;
    emit_export_progress(app, "下载原版 server.jar", 2, 4, None);
    download_file(&vanilla_server_url, &export_dir.join("server.jar")).await?;

    emit_export_progress(app, "写入启动脚本", 3, 4, None);
    let java_program = copy_export_java(pack, export_dir, include_java)?;
    std::fs::write(
        export_dir.join("run.sh"),
        format!(
            "#!/usr/bin/env bash\nset -e\ncd \"$(dirname \"$0\")\"\n{} -Xms{}M -Xmx{}M -jar \"{}\" nogui \"$@\"\n",
            java_program,
            pack.min_memory_mb.max(256),
            pack.max_memory_mb.max(pack.min_memory_mb),
            server_jar
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("fabric-server-launch.jar")
        ),
    )
    .map_err(|e| format!("写入 run.sh 失败: {e}"))?;
    std::fs::write(
        export_dir.join("run.bat"),
        format!(
            "@echo off\r\ncd /d \"%~dp0\"\r\n{} -Xms{}M -Xmx{}M -jar \"{}\" nogui %*\r\n",
            java_program.replace('/', "\\"),
            pack.min_memory_mb.max(256),
            pack.max_memory_mb.max(pack.min_memory_mb),
            server_jar
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("fabric-server-launch.jar")
        ),
    )
    .map_err(|e| format!("写入 run.bat 失败: {e}"))?;
    rewrite_server_script_file(&export_dir.join("run.sh"), &java_program)?;
    rewrite_server_script_file(&export_dir.join("run.bat"), &java_program)?;
    emit_export_progress(app, "完成 Fabric 服务端导出", 4, 4, None);
    Ok(())
}

async fn export_installer_server(
    app: &tauri::AppHandle,
    pack: &PackConfig,
    export_dir: &Path,
    loader: LoaderKind,
    include_java: bool,
) -> Result<(), String> {
    let loader_version = pack
        .loader_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "服务端导出缺少 loader_version".to_string())?;
    let label = match loader {
        LoaderKind::Forge => "Forge",
        LoaderKind::NeoForge => "NeoForge",
        _ => "Loader",
    };
    emit_export_progress(app, format!("准备 {label} installer"), 0, 5, None);

    let installer_path = installer_path_for_export(loader, &pack.mc_version, loader_version);
    if !installer_path.exists() {
        let installer_url = forge_installer_urls(loader, &pack.mc_version, loader_version)?;
        emit_export_progress(app, format!("下载 {label} installer"), 1, 5, None);
        download_file(&installer_url, &installer_path).await?;
    } else {
        emit_export_progress(
            app,
            format!("下载 {label} installer"),
            1,
            5,
            Some("使用缓存".to_string()),
        );
    }

    let temp_export = mmpc_root().join("tmp").join(format!(
        "server-export-{}-{}",
        pack.id,
        match loader {
            LoaderKind::Forge => "forge",
            LoaderKind::NeoForge => "neoforge",
            _ => "server",
        }
    ));
    remove_dir_if_exists(&temp_export)?;
    std::fs::create_dir_all(&temp_export)
        .map_err(|e| format!("创建临时服务端目录失败 ({}): {e}", temp_export.display()))?;

    emit_export_progress(app, format!("运行 {label} installer"), 2, 5, None);
    let java_path = workspace_java_path(pack)?;
    let install_arg = match loader {
        LoaderKind::Forge => "--installServer",
        LoaderKind::NeoForge => "--install-server",
        _ => return Err("当前 loader 不支持服务端 installer".to_string()),
    };
    let installer_path_clone = installer_path.clone();
    let temp_export_clone = temp_export.clone();
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(java_path)
            .arg("-jar")
            .arg(installer_path_clone)
            .arg(install_arg)
            .arg(&temp_export_clone)
            .output()
    })
    .await
    .map_err(|e| format!("执行 {label} installer 任务失败: {e}"))?
    .map_err(|e| format!("启动 {label} installer 失败: {e}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = [stdout.trim(), stderr.trim()]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(format!(
            "{label} installer 执行失败: {}",
            if detail.is_empty() {
                "未返回可用日志".to_string()
            } else {
                detail
            }
        ));
    }

    emit_export_progress(app, format!("复制 {label} 服务端文件"), 3, 5, None);
    copy_dir_recursive(&temp_export, export_dir)?;
    let filtered_mods = filter_mods(&pack.mods, ExportKind::Server);
    copy_filtered_mods(&workspace_dir(&pack.id), export_dir, &filtered_mods)?;

    emit_export_progress(app, "调整启动脚本", 4, 5, None);
    let java_program = copy_export_java(pack, export_dir, include_java)?;
    rewrite_server_script_file(&export_dir.join("run.sh"), &java_program)?;
    rewrite_server_script_file(&export_dir.join("run.bat"), &java_program)?;
    ensure_custom_user_jvm_args(export_dir, pack)?;

    emit_export_progress(
        app,
        format!("完成 {label} 服务端导出"),
        5,
        5,
        Some(format!("{} 个模组", filtered_mods.len())),
    );
    Ok(())
}

async fn export_server_runtime(
    app: &tauri::AppHandle,
    pack: &PackConfig,
    export_dir: &Path,
    include_java: bool,
) -> Result<usize, String> {
    let filtered_mods = filter_mods(&pack.mods, ExportKind::Server);
    match loader_kind(pack) {
        LoaderKind::Vanilla => {
            export_vanilla_server(app, pack, export_dir, include_java).await?;
            copy_filtered_mods(&workspace_dir(&pack.id), export_dir, &filtered_mods)?;
        }
        LoaderKind::Fabric => {
            export_fabric_server(app, pack, export_dir, include_java).await?;
            copy_filtered_mods(&workspace_dir(&pack.id), export_dir, &filtered_mods)?;
        }
        LoaderKind::Forge | LoaderKind::NeoForge => {
            export_installer_server(app, pack, export_dir, loader_kind(pack), include_java).await?;
        }
    }
    Ok(filtered_mods.len())
}

#[tauri::command]
pub async fn export_workspace(
    app: tauri::AppHandle,
    request: ExportRequest,
) -> Result<ExportResult, String> {
    let pack = read_pack_config(&request.workspace_id)?;
    let export_dir = prepare_export_dir(&request)?;
    emit_export_progress(
        &app,
        "开始导出",
        0,
        1,
        Some(format!(
            "{} / {}",
            pack.name,
            match request.export_kind {
                ExportKind::Client => "客户端",
                ExportKind::Server => "服务端",
                ExportKind::Full => "全量",
            }
        )),
    );

    let copied_mods = match request.export_kind {
        ExportKind::Server => {
            export_server_runtime(&app, &pack, &export_dir, request.include_java).await?
        }
        ExportKind::Client | ExportKind::Full => {
            export_client_runtime(&app, &request, &pack, &export_dir).await?
        }
    };

    open_dir(&app, &export_dir)?;
    app.emit(
        "game-status",
        serde_json::json!({
            "state": "log",
            "message": format!("导出完成: {}", export_dir.display())
        }),
    )
    .ok();
    emit_export_progress(
        &app,
        "导出完成",
        1,
        1,
        Some(export_dir.to_string_lossy().to_string()),
    );

    Ok(ExportResult {
        export_dir: export_dir.to_string_lossy().to_string(),
        copied_mods,
        included_java: request.include_java,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_mods_by_kind() {
        assert!(should_include_mod(ExportKind::Client, "client_only"));
        assert!(should_include_mod(ExportKind::Client, "client_and_server"));
        assert!(!should_include_mod(ExportKind::Client, "server_only"));
        assert!(should_include_mod(ExportKind::Server, "server_only"));
        assert!(!should_include_mod(ExportKind::Server, "client_only"));
        assert!(should_include_mod(ExportKind::Full, "development_only"));
    }

    #[test]
    fn rewrites_paths_and_java() {
        let source = PathBuf::from("/tmp/source");
        let assets = PathBuf::from("/tmp/.MMPC/assets");
        let libraries = PathBuf::from("/tmp/.MMPC/libraries");
        let target = PathBuf::from("/tmp/export");
        let input = "-cp /tmp/.MMPC/libraries/a.jar Main --gameDir /tmp/source --assetsDir /tmp/.MMPC/assets";
        let output = rewrite_argfile_paths(input, &source, &assets, &libraries, &target);
        assert!(output.contains("/tmp/export/libraries/a.jar"));
        assert!(output.contains("--gameDir /tmp/export"));
        assert!(output.contains("--assetsDir /tmp/export/assets"));
    }
}
