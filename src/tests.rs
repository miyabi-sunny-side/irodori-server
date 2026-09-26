use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    response::Response,
};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{AppState, Config, app};

struct Server {
    dir: tempfile::TempDir,
    state: AppState,
}

impl Server {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let state = open(dir.path());
        Self { dir, state }
    }

    /// Opens a second server over the same data directory, like a restart.
    fn restart(&self) -> AppState {
        open(self.dir.path())
    }

    async fn call(&self, method: &str, uri: &str, body: Option<Value>) -> Response {
        call(&self.state, method, uri, body).await
    }
}

fn open(dir: &std::path::Path) -> AppState {
    AppState::open(Config {
        data_dir: dir.join("data"),
        legacy_dictionary: dir.join("reading_dictionary.json"),
        worker: crate::worker::stub(),
    })
    .unwrap()
}

async fn call(state: &AppState, method: &str, uri: &str, body: Option<Value>) -> Response {
    let mut request = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    app(state.clone())
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap()
}

async fn json_body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

async fn get(uri: &str) -> Response {
    Server::new().call("GET", uri, None).await
}

#[tokio::test]
async fn generation_is_recorded_with_its_file_and_survives_a_restart() {
    let server = Server::new();
    server
        .call(
            "PUT",
            "/api/dictionary",
            Some(json!({"word": "Irodori", "reading": "いろどり"})),
        )
        .await;

    let response = server
        .call("POST", "/api/generate", Some(json!({"text": "Irodoriです。", "caption": "低い声", "num_candidates": 2, "seed": "42"})))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let generations = body["generations"].as_array().unwrap();
    assert_eq!(generations.len(), 2);
    assert_eq!(generations[0]["text"], "Irodoriです。");
    assert_eq!(generations[0]["text_applied"], "いろどりです。");
    assert_eq!(generations[0]["seed"], "42");
    assert!(
        body["log"]
            .as_str()
            .unwrap()
            .contains("\"text\": \"いろどりです。\""),
        "worker receives the applied text"
    );
    assert!(body["log"].as_str().unwrap().contains("話速: 1倍"));

    let restarted = server.restart();
    let listed = json_body(call(&restarted, "GET", "/api/generations", None).await).await;
    assert_eq!(listed["generations"].as_array().unwrap().len(), 2);
    let url = listed["generations"][0]["audio_url"]
        .as_str()
        .unwrap()
        .to_owned();
    let audio = call(&restarted, "GET", &format!("{url}?download=1"), None).await;
    assert_eq!(audio.status(), StatusCode::OK);
    assert!(
        audio.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment")
    );
    assert!(
        to_bytes(audio.into_body(), usize::MAX)
            .await
            .unwrap()
            .starts_with(b"RIFF")
    );
    assert!(
        std::fs::read_dir(server.dir.path().join("data/tmp"))
            .unwrap()
            .next()
            .is_none(),
        "scratch files are removed"
    );
}

#[tokio::test]
async fn deleting_a_generation_removes_the_record_and_the_file() {
    let server = Server::new();
    let body = json_body(
        server
            .call("POST", "/api/generate", Some(json!({"text": "消します。"})))
            .await,
    )
    .await;
    let id = body["generations"][0]["id"].as_i64().unwrap();
    let files = || {
        std::fs::read_dir(server.dir.path().join("data/audio"))
            .unwrap()
            .count()
    };
    assert_eq!(files(), 1);

    let response = server
        .call("DELETE", &format!("/api/generations/{id}"), None)
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(files(), 0);
    assert_eq!(
        server
            .call("GET", &format!("/api/generations/{id}"), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let listed = json_body(server.call("GET", "/api/generations", None).await).await;
    assert_eq!(listed, json!({"generations": [], "file_errors": []}));
}

#[tokio::test]
async fn invalid_requests_are_rejected_before_the_worker_runs() {
    let server = Server::new();
    for body in [
        json!({"text": " "}),
        json!({"text": "a", "mode": "clone"}),
        json!({"text": "a", "speed": 2.0}),
        json!({"text": "a", "mode": "clone", "reference_ids": [99]}),
    ] {
        let response = server
            .call("POST", "/api/generate", Some(body.clone()))
            .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{body}");
        assert!(json_body(response).await["error"].is_string());
    }
    assert_eq!(
        std::fs::read_dir(server.dir.path().join("data/audio"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn a_worker_crash_is_reported_and_the_next_generation_recovers() {
    let server = Server::new();
    let response = server
        .call("POST", "/api/generate", Some(json!({"text": "落ちる"})))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let response = server
        .call("POST", "/api/generate", Some(json!({"text": "戻る"})))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn uploaded_references_reach_the_worker_in_order() {
    let server = Server::new();
    let mut ids = Vec::new();
    for name in ["b.m4a", "a.WAV"] {
        let boundary = "XBOUNDARY";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: audio/mp4\r\n\r\nAUDIO\r\n--{boundary}--\r\n"
        );
        let request = Request::post("/api/references")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(Body::from(body))
            .unwrap();
        let response = app(server.state.clone()).oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let reply = json_body(response).await;
        assert_eq!(reply["name"], name);
        ids.push(reply["id"].as_i64().unwrap());
    }
    let body = json_body(
        server
            .call(
                "POST",
                "/api/generate",
                Some(json!({"text": "似せます。", "mode": "clone", "reference_ids": ids})),
            )
            .await,
    )
    .await;
    let log = body["log"].as_str().unwrap();
    let first = log.find(".m4a").unwrap();
    let second = log.find(".wav").unwrap();
    assert!(first < second, "reference order is kept: {log}");
    assert_eq!(body["generations"][0]["reference_ids"], json!(ids));
    let audio = server
        .call("GET", &format!("/api/references/{}/audio", ids[0]), None)
        .await;
    assert_eq!(
        to_bytes(audio.into_body(), usize::MAX).await.unwrap(),
        "AUDIO"
    );
}

#[tokio::test]
async fn references_with_unknown_extensions_are_rejected() {
    let server = Server::new();
    let body = "--X\r\nContent-Disposition: form-data; name=\"file\"; filename=\"../../evil.sh\"\r\n\r\necho\r\n--X--\r\n";
    let request = Request::post("/api/references")
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=X")
        .body(Body::from(body))
        .unwrap();
    let response = app(server.state.clone()).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        std::fs::read_dir(server.dir.path().join("data/references"))
            .unwrap()
            .count(),
        0
    );
}

#[tokio::test]
async fn dictionary_editing_and_preview() {
    let server = Server::new();
    let reply = json_body(
        server
            .call(
                "PUT",
                "/api/dictionary",
                Some(json!({"word": " TTS ", "reading": "てぃーてぃーえす"})),
            )
            .await,
    )
    .await;
    assert_eq!(reply["message"], "読み方を登録しました。");
    let reply = json_body(
        server
            .call(
                "PUT",
                "/api/dictionary",
                Some(json!({"word": "TTS", "reading": "ティーティーエス"})),
            )
            .await,
    )
    .await;
    assert_eq!(reply["message"], "読み方を更新しました。");
    assert_eq!(
        reply["entries"],
        json!([{"word": "TTS", "reading": "ティーティーエス"}])
    );
    let preview = json_body(
        server
            .call(
                "POST",
                "/api/dictionary/preview",
                Some(json!({"text": "TTSの話", "enabled": true})),
            )
            .await,
    )
    .await;
    assert_eq!(preview["text"], "ティーティーエスの話");
    let preview = json_body(
        server
            .call(
                "POST",
                "/api/dictionary/preview",
                Some(json!({"text": "TTSの話", "enabled": false})),
            )
            .await,
    )
    .await;
    assert_eq!(preview["text"], "TTSの話");
    assert_eq!(
        server
            .call(
                "PUT",
                "/api/dictionary",
                Some(json!({"word": "", "reading": "x"}))
            )
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let reply = json_body(server.call("DELETE", "/api/dictionary/TTS", None).await).await;
    assert_eq!(reply["entries"], json!([]));
    assert_eq!(
        server
            .call("DELETE", "/api/dictionary/TTS", None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn info_lists_models_and_grouped_emojis_from_the_worker() {
    let info = json_body(get("/api/info").await).await;
    assert_eq!(
        info["models"][1]["id"],
        "phasefield-audio/Irodori-TTS-v4.1-Anime"
    );
    assert_eq!(info["devices"], json!(["cpu"]));
    assert_eq!(info["max_candidates"], 4);
    assert_eq!(info["emoji_groups"][0]["title"], "ポジティブ");
}

#[tokio::test]
async fn unload_asks_the_worker() {
    let response = get("/api/unload").await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    let server = Server::new();
    let reply = json_body(server.call("POST", "/api/unload", None).await).await;
    assert!(
        reply["message"]
            .as_str()
            .unwrap()
            .starts_with("モデルをメモリから解放しました")
    );
}

#[tokio::test]
async fn ui_is_available_without_a_static_directory() {
    let response = get("/").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.starts_with(b"<!doctype html>"));
    assert!(std::str::from_utf8(&body).unwrap().contains("/assets/"));
}

#[tokio::test]
async fn compiled_assets_are_served_with_their_content_types() {
    let response = get("/").await;
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let html = std::str::from_utf8(&body).unwrap();
    for (attribute, extension, content_type) in [
        ("src=\"", ".js", "text/javascript"),
        ("href=\"", ".css", "text/css"),
    ] {
        let asset = html
            .split(attribute)
            .skip(1)
            .filter_map(|part| part.split('"').next())
            .find(|path| path.ends_with(extension))
            .expect("compiled HTML references its asset");
        let response = get(asset).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], content_type);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(!body.is_empty());
        assert!(!body.starts_with(b"<!doctype html>"));
    }
}

#[tokio::test]
async fn ui_rejects_mutating_requests() {
    let response = Server::new().call("POST", "/projects/example", None).await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn liveness_and_health() {
    assert_eq!(
        to_bytes(get("/healthz").await.into_body(), usize::MAX)
            .await
            .unwrap(),
        "ok\n"
    );
    assert_eq!(
        to_bytes(get("/api/health").await.into_body(), usize::MAX)
            .await
            .unwrap(),
        r#"{"status":"ok"}"#
    );
}

#[tokio::test]
async fn unknown_api_routes_do_not_fall_back_to_the_spa() {
    for uri in ["/api", "/api/", "/api/missing"] {
        assert_eq!(get(uri).await.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn unknown_client_routes_return_the_spa_with_success() {
    let response = get("/history").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/html");
}
