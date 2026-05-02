use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::launch::LaunchError;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionMetadata {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    #[serde(rename = "mainClass", default)]
    pub main_class: Option<String>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub arguments: Option<VersionArguments>,
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<RuleOs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleOs {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct LaunchArgumentContext {
    pub auth_player_name: String,
    pub auth_uuid: String,
    pub auth_access_token: String,
    pub version_name: String,
    pub game_directory: String,
    pub assets_root: String,
    pub assets_index_name: String,
    pub user_type: String,
    pub version_type: String,
    pub natives_directory: String,
    pub launcher_name: String,
    pub launcher_version: String,
    pub classpath: String,
    pub classpath_separator: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedLaunchArguments {
    pub version_name: String,
    pub main_class: String,
    pub asset_index_name: String,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

pub fn parse_version_metadata(content: &str) -> Result<VersionMetadata, LaunchError> {
    serde_json::from_str(content)
        .map_err(|e| LaunchError::InvalidConfig(format!("invalid version metadata: {e}")))
}

pub fn merge_version_metadata(parent: &VersionMetadata, child: &VersionMetadata) -> VersionMetadata {
    let mut merged = parent.clone();
    merged.id = if child.id.is_empty() { merged.id } else { child.id.clone() };
    merged.inherits_from = child.inherits_from.clone();
    if child.main_class.is_some() {
        merged.main_class = child.main_class.clone();
    }
    if child.minecraft_arguments.is_some() {
        merged.minecraft_arguments = child.minecraft_arguments.clone();
    }
    if child.asset_index.is_some() {
        merged.asset_index = child.asset_index.clone();
    }

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

pub fn resolve_launch_arguments(
    metadata: &VersionMetadata,
    context: &LaunchArgumentContext,
) -> Result<ResolvedLaunchArguments, LaunchError> {
    let main_class = metadata
        .main_class
        .clone()
        .ok_or_else(|| LaunchError::InvalidConfig("mainClass is missing in version metadata".into()))?;
    let asset_index_name = metadata
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| context.assets_index_name.clone());
    let version_name = if metadata.id.is_empty() {
        context.version_name.clone()
    } else {
        metadata.id.clone()
    };

    let mut replacements = HashMap::new();
    replacements.insert("auth_player_name", context.auth_player_name.clone());
    replacements.insert("auth_uuid", context.auth_uuid.clone());
    replacements.insert("auth_access_token", context.auth_access_token.clone());
    replacements.insert("version_name", version_name.clone());
    replacements.insert("game_directory", context.game_directory.clone());
    replacements.insert("assets_root", context.assets_root.clone());
    replacements.insert("assets_index_name", asset_index_name.clone());
    replacements.insert("user_type", context.user_type.clone());
    replacements.insert("version_type", context.version_type.clone());
    replacements.insert("natives_directory", context.natives_directory.clone());
    replacements.insert("launcher_name", context.launcher_name.clone());
    replacements.insert("launcher_version", context.launcher_version.clone());
    replacements.insert("classpath", context.classpath.clone());
    replacements.insert("classpath_separator", context.classpath_separator.clone());

    let jvm_args = metadata
        .arguments
        .as_ref()
        .map(|a| resolve_argument_specs(&a.jvm, &replacements))
        .unwrap_or_default();

    let game_args = if let Some(arguments) = metadata.arguments.as_ref() {
        resolve_argument_specs(&arguments.game, &replacements)
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

fn resolve_argument_specs(
    specs: &[ArgumentSpec],
    replacements: &HashMap<&str, String>,
) -> Vec<String> {
    let current_os = detect_os();
    let mut args = Vec::new();

    for spec in specs {
        match spec {
            ArgumentSpec::Plain(value) => args.push(substitute_placeholders(value, replacements)),
            ArgumentSpec::Conditional(argument) => {
                if !evaluate_rules(&argument.rules, &current_os) {
                    continue;
                }
                match &argument.value {
                    ArgumentValue::Single(value) => {
                        args.push(substitute_placeholders(value, replacements));
                    }
                    ArgumentValue::Multiple(values) => {
                        args.extend(values.iter().map(|value| substitute_placeholders(value, replacements)));
                    }
                }
            }
        }
    }

    args
}

fn substitute_placeholders(input: &str, replacements: &HashMap<&str, String>) -> String {
    let mut output = input.to_string();
    for (key, value) in replacements {
        output = output.replace(&format!("${{{key}}}"), value);
    }
    output
}

fn detect_os() -> String {
    match std::env::consts::OS {
        "macos" => "osx".to_string(),
        other => other.to_string(),
    }
}

fn evaluate_rules(rules: &[Rule], current_os: &str) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        let matches_os = match &rule.os {
            Some(os) => os.name.as_ref().map_or(true, |name| name == current_os),
            None => true,
        };
        if matches_os {
            match rule.action.as_str() {
                "allow" => allowed = true,
                "disallow" => allowed = false,
                _ => {}
            }
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_modern_arguments() {
        let content = r#"{
          "id": "fabric-loader-0.16.10-1.21",
          "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
          "arguments": {
            "jvm": [
              "-Djava.library.path=${natives_directory}",
              "-cp",
              "${classpath}"
            ],
            "game": [
              "--username",
              "${auth_player_name}",
              "--assetIndex",
              "${assets_index_name}"
            ]
          }
        }"#;
        let metadata = parse_version_metadata(content).expect("parse metadata");
        let resolved = resolve_launch_arguments(
            &metadata,
            &LaunchArgumentContext {
                auth_player_name: "Player".into(),
                auth_uuid: "uuid".into(),
                auth_access_token: "token".into(),
                version_name: "fabric-loader-0.16.10-1.21".into(),
                game_directory: "/game".into(),
                assets_root: "/assets".into(),
                assets_index_name: "17".into(),
                user_type: "legacy".into(),
                version_type: "release".into(),
                natives_directory: "/game/natives".into(),
                launcher_name: "mmpc".into(),
                launcher_version: "0.1.0".into(),
                classpath: "/libs/a.jar:/libs/b.jar".into(),
                classpath_separator: ":".into(),
            },
        )
        .expect("resolve arguments");

        assert_eq!(resolved.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert!(resolved.jvm_args.contains(&"-cp".to_string()));
        assert!(resolved.jvm_args.contains(&"/libs/a.jar:/libs/b.jar".to_string()));
        assert!(resolved.game_args.contains(&"Player".to_string()));
        assert!(resolved.game_args.contains(&"17".to_string()));
    }

    #[test]
    fn merges_parent_and_child_metadata() {
        let parent = parse_version_metadata(
            r#"{
              "id": "1.21",
              "mainClass": "net.minecraft.client.main.Main",
              "arguments": { "game": ["--demo"] }
            }"#,
        )
        .expect("parse parent");
        let child = parse_version_metadata(
            r#"{
              "id": "fabric-loader-0.16.10-1.21",
              "inheritsFrom": "1.21",
              "arguments": { "game": ["--quickPlaySingleplayer", "world"] }
            }"#,
        )
        .expect("parse child");

        let merged = merge_version_metadata(&parent, &child);
        let game_args = merged.arguments.expect("merged args").game;
        assert_eq!(merged.main_class.expect("main class"), "net.minecraft.client.main.Main");
        assert_eq!(game_args.len(), 3);
    }
}
