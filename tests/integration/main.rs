//! Integration tests for zuul against the GCP Secret Manager emulator.
//!
//! These tests require a running emulator on `localhost:9090`.
//! Start it with: `docker compose -f docker-compose.emulator.yml up -d`
//!
//! Run with: `cargo test --test integration -- --ignored`

mod access_control;
mod auth;
mod config_errors;
mod diff;
mod env;
mod export_import;
mod helpers;
mod init;
mod metadata;
mod secret;
