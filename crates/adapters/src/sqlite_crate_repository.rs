//! `SqliteCrateRepository` — SQLite-backed `CrateRepository` with dynamic find-or-create (T017/T034).
//!
//! A crate's optional energy role is stored as `''` (not NULL) so `UNIQUE(genre, energy_role)` and
//! equality lookups behave predictably. The shared-connection mutex serializes the find-then-insert
//! in `find_or_create`, so it is race-free without an explicit transaction.

use std::sync::Arc;

use application::ports::crate_repository::{CrateRepository, CrateSpec};
use application::ports::id_provider::IdProvider;
use application::ports::repo_error::RepoError;
use async_trait::async_trait;
use domain::crate_::{Crate, CrateId, CrateOrigin, EnergyRole};
use rusqlite::{params, OptionalExtension, Row};
use uuid::Uuid;

use crate::sqlite_support::{to_repo_error, SharedConnection};

/// Column list shared by every SELECT, in the order [`read_raw_row`] expects.
const SELECT_COLUMNS: &str = "id, genre, energy_role, created_by";

/// A crate row as raw SQLite primitives, before domain parsing.
struct RawCrateRow {
    id: String,
    genre: String,
    energy_role: String,
    created_by: String,
}

/// SQLite-backed crate store. Mints ids for new crates from the injected `IdProvider`.
pub struct SqliteCrateRepository {
    connection: SharedConnection,
    ids: Arc<dyn IdProvider>,
}

impl SqliteCrateRepository {
    /// Builds the repository over a shared connection and the id provider.
    #[must_use]
    pub fn new(connection: SharedConnection, ids: Arc<dyn IdProvider>) -> Self {
        Self { connection, ids }
    }
}

#[async_trait]
impl CrateRepository for SqliteCrateRepository {
    async fn find_or_create(&self, spec: &CrateSpec) -> Result<Crate, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let role_token = spec.role.map_or("", EnergyRole::as_str);

        let existing = conn
            .query_row(
                &format!(
                    "SELECT {SELECT_COLUMNS} FROM crates WHERE genre = ?1 AND energy_role = ?2"
                ),
                params![spec.genre, role_token],
                read_raw_row,
            )
            .optional()
            .map_err(to_repo_error)?;
        if let Some(raw) = existing {
            return raw_to_crate(raw);
        }

        let id = CrateId::from_uuid(self.ids.new_id());
        conn.execute(
            "INSERT INTO crates (id, genre, energy_role, created_by) VALUES (?1, ?2, ?3, ?4)",
            params![id.to_string(), spec.genre, role_token, spec.origin.as_str()],
        )
        .map_err(to_repo_error)?;
        Ok(Crate::new(id, spec.genre.clone(), spec.role, spec.origin))
    }

    async fn find_by_id(&self, id: &CrateId) -> Result<Option<Crate>, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let raw = conn
            .query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM crates WHERE id = ?1"),
                [id.to_string()],
                read_raw_row,
            )
            .optional()
            .map_err(to_repo_error)?;
        raw.map(raw_to_crate).transpose()
    }

    async fn list(&self) -> Result<Vec<Crate>, RepoError> {
        let conn = self.connection.lock().expect("sqlite mutex poisoned");
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {SELECT_COLUMNS} FROM crates ORDER BY genre, energy_role"
            ))
            .map_err(to_repo_error)?;
        let rows = stmt.query_map([], read_raw_row).map_err(to_repo_error)?;
        let mut crates = Vec::new();
        for row in rows {
            crates.push(raw_to_crate(row.map_err(to_repo_error)?)?);
        }
        Ok(crates)
    }
}

/// Reads one row into raw primitives.
fn read_raw_row(row: &Row<'_>) -> rusqlite::Result<RawCrateRow> {
    Ok(RawCrateRow {
        id: row.get(0)?,
        genre: row.get(1)?,
        energy_role: row.get(2)?,
        created_by: row.get(3)?,
    })
}

/// Converts a raw row into a domain `Crate`, mapping parse failures to `Serialization`.
fn raw_to_crate(raw: RawCrateRow) -> Result<Crate, RepoError> {
    let id =
        CrateId::from_uuid(
            Uuid::parse_str(&raw.id).map_err(|e| RepoError::Serialization {
                source: Box::new(e),
            })?,
        );
    let role = if raw.energy_role.is_empty() {
        None
    } else {
        Some(
            EnergyRole::from_token(&raw.energy_role).ok_or_else(|| RepoError::Serialization {
                source: "unknown energy role".into(),
            })?,
        )
    };
    let origin =
        CrateOrigin::from_token(&raw.created_by).ok_or_else(|| RepoError::Serialization {
            source: "unknown crate origin".into(),
        })?;
    Ok(Crate::new(id, raw.genre, role, origin))
}
