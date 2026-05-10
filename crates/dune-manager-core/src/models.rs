use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CommandFailure {
    pub message: String,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

pub type CommandResult<T> = Result<T, CommandFailure>;
