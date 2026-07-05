use std::fs;
use std::path::PathBuf;
use std::process::Command;

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaRuntime {
    pub id: String,
    pub name: String,
    pub path: String,
    pub version_text: String,
    pub major_version: Option<u32>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectJavaResult {
    pub version_text: String,
    pub major_version: Option<u32>,
}

fn mmpc_root() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.parent()
        .map(|p| p.join(".MMPC"))
        .unwrap_or_else(|| PathBuf::from(".MMPC"))
}

fn java_json_path() -> PathBuf {
    mmpc_root().join("javaruntimes.json")
}

fn parse_java_major_version(version_text: &str) -> Option<u32> {
    // Handles lines like:
    // openjdk version "21.0.2" ...
    // java version "17.0.11" ...
    // java version "1.8.0_381"
    let quoted = version_text
        .split('"')
        .nth(1)
        .unwrap_or("")
        .trim()
        .to_string();
    if quoted.is_empty() {
        return None;
    }
    if let Some(rest) = quoted.strip_prefix("1.") {
        return rest.split('.').next().and_then(|s| s.parse::<u32>().ok());
    }
    quoted.split('.').next().and_then(|s| s.parse::<u32>().ok())
}

fn detect_java_version(path: &str) -> Result<DetectJavaResult, String> {
    let out = Command::new(path)
        .arg("-version")
        .output()
        .map_err(|e| format!("执行 java -version 失败: {e}"))?;
    let txt = String::from_utf8_lossy(&out.stderr).to_string();
    if txt.trim().is_empty() {
        return Err("无法读取 java -version 输出".into());
    }
    let first_line = txt.lines().next().unwrap_or("").trim().to_string();
    let major = parse_java_major_version(&first_line);
    Ok(DetectJavaResult {
        version_text: first_line,
        major_version: major,
    })
}

fn java_executable_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

fn is_valid_java_binary(path: &str) -> bool {
    if path.trim().is_empty() {
        return false;
    }
    Command::new(path)
        .arg("-version")
        .output()
        .map(|output| output.status.success() || !output.stderr.is_empty())
        .unwrap_or(false)
}

fn load_runtimes() -> Result<Vec<JavaRuntime>, String> {
    let path = java_json_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取 Java 列表失败: {e}"))?;
    serde_json::from_str::<Vec<JavaRuntime>>(&content)
        .map_err(|e| format!("解析 Java 列表失败: {e}"))
}

fn runtime_path_key(path: &str) -> String {
    let raw = path.trim();
    if raw.is_empty() {
        return String::new();
    }
    match std::fs::canonicalize(raw) {
        Ok(resolved) => resolved.to_string_lossy().to_lowercase(),
        Err(_) => raw.to_lowercase(),
    }
}

fn save_runtimes(list: &[JavaRuntime]) -> Result<(), String> {
    let path = java_json_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 Java 目录失败: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(list).map_err(|e| format!("序列化 Java 列表失败: {e}"))?;
    fs::write(path, json).map_err(|e| format!("写入 Java 列表失败: {e}"))
}

fn append_runtime_if_absent(
    list: &mut Vec<JavaRuntime>,
    name: &str,
    path: &str,
) -> Result<bool, String> {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return Ok(false);
    }
    let key = runtime_path_key(trimmed_path);
    if key.is_empty() {
        return Ok(false);
    }
    if list
        .iter()
        .any(|runtime| runtime_path_key(&runtime.path) == key)
    {
        return Ok(false);
    }

    let detected = detect_java_version(trimmed_path)?;
    let runtime = JavaRuntime {
        id: format!("java-{}", Utc::now().timestamp_millis()),
        name: name.trim().to_string(),
        path: trimmed_path.to_string(),
        version_text: detected.version_text,
        major_version: detected.major_version,
        created_at: Utc::now().to_rfc3339(),
    };
    list.push(runtime);
    Ok(true)
}

#[tauri::command]
pub fn list_java_runtimes() -> Result<Vec<JavaRuntime>, String> {
    load_runtimes()
}

#[tauri::command]
pub fn detect_java_runtime(path: String) -> Result<DetectJavaResult, String> {
    detect_java_version(&path)
}

#[tauri::command]
pub fn add_java_runtime(name: String, path: String) -> Result<JavaRuntime, String> {
    if name.trim().is_empty() {
        return Err("Java 名称不能为空".into());
    }
    if path.trim().is_empty() {
        return Err("Java 路径不能为空".into());
    }
    let detected = detect_java_version(path.trim())?;

    let mut list = load_runtimes()?;
    if list.iter().any(|r| r.path == path.trim()) {
        return Err("该 Java 路径已存在".into());
    }

    let now = Utc::now().to_rfc3339();
    let id = format!("java-{}", Utc::now().timestamp_millis());
    let runtime = JavaRuntime {
        id,
        name: name.trim().to_string(),
        path: path.trim().to_string(),
        version_text: detected.version_text,
        major_version: detected.major_version,
        created_at: now,
    };
    list.push(runtime.clone());
    save_runtimes(&list)?;
    Ok(runtime)
}

#[tauri::command]
pub fn delete_java_runtime(id: String) -> Result<(), String> {
    let mut list = load_runtimes()?;
    let old_len = list.len();
    list.retain(|r| r.id != id);
    if list.len() == old_len {
        return Err("未找到指定 Java 运行时".into());
    }
    save_runtimes(&list)
}

pub fn resolve_java_path_by_id(id: &str) -> Result<Option<String>, String> {
    let list = load_runtimes()?;
    Ok(list.into_iter().find(|r| r.id == id).map(|r| r.path))
}

pub fn resolve_launch_java_path(runtime_id: Option<&str>) -> Result<String, String> {
    if let Some(id) = runtime_id {
        let configured = resolve_java_path_by_id(id)?
            .ok_or_else(|| format!("未找到已保存的 Java 运行时: {id}"))?;
        if is_valid_java_binary(&configured) {
            return Ok(configured);
        }
        return Err(format!("已保存的 Java 路径不可用: {configured}"));
    }

    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let candidate = PathBuf::from(java_home)
            .join("bin")
            .join(java_executable_name());
        if candidate.is_file() {
            let path = candidate.to_string_lossy().to_string();
            if is_valid_java_binary(&path) {
                return Ok(path);
            }
        }
    }

    if is_valid_java_binary("java") {
        return Ok("java".to_string());
    }

    Err("未找到可用的 Java 运行时，请先在“Java 管理”中添加可用的 Java".into())
}

pub fn auto_register_system_java() -> Result<usize, String> {
    let mut list = load_runtimes()?;
    let mut added = 0usize;

    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let java_home_path = PathBuf::from(java_home)
            .join("bin")
            .join(java_executable_name());
        if java_home_path.is_file() {
            let java_home_bin = java_home_path.to_string_lossy().to_string();
            if append_runtime_if_absent(&mut list, "System Java (JAVA_HOME)", &java_home_bin)? {
                added += 1;
            }
        }
    }

    if is_valid_java_binary("java")
        && append_runtime_if_absent(&mut list, "System Java (PATH)", "java")?
    {
        added += 1;
    }

    if added > 0 {
        save_runtimes(&list)?;
    }
    Ok(added)
}
