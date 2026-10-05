// Application crate: the library split exists for tests, not for external callers.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

pub mod db;
pub mod dictionary;
pub mod engine;
pub mod generation;

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
    routing::{delete, get, post, put},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;
use tower_http::{services::ServeFile, trace::TraceLayer};

use crate::{
    db::{Db, Generation},
    engine::{Engine, EngineError},
    generation::{GenerateRequest, MODELS, MODES, Runtime},
};

static UI: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/client/dist");

const MAX_REFERENCE_BYTES: usize = 50 * 1024 * 1024;
const REFERENCE_EXTENSIONS: &[&str] = &["wav", "mp3", "flac", "ogg", "opus", "m4a", "aac", "webm"];

/// Where the server keeps its files and which inference engine it calls.
pub struct Config {
    pub data_dir: PathBuf,
    pub legacy_dictionary: PathBuf,
    pub engine: Engine,
}

impl Config {
    /// Files relative to the working directory; the engine from `IRODORI_ENGINE_URL`.
    pub fn from_checkout(root: &Path) -> Self {
        Self {
            data_dir: root.join("data"),
            legacy_dictionary: root.join("config/reading_dictionary.json"),
            engine: Engine::from_env(),
        }
    }
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    db: Mutex<Db>,
    data_dir: PathBuf,
    engine: Engine,
    info: tokio::sync::Mutex<Option<(Runtime, Vec<Value>)>>,
    /// Batch progress lives in memory; the generations themselves are recorded as usual.
    batches: Mutex<Vec<Batch>>,
}

#[derive(Clone, serde::Serialize)]
struct Batch {
    id: String,
    character: String,
    total: usize,
    done: usize,
    generation_ids: Vec<i64>,
    failed: Vec<Value>,
    finished: bool,
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
            engine: config.engine,
            info: tokio::sync::Mutex::new(None),
            batches: Mutex::new(Vec::new()),
        })))
    }

    /// Caches the engine's device and emoji lists if it is already up.
    pub async fn warm_up(&self) {
        if let Err(error) = self.runtime().await {
            tracing::warn!(error = error.message, "inference engine is not ready yet");
        }
    }

    async fn runtime(&self) -> Result<(Runtime, Vec<Value>), AppError> {
        let mut info = self.0.info.lock().await;
        if info.is_none() {
            let reply = self
                .0
                .engine
                .call("info", &json!({}))
                .await
                .map_err(|error| {
                    tracing::error!(?error, "inference engine info failed");
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

    fn batches(&self) -> std::sync::MutexGuard<'_, Vec<Batch>> {
        self.0
            .batches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        .route("/generations/{id}/reference", post(save_reference))
        .route("/generations/cleanup", post(cleanup_generations))
        .route("/generations/{id}/favorite", put(set_favorite))
        .route("/batches", get(list_batches).post(start_batch))
        .route("/batches/{id}", get(get_batch))
        .route("/references", get(list_references))
        .route(
            "/references",
            post(upload_reference).layer(DefaultBodyLimit::max(MAX_REFERENCE_BYTES + 64 * 1024)),
        )
        .route("/references/{id}", delete(delete_reference))
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

/// Parses a generation request, taking omitted device and precision fields from the engine.
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
        .ok_or_else(|| AppError::internal("engine returned no audio"))?;
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
    let (text_applied, references, character) = {
        let db = state.db();
        let text = if request.dictionary_enabled {
            dictionary::apply(&request.text, &db.dictionary()?)
        } else {
            request.text.clone()
        };
        let mut references = Vec::new();
        let mut characters = Vec::new();
        if request.uses_references() {
            for id in &request.reference_ids {
                let (path, character) = db.reference_entry(*id)?.ok_or_else(|| {
                    AppError::new(
                        StatusCode::BAD_REQUEST,
                        "お手本の音声が見つかりません。もう一度追加してください。",
                    )
                })?;
                references.push(data.join(path));
                characters.push(character);
            }
        }
        (text, references, generation::shared_character(&characters))
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
        character,
    )
    .await;
    let _ = tokio::fs::remove_dir_all(&scratch).await;
    result
}

async fn synthesize(
    state: &AppState,
    request: &GenerateRequest,
    text_applied: &str,
    references: &[PathBuf],
    batch: &str,
    scratch: &Path,
    character: Option<String>,
) -> ApiResult<(Vec<Generation>, String)> {
    let mut sent = Vec::new();
    for path in references {
        let format = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default();
        let data = BASE64.encode(tokio::fs::read(path).await?);
        sent.push(json!({"format": format, "data": data}));
    }
    let reply = state
        .0
        .engine
        .call(
            "generate",
            &json!({"params": request.engine_params(text_applied), "references": sent}),
        )
        .await
        .map_err(engine_error)?;
    let mut produced = Vec::new();
    for (index, wav) in reply["wavs"].as_array().into_iter().flatten().enumerate() {
        let audio = BASE64
            .decode(wav.as_str().unwrap_or_default())
            .map_err(AppError::internal)?;
        let path = scratch.join(format!("engine_{:02}.wav", index + 1));
        tokio::fs::write(&path, audio).await?;
        produced.push(path);
    }
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
            saved: false,
            character: character.clone(),
            favorite: false,
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

fn engine_error(error: EngineError) -> AppError {
    match error {
        EngineError::Failed {
            error_type,
            error,
            trace,
        } => AppError {
            status: StatusCode::BAD_GATEWAY,
            message: generation::error_message(&error_type, &error).into(),
            log: Some(trace),
        },
        EngineError::Died(detail) => {
            tracing::error!(detail, "inference engine stopped replying");
            AppError::new(
                StatusCode::BAD_GATEWAY,
                "推論プロセスとの通信が途中で切れました。サーバーのログを確認して、もう一度お試しください。",
            )
        }
        EngineError::Unavailable(detail) => {
            tracing::error!(detail, "inference engine unavailable");
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
    remove_recorded(&state, &path).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes a file whose record is already gone; a failure is kept in `file_errors`.
async fn remove_recorded(state: &AppState, path: &str) -> ApiResult<()> {
    match tokio::fs::remove_file(state.0.data_dir.join(path)).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Ok(state.db().add_file_error(path, &error.to_string())?),
    }
}

#[derive(Deserialize)]
struct CharacterInput {
    #[serde(default)]
    character: String,
}

async fn save_reference(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
    Json(input): Json<CharacterInput>,
) -> ApiResult<Json<db::Reference>> {
    let character = generation::character_name(&input.character)
        .map_err(|message| AppError::new(StatusCode::BAD_REQUEST, message))?;
    let generation = find_generation(&state, id)?;
    let relative = format!("references/{}.wav", uuid::Uuid::new_v4().simple());
    let target = state.0.data_dir.join(&relative);
    let partial = state
        .0
        .data_dir
        .join("tmp")
        .join(format!("{}.partial", uuid::Uuid::new_v4().simple()));
    tokio::fs::copy(state.0.data_dir.join(&generation.path), &partial).await?;
    tokio::fs::rename(&partial, &target).await?;
    let saved = state.db().save_reference(&character, id, &relative);
    match saved {
        Ok(reference) => Ok(Json(reference)),
        Err(error) => {
            let _ = tokio::fs::remove_file(&target).await;
            Err(error.into())
        }
    }
}

async fn list_references(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"references": state.db().references()?})))
}

async fn delete_reference(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<StatusCode> {
    let path = state
        .db()
        .delete_reference(id)?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "お手本の音声が見つかりません。"))?;
    remove_recorded(&state, &path).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn cleanup_generations(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let paths = state.db().delete_unsaved_generations()?;
    for path in &paths {
        remove_recorded(&state, path).await?;
    }
    Ok(Json(json!({"deleted": paths.len()})))
}

#[derive(Deserialize)]
struct FavoriteInput {
    favorite: bool,
}

async fn set_favorite(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
    Json(input): Json<FavoriteInput>,
) -> ApiResult<Json<Generation>> {
    if !state.db().set_favorite(id, input.favorite)? {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            "生成の記録が見つかりません。",
        ));
    }
    find_generation(&state, id).map(Json)
}

#[derive(Deserialize)]
struct BatchInput {
    #[serde(default)]
    character: String,
    #[serde(default)]
    lines: String,
    /// Generation settings as for `/api/generate`; text, candidates and references are set per line.
    #[serde(default)]
    settings: Value,
}

async fn start_batch(
    State(state): State<AppState>,
    Json(input): Json<BatchInput>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let bad = |message: String| AppError::new(StatusCode::BAD_REQUEST, message);
    let lines = generation::batch_lines(&input.lines).map_err(bad)?;
    let settings = if input.settings.is_null() {
        json!({})
    } else {
        input.settings
    };
    let mut template = parse_request(&state, settings).await?;
    if !template.uses_references() {
        template.mode = "clone".into();
    }
    let mut reference_ids = state.db().character_references(&input.character)?;
    reference_ids.truncate(generation::MAX_REFERENCES);
    if reference_ids.is_empty() {
        return Err(bad(
            "このキャラクターのお手本がありません。生成履歴からお手本に保存してください。".into(),
        ));
    }
    template.reference_ids = reference_ids;
    template.num_candidates = 1;
    template.text = lines[0].clone();
    let (runtime, _) = state.runtime().await?;
    template.validate(&runtime).map_err(bad)?;

    let id = uuid::Uuid::new_v4().simple().to_string();
    let batch = Batch {
        id: id.clone(),
        character: input.character.clone(),
        total: lines.len(),
        done: 0,
        generation_ids: Vec::new(),
        failed: Vec::new(),
        finished: false,
    };
    {
        let mut batches = state.batches();
        batches.insert(0, batch);
        batches.truncate(20);
    }
    tokio::spawn(run_batch(state, id.clone(), template, lines.clone()));
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"id": id, "total": lines.len()})),
    ))
}

/// Generates the lines one by one, recording progress; the batch continues after a failed line.
async fn run_batch(state: AppState, id: String, template: GenerateRequest, lines: Vec<String>) {
    for line in lines {
        let request = GenerateRequest {
            text: line.clone(),
            ..template.clone()
        };
        let result = run_generation(state.clone(), request).await;
        let mut batches = state.batches();
        let Some(batch) = batches.iter_mut().find(|b| b.id == id) else {
            return;
        };
        batch.done += 1;
        match result {
            Ok((generations, _)) => batch
                .generation_ids
                .extend(generations.iter().map(|g| g.id)),
            Err(error) => batch
                .failed
                .push(json!({"line": line, "error": error.message})),
        }
    }
    if let Some(batch) = state.batches().iter_mut().find(|b| b.id == id) {
        batch.finished = true;
    }
}

async fn list_batches(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"batches": *state.batches()}))
}

async fn get_batch(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<Value>> {
    let batches = state.batches();
    let batch = batches.iter().find(|b| b.id == id).ok_or_else(|| {
        AppError::new(
            StatusCode::NOT_FOUND,
            "まとめて生成の記録が見つかりません。",
        )
    })?;
    Ok(Json(json!(batch)))
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
        .engine
        .call("unload", &json!({}))
        .await
        .map_err(engine_error)?;
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
