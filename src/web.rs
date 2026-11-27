use crate::{
    agents,
    engine::{Engine, ResearchSpec},
    provider::{ConnectionConfig, Provider},
};
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
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::RwLock;

#[derive(Clone)]
struct AppState {
    db: PathBuf,
    provider: Arc<RwLock<Option<Provider>>>,
    jobs: Arc<Mutex<Jobs>>,
}

#[derive(Default)]
struct Jobs {
    active: HashSet<String>,
    errors: HashMap<String, String>,
}

struct JobGuard {
    jobs: Arc<Mutex<Jobs>>,
    id: String,
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.active.remove(&self.id);
        }
    }
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
            "/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/app.js"),
                )
            }),
        )
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
        .route(
            "/api/connection",
            get(connection).post(connect).delete(disconnect),
        )
        .route("/api/connection/test", post(test_connection))
        .route("/api/runs/{id}/agents", post(start_agents))
        .route("/api/runs/{id}/retry", post(retry))
        .layer(DefaultBodyLimit::max(6_000_000))
        .with_state(AppState {
            db,
            provider: Arc::new(RwLock::new(None)),
            jobs: Arc::new(Mutex::new(Jobs::default())),
        });
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
    let run_id = id.clone();
    let mut value = db_call(state.clone(), move |db| {
        Ok(serde_json::to_value(db.get(&run_id)?)?)
    })
    .await?;
    let jobs = state.jobs.lock().expect("jobs lock");
    value["agent_active"] = json!(jobs.active.contains(&id));
    value["agent_error"] = json!(jobs.errors.get(&id));
    Ok(Json(value))
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
    let _guard = reserve_job(&state, &id)?;
    Ok(Json(
        db_call(state, move |db| {
            Ok(serde_json::to_value(db.work(&id, input.limit, 60)?)?)
        })
        .await?,
    ))
}

fn reserve_job(state: &AppState, id: &str) -> Result<JobGuard, ApiError> {
    let mut jobs = state.jobs.lock().expect("jobs lock");
    if !jobs.active.insert(id.into()) {
        return Err(ApiError(anyhow::anyhow!(
            "this run already has an active worker"
        )));
    }
    jobs.errors.remove(id);
    Ok(JobGuard {
        jobs: state.jobs.clone(),
        id: id.into(),
    })
}

async fn connection(State(state): State<AppState>) -> Json<Value> {
    let provider = state.provider.read().await;
    Json(
        provider
            .as_ref()
            .map(|p| json!(p.view()))
            .unwrap_or_else(|| json!({"configured":false,"tested":false})),
    )
}

async fn connect(
    State(state): State<AppState>,
    Json(config): Json<ConnectionConfig>,
) -> Result<Json<Value>, ApiError> {
    let provider = Provider::new(config).map_err(ApiError)?;
    let value = json!(provider.view());
    *state.provider.write().await = Some(provider);
    Ok(Json(value))
}

async fn disconnect(State(state): State<AppState>) -> Json<Value> {
    *state.provider.write().await = None;
    Json(json!({"configured":false,"tested":false}))
}

async fn test_connection(
    State(state): State<AppState>,
    Json(_): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let provider = state
        .provider
        .read()
        .await
        .clone()
        .ok_or_else(|| ApiError(anyhow::anyhow!("configure an API connection first")))?;
    provider.test().await.map_err(ApiError)?;
    let mut connection = state.provider.write().await;
    let current = connection
        .as_mut()
        .filter(|p| p.generation == provider.generation)
        .ok_or_else(|| {
            ApiError(anyhow::anyhow!(
                "connection changed during the test; test the new connection"
            ))
        })?;
    current.tested = true;
    Ok(Json(json!(current.view())))
}

async fn start_agents(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<WorkInput>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if !(1..=100).contains(&input.limit) {
        return Err(ApiError(anyhow::anyhow!("agent limit must be 1..100")));
    }
    let provider = state
        .provider
        .read()
        .await
        .clone()
        .filter(|p| p.tested)
        .ok_or_else(|| {
            ApiError(anyhow::anyhow!(
                "save and test an API connection before starting agents"
            ))
        })?;
    agents::preflight(&state.db, &id).await.map_err(ApiError)?;
    let guard = reserve_job(&state, &id)?;
    let run_id = id.clone();
    tokio::spawn(async move {
        let _guard = guard;
        if let Err(err) = agents::work(state.db, run_id.clone(), provider, input.limit).await {
            state
                .jobs
                .lock()
                .expect("jobs lock")
                .errors
                .insert(run_id, err.to_string());
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"id":id,"agent_active":true})),
    ))
}

async fn retry(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(_): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let _guard = reserve_job(&state, &id)?;
    let count = db_call(state, move |engine| engine.retry_failed(&id)).await?;
    Ok(Json(json!({"requeued":count})))
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
