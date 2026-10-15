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
    open_with(dir, crate::engine::stub())
}

fn open_with(dir: &std::path::Path, engine: crate::engine::Engine) -> AppState {
    AppState::open(Config {
        data_dir: dir.join("data"),
        legacy_dictionary: dir.join("reading_dictionary.json"),
        engine,
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
        "engine receives the applied text"
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
async fn speech_returns_one_recorded_wav() {
    let server = Server::new();
    let response = server
        .call(
            "POST",
            "/api/speech",
            Some(json!({"text": "動画用です。", "num_candidates": 3})),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "audio/wav");
    let id = response.headers()["x-generation-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.headers()["x-seed"], "42");
    let wav = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(wav.starts_with(b"RIFF"));

    let listed = json_body(server.call("GET", "/api/generations", None).await).await;
    assert_eq!(
        listed["generations"].as_array().unwrap().len(),
        1,
        "one candidate only"
    );
    assert_eq!(listed["generations"][0]["id"].to_string(), id);
    let stored = server
        .call("GET", &format!("/api/generations/{id}/audio"), None)
        .await;
    assert_eq!(to_bytes(stored.into_body(), usize::MAX).await.unwrap(), wav);

    let invalid = server
        .call("POST", "/api/speech", Some(json!({"text": ""})))
        .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
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

/// Uploads `bytes` as a reference and returns its id.
async fn upload(server: &Server, name: &str, bytes: &[u8]) -> Response {
    let boundary = "XBOUNDARY";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\r\n"
    )
    .into_bytes();
    body.extend(bytes);
    body.extend(format!("\r\n--{boundary}--\r\n").as_bytes());
    let request = Request::post("/api/references")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    app(server.state.clone()).oneshot(request).await.unwrap()
}

/// A silent 16-bit mono WAV of the given length.
fn wav(millis: u32) -> Vec<u8> {
    let rate = 8000u32;
    let frames = millis * rate / 1000;
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + frames * 2).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((frames * 2).to_le_bytes());
    out.resize(out.len() + frames as usize * 2, 0);
    out
}

const MF: &str = "Aratako/Irodori-TTS-v4.1-Small-MF";

async fn add_voice(server: &Server, body: Value) -> Response {
    server.call("POST", "/api/voices", Some(body)).await
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn a_voice_from_the_history_fixes_model_style_and_reference() {
    let server = Server::new();
    let body = json_body(
        server
            .call(
                "POST",
                "/api/generate",
                Some(json!({"text": "候補です。", "caption": "明るい声", "num_candidates": 3})),
            )
            .await,
    )
    .await;
    let ids: Vec<i64> = body["generations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["id"].as_i64().unwrap())
        .collect();

    for bad in [
        json!({"name": " ", "model": MF, "generation_id": ids[1]}),
        json!({"name": "a", "model": "nobody/model", "generation_id": ids[1]}),
        json!({"name": "a", "model": MF}),
        json!({"name": "a", "model": MF, "generation_id": ids[1], "reference_id": 1}),
        json!({"name": "a", "model": MF, "generation_id": 9999}),
    ] {
        let response = add_voice(&server, bad.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{bad}");
    }
    let voice = json_body(
        add_voice(
            &server,
            json!({"name": " ずんだ ", "model": MF, "caption": "明るく", "generation_id": ids[1]}),
        )
        .await,
    )
    .await;
    assert_eq!(voice["name"], "ずんだ");
    assert_eq!(voice["source_generation_id"], ids[1]);
    let reference = server
        .call("GET", voice["audio_url"].as_str().unwrap(), None)
        .await;
    assert!(
        to_bytes(reference.into_body(), usize::MAX)
            .await
            .unwrap()
            .starts_with(b"RIFF")
    );

    let cleaned = json_body(server.call("POST", "/api/generations/cleanup", None).await).await;
    assert_eq!(cleaned["deleted"], 2, "the voice's source is kept");
    let history = json_body(server.call("GET", "/api/generations", None).await).await;
    assert_eq!(history["generations"][0]["saved"], true);

    // The voice keeps its own copy, so it speaks after its source is deleted.
    server
        .call("DELETE", &format!("/api/generations/{}", ids[1]), None)
        .await;
    let spoken = server
        .call(
            "POST",
            "/api/speech",
            Some(json!({"voice_id": voice["id"], "text": "固定した声です。", "caption": "無視される"})),
        )
        .await;
    assert_eq!(spoken.status(), StatusCode::OK);
    let id = spoken.headers()["x-generation-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let generation = json_body(
        server
            .call("GET", &format!("/api/generations/{id}"), None)
            .await,
    )
    .await;
    assert_eq!(generation["model"], MF);
    assert_eq!(generation["caption"], "明るく");
    assert_eq!(generation["mode"], "both");
    assert_eq!(generation["character"], "ずんだ");
    assert_eq!(generation["voice_id"], voice["id"]);
    let log = generation["log"].as_str().unwrap();
    assert!(
        log.contains("\"num_steps\": 4"),
        "MF runs its own steps: {log}"
    );
    assert!(log.contains("references: [[\"wav\", \"RIFF"), "{log}");

    let stale = server
        .call(
            "PATCH",
            &format!("/api/voices/{}", voice["id"]),
            Some(json!({"name": "あかり", "revision": 0})),
        )
        .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let renamed = json_body(
        server
            .call(
                "PATCH",
                &format!("/api/voices/{}", voice["id"]),
                Some(json!({"name": "あかり", "revision": voice["revision"]})),
            )
            .await,
    )
    .await;
    assert_eq!(
        (renamed["name"].as_str(), renamed["model"].as_str()),
        (Some("あかり"), Some(MF))
    );

    let deleted = server
        .call("DELETE", &format!("/api/voices/{}", voice["id"]), None)
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let restarted = server.restart();
    let voices = json_body(call(&restarted, "GET", "/api/voices", None).await).await;
    assert_eq!(voices["voices"], json!([]));
    let gone = call(
        &restarted,
        "POST",
        "/api/speech",
        Some(json!({"voice_id": voice["id"], "text": "a"})),
    )
    .await;
    assert_eq!(gone.status(), StatusCode::BAD_REQUEST);
    let played = call(
        &restarted,
        "GET",
        &format!("/api/generations/{id}/audio"),
        None,
    )
    .await;
    assert_eq!(
        played.status(),
        StatusCode::OK,
        "history audio outlives the voice"
    );
}

#[tokio::test]
async fn an_uploaded_reference_registers_only_when_one_to_120_seconds_long() {
    let server = Server::new();
    let mut ids = Vec::new();
    for (name, bytes) in [
        ("ok.wav", wav(1500)),
        ("short.wav", wav(500)),
        ("long.wav", wav(121_500)),
        ("broken.wav", b"not audio".to_vec()),
    ] {
        let response = upload(&server, name, &bytes).await;
        assert_eq!(response.status(), StatusCode::OK);
        ids.push(json_body(response).await["id"].clone());
    }
    let ok = add_voice(
        &server,
        json!({"name": "手元", "model": MF, "reference_id": ids[0]}),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
    for id in &ids[1..] {
        let refused = add_voice(
            &server,
            json!({"name": "x", "model": MF, "reference_id": id}),
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "{id}");
    }
    let voices = json_body(server.call("GET", "/api/voices", None).await).await;
    assert_eq!(voices["voices"].as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_dir(server.dir.path().join("data/voices"))
            .unwrap()
            .count(),
        1,
        "refused audio leaves no file"
    );
}

async fn script(server: &Server) -> Value {
    json_body(server.call("GET", "/api/script", None).await).await
}

async fn put_script(server: &Server, revision: &Value, cards: Value) -> Response {
    server
        .call(
            "PUT",
            "/api/script",
            Some(
                json!({"revision": revision, "script": {"cards": cards, "options": {"txt": true}}}),
            ),
        )
        .await
}

/// Waits for the script's generation run to finish.
async fn finished(server: &Server) -> Value {
    loop {
        let current = script(server).await;
        if current["job"]["finished"] == true {
            return current;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn voice_from_history(server: &Server) -> i64 {
    let body = json_body(
        server
            .call("POST", "/api/generate", Some(json!({"text": "声"})))
            .await,
    )
    .await;
    let voice = json_body(
        add_voice(
            server,
            json!({"name": "ずんだ", "model": MF, "generation_id": body["generations"][0]["id"]}),
        )
        .await,
    )
    .await;
    voice["id"].as_i64().unwrap()
}

fn statuses(script: &Value) -> Vec<&str> {
    script["cards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["status"].as_str().unwrap())
        .collect()
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn a_script_generates_its_cards_and_exports_them_in_order() {
    let server = Server::new();
    let first = script(&server).await;
    assert_eq!(first["revision"], 1);
    assert_eq!(statuses(&first), ["empty"]);
    let voice = voice_from_history(&server).await;
    let cards = json!([
        {"id": "a", "text": "一行目", "voice_id": voice},
        {"id": "b", "text": ""},
        {"id": "c", "text": "落ちる", "voice_id": voice},
        {"id": "d", "text": "四行目", "voice_id": voice, "speed": 1.25, "silence": 0.5},
        {"id": "e", "text": "話者なし"},
    ]);
    let invalid = put_script(
        &server,
        &first["revision"],
        json!([{"id": "a", "speed": 3}]),
    )
    .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let saved = json_body(put_script(&server, &first["revision"], cards.clone()).await).await;
    assert_eq!(saved["revision"], 2);
    assert_eq!(statuses(&saved), ["new", "empty", "new", "new", "no_voice"]);
    let stale = put_script(&server, &first["revision"], cards.clone()).await;
    assert_eq!(
        stale.status(),
        StatusCode::CONFLICT,
        "another screen saved first"
    );

    let missing_voice = server
        .call(
            "POST",
            "/api/script/generate",
            Some(json!({"revision": 2, "target": "changed"})),
        )
        .await;
    assert_eq!(missing_voice.status(), StatusCode::BAD_REQUEST);
    assert!(
        json_body(missing_voice).await["error"]
            .as_str()
            .unwrap()
            .starts_with("5番")
    );
    let mut cards = cards;
    cards[4]["text"] = "".into();
    let saved = json_body(put_script(&server, &saved["revision"], cards.clone()).await).await;
    server
        .call(
            "POST",
            "/api/script/generate",
            Some(json!({"revision": saved["revision"], "target": "changed"})),
        )
        .await;
    let done = finished(&server).await;
    assert_eq!(
        statuses(&done),
        ["done", "empty", "failed", "done", "empty"]
    );
    assert_eq!(done["job"]["failed"], 1);
    assert!(done["cards"][2]["error"].as_str().unwrap().contains("通信"));
    let id = done["cards"][3]["generation_id"].as_i64().unwrap();
    let generation = json_body(
        server
            .call("GET", &format!("/api/generations/{id}"), None)
            .await,
    )
    .await;
    assert_eq!(
        generation["character"], "ずんだ",
        "card audio is in the history"
    );
    assert!(
        generation["log"]
            .as_str()
            .unwrap()
            .contains("末尾の無音: 0.5秒")
    );
    let date: String = generation["created_at"]
        .as_str()
        .unwrap()
        .chars()
        .take(10)
        .filter(|c| *c != '-')
        .collect();
    assert_eq!(
        done["cards"][3]["download_name"],
        format!("004_ずんだ_{date}.wav")
    );

    let refused = server.call("GET", "/api/script/playlist", None).await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    assert!(
        json_body(refused).await["error"]
            .as_str()
            .unwrap()
            .contains("セリフ 3 ")
    );
    let from_d = json_body(
        server
            .call("GET", "/api/script/playlist?from=d", None)
            .await,
    )
    .await;
    assert_eq!(from_d["items"][0]["id"], "d");

    // Editing a card's text marks it changed; restoring the text needs no regeneration.
    cards[3]["text"] = "変えた".into();
    let edited = json_body(put_script(&server, &done["revision"], cards.clone()).await).await;
    assert_eq!(statuses(&edited)[3], "changed");
    cards[2]["text"] = "".into();
    cards[3]["text"] = "四行目".into();
    let restored = json_body(put_script(&server, &edited["revision"], cards.clone()).await).await;
    assert_eq!(
        statuses(&restored),
        ["done", "empty", "empty", "done", "empty"]
    );
    let playlist = json_body(server.call("GET", "/api/script/playlist", None).await).await;
    let order: Vec<&str> = playlist["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["a", "d"]);

    let joined = server
        .call("GET", "/api/script/export?kind=wav", None)
        .await;
    assert_eq!(joined.status(), StatusCode::OK);
    assert_eq!(joined.headers()[header::CONTENT_TYPE], "audio/wav");
    let joined = to_bytes(joined.into_body(), usize::MAX).await.unwrap();
    let seconds = crate::audio::wav_seconds(&joined).unwrap();
    assert!(
        (seconds - (1.2 + 1.2 / 1.25 + 0.5)).abs() < 0.05,
        "{seconds}"
    );

    let zipped = server
        .call("GET", "/api/script/export?kind=zip", None)
        .await;
    assert_eq!(zipped.headers()[header::CONTENT_TYPE], "application/zip");
    let zipped = to_bytes(zipped.into_body(), usize::MAX).await.unwrap();
    assert!(zipped.starts_with(b"PK"));
    let listing = String::from_utf8_lossy(&zipped);
    for name in [
        format!("001_ずんだ_{date}.wav"),
        format!("001_ずんだ_{date}.txt"),
        format!("004_ずんだ_{date}.wav"),
    ] {
        assert!(listing.contains(&name), "{name}");
    }
    let unknown = server
        .call("GET", "/api/script/export?kind=mp3", None)
        .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn stopping_a_script_run_finishes_the_running_card_only() {
    let server = Server::new();
    let voice = voice_from_history(&server).await;
    let current = script(&server).await;
    let cards = json!([
        {"id": "a", "text": "ゆっくり", "voice_id": voice},
        {"id": "b", "text": "二", "voice_id": voice},
        {"id": "c", "text": "三", "voice_id": voice},
    ]);
    let saved = json_body(put_script(&server, &current["revision"], cards).await).await;
    let started = json_body(
        server
            .call(
                "POST",
                "/api/script/generate",
                Some(json!({"revision": saved["revision"], "target": "all"})),
            )
            .await,
    )
    .await;
    assert_eq!(statuses(&started), ["queued", "queued", "queued"]);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(statuses(&script(&server).await)[0], "running");
    let busy = server
        .call(
            "POST",
            "/api/script/generate",
            Some(json!({"revision": saved["revision"], "target": "all"})),
        )
        .await;
    assert_eq!(busy.status(), StatusCode::CONFLICT);
    let stopping = json_body(server.call("POST", "/api/script/stop", None).await).await;
    assert_eq!(statuses(&stopping), ["running", "new", "new"]);
    let done = finished(&server).await;
    assert_eq!(statuses(&done), ["done", "new", "new"]);
    assert_eq!(done["job"]["done"], 1);

    // The browser may close: a single card regenerates on its own.
    server
        .call(
            "POST",
            "/api/script/generate",
            Some(json!({"revision": done["revision"], "target": "c"})),
        )
        .await;
    assert_eq!(statuses(&finished(&server).await), ["done", "new", "done"]);
}

#[tokio::test]
async fn favourites_are_kept_by_bulk_deletion() {
    let server = Server::new();
    let body = json_body(
        server
            .call(
                "POST",
                "/api/generate",
                Some(json!({"text": "星です。", "num_candidates": 2})),
            )
            .await,
    )
    .await;
    let first = body["generations"][0]["id"].as_i64().unwrap();
    let starred = json_body(
        server
            .call(
                "PUT",
                &format!("/api/generations/{first}/favorite"),
                Some(json!({"favorite": true})),
            )
            .await,
    )
    .await;
    assert_eq!(starred["favorite"], true);
    assert_eq!(
        server
            .call(
                "PUT",
                "/api/generations/9999/favorite",
                Some(json!({"favorite": true}))
            )
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    let cleaned = json_body(server.call("POST", "/api/generations/cleanup", None).await).await;
    assert_eq!(cleaned["deleted"], 1);
    let history = json_body(call(&server.restart(), "GET", "/api/generations", None).await).await;
    assert_eq!(history["generations"].as_array().unwrap().len(), 1);
    assert_eq!(history["generations"][0]["favorite"], true);
}

#[tokio::test]
async fn invalid_requests_are_rejected_before_the_engine_runs() {
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
async fn a_lost_engine_connection_is_reported_and_the_next_generation_succeeds() {
    let server = Server::new();
    let response = server
        .call("POST", "/api/generate", Some(json!({"text": "落ちる"})))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert!(json_body(response).await["error"].is_string());
    let response = server
        .call("POST", "/api/generate", Some(json!({"text": "戻る"})))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn an_engine_exception_returns_its_message_and_trace() {
    let server = Server::new();
    let response = server
        .call("POST", "/api/generate", Some(json!({"text": "失敗"})))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let body = json_body(response).await;
    assert!(body["error"].as_str().unwrap().starts_with("入力値を確認"));
    assert_eq!(body["log"], "Traceback: seed");
}

#[tokio::test]
async fn an_unreachable_engine_returns_the_unavailable_message() {
    let dir = tempfile::tempdir().unwrap();
    let state = open_with(dir.path(), crate::engine::Engine::new("http://127.0.0.1:9"));
    for (method, uri, body) in [
        ("GET", "/api/info", None),
        ("POST", "/api/generate", Some(json!({"text": "a"}))),
    ] {
        let response = call(&state, method, uri, body).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert!(
            json_body(response).await["error"]
                .as_str()
                .unwrap()
                .starts_with("推論プロセスを起動できません")
        );
    }
}

#[tokio::test]
async fn uploaded_references_reach_the_engine_in_order() {
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
    assert!(
        log.contains(r#"references: [["m4a", "AUDIO"], ["wav", "AUDIO"]]"#),
        "reference bodies arrive in order: {log}"
    );
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
async fn info_lists_models_and_grouped_emojis_from_the_engine() {
    let info = json_body(get("/api/info").await).await;
    assert_eq!(
        info["models"][1]["id"],
        "phasefield-audio/Irodori-TTS-v4.1-Anime"
    );
    assert_eq!(info["models"][2]["steps"], 4);
    assert_eq!(info["models"][2]["meanflow"], true);
    assert_eq!(info["devices"], json!(["cpu"]));
    assert_eq!(info["max_candidates"], 4);
    assert_eq!(info["emoji_groups"][0]["title"], "ポジティブ");
}

#[tokio::test]
async fn unload_asks_the_engine() {
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
