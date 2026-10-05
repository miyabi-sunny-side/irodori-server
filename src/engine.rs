//! Client of the inference engine (`irodori_engine.py`), an HTTP server that may run on another
//! machine. The engine serves one request at a time; generation can take minutes, so calls have
//! no overall timeout. The engine's output during each request is written to this server's log.

use std::time::Duration;

use serde_json::Value;

pub const DEFAULT_URL: &str = "http://127.0.0.1:7861";

pub struct Engine {
    url: String,
    client: reqwest::Client,
}

#[derive(Debug)]
pub enum EngineError {
    /// The engine could not be reached.
    Unavailable(String),
    /// The connection broke or the reply could not be read.
    Died(String),
    /// The engine reported an exception.
    Failed {
        error_type: String,
        error: String,
        trace: String,
    },
}

impl Engine {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_owned(),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }

    /// The engine at `IRODORI_ENGINE_URL`, or on this machine.
    pub fn from_env() -> Self {
        Self::new(&std::env::var("IRODORI_ENGINE_URL").unwrap_or_else(|_| DEFAULT_URL.into()))
    }

    /// Posts one request to `/<op>` and waits for its reply.
    pub async fn call(&self, op: &str, request: &Value) -> Result<Value, EngineError> {
        let url = format!("{}/{op}", self.url);
        let response = self
            .client
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(request.to_string())
            .send()
            .await
            .map_err(|error| {
                if error.is_connect() {
                    EngineError::Unavailable(format!("{url}: {error}"))
                } else {
                    EngineError::Died(format!("{url}: {error}"))
                }
            })?;
        let body = response
            .bytes()
            .await
            .map_err(|error| EngineError::Died(format!("{url}: {error}")))?;
        let reply: Value = serde_json::from_slice(&body)
            .map_err(|error| EngineError::Died(format!("{url}: unreadable reply: {error}")))?;
        if let Some(log) = reply["engine_log"].as_str().filter(|log| !log.is_empty()) {
            tracing::info!(op, "engine output:\n{log}");
        }
        if reply["ok"] == Value::Bool(true) {
            return Ok(reply);
        }
        let text = |key: &str| reply[key].as_str().unwrap_or_default().to_owned();
        Err(EngineError::Failed {
            error_type: text("error_type"),
            error: text("error"),
            trace: text("trace"),
        })
    }
}

/// An engine backed by `tests/support/stub_engine.py`, shared by every test in the binary.
#[cfg(test)]
pub(crate) fn stub() -> Engine {
    use std::{
        io::BufRead,
        process::{Child, Command, Stdio},
        sync::OnceLock,
    };
    // The child keeps running until this process exits and closes its stdin.
    static STUB: OnceLock<(Child, String)> = OnceLock::new();
    let (_, url) = STUB.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut child = Command::new("python3")
            .arg(root.join("tests/support/stub_engine.py"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("python3 runs the stub engine");
        let mut port = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut port)
            .unwrap();
        (child, format!("http://127.0.0.1:{}", port.trim()))
    });
    Engine::new(url)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn replies_are_returned_and_failures_carry_the_exception() {
        let engine = stub();
        let info = engine.call("info", &json!({})).await.unwrap();
        assert_eq!(info["devices"], json!(["cpu"]));
        match engine.call("fail", &json!({})).await {
            Err(EngineError::Failed {
                error_type,
                error,
                trace,
            }) => assert_eq!(
                (error_type.as_str(), error.as_str(), trace.as_str()),
                ("ValueError", "bad", "Traceback: bad")
            ),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_connection_closed_without_a_reply_is_reported() {
        let engine = stub();
        assert!(matches!(
            engine
                .call("generate", &json!({"params": {"text": "落ちる"}}))
                .await,
            Err(EngineError::Died(_))
        ));
        assert!(engine.call("info", &json!({})).await.is_ok());
    }

    #[tokio::test]
    async fn an_unreachable_engine_is_unavailable() {
        let engine = Engine::new("http://127.0.0.1:9/");
        assert!(matches!(
            engine.call("info", &json!({})).await,
            Err(EngineError::Unavailable(_))
        ));
    }
}
