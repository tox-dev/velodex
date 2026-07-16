use std::fmt;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization as _;

/// An opaque server-user identifier that remains stable when account attributes change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(String);

impl UserId {
    /// Generate a random server-user identifier.
    #[must_use]
    pub fn random() -> Self {
        Self(format!("usr_{}", uuid::Uuid::new_v4().simple()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A validated display name and its case-insensitive lookup key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserName {
    display: String,
    canonical: String,
}

impl UserName {
    /// Preserve the trimmed display spelling and derive its canonical lookup key.
    ///
    /// # Errors
    /// Returns [`UserNameError::Empty`] when `value` contains only whitespace.
    pub fn new(value: &str) -> Result<Self, UserNameError> {
        let display = value.trim();
        if display.is_empty() {
            return Err(UserNameError::Empty);
        }
        Ok(Self {
            display: display.to_owned(),
            canonical: display.to_lowercase().nfc().collect(),
        })
    }

    #[must_use]
    pub fn display(&self) -> &str {
        &self.display
    }

    #[must_use]
    pub fn canonical(&self) -> &str {
        &self.canonical
    }
}

/// A display name that cannot identify a server user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UserNameError {
    #[error("user display name cannot be empty")]
    Empty,
}

/// Whether a server user may currently resolve as an identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserState {
    Active,
    Disabled,
}

/// One persistent server user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerUser {
    pub id: UserId,
    pub name: UserName,
    pub state: UserState,
    pub revision: u64,
}

/// An actor-neutral account change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UserLifecycleChange {
    Created {
        display_name: String,
    },
    Renamed {
        previous_display_name: String,
        display_name: String,
    },
    Disabled,
    Reactivated,
}

/// An ordered account-lifecycle record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserLifecycleEvent {
    pub user_id: UserId,
    pub sequence: u64,
    pub change: UserLifecycleChange,
}
