//! Core orchestration and CLI support for the Dune dedicated server manager.
//!
//! This crate intentionally contains no Tauri or UI code. It owns the
//! Windows/Hyper-V setup orchestration, guest SSH bootstrap flow, Kubernetes
//! battlegroup operations, managed external tool installation, and the
//! non-interactive CLI used to exercise those capabilities.

#![warn(missing_docs)]

/// Non-interactive command-line entry point and argument dispatch.
pub mod cli;
/// PostgreSQL access for the Dune game database.
pub mod database;
/// Shared error constructors and JSON parsing helpers.
pub mod errors;
/// Common command result types used by core operations.
pub mod models;
/// Controlled operation-log classification and redaction-aware capture.
pub mod operation_log;
/// Native setup, guest bootstrap, and battlegroup orchestration primitives.
pub mod orchestration;
/// Secret redaction helpers for plain text and JSON payloads.
pub mod security;
/// Small process and PowerShell execution helpers.
pub mod shell;
/// Managed installation of app-owned external tools.
pub mod toolchain;
/// Validation helpers for shell-safe and Kubernetes-safe values.
pub mod validation;
