use crate::{
    agents,
    contracts::{Binding, ContractImport, Mode, Trigger},
    engine::{Engine, ResearchSpec},
    execution::{self, SystemClock},
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
            Json(json!({"error": self.0.to_string(),"code":"request_failed","message":self.0.to_string(),"retryable":false,"stage":"request"})),
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
        .route("/api/contracts", get(contracts).post(register_contract))
        .route("/api/runs/{id}/contract", post(bind_contract))
        .route("/api/runs/{id}/execute", post(start_execution))
        .route("/api/recovery", get(recovery))
        .route("/api/runs/{id}/recovery", get(run_recovery))
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
    let jobs = state.jobs.clone();
    let value = db_call(state, move |db| {
        // Keep job termination/reservation outside this snapshot boundary.
        // Reading the flag after SQL can pair an old running snapshot with a
        // newly inactive job and make dashboard polling stop prematurely.
        let jobs = jobs.lock().expect("jobs lock");
        let mut value = serde_json::to_value(db.get(&id)?)?;
        value["agent_active"] = json!(jobs.active.contains(&id));
        value["agent_error"] = json!(jobs.errors.get(&id));
        Ok(value)
    })
    .await?;
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
    let content = db_call(state, move |db| db.materialize_report(&id)).await?;
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

async fn contracts(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(db_call(state, |e| Ok(json!(e.contracts()?))).await?))
}
async fn register_contract(
    State(state): State<AppState>,
    Json(input): Json<ContractImport>,
) -> Result<Json<Value>, ApiError> {
    db_call(state, move |e| e.register_contract(input)).await?;
    Ok(Json(json!({"registered":true})))
}
async fn bind_contract(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<Binding>,
) -> Result<Json<Value>, ApiError> {
    let _guard = reserve_job(&state, &id)?;
    db_call(state, move |e| e.bind_contract(&id, input)).await?;
    Ok(Json(json!({"bound":true})))
}
fn manual_trigger() -> Trigger {
    Trigger::Manual
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecuteInput {
    mode: Mode,
    #[serde(default = "manual_trigger")]
    trigger: Trigger,
}

async fn start_execution(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<ExecuteInput>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let run = id.clone();
    db_call(state.clone(), move |e| {
        e.checked_policy(&run)?;
        Ok(())
    })
    .await?;
    let guard = reserve_job(&state, &id)?;
    let provider = state.provider.read().await.clone();
    let run = id.clone();
    tokio::spawn(async move {
        let _guard = guard;
        match execution::execute(
            state.db,
            run.clone(),
            input.mode,
            input.trigger,
            provider,
            SystemClock::default(),
        )
        .await
        {
            Ok(result) if result.error.is_some() || !result.database_recorded => {
                state
                    .jobs
                    .lock()
                    .expect("jobs lock")
                    .errors
                    .insert(run, result.error.unwrap_or(result.stop_code));
            }
            Err(err) => {
                state
                    .jobs
                    .lock()
                    .expect("jobs lock")
                    .errors
                    .insert(run, execution::redact(&err.to_string(), None));
            }
            _ => {}
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"id":id,"agent_active":true,"accepted":true})),
    ))
}

async fn recovery(State(state): State<AppState>) -> Json<Value> {
    let database = db_call(state.clone(), |e| e.recovery_candidates()).await;
    let files = execution::read_recovery_files(&state.db, None);
    match database {
        Ok(runs) => Json(json!({"runs":runs,"files":files})),
        Err(err) => Json(
            json!({"runs":[],"files":files,"database_error":execution::redact(&err.0.to_string(),None)}),
        ),
    }
}
async fn run_recovery(State(state): State<AppState>, Path(id): Path<String>) -> Json<Value> {
    let run = id.clone();
    let database = db_call(state.clone(), move |e| Ok(json!(e.execution_view(&run)?))).await;
    let files = execution::read_recovery_files(&state.db, Some(&id));
    match database {
        Ok(view) => Json(json!({"execution":view,"files":files})),
        Err(err) => Json(
            json!({"execution":null,"files":files,"database_error":execution::redact(&err.0.to_string(),None)}),
        ),
    }
}
