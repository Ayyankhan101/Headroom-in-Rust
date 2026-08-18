//! Health endpoints: own /healthz always 200; /healthz/upstream reflects upstream.

mod common;

use common::{start_proxy, start_proxy_with};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn healthz_ok_when_upstream_down() {
    let proxy = start_proxy("http://127.0.0.1:1").await; // unroutable port
    let resp = reqwest::get(format!("{}/healthz", proxy.url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    proxy.shutdown().await;
}

#[tokio::test]
async fn healthz_upstream_503_when_upstream_down() {
    let proxy = start_proxy("http://127.0.0.1:1").await;
    let resp = reqwest::get(format!("{}/healthz/upstream", proxy.url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 503);
    proxy.shutdown().await;
}

#[tokio::test]
async fn healthz_upstream_200_when_upstream_healthy() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/healthz"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&upstream)
        .await;
    let proxy = start_proxy(&upstream.uri()).await;
    let resp = reqwest::get(format!("{}/healthz/upstream", proxy.url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    proxy.shutdown().await;
}

// --- Phase 2 Task 2.3: loopback /health exposes a config block so the
// Python CLI (`detect_running_proxy_backend`, `_agent_savings_config_mismatches`)
// keeps working against the Rust binary. Loopback callers get `config`;
// network callers get status-only — mirroring Python's gating in
// `_health_payload(include_config=_request_is_loopback(request))`.

#[tokio::test]
async fn health_loopback_exposes_config_backend() {
    let proxy = start_proxy("http://127.0.0.1:1").await;
    let resp = reqwest::get(format!("{}/health", proxy.url()))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload: Value = resp.json().await.unwrap();
    assert_eq!(payload["config"]["backend"], "rust");
    proxy.shutdown().await;
}

#[tokio::test]
async fn health_non_loopback_host_hides_config() {
    let proxy = start_proxy("http://127.0.0.1:1").await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/health", proxy.url()))
        .header("host", "attacker.example")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let payload: Value = resp.json().await.unwrap();
    assert!(
        payload.get("config").is_none(),
        "non-loopback Host must not get config"
    );
    proxy.shutdown().await;
}

#[tokio::test]
async fn health_config_optimize_reflects_compression_flag() {
    let proxy = start_proxy_with("http://127.0.0.1:1", |cfg| cfg.compression = true).await;
    let resp = reqwest::get(format!("{}/health", proxy.url()))
        .await
        .unwrap();
    let payload: Value = resp.json().await.unwrap();
    assert_eq!(payload["config"]["optimize"], true);
    assert_eq!(payload["config"]["backend"], "rust");
    proxy.shutdown().await;
}
