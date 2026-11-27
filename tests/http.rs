use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Server {
    child: Child,
    port: u16,
    _dir: tempfile::TempDir,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Server {
    fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let child = Command::new(env!("CARGO_BIN_EXE_researchops"))
            .arg("--db")
            .arg(dir.path().join("http.db"))
            .args(["serve", "--port", &port.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut server = Self {
            child,
            port,
            _dir: dir,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            assert!(
                server.child.try_wait().unwrap().is_none(),
                "server exited before becoming ready"
            );
            assert!(Instant::now() < deadline, "server readiness timed out");
            thread::sleep(Duration::from_millis(25));
        }
        server
    }

    fn request(&self, method: &str, path: &str, content_type: &str, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(stream,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",self.port,body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        // Axum emits Content-Length for these buffered JSON/HTML/text responses.
        assert!(!headers
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked"));
        (status, body.to_string())
    }

    fn json(&self, method: &str, path: &str, body: Value) -> (u16, Value) {
        let (status, content) = self.request(method, path, "application/json", &body.to_string());
        (status, serde_json::from_str(&content).unwrap())
    }
}

#[test]
fn dashboard_api_runs_research_and_exports_the_gap() {
    let server = Server::start();
    let (status, page) = server.request("GET", "/", "text/plain", "");
    assert_eq!(status, 200);
    assert!(page.contains("Follow the evidence."));
    let spec: Value = serde_json::from_str(include_str!("../examples/research.json")).unwrap();
    let (status, created) = server.json("POST", "/api/runs", spec);
    assert_eq!(status, 201);
    let id = created["id"].as_str().unwrap();
    let (status, result) = server.json("POST", &format!("/api/runs/{id}/work"), json!({}));
    assert_eq!(status, 200);
    assert_eq!(result["processed"], 3);
    assert_eq!(result["run"]["status"], "partial");
    assert_eq!(result["run"]["covered_questions"], 2);
    let (_, replayed) = server.json("POST", &format!("/api/runs/{id}/work"), json!({}));
    assert_eq!(replayed["processed"], 0);
    let (status, report) =
        server.request("GET", &format!("/api/runs/{id}/report"), "text/plain", "");
    assert_eq!(status, 200);
    assert!(report.contains("**Gap:**"));
    assert!(report.contains("fixture://researchops/recovery"));
}

#[test]
fn api_rejects_invalid_specs_and_non_json_mutations() {
    let server = Server::start();
    let (status, err) = server.json(
        "POST",
        "/api/runs",
        json!({"objective":"x","questions":[],"sources":[]}),
    );
    assert_eq!(status, 400);
    assert!(err["error"].as_str().unwrap().contains("questions"));
    let (status, _) = server.request("POST", "/api/runs", "text/plain", "{}");
    assert_eq!(status, 415);
    let (status, _) = server.json("POST", "/api/runs/nonexistent/work", json!({"limit":0}));
    assert_eq!(status, 400);
    let (status, runs) = server.json("GET", "/api/runs", json!(null));
    assert_eq!(status, 200);
    assert_eq!(runs.as_array().unwrap().len(), 0);
}

#[test]
fn api_connection_is_session_only_and_redacts_credentials() {
    let server = Server::start();
    let (status, initial) = server.json("GET", "/api/connection", json!(null));
    assert_eq!(status, 200);
    assert_eq!(initial["configured"], false);
    let (status,saved)=server.json("POST","/api/connection",json!({"base_url":"https://example.com/v1","researcher_model":"r","verifier_model":"v","api_key":"never-return-this-key"}));
    assert_eq!(status, 200);
    assert_eq!(saved["has_key"], true);
    assert_eq!(saved["tested"], false);
    assert!(!saved.to_string().contains("never-return-this-key"));
    let (_, view) = server.json("GET", "/api/connection", json!(null));
    assert!(!view.to_string().contains("never-return-this-key"));
    let (status, _) = server.json("POST", "/api/runs/missing/agents", json!({}));
    assert_eq!(status, 400);
    let (_, cleared) = server.json("DELETE", "/api/connection", json!({}));
    assert_eq!(cleared["configured"], false);
}

#[test]
fn shared_execution_api_reports_system_completion_separately_from_business_partial() {
    let server = Server::start();
    let (status, contracts) = server.json("GET", "/api/contracts", json!(null));
    assert_eq!(status, 200);
    assert!(!contracts.as_array().unwrap().is_empty());
    let spec: Value = serde_json::from_str(include_str!("../examples/research.json")).unwrap();
    let (_, created) = server.json("POST", "/api/runs", spec);
    let id = created["id"].as_str().unwrap();
    let (status, _) = server.json(
        "POST",
        &format!("/api/runs/{id}/execute"),
        json!({"mode":"local","trigger":"scheduled"}),
    );
    assert_eq!(status, 202);
    let mut finished = None;
    for _ in 0..100 {
        let (_, run) = server.json("GET", &format!("/api/runs/{id}"), json!(null));
        if run["agent_active"] == false {
            finished = Some(run);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let run = finished.expect("background execution did not finish");
    assert_eq!(run["execution"]["latest"]["system_status"], "completed");
    assert_eq!(run["execution"]["business_status"], "PARTIAL");
    assert_eq!(run["execution"]["latest"]["trigger"], "scheduled");
    assert_eq!(run["execution"]["counts"]["readback_verified"], 3);
    let (status, recovery) = server.json("GET", &format!("/api/runs/{id}/recovery"), json!(null));
    assert_eq!(status, 200);
    assert!(recovery["execution"]["resume"]["sql"]
        .as_str()
        .unwrap()
        .contains("?1"));
}

#[test]
fn cli_execute_and_recovery_use_the_shared_runner() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("cli.db");
    let created = Command::new(env!("CARGO_BIN_EXE_researchops"))
        .arg("--db")
        .arg(&db)
        .args(["create", "--spec"])
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/research.json"))
        .output()
        .unwrap();
    assert!(created.status.success());
    let id = String::from_utf8(created.stdout)
        .unwrap()
        .trim()
        .to_string();
    let executed = Command::new(env!("CARGO_BIN_EXE_researchops"))
        .arg("--db")
        .arg(&db)
        .args(["execute", "--run", &id, "--trigger", "recovery"])
        .output()
        .unwrap();
    assert!(
        executed.status.success(),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    let result: Value = serde_json::from_slice(&executed.stdout).unwrap();
    assert_eq!(result["business_status"], "PARTIAL");
    assert_eq!(result["after"]["readback_verified"], 3);
    let recovery = Command::new(env!("CARGO_BIN_EXE_researchops"))
        .arg("--db")
        .arg(&db)
        .args(["recovery", "--run", &id])
        .output()
        .unwrap();
    assert!(recovery.status.success());
    let value: Value = serde_json::from_slice(&recovery.stdout).unwrap();
    assert_eq!(value["runs"][0]["run_id"], id);
}
