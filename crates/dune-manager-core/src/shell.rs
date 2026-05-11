//! Host shell helpers used by strict command providers.

use std::process::Command;

use crate::errors::{command_failure, failure};
use crate::models::CommandResult;

/// Runs a program and returns trimmed stdout when it exits successfully.
pub fn run_program(program: &str, args: &[&str]) -> CommandResult<String> {
    let mut command = Command::new(program);
    suppress_console_window(&mut command);
    let output = command
        .args(args)
        .output()
        .map_err(|err| failure(format!("Failed to run {program}: {err}")))?;

    if !output.status.success() {
        return Err(command_failure(
            format!("{program} exited with an error"),
            output,
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Configures a child process so GUI launches on Windows do not flash a console window.
pub fn suppress_console_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Runs a PowerShell script with non-profile, bypassed execution policy options.
pub fn run_powershell(script: &str) -> CommandResult<String> {
    run_program(
        "powershell",
        &[
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
}

/// Escapes a string as a single-quoted PowerShell literal.
pub fn ps_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
