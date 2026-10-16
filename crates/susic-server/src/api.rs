use crate::config::Config;
use crate::db::{hash_secret, now, Db, PlaylistRow, TrackRow};
use crate::scan::{self, ScanReport};
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub const API_VERSION: u32 = 1;

pub struct AppState {
    pub db: Db,
    pub cfg: Config,
    pub started: Instant,
    pub scanning: AtomicBool,
    pub cancel: AtomicBool,
    pub last_scan: Mutex<Option<(i64, ScanReport)>>,
    pub last_scan_error: Mutex<Option<String>>,
    pub imports: crate::imports::Imports,
    pair_failures: Mutex<(u32, Instant)>,
}

impl AppState {
    pub fn new(db: Db, cfg: Config) -> Arc<Self> {
        Arc::new(Self {
            db,
            cfg,
            started: Instant::now(),
            scanning: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            last_scan: Mutex::new(None),
            last_scan_error: Mutex::new(None),
            imports: Default::default(),
            pair_failures: Mutex::new((0, Instant::now())),
        })
    }

    /// Run a scan unless one is already running. Blocking.
    pub fn run_scan(&self) -> Option<Result<ScanReport, String>> {
        if self.scanning.swap(true, Ordering::SeqCst) {
            return None;
        }
        let res = scan::scan(&self.db, &self.cfg, &self.cancel);
        match &res {
            Ok(r) => {
                crate::log(
                    "info",
                    &format!(
                        "scan complete: {} added, {} updated, {} moved, {} removed, {} unchanged, {} failed{}",
                        r.added, r.updated, r.moved, r.removed, r.unchanged, r.failed.len(),
                        if r.removal_skipped { " (removals skipped: music directory looks empty/unmounted)" } else { "" }
                    ),
                );
                *self.last_scan.lock().unwrap() = Some((now(), r.clone()));
                *self.last_scan_error.lock().unwrap() = None;
            }
            Err(e) => {
                crate::log("error", &format!("scan failed: {e}"));
                *self.last_scan_error.lock().unwrap() = Some(e.clone());
            }
        }
        self.scanning.store(false, Ordering::SeqCst);
        Some(res)
    }
}

type S = Arc<AppState>;

struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}
impl From<crate::db::Error> for ApiError {
    fn from(e: crate::db::Error) -> Self {
        crate::log("error", &format!("db error: {e}"));
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
    }
}
type ApiResult<T> = Result<T, ApiError>;

fn not_found(what: &str) -> ApiError {
    ApiError(StatusCode::NOT_FOUND, format!("{what} not found"))
}

pub fn router(state: S) -> Router {
    let authed = Router::new()
        .route("/status", get(status))
        .route("/library", get(library))
        .route("/tracks/{id}/stream", get(stream))
        .route("/tracks/{id}/artwork", get(artwork))
        .route("/tracks/{id}/lyrics", get(lyrics))
        .route("/tracks/{id}/favourite", put(set_favourite))
        .route("/playlists", get(list_playlists))
        .route("/playlists/{uid}", put(put_playlist))
        .route("/playlists/{uid}", delete(delete_playlist))
        .route("/devices", get(list_devices))
        .route("/devices/{id}", delete(revoke_device))
        .route("/scan", post(trigger_scan))
        .route("/youtube", get(youtube_caps))
        .route("/youtube/search", get(youtube_search))
        .route("/youtube/update", post(youtube_update))
        .route("/imports", get(list_imports))
        .route("/imports", post(create_import))
        .route("/imports/{id}", delete(delete_import))
        .layer(middleware::from_fn_with_state(state.clone(), auth));
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/pair", post(pair))
        .nest("/api/v1", authed)
        .with_state(state)
}

// ---- auth ----
fn token_from(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    if let Some(v) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(t) = v.strip_prefix("Bearer ") {
            return Some(t.trim().to_string());
        }
    }
    // Media players (Media3, <audio>) cannot always set headers.
    query?
        .split('&')
        .find_map(|kv| kv.strip_prefix("access_token="))
        .map(|s| s.to_string())
}

async fn auth(State(s): State<S>, req: Request, next: Next) -> Response {
    let Some(token) = token_from(req.headers(), req.uri().query()) else {
        return ApiError(StatusCode::UNAUTHORIZED, "missing credentials".into()).into_response();
    };
    let h = hash_secret(&token);
    let s2 = s.clone();
    let ok = tokio::task::spawn_blocking(move || s2.db.authenticate(&h)).await;
    match ok {
        Ok(Ok(Some(_))) => next.run(req).await,
        Ok(Ok(None)) => {
            ApiError(StatusCode::UNAUTHORIZED, "invalid or revoked token".into()).into_response()
        }
        _ => ApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()).into_response(),
    }
}

async fn blocking<T: Send + 'static>(
    s: &S,
    f: impl FnOnce(&AppState) -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    let s = s.clone();
    tokio::task::spawn_blocking(move || f(&s))
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "task failed".into()))?
}

// ---- public ----
#[derive(Serialize)]
struct Health {
    status: &'static str,
    name: String,
    version: &'static str,
    api: u32,
    uptime_secs: u64,
}

async fn health(State(s): State<S>) -> Json<Health> {
    Json(Health {
        status: "ok",
        name: s.cfg.server_name.clone(),
        version: env!("CARGO_PKG_VERSION"),
        api: API_VERSION,
        uptime_secs: s.started.elapsed().as_secs(),
    })
}

#[derive(Deserialize)]
struct PairReq {
    code: String,
    device_name: String,
}
#[derive(Serialize)]
struct PairResp {
    token: String,
    device_id: i64,
    server_name: String,
}

pub fn normalise_code(c: &str) -> String {
    c.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

async fn pair(State(s): State<S>, Json(req): Json<PairReq>) -> ApiResult<Json<PairResp>> {
    {
        let mut f = s.pair_failures.lock().unwrap();
        if f.1.elapsed() > Duration::from_secs(60) {
            *f = (0, Instant::now());
        }
        if f.0 >= 10 {
            return Err(ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "too many failed attempts; wait a minute".into(),
            ));
        }
    }
    let name = req.device_name.trim().chars().take(64).collect::<String>();
    if name.is_empty() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "device_name required".into(),
        ));
    }
    let code_hash = hash_secret(&normalise_code(&req.code));
    let resp = blocking(&s, move |st| {
        if !st.db.consume_pair_code(&code_hash)? {
            return Ok(None);
        }
        let token = crate::random_token();
        let id = st.db.add_device(&name, &hash_secret(&token))?;
        crate::log("info", &format!("paired device #{id} ({name})"));
        Ok(Some(PairResp {
            token,
            device_id: id,
            server_name: st.cfg.server_name.clone(),
        }))
    })
    .await?;
    match resp {
        Some(r) => Ok(Json(r)),
        None => {
            let mut f = s.pair_failures.lock().unwrap();
            f.0 += 1;
            Err(ApiError(
                StatusCode::FORBIDDEN,
                "invalid or expired pairing code".into(),
            ))
        }
    }
}

// ---- authed ----
#[derive(Serialize)]
struct Status {
    name: String,
    version: &'static str,
    api: u32,
    tracks: i64,
    bytes: i64,
    revision: i64,
    scanning: bool,
    last_scan_at: Option<i64>,
    last_scan: Option<ScanReport>,
    last_scan_error: Option<String>,
}

async fn status(State(s): State<S>) -> ApiResult<Json<Status>> {
    blocking(&s, |st| {
        let (tracks, bytes) = st.db.stats()?;
        let last = st.last_scan.lock().unwrap().clone();
        Ok(Json(Status {
            name: st.cfg.server_name.clone(),
            version: env!("CARGO_PKG_VERSION"),
            api: API_VERSION,
            tracks,
            bytes,
            revision: st.db.revision()?,
            scanning: st.scanning.load(Ordering::Relaxed),
            last_scan_at: last.as_ref().map(|l| l.0),
            last_scan: last.map(|l| l.1),
            last_scan_error: st.last_scan_error.lock().unwrap().clone(),
        }))
    })
    .await
}

#[derive(Serialize)]
struct LibraryResp {
    revision: i64,
    tracks: Vec<TrackRow>,
    removed: Vec<i64>,
}

async fn library(
    State(s): State<S>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<LibraryResp>> {
    let since: i64 = q.get("since").and_then(|v| v.parse().ok()).unwrap_or(0);
    blocking(&s, move |st| {
        let revision = st.db.revision()?;
        let (tracks, removed) = st.db.changes(since)?;
        Ok(Json(LibraryResp {
            revision,
            tracks,
            removed,
        }))
    })
    .await
}

async fn stream(State(s): State<S>, Path(id): Path<i64>, req: Request) -> ApiResult<Response> {
    let rel = blocking(&s, move |st| st.db.rel_path(id).map_err(Into::into))
        .await?
        .ok_or_else(|| not_found("track"))?;
    if rel.split('/').any(|c| c == ".." || c.is_empty()) {
        return Err(not_found("track"));
    }
    let path = s.cfg.music_dir.join(&rel);
    // ServeFile implements Range, If-Range and conditional requests.
    let (parts, _) = req.into_parts();
    let req = Request::from_parts(parts, Body::empty());
    match ServeFile::new(path).oneshot(req).await {
        Ok(resp) => Ok(resp.into_response()),
        Err(e) => match e {},
    }
}

async fn artwork(State(s): State<S>, Path(id): Path<i64>, req: Request) -> ApiResult<Response> {
    let name = blocking(&s, move |st| st.db.artwork(id).map_err(Into::into))
        .await?
        .ok_or_else(|| not_found("artwork"))?;
    if name.contains('/') || name.contains("..") {
        return Err(not_found("artwork"));
    }
    let (parts, _) = req.into_parts();
    let req = Request::from_parts(parts, Body::empty());
    match ServeFile::new(s.cfg.artwork_dir().join(name))
        .oneshot(req)
        .await
    {
        Ok(resp) => Ok(resp.into_response()),
        Err(e) => match e {},
    }
}

async fn lyrics(State(s): State<S>, Path(id): Path<i64>) -> ApiResult<Response> {
    let l = blocking(&s, move |st| st.db.lyrics(id).map_err(Into::into))
        .await?
        .ok_or_else(|| not_found("lyrics"))?;
    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], l).into_response())
}

#[derive(Deserialize)]
struct FavReq {
    favourite: bool,
}
async fn set_favourite(
    State(s): State<S>,
    Path(id): Path<i64>,
    Json(b): Json<FavReq>,
) -> ApiResult<StatusCode> {
    let ok = blocking(&s, move |st| {
        st.db.set_favourite(id, b.favourite).map_err(Into::into)
    })
    .await?;
    if ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found("track"))
    }
}

async fn list_playlists(
    State(s): State<S>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    let since: i64 = q.get("since").and_then(|v| v.parse().ok()).unwrap_or(0);
    blocking(&s, move |st| {
        let revision = st.db.revision()?;
        let playlists = st.db.playlists_since(since)?;
        Ok(Json(
            serde_json::json!({ "revision": revision, "playlists": playlists }),
        ))
    })
    .await
}

#[derive(Deserialize)]
struct PutPlaylist {
    name: String,
    track_ids: Vec<i64>,
    updated_at: i64,
}

async fn put_playlist(
    State(s): State<S>,
    Path(uid): Path<String>,
    Json(b): Json<PutPlaylist>,
) -> ApiResult<Response> {
    if uid.is_empty() || uid.len() > 64 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "bad playlist id".into()));
    }
    let (row, applied): (PlaylistRow, bool) = blocking(&s, move |st| {
        st.db
            .put_playlist(&uid, &b.name, &b.track_ids, b.updated_at, false)
            .map_err(Into::into)
    })
    .await?;
    let code = if applied {
        StatusCode::OK
    } else {
        StatusCode::CONFLICT
    };
    Ok((code, Json(row)).into_response())
}

#[derive(Deserialize)]
struct DelQuery {
    updated_at: Option<i64>,
}
async fn delete_playlist(
    State(s): State<S>,
    Path(uid): Path<String>,
    Query(q): Query<DelQuery>,
) -> ApiResult<StatusCode> {
    blocking(&s, move |st| {
        let cur = st
            .db
            .get_playlist(&uid)?
            .ok_or_else(|| not_found("playlist"))?;
        let t = q.updated_at.unwrap_or_else(now).max(cur.updated_at);
        st.db.put_playlist(&uid, &cur.name, &[], t, true)?;
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_devices(State(s): State<S>) -> ApiResult<Json<serde_json::Value>> {
    blocking(&s, |st| {
        Ok(Json(serde_json::json!({ "devices": st.db.devices()? })))
    })
    .await
}

async fn revoke_device(State(s): State<S>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    let ok = blocking(&s, move |st| st.db.revoke_device(id).map_err(Into::into)).await?;
    if ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found("device"))
    }
}

async fn trigger_scan(State(s): State<S>) -> StatusCode {
    if s.scanning.load(Ordering::Relaxed) {
        return StatusCode::CONFLICT;
    }
    let s2 = s.clone();
    tokio::task::spawn_blocking(move || {
        s2.run_scan();
    });
    StatusCode::ACCEPTED
}

// ---- YouTube search / import (opt-in, see imports.rs) ----
async fn youtube_caps(State(s): State<S>) -> ApiResult<Json<crate::imports::Capabilities>> {
    blocking(&s, |st| Ok(Json(crate::imports::capabilities(st)))).await
}

async fn youtube_search(
    State(s): State<S>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<serde_json::Value>> {
    let query = q.get("q").cloned().unwrap_or_default();
    if query.trim().is_empty() || query.len() > 300 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "q must be 1-300 characters".into(),
        ));
    }
    let opts = susic_import::ytdlp::SearchOptions {
        limit: q.get("limit").and_then(|v| v.parse().ok()).unwrap_or(12),
        creative_commons: q
            .get("cc")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false),
    };
    // Reserve a slot before taking a blocking thread; never queue behind other searches.
    struct Slot(S);
    impl Drop for Slot {
        fn drop(&mut self) {
            self.0.imports.searches.fetch_sub(1, Ordering::SeqCst);
        }
    }
    if s.imports.searches.fetch_add(1, Ordering::SeqCst) >= crate::imports::MAX_SEARCHES {
        s.imports.searches.fetch_sub(1, Ordering::SeqCst);
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Another search is still running on the server — try again in a moment.".into(),
        ));
    }
    let slot = Slot(s.clone());
    blocking(&s, move |st| {
        let _slot = slot;
        if let Some(why) = crate::imports::unavailable(st, false) {
            return Err(ApiError(StatusCode::FORBIDDEN, why));
        }
        let results = susic_import::ytdlp::search(&crate::imports::tools(st), &query, &opts)
            .map_err(|e| ApiError(StatusCode::FAILED_DEPENDENCY, e))?;
        Ok(Json(serde_json::json!({ "results": results })))
    })
    .await
}

async fn youtube_update(State(s): State<S>) -> ApiResult<Json<serde_json::Value>> {
    if !s.cfg.ytdlp_enabled {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "YouTube features are turned off on this server".into(),
        ));
    }
    if s.imports.updating.swap(true, Ordering::SeqCst) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "yt-dlp is already being updated".into(),
        ));
    }
    let r = blocking(&s, |st| {
        let dir = st.cfg.tools_dir();
        let msg = if crate::imports::tools(st).ytdlp.is_none() {
            susic_import::ytdlp::install_managed(&dir).map(|v| format!("Installed yt-dlp {v}"))
        } else {
            susic_import::ytdlp::update(&dir)
        }
        .map_err(|e| ApiError(StatusCode::FAILED_DEPENDENCY, e))?;
        crate::log("info", &msg);
        Ok(Json(serde_json::json!({ "message": msg })))
    })
    .await;
    s.imports.updating.store(false, Ordering::SeqCst);
    r
}

async fn list_imports(State(s): State<S>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "jobs": s.imports.list() }))
}

async fn create_import(
    State(s): State<S>,
    Json(req): Json<crate::imports::NewJob>,
) -> ApiResult<(StatusCode, Json<crate::imports::Job>)> {
    let s2 = s.clone();
    let r = tokio::task::spawn_blocking(move || crate::imports::submit(&s2, req))
        .await
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "task failed".into()))?;
    use crate::imports::Refused;
    match r {
        Ok(job) => Ok((StatusCode::ACCEPTED, Json(job))),
        Err(e) => {
            let code = match &e {
                Refused::Unavailable(_) => StatusCode::FORBIDDEN,
                Refused::Invalid(_) => StatusCode::BAD_REQUEST,
                Refused::Busy(_) => StatusCode::TOO_MANY_REQUESTS,
                Refused::NoSpace(_) => StatusCode::INSUFFICIENT_STORAGE,
            };
            Err(ApiError(code, e.to_string()))
        }
    }
}

async fn delete_import(State(s): State<S>, Path(id): Path<u64>) -> ApiResult<StatusCode> {
    if s.imports.cancel_or_remove(id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found("import"))
    }
}
