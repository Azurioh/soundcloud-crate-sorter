//! The `Crate` entity — an organizing bucket of genre × optional energy role.
//! (File named `crate_` because `crate` is a Rust keyword.)

use std::fmt;

use uuid::Uuid;

/// Stable identity of a `Crate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CrateId(Uuid);

impl CrateId {
    /// Wraps a raw UUID (minted by the application's `IdProvider`).
    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Returns the underlying UUID (for persistence at the adapter edge).
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for CrateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The energy position of a crate within a set (the sub-axis unlocked by audio analysis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergyRole {
    /// Early-set, lower-energy openers.
    Warmup,
    /// Mid-set groove builders.
    Groove,
    /// Peak-time high energy.
    Peak,
    /// Set-closing wind-down.
    Closing,
}

impl EnergyRole {
    /// Human-readable label used in crate display names.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Warmup => "Warmup",
            Self::Groove => "Groove",
            Self::Peak => "Peak",
            Self::Closing => "Closing",
        }
    }

    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::Groove => "groove",
            Self::Peak => "peak",
            Self::Closing => "closing",
        }
    }

    /// Parses a persistence token back into an energy role.
    ///
    /// # Errors
    /// Returns `None` for an unrecognized token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "warmup" => Some(Self::Warmup),
            "groove" => Some(Self::Groove),
            "peak" => Some(Self::Peak),
            "closing" => Some(Self::Closing),
            _ => None,
        }
    }
}

/// How a crate came to exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrateOrigin {
    /// Discovered from a genre/energy present in the scanned library.
    Auto,
    /// Created by the user during triage.
    Manual,
}

impl CrateOrigin {
    /// Lowercase persistence token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }

    /// Parses a persistence token back into an origin.
    ///
    /// # Errors
    /// Returns `None` for an unrecognized token.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "auto" => Some(Self::Auto),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// An organizing bucket. Crates are created dynamically from the genres/energies actually
/// present in the library (FR-009) — there is no fixed global taxonomy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crate {
    id: CrateId,
    genre: String,
    energy_role: Option<EnergyRole>,
    created_by: CrateOrigin,
}

impl Crate {
    /// Builds a crate. The display name is derived from `genre` and the optional `energy_role`.
    #[must_use]
    pub fn new(
        id: CrateId,
        genre: String,
        energy_role: Option<EnergyRole>,
        created_by: CrateOrigin,
    ) -> Self {
        Self {
            id,
            genre,
            energy_role,
            created_by,
        }
    }

    /// The crate's identity.
    #[must_use]
    pub const fn id(&self) -> &CrateId {
        &self.id
    }

    /// The primary genre axis.
    #[must_use]
    pub fn genre(&self) -> &str {
        &self.genre
    }

    /// The optional energy sub-role.
    #[must_use]
    pub const fn energy_role(&self) -> Option<EnergyRole> {
        self.energy_role
    }

    /// How the crate was created.
    #[must_use]
    pub const fn created_by(&self) -> CrateOrigin {
        self.created_by
    }

    /// Derived display name, e.g. `"Deep House · Peak"` or `"Deep House"`.
    #[must_use]
    pub fn display_name(&self) -> String {
        match self.energy_role {
            Some(role) => format!("{} · {}", self.genre, role.label()),
            None => self.genre.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crate_id() -> CrateId {
        CrateId::from_uuid(Uuid::nil())
    }

    #[test]
    fn display_name_without_role_is_just_genre() {
        let c = Crate::new(crate_id(), "Deep House".into(), None, CrateOrigin::Auto);
        assert_eq!(c.display_name(), "Deep House");
    }

    #[test]
    fn display_name_with_role_joins_genre_and_role() {
        let c = Crate::new(
            crate_id(),
            "Deep House".into(),
            Some(EnergyRole::Peak),
            CrateOrigin::Auto,
        );
        assert_eq!(c.display_name(), "Deep House · Peak");
    }
}
