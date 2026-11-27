use crate::engine::{Engine, ResearchSpec};
use anyhow::Result;
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Clone)]
struct AppState {
    db: PathBuf,
}

struct ApiError(anyhow::Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": self.0.to_string()})),
        )
            .into_response()
    }
}

async fn db_call<T: Send + 'static>(
    state: AppState,
    f: impl FnOnce(&mut Engine) -> Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(move || {
        let mut db = Engine::open(state.db)?;
        f(&mut db)
    })
    .await
    .map_err(|e| ApiError(e.into()))?
    .map_err(ApiError)
}

pub async fn serve(db: PathBuf, addr: SocketAddr) -> Result<()> {
    anyhow::ensure!(addr.ip().is_loopback(), "MVP server must bind to loopback");
    Engine::open(&db)?;
    let app = Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route(
            "/api/example",
            get(|| async {
                Json(
                    serde_json::from_str::<Value>(include_str!("../examples/research.json"))
                        .expect("bundled example is valid"),
                )
            }),
        )
        .route("/api/runs", get(list).post(create))
        .route("/api/runs/{id}", get(detail))
        .route("/api/runs/{id}/work", post(work))
        .route("/api/runs/{id}/report", get(report))
        .layer(DefaultBodyLimit::max(6_000_000))
        .with_state(AppState { db });
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("ResearchOps → http://{}", listener.local_addr()?);
    println!("Local-only MVP. Ctrl+C to stop; saved runs remain in SQLite.");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn list(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        db_call(state, |db| Ok(serde_json::to_value(db.list()?)?)).await?,
    ))
}

async fn create(
    State(state): State<AppState>,
    Json(spec): Json<ResearchSpec>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let id = db_call(state, move |db| db.create(spec)).await?;
    Ok((StatusCode::CREATED, Json(json!({"id":id}))))
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        db_call(state, move |db| Ok(serde_json::to_value(db.get(&id)?)?)).await?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkInput {
    #[serde(default = "default_limit")]
    limit: usize,
}
fn default_limit() -> usize {
    100
}

async fn work(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<WorkInput>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        db_call(state, move |db| {
            Ok(serde_json::to_value(db.work(&id, input.limit, 60)?)?)
        })
        .await?,
    ))
}

async fn report(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let content = db_call(state, move |db| db.report(&id)).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=researchops-report.md",
            ),
        ],
        content,
    ))
}
