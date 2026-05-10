//! Native orchestration primitives for replacing the vendor scripts.
//!
//! The UI-facing Tauri commands still contain legacy glue. This module is the
//! typed target shape: script behavior is expressed as explicit flow plans,
//! executor boundaries, and strict command contracts that can be reused by
//! Hyper-V now and Docker/Kubernetes providers later.

pub mod battlegroup_kubernetes;
pub mod battlegroup_management;
pub mod guest_bootstrap;
pub mod guest_bootstrap_ssh;
pub mod guest_ssh;
pub mod hyperv_bridge;
pub mod hyperv_initial_setup;
pub mod hyperv_lifecycle;
pub mod hyperv_setup;
pub mod kubernetes_ssh;
pub mod openssh_runner;
pub mod providers;
pub mod strict_command;
pub mod vendor_flows;

pub use battlegroup_kubernetes::*;
pub use battlegroup_management::*;
pub use guest_bootstrap::*;
pub use guest_bootstrap_ssh::*;
pub use guest_ssh::*;
pub use hyperv_bridge::*;
pub use hyperv_initial_setup::*;
pub use hyperv_lifecycle::*;
pub use hyperv_setup::*;
pub use kubernetes_ssh::*;
pub use openssh_runner::*;
pub use providers::*;
pub use strict_command::*;
pub use vendor_flows::*;
