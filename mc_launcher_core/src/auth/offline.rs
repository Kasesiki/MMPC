//! Offline (unauthenticated) user generation
//!
//! Generates a Mojang-style offline profile using the classic algorithm:
//! `UUID = md5("OfflinePlayer:" + username)`, with version & variant bits set.
//! This produces the same UUID that the vanilla Minecraft client generates for
//! offline-mode servers, ensuring compatibility.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AuthError, AuthProvider, SerializableUser};

/// An offline-mode user profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineUser {
    /// Player display name
    pub username: String,
    /// Offline UUID (v3-style, derived from username)
    pub uuid: String,
    /// Dummy access token (always empty for offline)
    pub access_token: String,
}

impl OfflineUser {
    /// Create a new offline user from a username
    ///
    /// Generates the UUID automatically via `generate_offline_uuid`.
    pub fn new(username: &str) -> Self {
        let uuid = generate_offline_uuid(username);
        Self {
            username: username.to_string(),
            uuid: uuid.to_string(),
            access_token: String::new(),
        }
    }
}

impl SerializableUser for OfflineUser {
    fn username(&self) -> &str {
        &self.username
    }
    fn uuid(&self) -> &str {
        &self.uuid
    }
    fn access_token(&self) -> &str {
        &self.access_token
    }
}

/// Auth provider that always returns an offline user
pub struct OfflineAuthProvider {
    pub username: String,
}

impl OfflineAuthProvider {
    pub fn new(username: &str) -> Self {
        Self {
            username: username.to_string(),
        }
    }
}

impl AuthProvider for OfflineAuthProvider {
    type User = OfflineUser;

    fn authenticate(&self) -> Result<Self::User, AuthError> {
        Ok(OfflineUser::new(&self.username))
    }
}

/// Generate a Mojang-compatible offline UUID from a username
///
/// Algorithm:
/// 1. Compute `MD5("OfflinePlayer:" + username)`
/// 2. Set UUID version to 3 (name-based) by modifying byte 6
/// 3. Set UUID variant to IETF (RFC 4122) by modifying byte 8
///
/// ## Example
///
/// ```
/// use mc_launcher_core::auth::offline::generate_offline_uuid;
/// let uuid = generate_offline_uuid("Steve");
/// assert_eq!(uuid.get_version(), Some(uuid::Version::Md5));  // v3
/// assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122);
/// ```
pub fn generate_offline_uuid(username: &str) -> Uuid {
    let mut hasher = Md5::new();
    hasher.update(b"OfflinePlayer:");
    hasher.update(username.as_bytes());
    let digest = hasher.finalize();

    let mut bytes: [u8; 16] = digest.into();

    // Set version to 3 (name-based, RFC 4122) — 4 bits at position 6
    bytes[6] = (bytes[6] & 0x0f) | 0x30;

    // Set variant to IETF (RFC 4122) — 2 bits at position 8
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_uuid_is_deterministic() {
        let uuid1 = generate_offline_uuid("Steve");
        let uuid2 = generate_offline_uuid("Steve");
        assert_eq!(uuid1, uuid2);
    }

    #[test]
    fn test_offline_uuid_differs_for_diff_names() {
        let uuid1 = generate_offline_uuid("Steve");
        let uuid2 = generate_offline_uuid("Alex");
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_offline_uuid_version_and_variant() {
        let uuid = generate_offline_uuid("Steve");
        assert_eq!(uuid.get_version(), Some(uuid::Version::Md5));
        assert_eq!(uuid.get_variant(), uuid::Variant::RFC4122);
    }

    #[test]
    fn test_offline_user_creation() {
        let user = OfflineUser::new("Notch");
        assert_eq!(user.username, "Notch");
        assert_eq!(user.access_token, "");
        // UUID should be parseable
        let parsed: Uuid = user.uuid.parse().unwrap();
        assert_eq!(parsed.get_version(), Some(uuid::Version::Md5));
    }
}
