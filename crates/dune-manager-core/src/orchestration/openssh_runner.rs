use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    errors::{command_failure, failure},
    models::CommandResult,
    orchestration::{RemoteCommandRunner, StrictCommandSpec},
    shell::suppress_console_window,
};

/// Connection settings for invoking OpenSSH against the guest VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshTarget {
    /// Path to the OpenSSH client executable.
    pub ssh_path: PathBuf,
    /// Path to the private key file.
    pub key_path: PathBuf,
    /// Remote username.
    pub user: String,
    /// Remote host or IP address.
    pub host: String,
    /// SSH connection timeout in seconds.
    pub connect_timeout_seconds: u64,
}

impl OpenSshTarget {
    /// Creates a target with the default connection timeout.
    pub fn new(
        ssh_path: impl Into<PathBuf>,
        key_path: impl Into<PathBuf>,
        user: impl Into<String>,
        host: impl Into<String>,
    ) -> Self {
        Self {
            ssh_path: ssh_path.into(),
            key_path: key_path.into(),
            user: user.into(),
            host: host.into(),
            connect_timeout_seconds: 8,
        }
    }

    /// Returns the `user@host` destination string.
    pub fn destination(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    /// Validates that required files and connection fields are present.
    pub fn validate(&self) -> CommandResult<()> {
        require_existing_file(&self.ssh_path, "ssh executable")?;
        require_existing_file(&self.key_path, "ssh key")?;
        if self.user.trim().is_empty() {
            return Err(failure("SSH user is required"));
        }
        if self.host.trim().is_empty() {
            return Err(failure("SSH host is required"));
        }
        Ok(())
    }
}

/// Remote command runner backed by the OpenSSH executable.
#[derive(Debug, Clone)]
pub struct OpenSshRunner {
    target: OpenSshTarget,
}

impl OpenSshRunner {
    /// Creates a runner for an OpenSSH target.
    pub fn new(target: OpenSshTarget) -> Self {
        Self { target }
    }

    /// Returns the target used by this runner.
    pub fn target(&self) -> &OpenSshTarget {
        &self.target
    }

    /// Builds a command spec for opening an interactive guest shell.
    pub fn interactive_shell_spec(&self) -> CommandResult<StrictCommandSpec> {
        self.target.validate()?;
        let mut args = self.base_args();
        args.push(self.target.destination());
        Ok(StrictCommandSpec::new(
            "ssh.interactive-shell",
            self.target.ssh_path.to_string_lossy(),
            args,
        ))
    }

    fn base_args(&self) -> Vec<String> {
        openssh_base_args(&self.target)
    }

    fn run_with_optional_stdin(
        &self,
        remote_command: &str,
        stdin_body: Option<&str>,
    ) -> CommandResult<String> {
        self.target.validate()?;
        let mut args = self.base_args();
        args.push(self.target.destination());
        args.push(remote_command.to_string());

        let mut command = Command::new(&self.target.ssh_path);
        suppress_console_window(&mut command);
        let mut child = command
            .args(args)
            .stdin(if stdin_body.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| failure(format!("Failed to start ssh: {err}")))?;

        if let (Some(body), Some(mut stdin)) = (stdin_body, child.stdin.take()) {
            stdin
                .write_all(body.as_bytes())
                .map_err(|err| failure(format!("Failed to write ssh script: {err}")))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|err| failure(format!("Failed to wait for ssh: {err}")))?;
        if !output.status.success() {
            return Err(command_failure("ssh exited with an error", output));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl RemoteCommandRunner for OpenSshRunner {
    fn run(&self, command: &str) -> CommandResult<String> {
        self.run_with_optional_stdin(command, None)
    }

    fn run_script(&self, script: &str) -> CommandResult<String> {
        self.run_with_optional_stdin("sh -s", Some(script))
    }
}

/// Returns the base OpenSSH options used for non-interactive guest commands.
pub fn openssh_base_args(target: &OpenSshTarget) -> Vec<String> {
    vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "PreferredAuthentications=publickey".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=no".to_string(),
        "-o".to_string(),
        "UserKnownHostsFile=NUL".to_string(),
        "-o".to_string(),
        "LogLevel=ERROR".to_string(),
        "-o".to_string(),
        format!("ConnectTimeout={}", target.connect_timeout_seconds),
        "-i".to_string(),
        target.key_path.to_string_lossy().to_string(),
    ]
}

fn require_existing_file(path: &Path, label: &str) -> CommandResult<()> {
    if path.as_os_str().is_empty() {
        return Err(failure(format!("{label} path is required")));
    }
    if !path.is_file() {
        return Err(failure(format!(
            "{label} was not found: {}",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_body_is_not_part_of_base_ssh_arguments() {
        let target = OpenSshTarget {
            ssh_path: "ssh.exe".into(),
            key_path: "key".into(),
            user: "dune".to_string(),
            host: "10.0.0.4".to_string(),
            connect_timeout_seconds: 11,
        };
        let args = openssh_base_args(&target);
        let joined = args.join(" ");

        assert!(joined.contains("ConnectTimeout=11"));
        assert!(joined.contains("-i key"));
        assert!(!joined.contains("self-host-token"));
    }

    #[test]
    fn destination_uses_user_and_host_only() {
        let target = OpenSshTarget::new("ssh.exe", "key", "dune", "10.0.0.4");
        assert_eq!(target.destination(), "dune@10.0.0.4");
    }

    #[test]
    fn interactive_shell_spec_contains_no_remote_command() {
        let temp = std::env::temp_dir();
        let ssh = temp.join("dune-test-ssh.exe");
        let key = temp.join("dune-test-key");
        std::fs::write(&ssh, "").unwrap();
        std::fs::write(&key, "").unwrap();
        let runner = OpenSshRunner::new(OpenSshTarget::new(&ssh, &key, "dune", "10.0.0.4"));
        let spec = runner.interactive_shell_spec().unwrap();

        assert_eq!(spec.program, ssh.to_string_lossy());
        assert_eq!(spec.args.last().unwrap(), "dune@10.0.0.4");
        assert!(!spec.args.iter().any(|arg| arg.contains("kubectl")));
        let _ = std::fs::remove_file(ssh);
        let _ = std::fs::remove_file(key);
    }
}
