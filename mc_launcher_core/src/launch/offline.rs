//! Offline Minecraft launcher — builds a Java `Command` for offline play

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::auth::offline::OfflineUser;
use crate::launch::version::{
    resolve_launch_plan, version_type_or_release, LaunchArgumentContext, LaunchLayout, LaunchPlan,
    VersionMetadata,
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
    pub fn builder() -> LaunchConfigBuilder {
        LaunchConfigBuilder::default()
    }

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

        let mut feature_flags = std::collections::HashMap::from([("is_demo_user".into(), self.demo)]);
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

#[derive(Debug, Default)]
pub struct LaunchConfigBuilder {
    java_path: Option<PathBuf>,
    version_metadata: Option<VersionMetadata>,
    version_jar: Option<PathBuf>,
    classpath: Vec<PathBuf>,
    main_class: Option<String>,
    game_dir: Option<PathBuf>,
    assets_dir: Option<PathBuf>,
    asset_index: Option<String>,
    library_dir: Option<PathBuf>,
    natives_dir: Option<PathBuf>,
    logging_config: Option<PathBuf>,
    max_mem: Option<String>,
    min_mem: Option<String>,
    extra_jvm_args: Vec<String>,
    extra_game_args: Vec<String>,
    demo: bool,
    width: Option<u32>,
    height: Option<u32>,
    launcher_name: Option<String>,
    launcher_version: Option<String>,
}

impl LaunchConfigBuilder {
    pub fn java_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.java_path = Some(path.into());
        self
    }

    pub fn version_metadata(mut self, metadata: VersionMetadata) -> Self {
        self.version_metadata = Some(metadata);
        self
    }

    pub fn minecraft_jar(mut self, path: impl Into<PathBuf>) -> Self {
        self.version_jar = Some(path.into());
        self
    }

    pub fn add_classpath(mut self, path: impl Into<PathBuf>) -> Self {
        self.classpath.push(path.into());
        self
    }

    pub fn classpath(mut self, paths: Vec<impl Into<PathBuf>>) -> Self {
        self.classpath = paths.into_iter().map(Into::into).collect();
        self
    }

    pub fn main_class(mut self, class: impl Into<String>) -> Self {
        self.main_class = Some(class.into());
        self
    }

    pub fn game_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.game_dir = Some(path.into());
        self
    }

    pub fn assets_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.assets_dir = Some(path.into());
        self
    }

    pub fn asset_index(mut self, index: impl Into<String>) -> Self {
        self.asset_index = Some(index.into());
        self
    }

    pub fn library_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.library_dir = Some(path.into());
        self
    }

    pub fn natives_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.natives_dir = Some(path.into());
        self
    }

    pub fn logging_config(mut self, path: impl Into<PathBuf>) -> Self {
        self.logging_config = Some(path.into());
        self
    }

    pub fn max_mem(mut self, mem: impl Into<String>) -> Self {
        self.max_mem = Some(mem.into());
        self
    }

    pub fn min_mem(mut self, mem: impl Into<String>) -> Self {
        self.min_mem = Some(mem.into());
        self
    }

    pub fn add_jvm_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_jvm_args.push(arg.into());
        self
    }

    pub fn add_game_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_game_args.push(arg.into());
        self
    }

    pub fn demo(mut self, demo: bool) -> Self {
        self.demo = demo;
        self
    }

    pub fn launcher_name(mut self, name: impl Into<String>) -> Self {
        self.launcher_name = Some(name.into());
        self
    }

    pub fn launcher_version(mut self, version: impl Into<String>) -> Self {
        self.launcher_version = Some(version.into());
        self
    }

    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn build(self) -> LaunchConfig {
        self.try_build()
            .expect("LaunchConfigBuilder: missing required fields")
    }

    pub fn try_build(self) -> Result<LaunchConfig, crate::launch::LaunchError> {
        let java_path = self.java_path.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("java_path is required".into())
        })?;
        let version_metadata = self.version_metadata.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("version_metadata is required".into())
        })?;
        let version_jar = self.version_jar.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("minecraft_jar is required".into())
        })?;
        let game_dir = self.game_dir.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("game_dir is required".into())
        })?;
        let assets_dir = self.assets_dir.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("assets_dir is required".into())
        })?;
        let asset_index = self.asset_index.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("asset_index is required".into())
        })?;
        let library_dir = self.library_dir.unwrap_or_else(|| game_dir.join("versions").join("libraries"));
        let natives_dir = self.natives_dir.unwrap_or_else(|| game_dir.join("natives"));

        Ok(LaunchConfig {
            java_path,
            version_metadata,
            version_jar,
            classpath: self.classpath,
            main_class: self.main_class,
            game_dir,
            assets_dir,
            asset_index,
            library_dir,
            natives_dir,
            logging_config: self.logging_config,
            max_mem: self.max_mem.unwrap_or_else(|| "2G".into()),
            min_mem: self.min_mem,
            extra_jvm_args: self.extra_jvm_args,
            extra_game_args: self.extra_game_args,
            demo: self.demo,
            width: self.width,
            height: self.height,
            launcher_name: self.launcher_name.unwrap_or_else(|| "mmpc".into()),
            launcher_version: self
                .launcher_version
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into()),
        })
    }
}

pub struct OfflineLauncher;

impl OfflineLauncher {
    pub fn build_command(&self, config: &LaunchConfig, user: &OfflineUser) -> Command {
        let plan = config
            .build_plan(user)
            .expect("OfflineLauncher: invalid launch config");

        let mut cmd = Command::new(&config.java_path);
        cmd.arg(format!("-Xmx{}", config.max_mem));
        if let Some(min) = &config.min_mem {
            cmd.arg(format!("-Xms{}", min));
        }

        for arg in &plan.jvm_args {
            cmd.arg(arg);
        }
        for arg in &config.extra_jvm_args {
            cmd.arg(arg);
        }
        cmd.arg(&plan.main_class);
        for arg in &plan.game_args {
            cmd.arg(arg);
        }
        for arg in &config.extra_game_args {
            cmd.arg(arg);
        }
        cmd
    }
}

impl super::Launcher for OfflineLauncher {
    type Config = LaunchConfig;

    fn build_command(&self, _config: &Self::Config) -> Command {
        unimplemented!("Use OfflineLauncher::build_command(&self, config, user) instead")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::offline::OfflineUser;
    use crate::launch::version::parse_version_metadata;

    #[test]
    fn builds_loader_aware_command() {
        let user = OfflineUser::new("TestPlayer");
        let metadata = parse_version_metadata(
            r#"{
                "id":"1.21-test",
                "mainClass":"net.minecraft.client.main.Main",
                "arguments": {
                    "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
                    "game": ["--username", "${auth_player_name}", "--assetIndex", "${assets_index_name}"]
                },
                "assetIndex": {"id":"1.21"}
            }"#,
        )
        .unwrap();

        let config = LaunchConfig::builder()
            .java_path("/usr/bin/java")
            .version_metadata(metadata)
            .minecraft_jar("/game/client.jar")
            .game_dir("/game")
            .assets_dir("/assets")
            .asset_index("1.21")
            .library_dir("/game/versions/libraries")
            .natives_dir("/game/natives")
            .add_classpath("/game/lib/a.jar")
            .build();

        let cmd = OfflineLauncher.build_command(&config, &user);
        let args = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert!(args.iter().any(|v| v.contains("-Xmx2G")));
        assert!(args.iter().any(|v| v == "net.minecraft.client.main.Main"));
        assert!(args.iter().any(|v| v == "TestPlayer"));
    }
}
