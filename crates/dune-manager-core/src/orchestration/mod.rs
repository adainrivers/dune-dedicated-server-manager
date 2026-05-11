//! Native orchestration primitives for replacing the vendor scripts.
//!
//! The UI-facing Tauri commands still contain legacy glue. This module is the
//! typed target shape: script behavior is expressed as explicit flow plans,
//! executor boundaries, and strict command contracts that can be reused by
//! Hyper-V now and Docker/Kubernetes providers later.

/// Kubernetes-backed battlegroup inspection, patching, shell, and log operations.
pub mod battlegroup_kubernetes;
/// High-level battlegroup lifecycle orchestration.
pub mod battlegroup_management;
/// Guest experimental swap and low-memory BattleGroup profile support.
pub mod experimental_swap;
/// Guest bootstrap planning and sequencing.
pub mod guest_bootstrap;
/// SSH implementation of the guest bootstrap provider.
pub mod guest_bootstrap_ssh;
/// SSH implementation of guest setup operations.
pub mod guest_ssh;
/// Strict PowerShell implementation of Hyper-V provider traits.
pub mod hyperv_bridge;
/// End-to-end Hyper-V initial setup orchestration.
pub mod hyperv_initial_setup;
/// Hyper-V VM lifecycle orchestration.
pub mod hyperv_lifecycle;
/// Hyper-V VM import and preparation orchestration.
pub mod hyperv_setup;
/// BattleGroup map instance partition management.
pub mod instance_management;
/// SSH-backed Kubernetes provider.
pub mod kubernetes_ssh;
/// SSH-based Manager API installation and health checks.
pub mod manager_api_installer;
/// OpenSSH command runner and shell command construction.
pub mod openssh_runner;
/// Provider traits and shared provider data models.
pub mod providers;
/// Strict command execution and strict JSON parsing.
pub mod strict_command;
/// Declarative flow descriptions derived from the vendor scripts.
pub mod vendor_flows;

pub use battlegroup_kubernetes::*;
pub use battlegroup_management::*;
pub use experimental_swap::*;
pub use guest_bootstrap::*;
pub use guest_bootstrap_ssh::*;
pub use guest_ssh::*;
pub use hyperv_bridge::*;
pub use hyperv_initial_setup::*;
pub use hyperv_lifecycle::*;
pub use hyperv_setup::*;
pub use instance_management::*;
pub use kubernetes_ssh::*;
pub use manager_api_installer::*;
pub use openssh_runner::*;
pub use providers::*;
pub use strict_command::*;
pub use vendor_flows::*;
