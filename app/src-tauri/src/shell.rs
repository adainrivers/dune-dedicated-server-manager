use std::process::Command;

use crate::errors::{command_failure, failure};
use crate::models::CommandResult;

pub fn run_program(program: &str, args: &[&str]) -> CommandResult<String> {
    let output = Command::new(program)
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

pub fn ps_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
