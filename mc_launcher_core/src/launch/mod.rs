//! Launch module — Minecraft game process management
//!
//! Builds the Java command-line invocation from a [`LaunchConfig`].
//! Supports offline launch; online / Forge / Fabric variants can be
//! added via the [`Launcher`] trait.

pub mod offline;

use std::process::Command;

/// Common trait for launcher implementations
pub trait Launcher {
    type Config;
    fn build_command(&self, config: &Self::Config) -> Command;
}

/// Errors that can occur during launch preparation
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("Java executable not found: {0}")]
    JavaNotFound(String),

    #[error("Minecraft JAR not found: {0}")]
    MinecraftJarNotFound(String),

    #[error("Library not found: {0}")]
    LibraryNotFound(String),

    #[error("Assets directory not found: {0}")]
    AssetsNotFound(String),

    #[error("Asset index not found: {0}")]
    AssetIndexNotFound(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
