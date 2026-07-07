use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::launch::LaunchError;
use crate::runtime::{GLOBAL_ASSETS, GLOBAL_LIBRARIES};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionMetadata {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(rename = "mainClass", default)]
    pub main_class: Option<String>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub arguments: Option<VersionArguments>,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub logging: Option<LoggingConfig>,
    #[serde(rename = "type", default)]
    pub version_type: Option<String>,
    #[serde(default)]
    pub downloads: Option<Downloads>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionArguments {
    #[serde(default)]
    pub game: Vec<ArgumentSpec>,
    #[serde(default)]
    pub jvm: Vec<ArgumentSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentSpec {
    Plain(String),
    Conditional(ConditionalArgument),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalArgument {
    #[serde(default)]
    pub rules: Vec<Rule>,
    pub value: ArgumentValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<RuleOs>,
    #[serde(default)]
    pub features: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleOs {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub natives: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<DownloadFile>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, DownloadFile>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggingConfig {
    #[serde(default)]
    pub client: Option<LoggingClientConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingClientConfig {
    pub argument: String,
    pub file: DownloadFile,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Downloads {
    #[serde(default)]
    pub client: Option<DownloadFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFile {
    pub id: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchArgumentContext {
    pub auth_player_name: String,
    pub auth_uuid: String,
    pub auth_access_token: String,
    pub auth_xuid: String,
    pub client_id: String,
    pub version_name: String,
    pub game_directory: String,
    pub assets_index_name: String,
    pub user_type: String,
    pub version_type: String,
    pub natives_directory: String,
    pub launcher_name: String,
    pub launcher_version: String,
    pub classpath: String,
    pub classpath_separator: String,
    pub resolution_width: Option<String>,
    pub resolution_height: Option<String>,
    pub feature_flags: HashMap<String, bool>,
    pub logging_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedLaunchArguments {
    pub version_name: String,
    pub main_class: String,
    pub asset_index_name: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchLayout {
    pub game_dir: PathBuf,
    pub natives_dir: PathBuf,
    pub client_jar: PathBuf,
    pub classpath_entries: Vec<PathBuf>,
    pub logging_config: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub version: VersionMetadata,
    pub layout: LaunchLayout,
    pub main_class: String,
    pub asset_index_id: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

pub fn parse_version_metadata(content: &str) -> Result<VersionMetadata, LaunchError> {
    serde_json::from_str(content)
        .map_err(|e| LaunchError::InvalidConfig(format!("invalid version metadata: {e}")))
}

pub fn merge_version_metadata(
    parent: &VersionMetadata,
    child: &VersionMetadata,
) -> VersionMetadata {
    let mut merged = parent.clone();

    if !child.id.is_empty() {
        merged.id = child.id.clone();
    }
    merged.inherits_from = child.inherits_from.clone();
    if child.assets.is_some() {
        merged.assets = child.assets.clone();
    }
    if child.main_class.is_some() {
        merged.main_class = child.main_class.clone();
    }
    if child.minecraft_arguments.is_some() {
        merged.minecraft_arguments = child.minecraft_arguments.clone();
    }
    if child.asset_index.is_some() {
        merged.asset_index = child.asset_index.clone();
    }
    if child.logging.is_some() {
        merged.logging = child.logging.clone();
    }
    if child.version_type.is_some() {
        merged.version_type = child.version_type.clone();
    }
    if child.downloads.is_some() {
        merged.downloads = child.downloads.clone();
    }

    merged.libraries = merge_libraries(&parent.libraries, &child.libraries);

    let mut merged_arguments = merged.arguments.clone().unwrap_or_default();
    if let Some(child_args) = &child.arguments {
        merged_arguments.jvm.extend(child_args.jvm.clone());
        merged_arguments.game.extend(child_args.game.clone());
    }
    if !merged_arguments.jvm.is_empty() || !merged_arguments.game.is_empty() {
        merged.arguments = Some(merged_arguments);
    }

    merged
}

fn merge_libraries(parent: &[Library], child: &[Library]) -> Vec<Library> {
    let mut merged = Vec::new();
    let mut indices = HashMap::<String, usize>::new();

    for lib in parent.iter().chain(child.iter()) {
        let key = library_key(&lib.name);
        if let Some(index) = indices.get(&key).copied() {
            merged[index] = lib.clone();
        } else {
            indices.insert(key, merged.len());
            merged.push(lib.clone());
        }
    }

    merged
}

fn library_key(name: &str) -> String {
    let trimmed = name.trim();
    let (coords, ext) = match trimmed.rsplit_once('@') {
        Some((coords, ext)) => (coords, ext),
        None => (trimmed, "jar"),
    };
    let parts = coords.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        return trimmed.to_string();
    }
    let classifier = parts.get(3).copied().unwrap_or("");
    format!("{}:{}:{}@{}", parts[0], parts[1], classifier, ext)
}

pub fn resolve_launch_plan(
    version: &VersionMetadata,
    layout: LaunchLayout,
    context: &LaunchArgumentContext,
) -> Result<LaunchPlan, LaunchError> {
    let arguments = resolve_launch_arguments(version, context)?;
    Ok(LaunchPlan {
        version: version.clone(),
        layout,
        main_class: arguments.main_class,
        asset_index_id: arguments.asset_index_name,
        jvm_args: arguments.jvm_args,
        game_args: arguments.game_args,
    })
}

pub fn resolve_launch_arguments(
    metadata: &VersionMetadata,
    context: &LaunchArgumentContext,
) -> Result<ResolvedLaunchArguments, LaunchError> {
    let main_class = metadata.main_class.clone().ok_or_else(|| {
        LaunchError::InvalidConfig("mainClass is missing in version metadata".into())
    })?;
    let asset_index_name = metadata
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .or_else(|| metadata.assets.clone())
        .unwrap_or_else(|| context.assets_index_name.clone());
    let version_name = if metadata.id.is_empty() {
        context.version_name.clone()
    } else {
        metadata.id.clone()
    };

    let replacements = build_replacements(context, &version_name, &asset_index_name);

    let mut jvm_args = metadata
        .arguments
        .as_ref()
        .map(|a| resolve_argument_specs(&a.jvm, &replacements, &context.feature_flags))
        .unwrap_or_default();

    if let Some(logging) = metadata
        .logging
        .as_ref()
        .and_then(|logging| logging.client.as_ref())
    {
        if let Some(logging_path) = &context.logging_path {
            let logging_argument = logging.argument.replace("${path}", logging_path);
            jvm_args.push(logging_argument);
        }
    }

    let game_args = if let Some(arguments) = metadata.arguments.as_ref() {
        resolve_argument_specs(&arguments.game, &replacements, &context.feature_flags)
    } else if let Some(old_args) = metadata.minecraft_arguments.as_ref() {
        old_args
            .split_whitespace()
            .map(|arg| substitute_placeholders(arg, &replacements))
            .collect()
    } else {
        Vec::new()
    };

    Ok(ResolvedLaunchArguments {
        version_name,
        main_class,
        asset_index_name,
        jvm_args,
        game_args,
    })
}

pub fn version_type_or_release(metadata: &VersionMetadata) -> String {
    metadata
        .version_type
        .clone()
        .unwrap_or_else(|| "release".to_string())
}

pub fn detect_os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "osx",
        other => other,
    }
}

pub fn build_replacements(
    context: &LaunchArgumentContext,
    version_name: &str,
    asset_index_name: &str,
) -> HashMap<&'static str, String> {
    let mut replacements = HashMap::new();
    replacements.insert("auth_player_name", context.auth_player_name.clone());
    replacements.insert("auth_uuid", context.auth_uuid.clone());
    replacements.insert("auth_access_token", context.auth_access_token.clone());
    replacements.insert("auth_xuid", context.auth_xuid.clone());
    replacements.insert("clientid", context.client_id.clone());
    replacements.insert("version_name", version_name.to_string());
    replacements.insert("game_directory", context.game_directory.clone());
    replacements.insert("assets_root", GLOBAL_ASSETS.to_string_lossy().to_string());
    replacements.insert("assets_index_name", asset_index_name.to_string());
    replacements.insert("user_type", context.user_type.clone());
    replacements.insert("version_type", context.version_type.clone());
    replacements.insert("natives_directory", context.natives_directory.clone());
    replacements.insert("library_directory", GLOBAL_LIBRARIES.to_string_lossy().to_string());
    replacements.insert("launcher_name", context.launcher_name.clone());
    replacements.insert("launcher_version", context.launcher_version.clone());
    replacements.insert("classpath", context.classpath.clone());
    replacements.insert("classpath_separator", context.classpath_separator.clone());
    if let Some(width) = &context.resolution_width {
        replacements.insert("resolution_width", width.clone());
    }
    if let Some(height) = &context.resolution_height {
        replacements.insert("resolution_height", height.clone());
    }
    replacements
}

pub fn resolve_argument_specs(
    specs: &[ArgumentSpec],
    replacements: &HashMap<&str, String>,
    feature_flags: &HashMap<String, bool>,
) -> Vec<String> {
    let current_os = detect_os_name();
    let current_arch = std::env::consts::ARCH;
    let mut args = Vec::new();

    for spec in specs {
        match spec {
            ArgumentSpec::Plain(value) => args.push(substitute_placeholders(value, replacements)),
            ArgumentSpec::Conditional(argument) => {
                if !evaluate_rules(&argument.rules, current_os, current_arch, feature_flags) {
                    continue;
                }
                match &argument.value {
                    ArgumentValue::Single(value) => {
                        args.push(substitute_placeholders(value, replacements));
                    }
                    ArgumentValue::Multiple(values) => {
                        args.extend(
                            values
                                .iter()
                                .map(|value| substitute_placeholders(value, replacements)),
                        );
                    }
                }
            }
        }
    }

    args
}

pub fn substitute_placeholders(input: &str, replacements: &HashMap<&str, String>) -> String {
    let mut output = input.to_string();
    for (key, value) in replacements {
        output = output.replace(&format!("${{{key}}}"), value);
    }
    output
}

pub fn evaluate_rules(
    rules: &[Rule],
    current_os: &str,
    current_arch: &str,
    feature_flags: &HashMap<String, bool>,
) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        let matches_os = match &rule.os {
            Some(os) => {
                let name_match = os.name.as_ref().map_or(true, |name| name == current_os);
                let arch_match = os.arch.as_ref().map_or(true, |arch| arch == current_arch);
                name_match && arch_match
            }
            None => true,
        };
        let matches_features = rule
            .features
            .iter()
            .all(|(name, expected)| feature_flags.get(name).copied().unwrap_or(false) == *expected);

        if matches_os && matches_features {
            match rule.action.as_str() {
                "allow" => allowed = true,
                "disallow" => allowed = false,
                _ => {}
            }
        }
    }

    allowed
}

pub fn default_logging_config_path(base_dir: &Path, logging: &LoggingConfig) -> Option<PathBuf> {
    let client = logging.client.as_ref()?;
    let relative = client.file.path.as_ref()?;
    Some(base_dir.join(relative))
}
