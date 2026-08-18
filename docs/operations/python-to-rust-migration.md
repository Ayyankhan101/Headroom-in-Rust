# Migrating to the Rust proxy — operator guide

## What changed

Headroom's request path has been migrated from the Python FastAPI proxy to a
single Rust binary, `headroom-proxy`. Nothing changes on the client side: your
LLM client still points at the same port and speaks the same API. What changed
is what listens on that port.

| | Before | After |
|---|---|---|
| Process | Python `headroom.proxy.server` (uvicorn) | Rust `headroom-proxy` binary |
| Image size | ~500 MB | ~50 MB (proxy image) |
| Bedrock/Vertex | LiteLLM Python converter | Native SigV4 / ADC routes in Rust |
| Config | Python CLI flags (`--host`, `--port`, ...) | `HEADROOM_PROXY_*` env vars |

## Docker images

The Dockerfile ships **two** runtime images:

| Image | Base | Size | Use case |
|---|---|---|---|
| `headroom` (default) | `python:3.13-slim` | ~500 MB | Full CLI: proxy + memory/learn/evals + `wrap` launchers |
| `headroom-proxy` | `gcr.io/distroless/static-debian12` | ~50 MB | Proxy only: single static binary, no Python, no shell |

**For proxy-only deployments**, use the slim image:

```bash
docker run -d -p 8787:8787 \
  -e HEADROOM_PROXY_UPSTREAM=https://api.anthropic.com \
  ghcr.io/headroomlabs-ai/headroom:latest-proxy \
  --listen 0.0.0.0:8787 \
  --upstream https://api.anthropic.com
```

**For the full CLI** (memory, learn, evals, `wrap`), use the default image:

```bash
docker run -d -p 8787:8787 \
  -e HEADROOM_PROXY_UPSTREAM=https://api.anthropic.com \
  ghcr.io/headroomlabs-ai/headroom:latest
```

The default image bundles the Rust binary at `/usr/local/bin/headroom-proxy`
and sets `HEADROOM_PROXY_BACKEND_BIN=/usr/local/bin/headroom-proxy`, so
`headroom proxy start` spawns the Rust binary automatically.

**docker-compose** (`docker-compose.yml`) uses `target: proxy` for the
`headroom-proxy` service by default.

## Key env vars (Rust proxy)

Source of truth: `crates/headroom-proxy/src/config.rs`. The load-bearing ones:

| Env var | Meaning | Default |
|---|---|---|
| `HEADROOM_PROXY_LISTEN` | Listen address (`host:port`) | `0.0.0.0:8787` |
| `HEADROOM_PROXY_UPSTREAM` | Upstream base URL (required, no default) | — |
| `HEADROOM_PROXY_COMPRESSION` | Master switch for LLM compression | `false` |
| `HEADROOM_PROXY_COMPRESSION_MODE` | `off` \| `live_zone` | `off` |
| `HEADROOM_PROXY_CACHE_CONTROL_AUTO_FROZEN` | Derive `frozen_message_count` from `cache_control` markers | `enabled` |
| `HEADROOM_PROXY_AUTH_MODE_POLICY_ENFORCEMENT` | Per-auth-mode compression policy | `enabled` |
| `HEADROOM_PROXY_STRIP_INTERNAL_HEADERS` | Strip `x-headroom-*` upstream-bound | `enabled` |
| `HEADROOM_PROXY_BETA_HEADER_STICKY` | Session-sticky `anthropic-beta` union | `enabled` |
| `HEADROOM_PROXY_ENABLE_RESPONSES_STREAMING` | `/v1/responses` SSE pipeline | `true` |
| `HEADROOM_PROXY_ENABLE_CONVERSATIONS_PASSTHROUGH` | `/v1/conversations*` passthrough logging | `true` |
| `HEADROOM_PROXY_ENABLE_BEDROCK_NATIVE` | Native Bedrock route | `true` |
| `HEADROOM_PROXY_BEDROCK_REGION` | AWS region for SigV4 | `us-east-1` |
| `HEADROOM_PROXY_BEDROCK_ENDPOINT` | Bedrock endpoint override (FIPS/VPC) | derived from region |
| `HEADROOM_PROXY_AWS_PROFILE` | AWS profile for credential chain | default chain |
| `HEADROOM_PROXY_BEDROCK_VALIDATE_EVENTSTREAM_CRC` | Validate Bedrock EventStream CRC | `true` |
| `HEADROOM_PROXY_VERTEX_REGION` | GCP Vertex region tag | `us-central1` |
| `HEADROOM_PROXY_VERTEX_ADC_SCOPE` | OAuth scope for GCP ADC | `cloud-platform` |
| `HEADROOM_ROLLOUT_CHANNEL` | `stable` \| `beta` \| `canary` \| `dev` | `stable` |
| `HEADROOM_FEATURES` | Comma-separated rollout features to enable | `""` |
| `HEADROOM_DISABLE_FEATURES` | Comma-separated rollout features to disable | `""` |

## Verifying the proxy

```bash
curl -s http://127.0.0.1:8787/healthz          # {"ok": true, "service": "headroom-proxy"}
curl -s http://127.0.0.1:8787/health           # loopback: includes a config block
```

The `/health` endpoint returns a `config` block for loopback callers (same
schema as the Python proxy did), so CLI-side backend detection
(`detect_running_proxy_backend`, savings-profile restart hints) continues
to work.

## Rollback

Images are retained for **30 days** post-cutover. Rollback by pinning to
the previous container image:

```bash
docker run -d -p 8787:8787 \
  ghcr.io/headroomlabs-ai/headroom:<previous-tag>
```

Or `git revert` the retirement PR.

## What stays the same

- `headroom wrap ...` launchers, `headroom evals`, `learn`, `memory` writers,
  TOIN, subscription tracking, Copilot auth — all off-path Python, unchanged.
- Client-facing API endpoints and ports.
- The byte-faithful passthrough invariant: any request the proxy does not
  intend to modify reaches upstream byte-equal.

## Local development: kompress parity harness needs `ORT_DYLIB_PATH`

If the `kompress-v2-base` ONNX model is present in the local HuggingFace
cache (`~/.cache/huggingface/hub/models--chopratejas--kompress-v2-base`),
`make test-parity` and `cargo test --workspace` require the ONNX Runtime
dylib path to be exported. CI is unaffected (no cached model → kompress
fixtures skip fast); this only bites developer machines with the model
downloaded.

Fix (install `onnxruntime` into the venv once, then export per shell):

```bash
pip install onnxruntime
# adjust the version suffix to whatever pip installed:
export ORT_DYLIB_PATH=$PWD/.venv/lib/python3.13/site-packages/onnxruntime/capi/libonnxruntime.1.28.0.dylib
make test-parity        # or: cargo test --workspace
```

Without it, kompress tests fail loud with an `ONNX Runtime unavailable`
error. Tracked as Finding F1 in `BASELINE.md`.

## Questions?

See `docs/superpowers/specs/2026-08-14-python-to-rust-current-state-analysis.md`
for the measured state of the migration, or open an issue.
