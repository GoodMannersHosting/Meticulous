//! Validate and resolve pipeline [`met_parser::SecretRef`] values against the database
//! and optional built-in master key (AES-256-GCM + HKDF).

mod error;
mod hints;
mod resolve;

pub use error::ResolveError;
pub use hints::{hints_json_from_secret_refs, SecretResolutionHints, SecretResolutionRefHint};
pub use resolve::{
    load_secret_refs_from_definition, materialization_for_kind, resolve_for_job_run_context,
    resolve_job_secrets_for_exchange, resolve_stored_secret_map, validate_secret_refs,
};
