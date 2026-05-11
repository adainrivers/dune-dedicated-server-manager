use std::{
    fmt,
    process::{Command, Stdio},
};

use serde::{de::DeserializeOwned, Serialize};

use crate::{
    errors::{command_failure, failure},
    models::CommandResult,
    shell::suppress_console_window,
};

/// Host execution bridge used by an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostBridge {
    /// Direct process invocation.
    Native,
    /// PowerShell invocation expected to emit exactly one JSON document.
    StrictJsonPowerShell,
}

impl fmt::Display for HostBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostBridge::Native => f.write_str("native"),
            HostBridge::StrictJsonPowerShell => f.write_str("strict-json-powershell"),
        }
    }
}

/// Concrete command request with a stable operation id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrictCommandSpec {
    /// Stable command id used in errors and logs.
    pub id: &'static str,
    /// Executable path or name.
    pub program: String,
    /// Command-line arguments.
    pub args: Vec<String>,
}

impl StrictCommandSpec {
    /// Creates a strict command spec.
    pub fn new(
        id: &'static str,
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            id,
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

/// Runs commands and optionally parses strict JSON output.
#[derive(Debug, Clone, Default)]
pub struct StrictCommandRunner;

impl StrictCommandRunner {
    /// Runs a command and returns trimmed stdout text.
    pub fn run_text(&self, spec: &StrictCommandSpec) -> CommandResult<String> {
        let mut command = Command::new(&spec.program);
        suppress_console_window(&mut command);
        let output = command
            .args(&spec.args)
            .stdin(Stdio::null())
            .output()
            .map_err(|err| failure(format!("Failed to run {}: {err}", spec.id)))?;

        if !output.status.success() {
            return Err(command_failure(
                format!("{} exited with an error", spec.id),
                output,
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Runs a command and parses stdout as a single JSON document.
    pub fn run_json<T: DeserializeOwned>(&self, spec: &StrictCommandSpec) -> CommandResult<T> {
        parse_single_json_document(&self.run_text(spec)?, spec.id)
    }
}

/// Parses text that must contain exactly one JSON document.
pub fn parse_single_json_document<T: DeserializeOwned>(
    text: &str,
    label: &str,
) -> CommandResult<T> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let value = T::deserialize(&mut deserializer)
        .map_err(|err| failure(format!("Failed to parse {label} JSON: {err}")))?;
    deserializer
        .end()
        .map_err(|err| failure(format!("{label} produced trailing non-JSON output: {err}")))?;
    Ok(value)
}

/// Builds a non-interactive PowerShell command that should emit JSON.
pub fn powershell_json_command(id: &'static str, script: &str) -> StrictCommandSpec {
    StrictCommandSpec::new(
        id,
        "powershell",
        [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_json_rejects_trailing_console_text() {
        let result =
            parse_single_json_document::<serde_json::Value>("{\"ok\":true}\nextra", "sample");
        assert!(result.is_err());
    }

    #[test]
    fn strict_json_accepts_single_document() {
        let value =
            parse_single_json_document::<serde_json::Value>("{\"ok\":true}\n", "sample").unwrap();
        assert_eq!(value["ok"], true);
    }
}
