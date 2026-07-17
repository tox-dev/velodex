use peryx_identity::{ServerUser, UserId, UserLifecycleEvent, UserState};
use peryx_storage::meta::{MetaError, MetaStore, UserStoreError};

/// Application operations over persistent server users.
#[derive(Debug, Clone)]
pub struct UserService {
    store: MetaStore,
}

impl UserService {
    #[must_use]
    pub const fn new(store: MetaStore) -> Self {
        Self { store }
    }

    /// Create an active server user.
    ///
    /// # Errors
    /// Returns a validation, uniqueness, or storage error.
    pub fn create(&self, display_name: &str) -> Result<ServerUser, UserStoreError> {
        self.store.create_user(display_name)
    }

    /// Inspect a server user by stable ID, whether active or disabled.
    ///
    /// # Errors
    /// Returns a storage error when the user cannot be read.
    pub fn inspect(&self, id: &UserId) -> Result<Option<ServerUser>, MetaError> {
        self.store.get_user(id)
    }

    /// Resolve an active identity by display name.
    ///
    /// Disabled users remain inspectable but do not resolve through this operation.
    ///
    /// # Errors
    /// Returns a validation or storage error when the lookup cannot be completed.
    pub fn identify(&self, display_name: &str) -> Result<Option<ServerUser>, UserStoreError> {
        Ok(self
            .store
            .get_user_by_name(display_name)?
            .filter(|user| user.state == UserState::Active))
    }

    /// Change a user's display name while preserving its stable ID.
    ///
    /// # Errors
    /// Returns a validation, uniqueness, missing-user, or storage error.
    pub fn rename(&self, id: &UserId, display_name: &str) -> Result<ServerUser, UserStoreError> {
        self.store.rename_user(id, display_name)
    }

    /// Stop a user from resolving as an identity.
    ///
    /// # Errors
    /// Returns a missing-user or storage error.
    pub fn disable(&self, id: &UserId) -> Result<ServerUser, UserStoreError> {
        self.store.set_user_state(id, UserState::Disabled)
    }

    /// Allow a disabled user to resolve as an identity again.
    ///
    /// # Errors
    /// Returns a missing-user or storage error.
    pub fn reactivate(&self, id: &UserId) -> Result<ServerUser, UserStoreError> {
        self.store.set_user_state(id, UserState::Active)
    }

    /// Read one user's actor-neutral lifecycle history.
    ///
    /// # Errors
    /// Returns a storage error when the events cannot be read.
    pub fn events(&self, id: &UserId) -> Result<Vec<UserLifecycleEvent>, MetaError> {
        self.store.user_events(id)
    }
}
