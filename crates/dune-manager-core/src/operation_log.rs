use serde::{Deserialize, Serialize};

use crate::security::redact_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEvent {
    pub stage: String,
    pub level: LogLevel,
    pub message: String,
}

impl OperationLogEvent {
    pub fn info(stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            level: LogLevel::Info,
            message: message.into(),
        }
    }

    pub fn warning(stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            level: LogLevel::Warning,
            message: message.into(),
        }
    }

    pub fn error(stage: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: stage.into(),
            level: LogLevel::Error,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct StreamLogCapture {
    pub raw: String,
    pub controlled: String,
    pub events: Vec<OperationLogEvent>,
}

impl StreamLogCapture {
    pub fn push_raw(&mut self, line: &str) {
        let line = redact_text(line).trim_end().to_string();
        if line.is_empty() {
            return;
        }
        self.raw.push_str(&line);
        self.raw.push('\n');
    }

    pub fn push_event(&mut self, event: OperationLogEvent) {
        let message = redact_text(&event.message).trim_end().to_string();
        if message.is_empty() {
            return;
        }
        self.controlled.push_str(&message);
        self.controlled.push('\n');
        self.events.push(OperationLogEvent { message, ..event });
    }

    pub fn push_controlled(&mut self, stage: &str, message: &str) {
        self.push_event(OperationLogEvent::info(stage, message));
    }
}

pub fn classify_command_output(stage: &str, line: &str) -> Option<OperationLogEvent> {
    let redacted = redact_text(line);
    let line = redacted.trim();
    if line.is_empty() || line == "<redacted>" {
        return None;
    }

    let lower = line.to_ascii_lowercase();
    let level = if lower.contains("error") || lower.contains("failed") {
        LogLevel::Error
    } else if lower.contains("warning") || lower.contains("warn") {
        LogLevel::Warning
    } else {
        LogLevel::Info
    };

    if is_low_value_command_noise(&line, &lower) {
        return None;
    }

    let message = if let Some(progress) = steam_progress(&line) {
        progress
    } else if lower.contains("connection timed out") {
        "SSH connection timed out while reaching the VM.".to_string()
    } else if lower.contains("connection refused") {
        "The remote service refused the connection.".to_string()
    } else if lower.contains("missing guest commands:") {
        line.to_string()
    } else if lower.contains("no such file or directory") {
        "A required guest file or command is missing.".to_string()
    } else if lower.contains("success!") && lower.contains("fully installed") {
        "Steam app installed successfully.".to_string()
    } else if lower == "loading steam api...ok" {
        "SteamCMD initialized.".to_string()
    } else if lower.contains("download complete") || lower.contains("download complete.") {
        "SteamCMD download completed.".to_string()
    } else if lower.contains("extracting package")
        || lower.contains("installing update")
        || lower.contains("cleaning up")
    {
        "SteamCMD is applying updates.".to_string()
    } else if lower.contains("update complete") {
        "SteamCMD update completed.".to_string()
    } else if lower.contains("verifying installation") || lower.contains("verifying update") {
        "SteamCMD is verifying files.".to_string()
    } else if lower.contains("connecting anonymously") {
        "SteamCMD connected anonymously.".to_string()
    } else if lower.starts_with("deployment.apps/") && lower.ends_with(" scaled") {
        "Kubernetes deployment scaled.".to_string()
    } else if lower.starts_with("deployment.apps/") && lower.contains(" image updated") {
        "Kubernetes deployment image updated.".to_string()
    } else if lower.starts_with("customresourcedefinition.")
        && (lower.ends_with(" replaced") || lower.ends_with(" configured"))
    {
        "Kubernetes custom resource definition updated.".to_string()
    } else if lower.starts_with("clusterrole.") && lower.ends_with(" replaced") {
        "Kubernetes cluster role updated.".to_string()
    } else if lower.starts_with("namespace/") && lower.ends_with(" created") {
        "Kubernetes namespace created.".to_string()
    } else if lower.starts_with("secret/") && lower.ends_with(" created") {
        "Kubernetes secret created.".to_string()
    } else if lower.starts_with("battlegroup.") && lower.ends_with(" created") {
        "Battlegroup resource created.".to_string()
    } else if lower.starts_with("battlegroup.") && lower.ends_with(" patched") {
        "Battlegroup resource patched.".to_string()
    } else if lower.starts_with("warning: permanently added") {
        return None;
    } else if stage == "guest-images" && lower.starts_with("registry.") {
        return None;
    } else if is_controlled_status_line(&lower) {
        line.to_string()
    } else if level == LogLevel::Warning {
        "Command reported a warning.".to_string()
    } else if level == LogLevel::Error {
        "Command reported an error.".to_string()
    } else {
        return None;
    };

    Some(OperationLogEvent {
        stage: stage.to_string(),
        level,
        message,
    })
}

fn is_low_value_command_noise(line: &str, lower: &str) -> bool {
    lower == "saved"
        || lower == "importing"
        || lower.starts_with("elapsed:")
        || lower.starts_with("total:")
        || lower.ends_with("b/s)")
        || lower.starts_with("application/vnd.")
        || lower.starts_with("sha256:")
        || lower.starts_with("redirecting stderr to")
        || lower.starts_with("logging directory:")
        || lower == "ok"
        || lower.starts_with("waiting for client config")
        || lower.starts_with("waiting for user info")
        || lower.starts_with("logging off current session")
        || lower.starts_with("unloading steam api")
        || lower.starts_with("steamcmd has been disconnected")
        || lower.starts_with("updateui:")
        || lower.contains("restarting steamcmd by request")
        || lower.starts_with("docker.io/")
        || lower.starts_with("quay.io/")
        || lower.starts_with("registry.funcom.com/")
        || lower.starts_with("-- type 'quit'")
        || lower.starts_with("steam console client")
        || line.chars().all(|ch| ch == '-' || ch == '[' || ch == ']')
}

fn is_controlled_status_line(lower: &str) -> bool {
    lower.starts_with("starting ")
        || lower.starts_with("waiting ")
        || lower.starts_with("loading ")
        || lower.starts_with("patching ")
        || lower.starts_with("updating ")
        || lower.starts_with("deploying ")
        || lower.starts_with("using dhcp")
        || lower.starts_with("root filesystem has ")
        || lower.starts_with("guest disk has ")
        || lower.starts_with("guest server payload")
        || lower.starts_with("player-facing ip saved")
        || lower.starts_with("current operator version:")
        || lower.starts_with("downloaded operator version:")
        || lower.starts_with("downloaded battlegroup version:")
        || lower.starts_with("operator version is already current")
        || lower.starts_with("node did not reach ready")
        || lower == "k3s and operators are ready."
        || lower == "battlegroup world resource created."
        || lower == "battlegroup images loaded and resource patched."
        || lower == "default user settings deployed."
}

fn steam_progress(line: &str) -> Option<String> {
    if let Some(percent) = line
        .split("progress:")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
    {
        return Some(format!("SteamCMD progress: {percent}%."));
    }

    if line.starts_with('[') && line.contains('%') {
        let end = line.find('%')?;
        let digits = line[..end]
            .chars()
            .filter(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if !digits.is_empty() {
            return Some(format!("SteamCMD update progress: {digits}%."));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_steam_progress_without_raw_line() {
        let event = classify_command_output(
            "guest-download",
            " Update state (0x61) downloading, progress: 48.23 (2505819648 / 5196009376)",
        )
        .unwrap();
        assert_eq!(event.message, "SteamCMD progress: 48.23%.");
    }

    #[test]
    fn hides_container_import_noise() {
        assert!(classify_command_output(
            "guest-k3s",
            "application/vnd.oci.image.index.v1+json sha256:abc"
        )
        .is_none());
        assert!(classify_command_output("guest-k3s", "elapsed: 0.3 s").is_none());
    }

    #[test]
    fn suppresses_unclassified_cli_lines() {
        assert!(classify_command_output("guest-k3s", "some-tool --verbose raw detail").is_none());
    }

    #[test]
    fn maps_kubernetes_resources_to_controlled_messages() {
        let event = classify_command_output(
            "guest-world",
            "battlegroup.igw.funcom.com/sh-examplehost-abcdef created",
        )
        .unwrap();
        assert_eq!(event.message, "Battlegroup resource created.");
    }
}
