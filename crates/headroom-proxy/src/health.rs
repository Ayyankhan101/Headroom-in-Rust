//! Health endpoints. These are intercepted by Rust and never forwarded.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::proxy::AppState;

/// Own health: 200 if the proxy process is up.
pub async fn healthz() -> impl IntoResponse {
    Json(json!({ "ok": true, "service": "headroom-proxy" }))
}

/// `/health` — mirrors the Python proxy's `_health_payload(include_config=...)`
/// gating: loopback callers get the `config` block the Python CLI reads
/// (`detect_running_proxy_backend` → `config.backend`, and
/// `_agent_savings_config_mismatches` → the savings keys), while network
/// callers get status-only so upstream URLs and backend settings never leak
/// to external scanners.
pub async fn health(
    State(state): State<AppState>,
    ConnectInfo(client_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let mut payload = json!({
        "service": "headroom-proxy",
        "status": "healthy",
        "ready": true,
    });
    if request_is_loopback(&client_addr, &headers) {
        payload["config"] = config_block(&state);
    }
    (StatusCode::OK, Json(payload)).into_response()
}

/// Two-gate loopback check mirroring Python's `_request_is_loopback`
/// (loopback client IP + loopback `Host` header, the DNS-rebinding defence).
fn request_is_loopback(client_addr: &SocketAddr, headers: &HeaderMap) -> bool {
    if !client_addr.ip().is_loopback() {
        return false;
    }
    is_loopback_host_header(headers.get("host").and_then(|v| v.to_str().ok()))
}

/// True when a `Host:` header names a loopback address. Accepts a port
/// (`127.0.0.1:8787`, `[::1]:8787`, `localhost:8787`) and bracket notation
/// for raw IPv6 literals per RFC 3986; missing/empty is False, matching
/// `headroom.proxy.loopback_guard.is_loopback_host_header`.
fn is_loopback_host_header(header_value: Option<&str>) -> bool {
    let Some(candidate) = header_value.map(str::trim) else {
        return false;
    };
    if candidate.is_empty() {
        return false;
    }
    let host_part = if let Some(rest) = candidate.strip_prefix('[') {
        // Bracketed IPv6: [::1] or [::1]:8787 — strip brackets + port suffix.
        match rest.find(']') {
            Some(closing) => &rest[..closing],
            None => return false,
        }
    } else if candidate.matches(':').count() == 1 {
        // Single colon = host:port for IPv4 / hostname. A bare IPv6
        // literal without brackets has multiple colons and is kept whole.
        candidate
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(candidate)
    } else {
        candidate
    };
    is_loopback_host(host_part)
}

/// True when `host` is `localhost` (case-insensitive) or a loopback IP
/// literal, mirroring `headroom.proxy.loopback_guard.is_loopback_host`.
fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// The `config` block of `/health`, populated for loopback callers only.
/// `backend` is always `"rust"`; the savings knobs are read from the
/// process environment (the CLI passes `HEADROOM_*` through, so a proxy
/// spawned via `HEADROOM_PROXY_BACKEND=rust` answers exactly what the
/// operator set) with the same types `_agent_savings_config_mismatches`
/// compares, so the restart-hint logic never misfires.
fn config_block(state: &AppState) -> Value {
    json!({
        "backend": "rust",
        "optimize": state.config.compression,
        "cache": false,
        "rate_limit": false,
        "memory": false,
        "learn": false,
        "code_graph": false,
        "anthropic_api_url": Value::Null,
        "openai_api_url": Value::Null,
        "gemini_api_url": Value::Null,
        "cloudcode_api_url": Value::Null,
        "vertex_api_url": Value::Null,
        "savings_profile": env_str("HEADROOM_SAVINGS_PROFILE"),
        "target_ratio": env_f64("HEADROOM_TARGET_RATIO"),
        "compress_user_messages": env_bool("HEADROOM_COMPRESS_USER_MESSAGES"),
        "compress_system_messages": env_bool("HEADROOM_COMPRESS_SYSTEM_MESSAGES"),
        "protect_recent": env_i64("HEADROOM_PROTECT_RECENT"),
        "protect_analysis_context": env_bool("HEADROOM_PROTECT_ANALYSIS_CONTEXT"),
        "min_tokens_to_crush": env_i64("HEADROOM_MIN_TOKENS"),
        "max_items_after_crush": env_i64("HEADROOM_MAX_ITEMS"),
        "smart_crusher_with_compaction": env_bool("HEADROOM_SMART_CRUSHER_COMPACTION"),
        "accuracy_guard": env_str("HEADROOM_ACCURACY_GUARD"),
        "pid": std::process::id(),
    })
}

fn env_str(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn env_f64(key: &str) -> Option<f64> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn env_i64(key: &str) -> Option<i64> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

/// Python's `_env_bool_value`: truthy iff trimmed/lowercased in
/// {1, true, yes, on}. Emits the JSON bool `_agent_savings_config_mismatches`
/// compares (`bool(actual) is _env_bool_value(expected)`).
fn env_bool(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|v| {
        matches!(
            v.trim().to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Effective rollout state of this running Rust proxy process.
pub async fn rollout_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.config.rollout.to_value())
}

/// Upstream health: GETs upstream `/healthz`. Returns 200 when reachable +
/// 2xx, 503 otherwise. The endpoint name is reserved by the proxy and is
/// not forwarded; operators must not name a real upstream route this.
pub async fn healthz_upstream(State(state): State<AppState>) -> Response {
    // Use an absolute path so upstream URLs with non-trailing-slash paths
    // (e.g. http://localhost:8788/api) resolve to /healthz, not replace the
    // last segment. Url::join("healthz") would strip "api" per RFC 3986.
    let mut url = state.config.upstream.clone();
    url.set_path("/healthz");
    url.set_query(None);
    match state.client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => {
            (StatusCode::OK, Json(json!({"ok": true}))).into_response()
        }
        Ok(resp) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"ok": false, "upstream_status": resp.status().as_u16()})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"ok": false, "error": e.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    #[tokio::test]
    async fn rollout_status_exposes_running_snapshot() {
        let state = AppState::new(Config::for_test("http://127.0.0.1:9".parse().unwrap())).unwrap();
        let expected = state.config.rollout.snapshot_digest();
        let Json(payload) = rollout_status(State(state)).await;
        assert_eq!(payload["snapshot_digest"], expected);
        assert_eq!(payload["qualification_eligible"], true);
    }

    #[test]
    fn config_block_backend_is_rust() {
        let state = AppState::new(Config::for_test("http://127.0.0.1:9".parse().unwrap())).unwrap();
        let block = config_block(&state);
        assert_eq!(block["backend"], "rust");
        assert_eq!(block["optimize"], false);
        assert!(block["pid"].as_i64().is_some());
    }

    #[test]
    fn loopback_host_header_accepts_localhost_and_ports() {
        for host in [
            "127.0.0.1",
            "127.0.0.1:8787",
            "localhost",
            "LOCALHOST:8787",
            "::1",
            "[::1]:8787",
        ] {
            assert!(
                is_loopback_host_header(Some(host)),
                "{host} should be loopback"
            );
        }
        for host in ["example.com", "", "[::1", "10.0.0.1:8787"] {
            assert!(
                !is_loopback_host_header(Some(host)),
                "{host} must not be loopback"
            );
        }
        assert!(!is_loopback_host_header(None));
    }
}
