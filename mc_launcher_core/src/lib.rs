//! `mc_launcher_core` — Minecraft Launcher SDK
//!
//! A modular SDK for launching Minecraft instances, starting with
//! offline authentication and offline game launch support.
//!
//! # Quick start — offline launch
//!
//! ```rust,no_run
//! use mc_launcher_core::auth::offline::OfflineUser;
//! use mc_launcher_core::launch::offline::{LaunchConfig, OfflineLauncher};
//! use mc_launcher_core::launch::version::VersionMetadata;
//! use std::path::PathBuf;
//!
//! let user = OfflineUser::new("Steve");
//! let config = LaunchConfig {
//!     java_path: PathBuf::from("/usr/bin/java"),
//!     version_metadata: VersionMetadata::default(),
//!     version_jar: PathBuf::from("/path/to/1.21/client.jar"),
//!     classpath: vec![],
//!     main_class: None,
//!     game_dir: PathBuf::from("/path/to/game"),
//!     assets_dir: PathBuf::from("/path/to/assets"),
//!     asset_index: String::from("1.21"),
//!     library_dir: PathBuf::from("/path/to/game/versions/libraries"),
//!     natives_dir: PathBuf::from("/path/to/game/natives"),
//!     logging_config: None,
//!     max_mem: String::from("4G"),
//!     min_mem: None,
//!     extra_jvm_args: vec![],
//!     extra_game_args: vec![],
//!     demo: false,
//!     width: None,
//!     height: None,
//!     launcher_name: String::from("mmpc"),
//!     launcher_version: env!("CARGO_PKG_VERSION").into(),
//! };
//!
//! let launcher = OfflineLauncher;
//! let cmd = launcher.build_command(&config, &user);
//! // cmd.spawn() etc.
//! ```

pub mod auth;
pub mod launch;
pub mod runtime;
