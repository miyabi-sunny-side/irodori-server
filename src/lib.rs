// Application crate: the library split exists for tests, not for external callers.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

pub mod db;
pub mod dictionary;
pub mod generation;
pub mod worker;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path as UrlPath, Query, Request, State},
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;
use tower_http::{services::ServeFile, trace::TraceLayer};

use crate::{
    db::{Db, Generation},
    generation::{GenerateRequest, MODELS, MODES, Runtime},
    worker::{Worker, WorkerError},
};

static UI: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/client/dist");

const MAX_REFERENCE_BYTES: usize = 50 * 1024 * 1024;
const REFERENCE_EXTENSIONS: &[&str] = &["wav", "mp3", "flac", "ogg", "opus", "m4a", "aac", "webm"];

/// Where the server keeps its files and how it starts the inference worker.
pub struct Config {
    pub data_dir: PathBuf,
    pub legacy_dictionary: PathBuf,
    pub worker: Worker,
}

impl Config {
    /// Layout of a checkout prepared by `irodori.sh setup`, relative to the working directory.
    pub fn from_checkout(root: &Path) -> Self {
        Self {
            data_dir: root.join("data"),
            legacy_dictionary: root.join("config/reading_dictionary.json"),
            worker: Worker::new(
                root.join(".venv/bin/python"),
                vec!["irodori_worker.py".into()],
                root.into(),
            ),
        }
    }
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    db: Mutex<Db>,
    data_dir: PathBuf,
    worker: Worker,
    info: tokio::sync::Mutex<Option<(Runtime, Vec<Value>)>>,
}

impl AppState {
    pub fn open(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        for dir in ["audio", "references", "tmp"] {
            std::fs::create_dir_all(config.data_dir.join(dir))?;
        }
        let db = Db::open(
            &config.data_dir.join("irodori.sqlite3"),
            &config.legacy_dictionary,
        )?;
        Ok(Self(Arc::new(Inner {
            db: Mutex::new(db),
            data_dir: config.data_dir,
            worker: config.worker,
            info: tokio::sync::Mutex::new(None),
        })))
    }

    /// Starts the inference worker and caches its device and emoji lists.
    pub async fn warm_up(&self) {
        if let Err(error) = self.runtime().await {
            tracing::warn!(error = error.message, "inference worker is not ready yet");
        }
    }

    async fn runtime(&self) -> Result<(Runtime, Vec<Value>), AppError> {
        let mut info = self.0.info.lock().await;
        if info.is_none() {
            let reply = self
                .0
                .worker
                .call(&json!({"op": "info"}))
                .await
                .map_err(|error| {
                    tracing::error!(?error, "inference worker info failed");
                    AppError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "推論プロセスを起動できません。サーバーのログを確認してください。",
                    )
                })?;
            let mut runtime: Runtime = serde_json::from_value(reply.clone()).map_err(|_| {
                AppError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "推論プロセスの応答を読めません。",
                )
            })?;
            generation::prefer_bf16(&mut runtime);
            let emojis = reply["emojis"].as_array().cloned().unwrap_or_default();
            *info = Some((runtime, generation::emoji_groups(&emojis)));
        }
        Ok(info.clone().expect("filled above"))
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Db> {
        self.0
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(api_health))
        .route("/info", get(info))
        .route("/generate", post(generate))
        .route("/speech", post(speech))
        .route("/generations", get(list_generations))
        .route(
            "/generations/{id}",
            get(get_generation).delete(delete_generation),
        )
        .route("/generations/{id}/audio", get(generation_audio))
        .route(
            "/references",
            post(upload_reference).layer(DefaultBodyLimit::max(MAX_REFERENCE_BYTES + 64 * 1024)),
        )
        .route("/references/{id}/audio", get(reference_audio))
        .route("/dictionary", get(get_dictionary).put(put_dictionary))
        .route("/dictionary/preview", post(preview_dictionary))
        .route("/dictionary/{word}", delete(delete_dictionary))
        .route("/unload", post(unload))
        .fallback(api_not_found)
        .with_state(state);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/api", axum::routing::any(api_not_found))
        .route("/api/", axum::routing::any(api_not_found))
        .nest("/api", api)
        .fallback_service(get(ui))
        .layer(TraceLayer::new_for_http())
}

pub struct AppError {
    status: StatusCode,
    message: String,
    log: Option<String>,
}

impl AppError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            log: None,
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, "request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "サーバーでエラーが発生しました。サーバーのログを確認してください。",
        )
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        Self::internal(error)
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::internal(error)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let mut body = json!({"error": self.message});
        if let Some(log) = self.log {
            body["log"] = log.into();
        }
        (self.status, Json(body)).into_response()
    }
}

type ApiResult<T> = Result<T, AppError>;

async fn info(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let (runtime, emoji_groups) = state.runtime().await?;
    let labelled = |items: &[(&str, &str)]| {
        items
            .iter()
            .map(|(id, label)| json!({"id": id, "label": label}))
            .collect::<Vec<_>>()
    };
    Ok(Json(json!({
        "models": labelled(MODELS),
        "modes": labelled(MODES),
        "devices": runtime.devices,
        "precisions": runtime.precisions,
        "max_candidates": runtime.max_candidates,
        "emoji_groups": emoji_groups,
    })))
}

/// Parses a generation request, taking omitted device and precision fields from the worker.
async fn parse_request(state: &AppState, body: Value) -> ApiResult<GenerateRequest> {
    let (runtime, _) = state.runtime().await?;
    serde_json::from_value(generation::with_runtime_defaults(body, &runtime)).map_err(|error| {
        AppError::new(
            StatusCode::BAD_REQUEST,
            format!("生成の指定を読めません: {error}"),
        )
    })
}

async fn generate(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let request = parse_request(&state, body).await?;
    // Run detached so a closed browser tab does not abandon a half-recorded generation.
    tokio::spawn(run_generation(state, request))
        .await
        .map_err(AppError::internal)?
        .map(|(generations, log)| Json(json!({"generations": generations, "log": log})))
}

/// One WAV for scripts such as video production: a single candidate, recorded like any other.
async fn speech(State(state): State<AppState>, Json(body): Json<Value>) -> ApiResult<Response> {
    let mut request = parse_request(&state, body).await?;
    request.num_candidates = 1;
    let (generations, _) = tokio::spawn(run_generation(state.clone(), request))
        .await
        .map_err(AppError::internal)??;
    let generation = generations
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("worker returned no audio"))?;
    let audio = tokio::fs::read(state.0.data_dir.join(&generation.path)).await?;
    let mut response = ([(header::CONTENT_TYPE, "audio/wav")], audio).into_response();
    let headers = response.headers_mut();
    headers.insert("x-generation-id", generation.id.into());
    if let Some(seed) = generation
        .seed
        .as_deref()
        .and_then(|seed| seed.parse().ok())
    {
        headers.insert("x-seed", seed);
    }
    Ok(response)
}

async fn run_generation(
    state: AppState,
    request: GenerateRequest,
) -> ApiResult<(Vec<Generation>, String)> {
    let (runtime, _) = state.runtime().await?;
    request
        .validate(&runtime)
        .map_err(|message| AppError::new(StatusCode::BAD_REQUEST, message))?;
    let data = state.0.data_dir.clone();
    let (text_applied, references) = {
        let db = state.db();
        let text = if request.dictionary_enabled {
            dictionary::apply(&request.text, &db.dictionary()?)
        } else {
            request.text.clone()
        };
        let mut references = Vec::new();
        if request.uses_references() {
            for id in &request.reference_ids {
                let path = db.reference_path(*id)?.ok_or_else(|| {
                    AppError::new(
                        StatusCode::BAD_REQUEST,
                        "お手本の音声が見つかりません。もう一度追加してください。",
                    )
                })?;
                references.push(data.join(path).display().to_string());
            }
        }
        (text, references)
    };

    let batch = format!(
        "{}-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        &uuid::Uuid::new_v4().simple().to_string()[..6]
    );
    let scratch = data.join("tmp").join(&batch);
    tokio::fs::create_dir_all(&scratch).await?;
    let result = synthesize(
        &state,
        &request,
        &text_applied,
        &references,
        &batch,
        &scratch,
    )
    .await;
    let _ = tokio::fs::remove_dir_all(&scratch).await;
    result
}

async fn synthesize(
    state: &AppState,
    request: &GenerateRequest,
    text_applied: &str,
    references: &[String],
    batch: &str,
    scratch: &Path,
) -> ApiResult<(Vec<Generation>, String)> {
    let reply = state
        .0
        .worker
        .call(&json!({
            "op": "generate",
            "params": request.worker_params(text_applied, references),
            "out_dir": scratch,
        }))
        .await
        .map_err(worker_error)?;
    let produced: Vec<PathBuf> = reply["paths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect();
    let log = format!(
        "{}\n話速: {}倍",
        reply["log"].as_str().unwrap_or_default(),
        request.speed
    );

    // Finish every WAV under a temporary name, move it to its final name, then record it.
    let mut rows = Vec::new();
    let mut finals = Vec::new();
    for (index, source) in produced.iter().enumerate() {
        let candidate = index + 1;
        let relative = format!("audio/{batch}_{candidate:02}.wav");
        let target = state.0.data_dir.join(&relative);
        let moved = async {
            if (request.speed - 1.0).abs() > f64::EPSILON {
                let partial = scratch.join(format!("speed_{candidate:02}.partial.wav"));
                change_speed(source, &partial, request.speed).await?;
                tokio::fs::rename(&partial, &target).await
            } else {
                tokio::fs::rename(source, &target).await
            }
        }
        .await;
        if let Err(error) = moved {
            remove_all(&finals).await;
            return Err(AppError::internal(error));
        }
        finals.push(target);
        rows.push(Generation {
            id: 0,
            batch: batch.into(),
            candidate: i64::try_from(candidate).unwrap_or(i64::MAX),
            created_at: db::now(),
            text: request.text.clone(),
            text_applied: text_applied.into(),
            mode: request.mode.clone(),
            caption: request.caption.clone(),
            reference_ids: if request.uses_references() {
                request.reference_ids.clone()
            } else {
                Vec::new()
            },
            model: request.model.clone(),
            seed: reply["seed"].as_str().map(str::to_owned),
            speed: request.speed,
            params: serde_json::to_value(request).unwrap_or(Value::Null),
            log: log.clone(),
            path: relative,
            audio_url: String::new(),
        });
    }
    let saved = state.db().insert_generations(&rows);
    match saved {
        Ok(saved) => Ok((saved, log)),
        Err(error) => {
            remove_all(&finals).await;
            Err(error.into())
        }
    }
}

async fn remove_all(paths: &[PathBuf]) {
    for path in paths {
        let _ = tokio::fs::remove_file(path).await;
    }
}

/// Pitch-preserving tempo change; the output is written to `target` only.
async fn change_speed(source: &Path, target: &Path, speed: f64) -> std::io::Result<()> {
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-n", "-i"])
        .arg(source)
        .args([
            "-map",
            "0:a:0",
            "-af",
            &format!("atempo={speed:.4}"),
            "-c:a",
            "pcm_s24le",
        ])
        .arg(target)
        .output()
        .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

fn worker_error(error: WorkerError) -> AppError {
    match error {
        WorkerError::Failed {
            error_type,
            error,
            trace,
        } => AppError {
            status: StatusCode::BAD_GATEWAY,
            message: generation::error_message(&error_type, &error).into(),
            log: Some(trace),
        },
        WorkerError::Died => AppError::new(
            StatusCode::BAD_GATEWAY,
            "推論プロセスが終了しました。次の生成で自動的に起動し直します。もう一度お試しください。",
        ),
        WorkerError::Unavailable(detail) => {
            tracing::error!(detail, "inference worker unavailable");
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "推論プロセスを起動できません。サーバーのログを確認してください。",
            )
        }
    }
}

async fn list_generations(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let db = state.db();
    Ok(Json(
        json!({"generations": db.generations()?, "file_errors": db.file_errors()?}),
    ))
}

fn find_generation(state: &AppState, id: i64) -> ApiResult<Generation> {
    state
        .db()
        .generation(id)?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "生成の記録が見つかりません。"))
}

async fn get_generation(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Generation>> {
    find_generation(&state, id).map(Json)
}

#[derive(Deserialize)]
struct AudioQuery {
    download: Option<String>,
}

async fn generation_audio(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
    Query(query): Query<AudioQuery>,
    request: Request,
) -> ApiResult<Response> {
    let generation = find_generation(&state, id)?;
    let mut response = serve(&state.0.data_dir.join(&generation.path), request).await?;
    if query.download.is_some() {
        let disposition = format!("attachment; filename=\"irodori-{}.wav\"", generation.id);
        response.headers_mut().insert(
            header::CONTENT_DISPOSITION,
            disposition.parse().expect("ascii header"),
        );
    }
    Ok(response)
}

async fn serve(path: &Path, request: Request) -> ApiResult<Response> {
    if !path.is_file() {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            "音声ファイルが見つかりません。",
        ));
    }
    Ok(ServeFile::new(path)
        .oneshot(request)
        .await
        .map_err(AppError::internal)?
        .map(Body::new))
}

async fn delete_generation(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<StatusCode> {
    let path = state
        .db()
        .delete_generation(id)?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "生成の記録が見つかりません。"))?;
    match tokio::fs::remove_file(state.0.data_dir.join(&path)).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => state.db().add_file_error(&path, &error.to_string())?,
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_reference(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    let bad = |message: &str| AppError::new(StatusCode::BAD_REQUEST, message);
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad("音声ファイルを受け取れませんでした。"))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let name: String = field
            .file_name()
            .unwrap_or("reference")
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        let extension = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();
        if !REFERENCE_EXTENSIONS.contains(&extension.as_str()) {
            return Err(bad(
                "対応していない形式です。wav・mp3・flac・ogg・opus・m4a・aac・webmの音声を選んでください。",
            ));
        }
        let bytes = field
            .bytes()
            .await
            .map_err(|_| bad("音声ファイルは50MBまでです。"))?;
        if bytes.is_empty() || bytes.len() > MAX_REFERENCE_BYTES {
            return Err(bad("音声ファイルは50MBまでです。"));
        }
        let relative = format!("references/{}.{extension}", uuid::Uuid::new_v4().simple());
        let target = state.0.data_dir.join(&relative);
        let partial = state
            .0
            .data_dir
            .join("tmp")
            .join(format!("{}.partial", uuid::Uuid::new_v4().simple()));
        tokio::fs::write(&partial, &bytes).await?;
        tokio::fs::rename(&partial, &target).await?;
        let id = state.db().add_reference(&name, &relative);
        return match id {
            Ok(id) => Ok(Json(json!({"id": id, "name": name}))),
            Err(error) => {
                let _ = tokio::fs::remove_file(&target).await;
                Err(error.into())
            }
        };
    }
    Err(bad("音声ファイルを選んでください。"))
}

async fn reference_audio(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
    request: Request,
) -> ApiResult<Response> {
    let path = state
        .db()
        .reference_path(id)?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "お手本の音声が見つかりません。"))?;
    serve(&state.0.data_dir.join(path), request).await
}

fn dictionary_json(state: &AppState) -> ApiResult<Value> {
    let entries = state.db().dictionary()?;
    Ok(json!(
        entries
            .into_iter()
            .map(|(word, reading)| json!({"word": word, "reading": reading}))
            .collect::<Vec<_>>()
    ))
}

async fn get_dictionary(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"entries": dictionary_json(&state)?})))
}

#[derive(Deserialize)]
struct WordInput {
    #[serde(default)]
    word: String,
    #[serde(default)]
    reading: String,
}

async fn put_dictionary(
    State(state): State<AppState>,
    Json(input): Json<WordInput>,
) -> ApiResult<Json<Value>> {
    let (word, reading) = dictionary::validate(&input.word, &input.reading)
        .map_err(|message| AppError::new(StatusCode::BAD_REQUEST, message))?;
    let updated = state.db().put_word(&word, &reading)?;
    let message = if updated {
        "読み方を更新しました。"
    } else {
        "読み方を登録しました。"
    };
    Ok(Json(
        json!({"message": message, "entries": dictionary_json(&state)?}),
    ))
}

async fn delete_dictionary(
    State(state): State<AppState>,
    UrlPath(word): UrlPath<String>,
) -> ApiResult<Json<Value>> {
    if !state.db().delete_word(&word)? {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            "削除する登録を一覧から選んでください。",
        ));
    }
    Ok(Json(
        json!({"message": "選んだ登録を削除しました。", "entries": dictionary_json(&state)?}),
    ))
}

#[derive(Deserialize)]
struct PreviewInput {
    #[serde(default)]
    text: String,
    #[serde(default = "enabled")]
    enabled: bool,
}

fn enabled() -> bool {
    true
}

async fn preview_dictionary(
    State(state): State<AppState>,
    Json(input): Json<PreviewInput>,
) -> ApiResult<Json<Value>> {
    let text = if input.enabled {
        dictionary::apply(&input.text, &state.db().dictionary()?)
    } else {
        input.text
    };
    Ok(Json(json!({"text": text})))
}

async fn unload(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    state
        .0
        .worker
        .call(&json!({"op": "unload"}))
        .await
        .map_err(worker_error)?;
    Ok(Json(
        json!({"message": "モデルをメモリから解放しました。次回生成時に再読み込みします。"}),
    ))
}

async fn ui(uri: Uri) -> Response {
    let file = UI
        .get_file(uri.path().trim_start_matches('/'))
        .unwrap_or_else(|| {
            UI.get_file("index.html")
                .expect("build requires index.html")
        });
    let content_type = mime_guess::from_path(file.path()).first_or_octet_stream();
    (
        [(header::CONTENT_TYPE, content_type.as_ref())],
        file.contents(),
    )
        .into_response()
}

async fn healthz() -> &'static str {
    "ok\n"
}

async fn api_health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn api_not_found() -> impl IntoResponse {
    AppError::new(StatusCode::NOT_FOUND, "API route not found")
}

#[cfg(test)]
mod tests;
