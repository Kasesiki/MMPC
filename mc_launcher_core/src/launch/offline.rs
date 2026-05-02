//! Offline Minecraft launcher — builds a Java `Command` for offline play
//!
//! # Example
//! ```no_run
//! use mc_launcher_core::launch::offline::{LaunchConfig, OfflineLauncher};
//! use mc_launcher_core::auth::offline::OfflineUser;
//!
//! let user = OfflineUser::new("Steve");
//! let config = LaunchConfig::builder()
//!     .java_path("/usr/bin/java")
//!     .minecraft_jar("/home/user/.minecraft/versions/1.21/1.21.jar")
//!     .main_class("net.minecraft.client.main.Main")
//!     .game_dir("/home/user/.minecraft")
//!     .assets_dir("/home/user/.minecraft/assets")
//!     .asset_index("1.21")
//!     .max_mem("2G")
//!     .build();
//!
//! let launcher = OfflineLauncher;
//! let cmd = launcher.build_command(&config, &user);
//! ```

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::auth::offline::OfflineUser;
use crate::launch::version::{
    resolve_launch_arguments, LaunchArgumentContext, ResolvedLaunchArguments, VersionMetadata,
};

/// Configuration for launching Minecraft (offline mode)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    /// Path to the Java executable
    pub java_path: PathBuf,
    /// Path to the Minecraft version JAR
    pub minecraft_jar: PathBuf,
    /// Full classpath — library JARs concatenated with the platform separator
    pub classpath: Vec<PathBuf>,
    /// Fully-qualified main class name
    pub main_class: String,
    /// Game directory (`.minecraft` or custom)
    pub game_dir: PathBuf,
    /// Assets root directory
    pub assets_dir: PathBuf,
    /// Asset index name (e.g. "1.21", "1.20.4")
    pub asset_index: String,
    /// Max heap size (e.g. "2G", "4096M")
    pub max_mem: String,
    /// Min heap size (optional)
    pub min_mem: Option<String>,
    /// Extra JVM arguments
    pub jvm_args: Vec<String>,
    /// Extra game arguments
    pub game_args: Vec<String>,
    /// Whether to enable the demo user (for demo versions)
    pub demo: bool,
    /// Custom resolution width
    pub width: Option<u32>,
    /// Custom resolution height
    pub height: Option<u32>,
    /// Optional parsed version metadata for loader-aware launch
    pub version_metadata: Option<VersionMetadata>,
    /// Launcher display name used in placeholder expansion
    pub launcher_name: String,
    /// Launcher version used in placeholder expansion
    pub launcher_version: String,
}

impl LaunchConfig {
    /// Create a new [`LaunchConfigBuilder`]
    pub fn builder() -> LaunchConfigBuilder {
        LaunchConfigBuilder::default()
    }
}

/// Builder for [`LaunchConfig`]
#[derive(Debug, Default)]
pub struct LaunchConfigBuilder {
    java_path: Option<PathBuf>,
    minecraft_jar: Option<PathBuf>,
    classpath: Vec<PathBuf>,
    main_class: Option<String>,
    game_dir: Option<PathBuf>,
    assets_dir: Option<PathBuf>,
    asset_index: Option<String>,
    max_mem: Option<String>,
    min_mem: Option<String>,
    jvm_args: Vec<String>,
    game_args: Vec<String>,
    demo: bool,
    width: Option<u32>,
    height: Option<u32>,
    version_metadata: Option<VersionMetadata>,
    launcher_name: Option<String>,
    launcher_version: Option<String>,
}

impl LaunchConfigBuilder {
    /// Sets the Java executable path
    pub fn java_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.java_path = Some(path.into());
        self
    }

    /// Sets the Minecraft version JAR path
    pub fn minecraft_jar(mut self, path: impl Into<PathBuf>) -> Self {
        self.minecraft_jar = Some(path.into());
        self
    }

    /// Adds a library JAR to the classpath
    pub fn add_classpath(mut self, path: impl Into<PathBuf>) -> Self {
        self.classpath.push(path.into());
        self
    }

    /// Sets the full classpath from a slice of paths
    pub fn classpath(mut self, paths: Vec<impl Into<PathBuf>>) -> Self {
        self.classpath = paths.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the main class name
    pub fn main_class(mut self, class: impl Into<String>) -> Self {
        self.main_class = Some(class.into());
        self
    }

    /// Sets the game directory
    pub fn game_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.game_dir = Some(path.into());
        self
    }

    /// Sets the assets root directory
    pub fn assets_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.assets_dir = Some(path.into());
        self
    }

    /// Sets the asset index name (e.g. "1.21")
    pub fn asset_index(mut self, index: impl Into<String>) -> Self {
        self.asset_index = Some(index.into());
        self
    }

    /// Sets the maximum heap size (e.g. "2G")
    pub fn max_mem(mut self, mem: impl Into<String>) -> Self {
        self.max_mem = Some(mem.into());
        self
    }

    /// Sets the minimum heap size (e.g. "512M")
    pub fn min_mem(mut self, mem: impl Into<String>) -> Self {
        self.min_mem = Some(mem.into());
        self
    }

    /// Adds a JVM argument
    pub fn add_jvm_arg(mut self, arg: impl Into<String>) -> Self {
        self.jvm_args.push(arg.into());
        self
    }

    /// Adds a game argument
    pub fn add_game_arg(mut self, arg: impl Into<String>) -> Self {
        self.game_args.push(arg.into());
        self
    }

    /// Enable demo mode
    pub fn demo(mut self, demo: bool) -> Self {
        self.demo = demo;
        self
    }

    /// Attach cached version metadata for loader-aware launch
    pub fn version_metadata(mut self, metadata: VersionMetadata) -> Self {
        self.version_metadata = Some(metadata);
        self
    }

    /// Set launcher name used in placeholders
    pub fn launcher_name(mut self, name: impl Into<String>) -> Self {
        self.launcher_name = Some(name.into());
        self
    }

    /// Set launcher version used in placeholders
    pub fn launcher_version(mut self, version: impl Into<String>) -> Self {
        self.launcher_version = Some(version.into());
        self
    }

    /// Set custom window resolution
    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Consume the builder and produce a [`LaunchConfig`], filling defaults
    /// where possible and panicking on missing required fields.
    ///
    /// For a fallible version see [`try_build`](Self::try_build).
    pub fn build(self) -> LaunchConfig {
        self.try_build()
            .expect("LaunchConfigBuilder: missing required fields")
    }

    /// Consume the builder and produce a [`LaunchConfig`], returning an error
    /// if any required fields are missing.
    pub fn try_build(self) -> Result<LaunchConfig, crate::launch::LaunchError> {
        let java_path = self.java_path.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("java_path is required".into())
        })?;
        let minecraft_jar = self.minecraft_jar.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("minecraft_jar is required".into())
        })?;
        let main_class = self
            .main_class
            .unwrap_or_else(|| "net.minecraft.client.main.Main".into());
        let game_dir = self.game_dir.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("game_dir is required".into())
        })?;
        let assets_dir = self.assets_dir.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("assets_dir is required".into())
        })?;
        let asset_index = self.asset_index.ok_or_else(|| {
            crate::launch::LaunchError::InvalidConfig("asset_index is required".into())
        })?;

        Ok(LaunchConfig {
            java_path,
            minecraft_jar,
            classpath: self.classpath,
            main_class,
            game_dir,
            assets_dir,
            asset_index,
            max_mem: self.max_mem.unwrap_or_else(|| "2G".into()),
            min_mem: self.min_mem,
            jvm_args: self.jvm_args,
            game_args: self.game_args,
            demo: self.demo,
            width: self.width,
            height: self.height,
            version_metadata: self.version_metadata,
            launcher_name: self.launcher_name.unwrap_or_else(|| "mmpc".into()),
            launcher_version: self
                .launcher_version
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into()),
        })
    }
}

/// Offline launcher engine
///
/// Builds a [`std::process::Command`] that launches Minecraft in offline mode.
pub struct OfflineLauncher;

impl OfflineLauncher {
    /// Build a Java `Command` from the launch configuration and offline user.
    ///
    /// The returned `Command` is not spawned — the caller decides how to handle
    /// stdout/stderr, environment, working directory, etc.
    pub fn build_command(&self, config: &LaunchConfig, user: &OfflineUser) -> Command {
        let mut cmd = Command::new(&config.java_path);
        let separator = if cfg!(windows) { ";" } else { ":" };
        let mut cp_entries = config
            .classpath
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        cp_entries.push(config.minecraft_jar.to_string_lossy().to_string());
        let classpath = cp_entries.join(separator);

        let resolved = resolve_loader_aware_arguments(config, user, &classpath, separator);

        // --- JVM arguments ---

        // Memory
        cmd.arg(format!("-Xmx{}", config.max_mem));
        if let Some(min) = &config.min_mem {
            cmd.arg(format!("-Xms{}", min));
        }

        if let Some(arguments) = resolved {
            for arg in arguments.jvm_args {
                cmd.arg(arg);
            }
            for arg in &config.jvm_args {
                cmd.arg(arg);
            }
            cmd.arg(arguments.main_class);
            for arg in arguments.game_args {
                cmd.arg(arg);
            }
        } else {
            // Minecraft required JVM flags
            cmd.arg("-Djava.library.path=natives");
            cmd.arg("-Dminecraft.applet.WrapperClass=net.minecraft.client.Minecraft");
            cmd.arg("-cp");
            cmd.arg(classpath);

            // Custom JVM args
            for arg in &config.jvm_args {
                cmd.arg(arg);
            }

            // --- Game arguments ---
            cmd.arg(&config.main_class);

            cmd.arg("--username").arg(&user.username);
            cmd.arg("--uuid").arg(user.uuid.to_string());
            cmd.arg("--accessToken").arg(&user.access_token);
            cmd.arg("--version").arg(&config.asset_index);
            cmd.arg("--gameDir")
                .arg(config.game_dir.to_string_lossy().as_ref());
            cmd.arg("--assetsDir")
                .arg(config.assets_dir.to_string_lossy().as_ref());
            cmd.arg("--assetIndex").arg(&config.asset_index);

            if config.demo {
                cmd.arg("--demo");
            }

            if let (Some(w), Some(h)) = (config.width, config.height) {
                cmd.arg("--width").arg(w.to_string());
                cmd.arg("--height").arg(h.to_string());
            }

            // Custom game args
            for arg in &config.game_args {
                cmd.arg(arg);
            }
        }

        cmd
    }
}

fn resolve_loader_aware_arguments(
    config: &LaunchConfig,
    user: &OfflineUser,
    classpath: &str,
    classpath_separator: &str,
) -> Option<ResolvedLaunchArguments> {
    let metadata = config.version_metadata.as_ref()?;
    let natives_directory = config.game_dir.join("natives");
    let mut resolved = resolve_launch_arguments(
        metadata,
        &LaunchArgumentContext {
            auth_player_name: user.username.clone(),
            auth_uuid: user.uuid.to_string(),
            auth_access_token: user.access_token.clone(),
            version_name: metadata.id.clone(),
            game_directory: config.game_dir.to_string_lossy().to_string(),
            assets_root: config.assets_dir.to_string_lossy().to_string(),
            assets_index_name: config.asset_index.clone(),
            user_type: "legacy".into(),
            version_type: "release".into(),
            natives_directory: natives_directory.to_string_lossy().to_string(),
            launcher_name: config.launcher_name.clone(),
            launcher_version: config.launcher_version.clone(),
            classpath: classpath.to_string(),
            classpath_separator: classpath_separator.to_string(),
            resolution_width: config.width.map(|v| v.to_string()),
            resolution_height: config.height.map(|v| v.to_string()),
            feature_flags: std::collections::HashMap::from([
                ("is_demo_user".into(), config.demo),
                (
                    "has_custom_resolution".into(),
                    config.width.is_some() && config.height.is_some(),
                ),
            ]),
        },
    )
    .ok()?;

    if !config.jvm_args.is_empty() {
        resolved.jvm_args.extend(config.jvm_args.clone());
    }
    if config.demo {
        resolved.game_args.push("--demo".into());
    }
    if let (Some(w), Some(h)) = (config.width, config.height) {
        resolved.game_args.push("--width".into());
        resolved.game_args.push(w.to_string());
        resolved.game_args.push("--height".into());
        resolved.game_args.push(h.to_string());
    }
    if !config.game_args.is_empty() {
        resolved.game_args.extend(config.game_args.clone());
    }

    Some(resolved)
}

impl super::Launcher for OfflineLauncher {
    type Config = LaunchConfig;

    fn build_command(&self, _config: &Self::Config) -> Command {
        // For offline launch we still need a user — this trait method uses
        // the auth module separately. The concrete `OfflineLauncher::build_command`
        // that takes an `OfflineUser` is the primary API.
        unimplemented!("Use OfflineLauncher::build_command(&self, config, user) instead")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::offline::OfflineUser;

    #[test]
    fn test_build_offline_command() {
        let user = OfflineUser::new("TestPlayer");
        let config = LaunchConfig::builder()
            .java_path("/usr/bin/java")
            .minecraft_jar("/home/user/.minecraft/versions/1.21/1.21.jar")
            .main_class("net.minecraft.client.main.Main")
            .game_dir("/home/user/.minecraft")
            .assets_dir("/home/user/.minecraft/assets")
            .asset_index("1.21")
            .max_mem("2G")
            .build();

        let launcher = OfflineLauncher;
        let cmd = launcher.build_command(&config, &user);

        let program = cmd.get_program();
        assert_eq!(program, "/usr/bin/java");

        // Convert OsStr args to strings for inspection
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"-Xmx2G".to_string()));
        assert!(args.contains(&"--username".to_string()));
        assert!(args.contains(&"TestPlayer".to_string()));
        assert!(args.contains(&"--uuid".to_string()));
        assert!(args.contains(&format!("{}", user.uuid))); // uuid without hyphens by default in game
        assert!(args.contains(&"--accessToken".to_string()));
        assert!(args.contains(&"--gameDir".to_string()));
        assert!(args.contains(&"/home/user/.minecraft".to_string()));
        assert!(args.contains(&"--assetIndex".to_string()));
        assert!(args.contains(&"1.21".to_string()));
    }

    #[test]
    fn test_launch_config_builder_missing_fields() {
        let result = LaunchConfig::builder()
            .java_path("/usr/bin/java")
            .try_build();

        assert!(result.is_err());
    }

    #[test]
    fn test_launch_config_builder_defaults() {
        let config = LaunchConfig::builder()
            .java_path("/usr/bin/java")
            .minecraft_jar("a.jar")
            .game_dir("/minecraft")
            .assets_dir("/assets")
            .asset_index("1.21")
            .build();

        assert_eq!(config.max_mem, "2G");
        assert_eq!(config.main_class, "net.minecraft.client.main.Main");
    }
}
