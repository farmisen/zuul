//! Integration tests for the file-based encrypted backend.
//!
//! Two levels of testing:
//! - Direct Backend trait tests (environments, secrets, metadata modules)
//! - CLI subprocess tests (cli_* modules) — same coverage as GCP emulator suite
//!
//! All tests use `ZUUL_PASSPHRASE` env var for non-interactive encryption.
//! No external dependencies — runs with plain `cargo test`.
//!
//! Run with: `cargo test --test file_backend`

mod environments;
mod helpers;
mod metadata;
mod secrets;

mod cli_diff;
mod cli_edge_cases;
mod cli_env;
mod cli_export_import;
mod cli_init;
mod cli_metadata;
mod cli_secret;
