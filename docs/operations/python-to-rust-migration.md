# Migrating to the Rust proxy — operator guide (preview)

> **Preview (2026-08-14).** This document accompanies the design and
> implementation plan for finishing the Python → Rust conversion. It will be
> finalized and verified end-to-end in Phase 4 of the plan; until then,
> treat the exact env-var surface as provisional and check
> `crates/headroom-proxy/src/config.rs` for the authoritative flag list.

## What is changing

Headroom's request path is being migrated from the Python FastAPI proxy to a
single Rust binary, `headroom-proxy`. Nothing changes on the client side: your
LLM client still points at the same port and speaks the same API. What changes
is what listens on that port.

| | Today | After cutover |
|---|---|---|
| Process | Python `headroom.proxy.server` (uvicorn) | Rust `headroom-proxy` binary |
| Image size | ~500 MB | ~50 MB |
| Bedrock/Vertex | LiteLLM Python converter | Native SigV4 / ADC routes in Rust |
| Config | Python CLI flags (`--host`, `--port`, ...) | `HEADROOM_PROXY_*` env vars |

## How to switch

Set the backend switch before starting the proxy:

```bash
export HEADROOM_PROXY_BACKEND=rust   # or =python to stay on the Python proxy
headroom proxy start
```

- `python` (default during the canary period) — boots the Python FastAPI server.
- `rust` — boots `./target/release/headroom-proxy` with the equivalent flags/env.
- The `headroom wrap ...` launchers inherit the same variable.

After the canary period the default flips to `rust`, and once Python is
retired the switch is removed — `headroom proxy start` always boots the Rust
binary.

## Key env vars (Rust proxy)

Source of truth: `crates/headroom-proxy/src/config.rs`. The load-bearing ones:

| Env var | Meaning | Default |
|---|---|---|
| `HEADROOM_PROXY_LISTEN` | Listen address (`host:port`) | `0.0.0.0:8787` |
| `HEADROOM_PROXY_UPSTREAM` | Upstream base URL (required, no default) | — |
| `HEADROOM_PROXY_COMPRESSION` | Master switch for LLM compression | `0` |
| `HEADROOM_PROXY_COMPRESSION_MODE` | `off` \| `live_zone` | `off` |
| `HEADROOM_PROXY_STRIP_INTERNAL_HEADERS` | Strip `x-headroom-*` upstream-bound | `enabled` |
| `HEADROOM_PROXY_BETA_HEADER_STICKY` | Session-sticky `anthropic-beta` union | `enabled` |
| `HEADROOM_PROXY_ENABLE_RESPONSES_STREAMING` | `/v1/responses` SSE pipeline | `true` |
| `HEADROOM_PROXY_ENABLE_BEDROCK_NATIVE` | Native Bedrock route | `true` |
| `HEADROOM_PROXY_BEDROCK_REGION` | AWS region for SigV4 | `us-east-1` |
| `HEADROOM_PROXY_VERTEX_REGION` | GCP Vertex region tag | `us-central1` |
| `HEADROOM_ROLLOUT_CHANNEL` | `stable` \| `beta` \| `canary` \| `dev` | `stable` |

## Verifying the switch

```bash
curl -s http://127.0.0.1:8787/healthz          # {"ok": true, "service": "headroom-proxy"}
curl -s http://127.0.0.1:8787/health           # loopback: includes a config block
```

The `/health` endpoint keeps the same schema across both backends so CLI-side
detection (`detect_running_proxy_backend`, savings-profile restart hints)
keeps working during the canary.

## Rollback

During the canary, rollback is a one-line change — set
`HEADROOM_PROXY_BACKEND=python` and restart. After Python is retired, rollback
is:

1. `git revert` the retirement PR, or
2. Pin to the previous container image (images are retained for **30 days**
   post-cutover).

## What stays the same

- `headroom wrap ...` launchers, `headroom evals`, `learn`, `memory` writers,
  TOIN, subscription tracking, Copilot auth — all off-path Python, unchanged.
- Client-facing API endpoints and ports.
- The byte-faithful passthrough invariant: any request the proxy does not
  intend to modify reaches upstream byte-equal.

## Local development: kompress parity harness needs `ORT_DYLIB_PATH`

If the `kompress-v2-base` ONNX model is present in the local HuggingFace
cache (`~/.cache/huggingface/hub/models--chopratejas--kompress-v2-base`),
`make test-parity` and `cargo test --workspace` **hang indefinitely** on the
kompress comparator unless the ONNX Runtime dylib path is exported — 0% CPU,
sleeping in `ort::load_dylib_from_path` (re-entrant `Once` deadlock). CI is
unaffected (no cached model → kompress fixtures skip fast); this only bites
developer machines with the model downloaded.

Fix (install `onnxruntime` into the venv once, then export per shell):

```bash
pip install onnxruntime
# adjust the version suffix to whatever pip installed:
export ORT_DYLIB_PATH=$PWD/.venv/lib/python3.13/site-packages/onnxruntime/capi/libonnxruntime.1.28.0.dylib
make test-parity        # or: cargo test --workspace
```

Without it, kompress looks like a slow tail but never finishes. Tracked as
Finding F1 in `BASELINE.md`. Since 2026-08-17 the failure mode is a loud
error instead of a hang (the session builder runs the same ort dylib guard
magika uses and returns "ONNX Runtime unavailable: ... set ORT_DYLIB_PATH"),
but the env var is still required for kompress parity to actually run.

## Questions?

See `docs/superpowers/specs/2026-08-14-python-to-rust-current-state-analysis.md`
for the measured state of the migration, or open an issue.
