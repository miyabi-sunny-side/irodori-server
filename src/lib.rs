// Application crate: the library split exists for tests, not for external callers.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

pub mod audio;
pub mod db;
pub mod dictionary;
pub mod engine;
pub mod generation;
pub mod script;

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
    routing::{delete, get, patch, post, put},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;
use tower_http::{services::ServeFile, trace::TraceLayer};

use crate::{
    db::{Db, Generation, Rename},
    engine::{Engine, EngineError},
    generation::{GenerateRequest, MODELS, MODES, Runtime},
    script::{Job, Script, Status},
};

static UI: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/client/dist");

const MAX_REFERENCE_BYTES: usize = 50 * 1024 * 1024;
const REFERENCE_EXTENSIONS: &[&str] = &["wav", "mp3", "flac", "ogg", "opus", "m4a", "aac", "webm"];
/// A script JSON of 1000 long cards stays well below this.
const MAX_SCRIPT_BYTES: usize = 16 * 1024 * 1024;
const MAX_CAPTION_CHARS: usize = 2000;

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
    /// The script's generation run lives in memory; each card's audio is recorded as it finishes.
    job: Mutex<Option<ScriptJob>>,
}

#[derive(Clone, serde::Serialize)]
struct ScriptJob {
    /// Cards still to generate, the running one first.
    pending: Vec<String>,
    running: bool,
    total: usize,
    done: usize,
    failed: usize,
    stopping: bool,
    finished: bool,
}

impl ScriptJob {
    fn of(&self, card: &str) -> Option<Job> {
        let index = self.pending.iter().position(|id| id == card)?;
        Some(if index == 0 && self.running {
            Job::Running
        } else {
            Job::Queued
        })
    }
}

impl AppState {
    pub fn open(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        for dir in ["audio", "references", "voices", "tmp"] {
            std::fs::create_dir_all(config.data_dir.join(dir))?;
        }
        let db = Db::open(
            &config.data_dir.join("irodori.sqlite3"),
            &config.legacy_dictionary,
        )?;
        if db.script()?.is_none() {
            let first = Script {
                cards: vec![script::Card {
                    id: uuid::Uuid::new_v4().simple().to_string(),
                    ..script::Card::default()
                }],
                ..Script::default()
            };
            db.put_script(0, &serde_json::to_string(&first)?)?;
        }
        Ok(Self(Arc::new(Inner {
            db: Mutex::new(db),
            data_dir: config.data_dir,
            engine: config.engine,
            info: tokio::sync::Mutex::new(None),
            job: Mutex::new(None),
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

    fn job(&self) -> std::sync::MutexGuard<'_, Option<ScriptJob>> {
        self.0
            .job
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
        .route("/generations/cleanup", post(cleanup_generations))
        .route("/generations/{id}/favorite", put(set_favorite))
        .route("/voices", get(list_voices).post(add_voice))
        .route("/voices/{id}", patch(rename_voice).delete(delete_voice))
        .route("/voices/{id}/audio", get(voice_audio))
        .route(
            "/references",
            post(upload_reference).layer(DefaultBodyLimit::max(MAX_REFERENCE_BYTES + 64 * 1024)),
        )
        .route("/references/{id}/audio", get(reference_audio))
        .route(
            "/script",
            get(get_script)
                .put(put_script)
                .layer(DefaultBodyLimit::max(MAX_SCRIPT_BYTES)),
        )
        .route("/script/generate", post(generate_script))
        .route("/script/stop", post(stop_script))
        .route("/script/playlist", get(script_playlist))
        .route("/script/export", get(export_script))
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

    fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
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
    let modes: Vec<Value> = MODES
        .iter()
        .map(|(id, label)| json!({"id": id, "label": label}))
        .collect();
    Ok(Json(json!({
        "models": MODELS,
        "modes": modes,
        "devices": runtime.devices,
        "precisions": runtime.precisions,
        "cuda_capability": runtime.cuda_capability,
        "max_candidates": runtime.max_candidates,
        "emoji_groups": emoji_groups,
    })))
}

/// Parses a generation request, taking omitted device and precision fields from the engine.
async fn parse_request(state: &AppState, body: Value) -> ApiResult<GenerateRequest> {
    let (runtime, _) = state.runtime().await?;
    serde_json::from_value(generation::with_runtime_defaults(body, &runtime))
        .map_err(|error| AppError::bad(format!("生成の指定を読めません: {error}")))
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
    mut request: GenerateRequest,
) -> ApiResult<(Vec<Generation>, String)> {
    let (runtime, _) = state.runtime().await?;
    let data = state.0.data_dir.clone();
    let (text_applied, references, character) = {
        let db = state.db();
        let mut references = Vec::new();
        let mut character = None;
        if let Some(id) = request.voice_id {
            let voice = db
                .voice(id)?
                .ok_or_else(|| AppError::bad("話者が見つかりません。話者を選び直してください。"))?;
            request.voiced_by(&voice.model, &voice.caption);
            references.push(data.join(&voice.path));
            character = Some(voice.name);
        } else if request.uses_references() {
            for id in &request.reference_ids {
                let path = db.reference_path(*id)?.ok_or_else(|| {
                    AppError::bad("お手本の音声が見つかりません。もう一度追加してください。")
                })?;
                references.push(data.join(path));
            }
        }
        let text = if request.dictionary_enabled {
            dictionary::apply(&request.text, &db.dictionary()?)
        } else {
            request.text.clone()
        };
        (text, references, character)
    };
    request
        .validate(&runtime, references.len())
        .map_err(AppError::bad)?;

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
    let mut log = format!(
        "{}\n話速: {}倍",
        reply["log"].as_str().unwrap_or_default(),
        request.speed
    );
    if request.silence > 0.0 {
        use std::fmt::Write as _;
        let _ = write!(log, "\n末尾の無音: {}秒", request.silence);
    }

    // Finish every WAV under a temporary name, move it to its final name, then record it.
    let filter = audio::filter(request.speed, request.silence);
    let mut rows = Vec::new();
    let mut finals = Vec::new();
    for (index, source) in produced.iter().enumerate() {
        let candidate = index + 1;
        let relative = format!("audio/{batch}_{candidate:02}.wav");
        let target = state.0.data_dir.join(&relative);
        let moved = async {
            if let Some(filter) = &filter {
                let partial = scratch.join(format!("finish_{candidate:02}.partial.wav"));
                audio::apply_filter(source, &partial, filter).await?;
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
            voice_id: request.voice_id,
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

async fn list_voices(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"voices": state.db().voices()?})))
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct VoiceInput {
    name: String,
    model: String,
    caption: String,
    /// Exactly one source: a generation in the history or an uploaded reference.
    generation_id: Option<i64>,
    reference_id: Option<i64>,
    revision: i64,
}

/// Registers a voice. Its reference is a WAV copy of the source, so the source may go later.
async fn add_voice(
    State(state): State<AppState>,
    Json(input): Json<VoiceInput>,
) -> ApiResult<Json<db::Voice>> {
    let name = generation::voice_name(&input.name).map_err(AppError::bad)?;
    if generation::find_model(&input.model).is_none() {
        return Err(AppError::bad("使用するモデルを選んでください。"));
    }
    let caption = input.caption.trim().to_owned();
    if caption.chars().count() > MAX_CAPTION_CHARS {
        return Err(AppError::bad(format!(
            "話し方は{MAX_CAPTION_CHARS}文字までです。"
        )));
    }
    let source = {
        let db = state.db();
        match (input.generation_id, input.reference_id) {
            (Some(id), None) => db.generation(id)?.map(|g| g.path),
            (None, Some(id)) => db.reference_path(id)?,
            _ => return Err(AppError::bad("登録する音声を1つ選んでください。")),
        }
    }
    .ok_or_else(|| AppError::bad("登録する音声が見つかりません。選び直してください。"))?;

    let relative = format!("voices/{}.wav", uuid::Uuid::new_v4().simple());
    let target = state.0.data_dir.join(&relative);
    let partial = state
        .0
        .data_dir
        .join("tmp")
        .join(format!("{}.partial.wav", uuid::Uuid::new_v4().simple()));
    let prepared = async {
        audio::to_wav(&state.0.data_dir.join(&source), &partial).await?;
        let seconds = audio::wav_seconds(&tokio::fs::read(&partial).await?);
        if !seconds.is_some_and(|s| (1.0..=120.0).contains(&s)) {
            return Ok(Err(AppError::bad(
                "参照音声は1～120秒の音声にしてください。",
            )));
        }
        tokio::fs::rename(&partial, &target).await.map(Ok)
    }
    .await;
    let _ = tokio::fs::remove_file(&partial).await;
    match prepared {
        Err(error) => {
            tracing::warn!(%error, source, "voice reference could not be read");
            return Err(AppError::bad(
                "音声を読み込めません。形式を確認してください。",
            ));
        }
        Ok(Err(error)) => return Err(error),
        Ok(Ok(())) => {}
    }
    let saved = state.db().add_voice(
        &name,
        &input.model,
        &caption,
        &relative,
        input.generation_id,
    );
    match saved {
        Ok(voice) => Ok(Json(voice)),
        Err(error) => {
            let _ = tokio::fs::remove_file(&target).await;
            Err(error.into())
        }
    }
}

async fn rename_voice(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
    Json(input): Json<VoiceInput>,
) -> ApiResult<Json<db::Voice>> {
    let name = generation::voice_name(&input.name).map_err(AppError::bad)?;
    match state.db().rename_voice(id, &name, input.revision)? {
        Ok(voice) => Ok(Json(voice)),
        Err(Rename::Missing) => Err(AppError::new(
            StatusCode::NOT_FOUND,
            "話者が見つかりません。選び直してください。",
        )),
        Err(Rename::Conflict) => Err(AppError::new(
            StatusCode::CONFLICT,
            "話者が更新されています。一覧から読み直してください。",
        )),
    }
}

async fn delete_voice(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<StatusCode> {
    if state.db().delete_voice(id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::new(
            StatusCode::NOT_FOUND,
            "話者が見つかりません。選び直してください。",
        ))
    }
}

async fn voice_audio(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<i64>,
    request: Request,
) -> ApiResult<Response> {
    let voice = state
        .db()
        .voice(id)?
        .ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "話者が見つかりません。"))?;
    serve(&state.0.data_dir.join(voice.path), request).await
}

async fn upload_reference(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    let bad = |message: &str| AppError::bad(message);
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
            Ok(id) => Ok(Json(
                json!({"id": id, "name": name, "audio_url": format!("/api/references/{id}/audio")}),
            )),
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

/// One card as the editor shows it.
#[derive(serde::Serialize)]
struct CardView {
    id: String,
    status: Status,
    error: String,
    signature: String,
    generation_id: Option<i64>,
    audio_url: Option<String>,
    download_name: Option<String>,
    #[serde(skip)]
    path: Option<String>,
}

/// The stored script with each card's status, and the running job.
struct ScriptState {
    revision: i64,
    script: Script,
    cards: Vec<CardView>,
}

fn load_script(state: &AppState) -> ApiResult<ScriptState> {
    let job = state.job().clone();
    let db = state.db();
    let (revision, doc) = db
        .script()?
        .ok_or_else(|| AppError::internal("script missing"))?;
    let script: Script = serde_json::from_str(&doc).map_err(AppError::internal)?;
    let entries = db.dictionary()?;
    let voices: std::collections::HashMap<i64, String> =
        db.voices()?.into_iter().map(|v| (v.id, v.name)).collect();
    let results = db.card_results()?;
    let mut cards = Vec::with_capacity(script.cards.len());
    for (index, card) in script.cards.iter().enumerate() {
        let request = script.request(card);
        let applied = if script.dictionary_enabled {
            dictionary::apply(&card.text, &entries)
        } else {
            card.text.clone()
        };
        let current = script::signature(card, &applied, &request);
        let result = results.get(&card.id);
        let outcome = result.map(|r| script::Outcome {
            audio: r.audio,
            signature: r.signature.clone(),
            error: r.error.clone(),
        });
        let voice = card.voice_id.and_then(|id| voices.get(&id));
        let status = script::status(
            card,
            voice.is_some(),
            job.as_ref().and_then(|j| j.of(&card.id)),
            outcome.as_ref(),
            &current,
        );
        let generation = match result {
            Some(r) if r.audio => r
                .generation_id
                .map(|id| db.generation(id))
                .transpose()?
                .flatten(),
            _ => None,
        };
        let voice_name = voice
            .cloned()
            .or_else(|| generation.as_ref().and_then(|g| g.character.clone()))
            .unwrap_or_default();
        let download_name = generation.as_ref().map(|g| {
            let date: String = g
                .created_at
                .chars()
                .take(10)
                .filter(|c| *c != '-')
                .collect();
            format!(
                "{}.wav",
                script::file_name(
                    &script.options.pattern,
                    index + 1,
                    &voice_name,
                    &card.text,
                    &date
                )
            )
        });
        cards.push(CardView {
            id: card.id.clone(),
            status,
            error: result.map(|r| r.error.clone()).unwrap_or_default(),
            signature: current,
            generation_id: generation.as_ref().map(|g| g.id),
            audio_url: generation.as_ref().map(|g| g.audio_url.clone()),
            download_name,
            path: generation.map(|g| g.path),
        });
    }
    Ok(ScriptState {
        revision,
        script,
        cards,
    })
}

fn script_json(state: &AppState) -> ApiResult<Json<Value>> {
    let loaded = load_script(state)?;
    Ok(Json(json!({
        "revision": loaded.revision,
        "script": loaded.script,
        "cards": loaded.cards,
        "job": *state.job(),
    })))
}

async fn get_script(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    script_json(&state)
}

#[derive(Deserialize)]
struct ScriptInput {
    revision: i64,
    script: Script,
}

const SCRIPT_CONFLICT: &str =
    "別の画面で台本が更新されました。最新の台本を読み込み直してから操作してください。";

async fn put_script(
    State(state): State<AppState>,
    Json(input): Json<ScriptInput>,
) -> ApiResult<Json<Value>> {
    let mut script = input.script;
    script.validate().map_err(AppError::bad)?;
    let doc = serde_json::to_string(&script).map_err(AppError::internal)?;
    if state.db().put_script(input.revision, &doc)?.is_none() {
        return Err(AppError::new(StatusCode::CONFLICT, SCRIPT_CONFLICT));
    }
    script_json(&state)
}

#[derive(Deserialize)]
struct GenerateInput {
    revision: i64,
    /// `all`, `changed` (not generated, changed or failed) or one card's id.
    target: String,
}

async fn generate_script(
    State(state): State<AppState>,
    Json(input): Json<GenerateInput>,
) -> ApiResult<Json<Value>> {
    let loaded = load_script(&state)?;
    if loaded.revision != input.revision {
        return Err(AppError::new(StatusCode::CONFLICT, SCRIPT_CONFLICT));
    }
    let mut runs = Vec::new();
    for (index, (card, view)) in loaded.script.cards.iter().zip(&loaded.cards).enumerate() {
        let wanted = match input.target.as_str() {
            "all" => true,
            "changed" => matches!(
                view.status,
                Status::New | Status::Changed | Status::Failed | Status::NoVoice
            ),
            id => card.id == id,
        };
        if !wanted || view.status == Status::Empty {
            continue;
        }
        if view.status == Status::NoVoice {
            return Err(AppError::bad(format!(
                "{}番のセリフの話者を選んでください。",
                index + 1
            )));
        }
        runs.push((
            card.id.clone(),
            loaded.script.request(card),
            view.signature.clone(),
        ));
    }
    if runs.is_empty() {
        return Err(AppError::bad(
            "生成するセリフがありません。文章と生成の状態を確認してください。",
        ));
    }
    {
        let mut job = state.job();
        if job.as_ref().is_some_and(|j| !j.finished) {
            return Err(AppError::new(
                StatusCode::CONFLICT,
                "現在の生成が終わるか、停止してから実行してください。",
            ));
        }
        *job = Some(ScriptJob {
            pending: runs.iter().map(|(id, _, _)| id.clone()).collect(),
            running: false,
            total: runs.len(),
            done: 0,
            failed: 0,
            stopping: false,
            finished: false,
        });
    }
    tokio::spawn(run_script(state.clone(), runs));
    script_json(&state)
}

/// Generates the cards one at a time from the inputs captured when the run started.
/// A failed card keeps its earlier audio and the run moves on; stopping waits for the running card.
async fn run_script(state: AppState, runs: Vec<(String, GenerateRequest, String)>) {
    for (card, request, signature) in runs {
        {
            let mut job = state.job();
            let Some(job) = job.as_mut() else { return };
            if job.stopping {
                break;
            }
            job.running = true;
        }
        let result = run_generation(state.clone(), request).await;
        let recorded = {
            let db = state.db();
            match &result {
                Ok((generations, _)) => db.set_card_audio(&card, generations[0].id, &signature),
                Err(error) => db.set_card_error(&card, &error.message),
            }
        };
        if let Err(error) = recorded {
            tracing::error!(%error, card, "card result was not recorded");
        }
        let mut job = state.job();
        if let Some(job) = job.as_mut() {
            job.pending.retain(|id| *id != card);
            job.running = false;
            job.done += 1;
            job.failed += usize::from(result.is_err());
        }
    }
    if let Some(job) = state.job().as_mut() {
        job.pending.clear();
        job.running = false;
        job.finished = true;
    }
}

async fn stop_script(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    if let Some(job) = state.job().as_mut().filter(|j| !j.finished) {
        job.stopping = true;
        job.pending.truncate(usize::from(job.running));
    }
    script_json(&state)
}

#[derive(Deserialize)]
struct PlaylistQuery {
    /// Start at this card instead of the first.
    from: Option<String>,
}

/// The audio of the cards to play in order, or why some cards have none.
fn ready_cards(loaded: &ScriptState, from: Option<&str>) -> ApiResult<Vec<usize>> {
    let start = match from {
        Some(id) => loaded
            .cards
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| {
                AppError::bad("再生を始めるセリフが見つかりません。選び直してください。")
            })?,
        None => 0,
    };
    let statuses: Vec<Status> = loaded.cards.iter().map(|c| c.status).collect();
    let mut ready = script::ready(&statuses[start..])
        .map_err(|message| AppError::new(StatusCode::CONFLICT, message))?;
    // Report card numbers from the start of the script, not of the slice.
    for index in &mut ready {
        *index += start;
    }
    Ok(ready)
}

async fn script_playlist(
    State(state): State<AppState>,
    Query(query): Query<PlaylistQuery>,
) -> ApiResult<Json<Value>> {
    let loaded = load_script(&state)?;
    let items: Vec<Value> = ready_cards(&loaded, query.from.as_deref())?
        .into_iter()
        .map(|i| json!({"id": loaded.cards[i].id, "audio_url": loaded.cards[i].audio_url}))
        .collect();
    Ok(Json(json!({"items": items})))
}

#[derive(Deserialize)]
struct ExportQuery {
    kind: String,
}

/// The whole script as one joined WAV or a ZIP of each card's WAV, in script order.
async fn export_script(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
    let loaded = load_script(&state)?;
    let ready = ready_cards(&loaded, None)?;
    let data = state.0.data_dir.clone();
    let sources: Vec<PathBuf> = ready
        .iter()
        .map(|&i| data.join(loaded.cards[i].path.as_deref().unwrap_or_default()))
        .collect();
    let work = data
        .join("tmp")
        .join(format!("export-{}", uuid::Uuid::new_v4().simple()));
    tokio::fs::create_dir_all(&work).await?;
    let result = async {
        match query.kind.as_str() {
            "wav" => {
                let target = work.join("joined.wav");
                audio::join(&sources, &target).await?;
                Ok(("audio/wav", "script.wav", tokio::fs::read(&target).await?))
            }
            "zip" => {
                let names: Vec<String> = ready
                    .iter()
                    .map(|&i| {
                        let view = &loaded.cards[i];
                        view.download_name
                            .clone()
                            .unwrap_or_default()
                            .trim_end_matches(".wav")
                            .to_owned()
                    })
                    .collect();
                let names = script::unique_names(&names);
                let texts: Vec<String> = ready
                    .iter()
                    .map(|&i| loaded.script.cards[i].text.clone())
                    .collect();
                let txt = loaded.script.options.txt;
                let target = work.join("cards.zip");
                let path = target.clone();
                tokio::task::spawn_blocking(move || {
                    write_zip(&path, &names, &sources, txt.then_some(texts.as_slice()))
                })
                .await
                .map_err(std::io::Error::other)??;
                Ok((
                    "application/zip",
                    "script.zip",
                    tokio::fs::read(&target).await?,
                ))
            }
            _ => Err(std::io::Error::other("unknown export kind")),
        }
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&work).await;
    let (content_type, name, body) = result.map_err(|error| {
        if matches!(query.kind.as_str(), "wav" | "zip") {
            AppError::internal(error)
        } else {
            AppError::bad("保存形式はwavかzipを指定してください。")
        }
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        body,
    )
        .into_response())
}

/// Stores each WAV (and its text, when given) uncompressed under the given names.
fn write_zip(
    target: &Path,
    names: &[String],
    sources: &[PathBuf],
    texts: Option<&[String]>,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .large_file(true);
    let mut archive = zip::ZipWriter::new(std::fs::File::create(target)?);
    for (index, (name, source)) in names.iter().zip(sources).enumerate() {
        archive.start_file(format!("{name}.wav"), options)?;
        std::io::copy(&mut std::fs::File::open(source)?, &mut archive)?;
        if let Some(texts) = texts {
            archive.start_file(format!("{name}.txt"), options)?;
            // A BOM, as EasyIrodoriTTS writes, so Windows editors read the text as UTF-8.
            archive.write_all("\u{feff}".as_bytes())?;
            archive.write_all(texts[index].as_bytes())?;
        }
    }
    archive.finish()?;
    Ok(())
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
    let (word, reading) =
        dictionary::validate(&input.word, &input.reading).map_err(AppError::bad)?;
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
