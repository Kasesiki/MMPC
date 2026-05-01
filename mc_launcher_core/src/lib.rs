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
//!
//! let user = OfflineUser::new("Steve");
//! let config = LaunchConfig::builder()
//!     .java_path("/usr/bin/java")
//!     .minecraft_jar("/path/to/1.21/client.jar")
//!     .game_dir("/path/to/game")
//!     .assets_dir("/path/to/assets")
//!     .asset_index("1.21")
//!     .max_mem("4G")
//!     .build();
//!
//! let launcher = OfflineLauncher;
//! let cmd = launcher.build_command(&config, &user);
//! // cmd.spawn() etc.
//! ```

pub mod auth;
pub mod launch;
