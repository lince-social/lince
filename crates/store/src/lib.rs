//! Lince's SQLite Store and the storage operations for its domains.

pub mod access_snapshot;
pub mod action_intents;
pub mod artifact_deliveries;
pub mod assertions;
pub mod auth;
pub mod backoff;
pub mod blob_sync;
pub mod budget;
pub mod cells;
pub mod claim_recipes;
pub mod communication;
pub mod concepts;
pub mod config;
pub mod contact_disclosures;
pub mod contact_rate;
pub mod contact_share;
pub mod data_visibility;
mod database;
pub mod door;
pub mod entries;
pub mod evidence_custody;
pub mod evidence_forwarding;
pub mod evidence_serving;
pub mod exact;
pub mod executor;
pub mod facts;
pub mod file_sync_baselines;
pub mod frequency;
pub mod identity_roots;
pub mod invites;
pub mod karma;
pub mod karma_bindings;
pub mod karma_commands;
pub mod karma_fields;
pub mod karma_schedules;
pub mod karma_stages;
pub mod knowledge;
pub mod ledger;
pub mod linguas;
pub mod location;
pub mod logins;
pub mod mail_left;
pub mod mailbox;
pub mod message_content;
pub mod misc;
pub mod offers;
pub mod operation_receipts;
pub mod organ_access;
pub mod organs;
pub mod peer_delivery;
pub mod people;
pub mod person_roles;
pub mod places;
pub mod private_contacts;
pub mod profile_claims;
pub mod projection;
pub mod read_filter;
pub mod receipt_events;
pub mod receipt_references;
pub mod receipt_sources;
pub mod record_changes;
pub mod record_docs;
pub mod record_edit_drafts;
pub mod record_move;
pub mod record_revisions;
pub mod records;
pub mod recurrence;
pub mod replica;
pub mod role_permissions;
pub mod role_policies;
pub mod roles;
pub mod roster;
pub mod sand_packages;
pub mod seed;
pub mod senses;
pub mod session_access;
pub mod signal_endpoints;
pub mod signal_streams;
pub mod simulation_checks;
pub mod snapshot;
pub mod social;
pub mod source_locations;
pub mod sync_activity;
pub mod sync_apply;
pub mod sync_ops;
pub mod transfer_accounting;
pub mod transfer_agreement;
pub mod transfer_balances;
pub mod transfer_cancellations;
pub mod transfer_children;
pub mod transfer_delivery;
pub mod transfer_effects;
pub mod transfer_evidence;
pub mod transfer_loans;
pub mod transfer_outcomes;
pub mod transfer_receipts;
pub mod transfer_replication;
pub mod transfer_stock;
pub mod transfers;
pub mod transport;
pub mod trust_queries;
pub mod visibility;
pub mod vocabulary;

pub use database::write_tx;
use nucleus::projection::ExecutionIdentity;
pub use sqlx;
use sqlx::SqlitePool;

/// A SQLite connection pool and the optional identity of its execution environment.
#[derive(Debug, Clone)]
pub struct Store {
    /// Connections used by Store queries and write transactions.
    pub pool: SqlitePool,
    execution_identity: Option<&'static ExecutionIdentity>,
}

/// Error reported by SQLite operations and Store invariant checks.
pub type StoreError = sqlx::Error;

impl Store {
    /// Wraps the supplied SQLite pool as-is.
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self {
            pool,
            execution_identity: None,
        }
    }

    /// Associates a trusted execution identity with this Store handle.
    pub fn with_execution_identity(mut self, identity: &'static ExecutionIdentity) -> Self {
        self.execution_identity = Some(identity);
        self
    }

    /// Returns the execution identity explicitly associated with this handle.
    pub fn execution_identity(&self) -> Option<&ExecutionIdentity> {
        self.execution_identity
    }
}
