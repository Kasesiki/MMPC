//! Offline Minecraft launcher — builds a Java `Command` for offline play

use std::path::PathBuf;
use std::process::Command;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::auth::offline::OfflineUser;
use crate::launch::version::{
    LaunchArgumentContext, LaunchLayout, LaunchPlan, VersionMetadata, resolve_launch_plan,
    version_type_or_release,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    pub java_path: PathBuf,
    pub version_metadata: VersionMetadata,
    pub version_jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub main_class: Option<String>,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub asset_index: String,
    pub library_dir: PathBuf,
    pub natives_dir: PathBuf,
    pub logging_config: Option<PathBuf>,
    pub max_mem: String,
    pub min_mem: Option<String>,
    pub extra_jvm_args: Vec<String>,
    pub extra_game_args: Vec<String>,
    pub demo: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub launcher_name: String,
    pub launcher_version: String,
}

impl LaunchConfig {
    pub fn build_plan(&self, user: &OfflineUser) -> Result<LaunchPlan, crate::launch::LaunchError> {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let mut cp_entries = self
            .classpath
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        cp_entries.push(self.version_jar.to_string_lossy().to_string());
        let classpath = cp_entries.join(separator);

        let main_class = self
            .main_class
            .clone()
            .or_else(|| self.version_metadata.main_class.clone());

        let mut version = self.version_metadata.clone();
        if version.main_class.is_none() {
            version.main_class = main_class;
        }

        let layout = LaunchLayout {
            game_dir: self.game_dir.clone(),
            assets_dir: self.assets_dir.clone(),
            library_dir: self.library_dir.clone(),
            natives_dir: self.natives_dir.clone(),
            client_jar: self.version_jar.clone(),
            classpath_entries: self.classpath.clone(),
            logging_config: self.logging_config.clone(),
        };

        let mut feature_flags =
            std::collections::HashMap::from([("is_demo_user".into(), self.demo)]);
        feature_flags.insert(
            "has_custom_resolution".into(),
            self.width.is_some() && self.height.is_some(),
        );

        resolve_launch_plan(
            &version,
            layout,
            &LaunchArgumentContext {
                auth_player_name: user.username.clone(),
                auth_uuid: user.uuid.clone(),
                auth_access_token: user.access_token.clone(),
                auth_xuid: String::new(),
                client_id: String::new(),
                version_name: version.id.clone(),
                game_directory: self.game_dir.to_string_lossy().to_string(),
                assets_root: self.assets_dir.to_string_lossy().to_string(),
                assets_index_name: self.asset_index.clone(),
                user_type: "legacy".into(),
                version_type: version_type_or_release(&version),
                natives_directory: self.natives_dir.to_string_lossy().to_string(),
                library_directory: self.library_dir.to_string_lossy().to_string(),
                launcher_name: self.launcher_name.clone(),
                launcher_version: self.launcher_version.clone(),
                classpath,
                classpath_separator: separator.to_string(),
                resolution_width: self.width.map(|v| v.to_string()),
                resolution_height: self.height.map(|v| v.to_string()),
                feature_flags,
                logging_path: self
                    .logging_config
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
            },
        )
    }
}

pub struct OfflineLauncher;

impl OfflineLauncher {
    pub fn build_arg(&self, config: &LaunchConfig, user: &OfflineUser) -> anyhow::Result<PathBuf> {
        let plan = config
            .build_plan(user)
            .expect("OfflineLauncher: invalid launch config");

        let mut args = [
            plan.jvm_args,
            config.extra_jvm_args.clone(),
            vec![plan.main_class],
            plan.game_args,
            config.extra_game_args.clone(),
        ]
        .concat();

        for arg in args.iter_mut() {
            *arg = arg.replace(" ", "");
            if arg.is_empty() {
                *arg = String::from("\"\"");
            }
        }

        let argfile = config.game_dir.join("java.args");
        let content = args.join(" ");
        std::fs::write(&argfile, content)
            .with_context(|| format!("写入 argfile 失败: {}", argfile.display()))?;
        Ok(argfile)
    }
}

impl super::Launcher for OfflineLauncher {
    type Config = LaunchConfig;

    fn build_command(&self, _config: &Self::Config) -> Command {
        unimplemented!("Use OfflineLauncher::build_command(&self, config, user) instead")
    }
}
