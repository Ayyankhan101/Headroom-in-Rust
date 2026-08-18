# Headroom Python → Rust Conversion — Current-State Analysis

**Date:** 2026-08-14
**Branch:** `realign-H0-doc-finish-python-to-rust-conversion`
**Owner:** fork maintainer (`Ayyankhan101/Headroom-in-Rust`)
**Upstream:** `headroomlabs-ai/headroom`

This is the expanded current-state analysis referenced as §1 of the design doc
(`docs/superpowers/specs/2026-08-14-finish-python-to-rust-conversion-design.md`).
It is measurement-driven: every number below was captured from the tree on
2026-08-14, not estimated.

---

## 1. Where the migration stands

Headroom is an LLM context-optimization proxy, mid-migration from Python to
Rust. The request path is **already in Rust**; what remains is proving
byte-equality, switching operators over, and deleting the Python request path.

### 1.1 Measured codebase state

| Metric | Value | How measured |
|---|---|---|
| Python source LOC (`headroom/`) | **203,362** | `find headroom -name '*.py' -exec cat {} + \| wc -l` |
| Python source modules | **520** | `find headroom -name '*.py' \| wc -l` |
| Python test LOC (`tests/`) | **237,307** | `find tests -name '*.py' -exec cat {} + \| wc -l` |
| Python test modules | **798** | `find tests -name '*.py' \| wc -l` |
| Rust source LOC (`crates/`) | **79,375** | `find crates -name '*.rs' -exec cat {} + \| wc -l` |
| Rust source modules | **197** | `find crates -name '*.rs' \| wc -l` |
| Rust files with in-crate `#[cfg(test)]` | **107** | `grep -rl '#\[cfg(test)\]' crates` |
| Fork drift vs upstream `main` | **26 commits behind** | `git rev-list --count HEAD..upstream/main` |

The design doc's earlier figure of "~2,864 LOC" for `headroom/proxy/server.py`
is **stale — the file is 5,933 lines** as measured. The deletion inventory in
the implementation plan uses measured numbers only.

### 1.2 The Rust workspace (`crates/`)

| Crate | Role |
|---|---|
| `headroom-core` | Shared types + transform surface: SmartCrusher, Diff/Log/Code/Kompress (`onnx_cpu.rs`)/TextCrusher compressors, tokenizer, CCR with persistent backends (in-memory / sqlite / redis), `signals/` trait module, `auth_mode`, `rollout`, `cache_control`, `compression_policy` |
| `headroom-proxy` | The request-path proxy: Anthropic/OpenAI/Responses/streaming handlers, byte-faithful SSE parser, WebSocket, native Bedrock (SigV4) and Vertex (ADC) routes, cache stabilization (volatile detector, drift telemetry, tool-def normalization), Prometheus/OTel observability, `responses_items.rs` |
| `headroom-py` | PyO3 cdylib exposing `headroom._core`; Python transform modules (e.g. `smart_crusher.py`) are thin shims that delegate to Rust |
| `headroom-version-parity` | Rust-vs-Rust version-parity harness (11 fixture sets, 218 recorded JSON fixtures; renamed from `headroom-parity` per Q12) |
| `headroom-simulators` | Offline upstream simulators for tests |

### 1.3 Parity harness state

- **Comparators:** 8 of 10 real (`log_compressor`, `diff_compressor`,
  `tokenizer`, `smart_crusher`, `content_detector`, `text_crusher`,
  `kompress`, `code_aware_compressor`); **2 stubs** report `Skipped`:
  `cache_aligner` (20 fixtures) and `ccr` (25 fixtures).
- **Fixture sets:** 11 — `cache_aligner`, `ccr`, `code_aware_compressor`,
  `codex_openai_contracts` (schema-only, reviewed), `content_detector`,
  `diff_compressor`, `kompress`, `log_compressor`, `smart_crusher`,
  `text_crusher`, `tokenizer`. Total **228 JSON fixtures**.
- **CI gate:** the per-PR parity job already exists in
  `.github/workflows/rust.yml` (`Skipped` allowed, `Diff` blocks merge).
  Measured on main: **111 matched / 65 skipped / 0 diffed** (this count is from
  the CI comment and may predate the most recent fixture recordings — the plan
  re-captures it in Phase 0).
- **Known fixture staleness (verified 2026-08-14):** the recorded
  `cache_aligner` fixtures capture the pre-PR-A2 system-prompt *rewrite* path
  (a `[Dynamic Context]` block in `output.messages`) that was deliberately
  removed — current Python `headroom/transforms/cache_aligner.py` is
  detector-only. The recorded `ccr` fixtures include a `query` input property
  the current `create_ccr_tool_definition` no longer emits, and were recorded
  on the sticky-on path. Both fixture sets require re-recording before their
  comparators can be promoted (implementation plan Phase 1).

### 1.4 What the REALIGNMENT work already delivered

The REALIGNMENT plan (phases A–G, drafted 2026-05-01) is largely implemented —
the branches exist and their code is in-tree:

- `realign-A*` / `realign-phase-AB` — cache-safety lockdown + live-zone engine
- `realign-C1`…`realign-C5` — Rust SSE parser, chat completions, Responses
  HTTP + streaming, responses-converter retirement
- `realign-D1`…`realign-D4` — native Bedrock invoke + EventStream, Vertex
  native routes, observability
- `realign-E1`…`realign-E6` — tool/schema sort, Anthropic cache_control,
  OpenAI prompt-cache-key, volatile detector, cache-drift telemetry
- `realign-F1`…`realign-F4` — auth-mode classification + policy gates, TOIN
  per-tenant, trusted-forwarded-only
- `realign-G1`…`realign-G3` — wrap-agent breadth, `tokens_saved_rtk`, metrics
- `realign-I6` — per-PR parity gate

The Rust proxy now has native Bedrock/Vertex, SSE, WebSocket, cache
stabilization, and observability in-tree. The Python proxy remains the
**booted** backend only because the operator switch and deletion have not been
executed.

---

## 2. What remains

1. **Parity proof** — re-record the stale `cache_aligner`/`ccr` fixtures,
   promote their comparators, and confirm the Rust proxy is byte-equivalent on
   the request paths it claims to own (shadow test).
2. **Backend switch** — `headroom proxy start` (Python Click CLI,
   `headroom/cli/proxy.py`, 1,638 LOC) currently boots the Python FastAPI
   server (`headroom/proxy/server.py`, 5,933 LOC; handlers add ~25.7K LOC). It
   gains `HEADROOM_PROXY_BACKEND={python|rust}` and spawns the Rust binary.
   The `wrap` launcher (`headroom/cli/wrap.py`, `_start_proxy_process`) spawns
   `python -m headroom.cli proxy` directly and must route through the same
   switch; the `/health` `config` block (read by
   `headroom/providers/copilot/wrap.py:detect_running_proxy_backend` and
   `headroom/cli/wrap.py:_agent_savings_config_mismatches`) must stay stable
   across backends.
3. **Retirement** — delete the Python request path once parity is proven:
   `headroom/proxy/*` (server, handlers, interceptors, policies, cost, rate
   limiter, request logger, prometheus metrics), request-path
   `headroom/transforms/*` Python, `headroom/backends/litellm.py`, semantic
   cache, memory request-path modules, and the proxy-only test files
   (`tests/test_proxy_*.py` alone number 66; the full delete list is measured
   in the plan's Phase 0).
4. **Ops cutover** — Dockerfile/docker-compose entrypoint → Rust binary
   (target image ~50 MB vs ~500 MB today), README/wiki/docs refresh, operator
   migration guide, CHANGELOG breaking entry.
5. **Documentation + PR** — this analysis, the design doc, and the
   implementation plan, packaged as a documentation PR to the fork's public
   `main`.

## 3. What survives in Python (off-path)

| Area | Reason it stays |
|---|---|
| `headroom/cli/*` | Click launchers: wrap, evals, init, install, learn, memory, perf, proxy, tools |
| `headroom/rtk/installer.py` | RTK binary downloader |
| `headroom/providers/{codex,claude}/install.py` | Client config installation |
| `headroom/evals/`, `learn/`, `memory/` writers | Research / batch tooling |
| `headroom/tokenizers/` | Parity backstop |
| `headroom/telemetry/toin.py` | TOIN learning loop (observation-only) |
| `headroom/subscription/` | Subscription usage poller |
| `headroom/copilot_auth.py` | Copilot OAuth refresh |

## 4. Source of truth

- Design doc: `docs/superpowers/specs/2026-08-14-finish-python-to-rust-conversion-design.md`
- Implementation plan: `docs/superpowers/plans/2026-08-14-finish-python-to-rust-conversion.md`
- REALIGNMENT index: `REALIGNMENT/INDEX.md` (phases A–G done; Phase H superseded by the plan's Phase 3)
- Operator migration guide: `docs/operations/python-to-rust-migration.md`
