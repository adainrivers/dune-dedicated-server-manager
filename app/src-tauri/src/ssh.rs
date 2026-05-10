use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use tauri::AppHandle;

use crate::{
    config_store::{app_data_dir, default_key_path, read_app_config},
    errors::{command_failure, failure},
    models::CommandResult,
    shell::{ps_single_quoted, run_powershell, run_program},
};

pub fn prepare_key(app: &AppHandle, install_path: &str) -> CommandResult<PathBuf> {
    let source = default_key_path(install_path);
    if !source.exists() {
        return Err(failure(format!(
            "Bundled SSH key was not found at {}",
            source.display()
        )));
    }

    let key_dir = app_data_dir(app)?.join("keys");
    fs::create_dir_all(&key_dir)
        .map_err(|err| failure(format!("Failed to create key directory: {err}")))?;
    let target = key_dir.join("server-manager-ssh-key");

    let script = format!(
        r#"
$src = {src}
$dst = {dst}
if (Test-Path $dst) {{
  takeown /f $dst 2>&1 | Out-Null
  icacls $dst /reset 2>&1 | Out-Null
  Remove-Item -Path $dst -Force -ErrorAction SilentlyContinue
}}
Copy-Item -Path $src -Destination $dst -Force
icacls $dst /inheritance:r /grant:r "${{env:USERNAME}}:(R)" | Out-Null
[pscustomobject]@{{ path = $dst }} | ConvertTo-Json -Compress
"#,
        src = ps_single_quoted(&source.to_string_lossy()),
        dst = ps_single_quoted(&target.to_string_lossy())
    );

    run_powershell(&script)?;
    Ok(target)
}

pub fn run_ssh(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    remote_command: &str,
) -> CommandResult<String> {
    let key = prepare_key(app, install_path)?;
    let destination = format!("{ssh_user}@{ip}");
    let key_str = key.to_string_lossy().to_string();
    let ssh_path = read_app_config(app)
        .map(|config| config.ssh_path)
        .unwrap_or_default();
    if ssh_path.is_empty() {
        return Err(failure("SSH path is not configured"));
    }
    run_program(
        &ssh_path,
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=NUL",
            "-o",
            "ConnectTimeout=6",
            "-i",
            &key_str,
            &destination,
            remote_command,
        ],
    )
}

pub fn run_ssh_with_stdin(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    remote_command: &str,
    stdin_text: &str,
) -> CommandResult<String> {
    let key = prepare_key(app, install_path)?;
    let destination = format!("{ssh_user}@{ip}");
    let key_str = key.to_string_lossy().to_string();
    let ssh_path = read_app_config(app)
        .map(|config| config.ssh_path)
        .unwrap_or_default();
    if ssh_path.is_empty() {
        return Err(failure("SSH path is not configured"));
    }

    let mut child = Command::new(&ssh_path)
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=NUL",
            "-o",
            "ConnectTimeout=6",
            "-i",
            &key_str,
            &destination,
            remote_command,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| failure(format!("Failed to run SSH: {err}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(stdin_text.as_bytes())
            .map_err(|err| failure(format!("Failed to send SSH stdin: {err}")))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|err| failure(format!("Failed to wait for SSH: {err}")))?;

    if !output.status.success() {
        return Err(command_failure("SSH command exited with an error", output));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn copy_to_guest(
    app: &AppHandle,
    install_path: &str,
    ip: &str,
    ssh_user: &str,
    source: &str,
    destination: &str,
) -> CommandResult<()> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        return Err(failure(format!(
            "Manager API binary was not found at {}",
            source_path.display()
        )));
    }

    let key = prepare_key(app, install_path)?;
    let key_str = key.to_string_lossy().to_string();
    let target = format!("{ssh_user}@{ip}:{destination}");
    let scp = scp_path(app)?;
    run_program(
        &scp,
        &[
            "-o",
            "BatchMode=yes",
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "UserKnownHostsFile=NUL",
            "-o",
            "ConnectTimeout=6",
            "-i",
            &key_str,
            source,
            &target,
        ],
    )?;
    Ok(())
}

pub fn discover_ip_from_logs(install_path: &str) -> Option<String> {
    let log_dir = Path::new(install_path).join(".logs");
    let mut logs = fs::read_dir(log_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "log"))
        .collect::<Vec<_>>();
    logs.sort_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
    logs.reverse();

    for entry in logs {
        if let Ok(text) = fs::read_to_string(entry.path()) {
            for line in text.lines().rev() {
                if let Some(value) = line.split("VM IP address:").nth(1) {
                    let ip = value.trim().to_string();
                    if !ip.is_empty() {
                        return Some(ip);
                    }
                }
            }
        }
    }

    None
}

fn scp_path(app: &AppHandle) -> CommandResult<String> {
    let ssh_path = read_app_config(app)
        .map(|config| config.ssh_path)
        .unwrap_or_default();
    if ssh_path.is_empty() {
        return Err(failure("SSH path is not configured"));
    }

    let path = Path::new(&ssh_path);
    let scp = path
        .parent()
        .map(|parent| parent.join("scp.exe"))
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| PathBuf::from("scp.exe"));
    Ok(scp.to_string_lossy().to_string())
}
