//! The `Crate` entity — an organizing bucket of genre × optional energy role.
//! (File named `crate_` because `crate` is a Rust keyword.)

use std::fmt;

use uuid::Uuid;

use crate::confidence::Energy;

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

/// Exclusive upper bound of the `Closing` energy band.
const CLOSING_ENERGY_MAX: u8 = 19;
/// Exclusive upper bound of the `Warmup` energy band.
const WARMUP_ENERGY_MAX: u8 = 44;
/// Exclusive upper bound of the `Groove` energy band; anything above is `Peak`.
const GROOVE_ENERGY_MAX: u8 = 69;

impl EnergyRole {
    /// The role for a measured `energy`, as four ascending bands:
    /// `Closing` (0–19) → `Warmup` (20–44) → `Groove` (45–69) → `Peak` (70–100).
    ///
    /// The roles name a set position, but analysis only measures loudness — so the mapping is a
    /// reading of energy, not knowledge of where a track sits in a set. `Closing` takes the lowest
    /// band because a comedown/outro is typically quieter than even a warmup opener, which is low
    /// but still driving. That is a judgement call energy alone cannot settle: a quiet opener will
    /// land in `Closing`. It is a sub-role on a genre crate, never the genre itself, and triage lets
    /// the human override it (Principle V), so the cost of a wrong band is a re-file, not a lost track.
    #[must_use]
    pub fn for_energy(energy: Energy) -> Self {
        match energy.value() {
            0..=CLOSING_ENERGY_MAX => Self::Closing,
            20..=WARMUP_ENERGY_MAX => Self::Warmup,
            45..=GROOVE_ENERGY_MAX => Self::Groove,
            _ => Self::Peak,
        }
    }

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

    fn role_at(energy: u8) -> EnergyRole {
        EnergyRole::for_energy(Energy::new(energy).expect("energy in range"))
    }

    #[test]
    fn energy_maps_to_four_ascending_bands() {
        assert_eq!(role_at(0), EnergyRole::Closing);
        assert_eq!(role_at(30), EnergyRole::Warmup);
        assert_eq!(role_at(50), EnergyRole::Groove);
        assert_eq!(role_at(100), EnergyRole::Peak);
    }

    /// The bands must tile `0..=100` with no gap and no overlap: every boundary pair belongs to
    /// adjacent roles, so no measurable energy can fall through to a wrong band.
    #[test]
    fn band_boundaries_are_contiguous() {
        assert_eq!(role_at(19), EnergyRole::Closing);
        assert_eq!(role_at(20), EnergyRole::Warmup);
        assert_eq!(role_at(44), EnergyRole::Warmup);
        assert_eq!(role_at(45), EnergyRole::Groove);
        assert_eq!(role_at(69), EnergyRole::Groove);
        assert_eq!(role_at(70), EnergyRole::Peak);
    }

    /// Energy is a scale, so the mapping must never invert: a louder track never files to a
    /// lower-energy role than a quieter one.
    #[test]
    fn mapping_is_monotonic_across_the_whole_scale() {
        let rank = |role: EnergyRole| match role {
            EnergyRole::Closing => 0,
            EnergyRole::Warmup => 1,
            EnergyRole::Groove => 2,
            EnergyRole::Peak => 3,
        };
        for energy in 0..100u8 {
            assert!(
                rank(role_at(energy)) <= rank(role_at(energy + 1)),
                "energy {energy} outranks {}",
                energy + 1
            );
        }
    }
}
