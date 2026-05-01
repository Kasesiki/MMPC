//! Authentication module — offline & online user models
//!
//! Currently supports offline (local) user generation.
//! The [`AuthProvider`] trait can be implemented for online auth flows
//! (Mojang / Microsoft) in the future.

pub mod offline;

/// Common trait for all auth providers
pub trait AuthProvider {
    type User: SerializableUser;
    fn authenticate(&self) -> Result<Self::User, AuthError>;
}

/// Minimal user interface shared by all auth backends
pub trait SerializableUser {
    fn username(&self) -> &str;
    fn uuid(&self) -> &str;
    fn access_token(&self) -> &str;
}

/// Generic auth error
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Authentication failed: {0}")]
    General(String),
}
