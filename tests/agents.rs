use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use researchops::{
    agents,
    engine::{Engine, ResearchSpec, SourceInput},
    provider::{ConnectionConfig, Provider},
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[derive(Clone)]
struct Mock {
    researcher: Arc<AtomicUsize>,
    verifier: Arc<AtomicUsize>,
    feedback: Arc<Mutex<Vec<String>>>,
    always_reject: bool,
    malformed_first: bool,
}

async fn completion(
    State(state): State<Mock>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    assert_eq!(headers.get("authorization").unwrap(), "Bearer test-key");
    let input: Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    let system = body["messages"][0]["content"].as_str().unwrap();
    let output = if system.starts_with("Connection test") {
        json!({"ok":true})
    } else if system.starts_with("You are the Researcher") {
        let count = state.researcher.fetch_add(1, Ordering::SeqCst);
        state
            .feedback
            .lock()
            .unwrap()
            .push(input["verifier_feedback"].as_str().unwrap().into());
        let id = input["sources"][0]["id"].as_str().unwrap();
        json!({"answer":format!("Leases enable recovery [{id}]."),"evidence":[{"source_id":id,"quote":if count==0 && state.malformed_first {"Invented quote."} else {"Leases enable recovery."}}]})
    } else {
        assert_eq!(body["model"], "verifier-model");
        let count = state.verifier.fetch_add(1, Ordering::SeqCst);
        json!({"approved": !state.always_reject && count>0,"feedback":if count==0 || state.always_reject {"Explain recovery more carefully."} else {"Supported by supplied evidence."}})
    };
    (
        StatusCode::OK,
        Json(
            json!({"choices":[{"finish_reason":"stop","message":{"content":output.to_string()}}]}),
        ),
    )
}

async fn setup(
    always_reject: bool,
    malformed_first: bool,
) -> (
    tempfile::TempDir,
    String,
    Provider,
    Mock,
    tokio::task::JoinHandle<()>,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::open(dir.path().join("agents.db")).unwrap();
    let id = engine
        .create(ResearchSpec {
            objective: "Explain recovery".into(),
            questions: vec!["What enables recovery?".into()],
            sources: vec![SourceInput {
                title: "Fixture".into(),
                uri: "fixture://recovery".into(),
                content: "Leases enable recovery.".into(),
            }],
        })
        .unwrap();
    let state = Mock {
        researcher: Arc::new(AtomicUsize::new(0)),
        verifier: Arc::new(AtomicUsize::new(0)),
        feedback: Arc::new(Mutex::new(vec![])),
        always_reject,
        malformed_first,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = Router::new()
        .route("/v1/chat/completions", post(completion))
        .with_state(state.clone());
    let handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let provider = Provider::new(ConnectionConfig {
        base_url: format!("http://127.0.0.1:{port}/v1"),
        researcher_model: "researcher-model".into(),
        verifier_model: "verifier-model".into(),
        api_key: "test-key".into(),
    })
    .unwrap();
    (dir, id, provider, state, handle)
}

#[tokio::test]
async fn researcher_verifier_feedback_loop_rejects_fabrication_and_recovers() {
    let (dir, id, provider, state, handle) = setup(false, true).await;
    provider.test().await.unwrap();
    let result = agents::work(dir.path().join("agents.db"), id.clone(), provider, 1)
        .await
        .unwrap();
    handle.abort();
    assert_eq!(result.run.status, "complete");
    assert_eq!(state.researcher.load(Ordering::SeqCst), 3);
    assert_eq!(state.verifier.load(Ordering::SeqCst), 2);
    let feedback = state.feedback.lock().unwrap();
    assert!(feedback[1].contains("quote does not match"));
    assert!(feedback[2].contains("Explain recovery"));
    assert!(result.run.tasks[0]
        .answer
        .as_ref()
        .unwrap()
        .contains("Leases"));
    assert_eq!(result.run.tasks[0].findings.len(), 1);
    assert_eq!(
        result
            .run
            .agent_events
            .iter()
            .filter(|e| e.status == "retry")
            .count(),
        2
    );
    let engine = Engine::open(dir.path().join("agents.db")).unwrap();
    assert!(engine
        .report(&id)
        .unwrap()
        .contains("Model-reviewed answer"));
    assert!(!serde_json::to_string(&engine.get(&id).unwrap())
        .unwrap()
        .contains("test-key"));
}

#[tokio::test]
async fn repeated_rejection_is_bounded_and_failed_task_can_be_requeued() {
    let (dir, id, provider, state, handle) = setup(true, false).await;
    let result = agents::work(dir.path().join("agents.db"), id.clone(), provider, 1)
        .await
        .unwrap();
    handle.abort();
    assert_eq!(state.researcher.load(Ordering::SeqCst), 3);
    assert_eq!(state.verifier.load(Ordering::SeqCst), 3);
    assert_eq!(result.run.status, "partial");
    assert_eq!(result.run.tasks[0].status, "failed");
    assert!(result.run.tasks[0].findings.is_empty());
    assert!(result.run.tasks[0].answer.is_none());
    let mut engine = Engine::open(dir.path().join("agents.db")).unwrap();
    assert_eq!(engine.retry_failed(&id).unwrap(), 1);
    assert_eq!(engine.get(&id).unwrap().tasks[0].status, "pending");
    assert_eq!(engine.get(&id).unwrap().tasks[0].attempts, 1);
}

#[tokio::test]
async fn dashboard_can_test_connection_and_run_agents_in_background() {
    let (dir, id, provider, _state, mock_handle) = setup(false, false).await;
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let path = dir.path().join("agents.db");
    let app_handle =
        tokio::spawn(async move { researchops::web::serve(path, addr).await.unwrap() });
    let client = reqwest::Client::new();
    let base = format!("http://{addr}");
    for attempt in 0..100 {
        if client.get(format!("{base}/health")).send().await.is_ok() {
            break;
        }
        assert!(attempt < 99, "app did not start");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let config = provider.view();
    let saved=client.post(format!("{base}/api/connection")).json(&json!({"base_url":config.base_url,"researcher_model":config.researcher_model,"verifier_model":config.verifier_model,"api_key":"test-key"})).send().await.unwrap();
    assert_eq!(saved.status(), 200);
    let tested = client
        .post(format!("{base}/api/connection/test"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(tested.status(), 200);
    let accepted = client
        .post(format!("{base}/api/runs/{id}/agents"))
        .json(&json!({"limit":1}))
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), 202);
    let duplicate = client
        .post(format!("{base}/api/runs/{id}/agents"))
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 400);
    let mut completed = false;
    for _ in 0..100 {
        let run: Value = client
            .get(format!("{base}/api/runs/{id}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if run["agent_active"] == false {
            assert_eq!(run["status"], "complete");
            assert!(run["tasks"][0]["answer"].is_string());
            assert!(run["agent_events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["status"] == "retry"));
            completed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    app_handle.abort();
    mock_handle.abort();
    assert!(completed, "agent background job did not complete");
}
