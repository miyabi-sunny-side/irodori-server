//! The Python inference process: owned by the server, one JSON line per request and reply.
//! A dead process is replaced on the next request.

use std::{path::PathBuf, process::Stdio};

use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

pub struct Worker {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    process: Mutex<Option<Process>>,
}

struct Process {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Debug)]
pub enum WorkerError {
    /// The process could not be started.
    Unavailable(String),
    /// The process exited before replying.
    Died,
    /// The worker reported an exception.
    Failed {
        error_type: String,
        error: String,
        trace: String,
    },
}

impl Worker {
    pub fn new(program: PathBuf, args: Vec<String>, cwd: PathBuf) -> Self {
        Self {
            program,
            args,
            cwd,
            process: Mutex::new(None),
        }
    }

    /// Sends one request and waits for its reply. Requests are served one at a time.
    pub async fn call(&self, request: &Value) -> Result<Value, WorkerError> {
        let mut guard = self.process.lock().await;
        let alive = match guard.as_mut() {
            Some(process) => matches!(process.child.try_wait(), Ok(None)),
            None => false,
        };
        if !alive {
            *guard = Some(self.spawn()?);
        }
        let process = guard.as_mut().expect("spawned above");
        let mut line = request.to_string();
        line.push('\n');
        let mut reply = String::new();
        let exchanged = async {
            process.stdin.write_all(line.as_bytes()).await?;
            process.stdin.flush().await?;
            process.stdout.read_line(&mut reply).await
        }
        .await;
        if !matches!(exchanged, Ok(n) if n > 0) {
            if let Some(mut process) = guard.take() {
                let _ = process.child.kill().await;
            }
            return Err(WorkerError::Died);
        }
        let reply: Value = serde_json::from_str(&reply).map_err(|_| WorkerError::Died)?;
        if reply["ok"] == Value::Bool(true) {
            return Ok(reply);
        }
        let text = |key: &str| reply[key].as_str().unwrap_or_default().to_owned();
        Err(WorkerError::Failed {
            error_type: text("error_type"),
            error: text("error"),
            trace: text("trace"),
        })
    }

    /// Process id of the current worker, if one is running.
    pub async fn pid(&self) -> Option<u32> {
        self.process
            .lock()
            .await
            .as_ref()
            .and_then(|process| process.child.id())
    }

    fn spawn(&self) -> Result<Process, WorkerError> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .current_dir(&self.cwd)
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONUTF8", "1")
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                WorkerError::Unavailable(format!("{}: {error}", self.program.display()))
            })?;
        tracing::info!(pid = child.id(), "inference worker started");
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(Process {
            child,
            stdin,
            stdout,
        })
    }
}

#[cfg(test)]
pub(crate) fn stub() -> Worker {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    Worker::new(
        "python3".into(),
        vec![
            root.join("tests/support/stub_worker.py")
                .display()
                .to_string(),
        ],
        root,
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn replies_are_returned_and_failures_carry_the_exception() {
        let worker = stub();
        let info = worker.call(&json!({"op": "info"})).await.unwrap();
        assert_eq!(info["devices"], json!(["cpu"]));
        match worker.call(&json!({"op": "fail"})).await {
            Err(WorkerError::Failed {
                error_type, error, ..
            }) => assert_eq!((error_type.as_str(), error.as_str()), ("ValueError", "bad")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_killed_process_is_replaced_on_the_next_request() {
        let worker = stub();
        worker.call(&json!({"op": "info"})).await.unwrap();
        let first = worker.pid().await.unwrap();
        std::process::Command::new("kill")
            .args(["-9", &first.to_string()])
            .status()
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        worker.call(&json!({"op": "info"})).await.unwrap();
        assert_ne!(worker.pid().await.unwrap(), first);
    }

    #[tokio::test]
    async fn a_process_dying_mid_request_reports_and_recovers() {
        let worker = stub();
        assert!(matches!(
            worker.call(&json!({"op": "die"})).await,
            Err(WorkerError::Died)
        ));
        assert!(worker.call(&json!({"op": "info"})).await.is_ok());
    }

    #[tokio::test]
    async fn a_missing_program_is_unavailable() {
        let worker = Worker::new("/nonexistent/python".into(), Vec::new(), ".".into());
        assert!(matches!(
            worker.call(&json!({"op": "info"})).await,
            Err(WorkerError::Unavailable(_))
        ));
    }
}
