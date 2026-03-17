//! Integration tests for the file-based encrypted backend.
//!
//! These tests exercise `FileBackend` directly (no subprocess, no emulator).
//! They use `ZUUL_PASSPHRASE` env var for non-interactive encryption.
//!
//! Run with: `cargo test --test file_backend`

mod environments;
mod metadata;
mod secrets;
