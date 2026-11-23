//! Runs Horizon's own `install` and `prepare` steps from inside Nebula and collects their output.
//! Downloading, signature checks and reading the player's Minecraft files stay with Horizon.

use std::fmt;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

const MAX_LOG_LINES: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetupStep {
    Install,
    Prepare,
}

impl SetupStep {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Install => &["install", "latest"],
            Self::Prepare => &["prepare"],
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Install => "Installing Horizon",
            Self::Prepare => "Preparing game files",
        }
    }

    pub(crate) fn done_message(self) -> &'static str {
        match self {
            Self::Install => "Horizon is installed. Prepare the game files next.",
            Self::Prepare => "Game files are ready.",
        }
    }
}

#[derive(Debug)]
pub(crate) enum SetupError {
    Spawn(io::Error),
    Failed { step: SetupStep, code: Option<i32> },
    Wait(io::Error),
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "could not start Horizon's launcher: {error}"),
            Self::Wait(error) => write!(f, "could not check progress: {error}"),
            Self::Failed { step, code: Some(code) } => {
                write!(f, "{} stopped with exit code {code}", step.title().to_lowercase())
            }
            Self::Failed { step, code: None } => {
                write!(f, "{} was interrupted", step.title().to_lowercase())
            }
        }
    }
}

impl std::error::Error for SetupError {}

pub(crate) enum SetupPoll {
    Running,
    Finished(Result<SetupStep, SetupError>),
}

/// A running setup step. Output lines arrive from reader threads that end when the child closes
/// its pipes.
pub(crate) struct SetupRun {
    step: SetupStep,
    child: Child,
    lines: Arc<Mutex<Vec<String>>>,
}

impl SetupRun {
    pub(crate) fn start(launcher: &Path, step: SetupStep) -> Result<Self, SetupError> {
        let mut child = Command::new(launcher)
            .args(step.args())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(SetupError::Spawn)?;
        let lines = Arc::new(Mutex::new(Vec::new()));
        if let Some(out) = child.stdout.take() {
            pump(out, Arc::clone(&lines));
        }
        if let Some(err) = child.stderr.take() {
            pump(err, Arc::clone(&lines));
        }
        Ok(Self { step, child, lines })
    }

    pub(crate) fn step(&self) -> SetupStep {
        self.step
    }

    pub(crate) fn poll(&mut self) -> SetupPoll {
        match self.child.try_wait() {
            Ok(None) => SetupPoll::Running,
            Ok(Some(status)) if status.success() => SetupPoll::Finished(Ok(self.step)),
            Ok(Some(status)) => SetupPoll::Finished(Err(SetupError::Failed {
                step: self.step,
                code: status.code(),
            })),
            Err(error) => SetupPoll::Finished(Err(SetupError::Wait(error))),
        }
    }

    /// Lends the newest `count` output lines to `show` without copying them.
    pub(crate) fn with_tail<R>(&self, count: usize, show: impl FnOnce(&[String]) -> R) -> R {
        let lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        let start = lines.len().saturating_sub(count);
        show(&lines[start..])
    }

    pub(crate) fn cancel(&mut self) {
        let _ = self.child.kill();
    }
}

fn pump(mut source: impl Read + Send + 'static, lines: Arc<Mutex<Vec<String>>>) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while let Ok(read) = source.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&buffer[..read]);
            let mut log = lines.lock().unwrap_or_else(PoisonError::into_inner);
            for line in split_progress(&text) {
                log.push(line.to_owned());
            }
            let excess = log.len().saturating_sub(MAX_LOG_LINES);
            log.drain(..excess);
        }
    });
}

/// Splits a chunk of terminal output into lines. A carriage return redraws the line, so only
/// the last redraw of each line is kept.
fn split_progress(chunk: &str) -> impl Iterator<Item = &str> {
    chunk
        .split('\n')
        .filter_map(|line| line.rsplit('\r').find(|part| !part.trim().is_empty()))
        .map(str::trim_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_last_redraw_of_a_progress_line() {
        let lines: Vec<_> = split_progress("Downloading 10%\rDownloading 55%\rDownloading 100%\nDone\n").collect();
        assert_eq!(lines, ["Downloading 100%", "Done"]);
    }

    #[test]
    fn drops_blank_lines() {
        assert_eq!(split_progress("\n  \n\r\n").count(), 0);
    }

    #[test]
    fn failure_messages_name_the_step() {
        let error = SetupError::Failed { step: SetupStep::Install, code: Some(2) };
        assert_eq!(error.to_string(), "installing horizon stopped with exit code 2");
    }
}
