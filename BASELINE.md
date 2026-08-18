# BASELINE — Measured Phase 0 Baseline (Python → Rust conversion)

**Date measured:** 2026-08-17
**Tree measured:** branch `realign-P0-baseline` @ `bbe90131` (`deps: bump tokio-tungstenite from 0.24.0 to 0.30.0 (#2967)`)
**Machine:** macOS (arm64), Python 3.13.13 (`.venv`), cargo 1.95.0
**Plan:** `docs/superpowers/plans/2026-08-14-finish-python-to-rust-conversion.md` (Task 0.1)
**Owner:** fork maintainer (`Ayyankhan101/Headroom-in-Rust`)

Every number below was **measured from this tree**, not estimated. Later PRs (Phases 1–4) diff against this file as the single source of truth.

---

## 0. Upstream-sync decision (record)

- The plan's Phase 0 preamble records the 2026-08-14 sync: 27 commits, clean fast-forward onto `origin/main`, applied **before** the plan ran (plan docs commit on `realign-H0-doc-finish-python-to-rust-conversion`, whose base is `e269afb9` = `fix(ci): unjam release and Docker publishing (#2958)`).
- **Decision (2026-08-17, user):** the baseline is measured in the `realign-P0-baseline` worktree, which sits **27 commits ahead** of `e269afb9` on a newer `origin/main`. The worktree's `origin/main` ref resolves to `e269afb9`; the branch head `bbe90131` includes 27 additional upstream commits (e.g. #2964, #2967, #2993, #3009, #3012).
- **Impact on the plan's recorded numbers:** the delete list differs from the plan's illustrative figures. `headroom/proxy/` changed by **12 files, +1,460/−432** between `e269afb9` and `bbe90131`. The measurements below **supersede** the plan's numbers wherever they differ (notably: proxy 104 files / 49,446 LOC vs the plan's 93 / 28,531; `server.py` 5,943 vs 5,933; handlers 20,212 LOC vs ~20,212 — matches; `headroom/` total 205,536 vs 203,362).

---

## 1. Codebase totals (measured)

| Metric | Value | Command |
|---|---|---|
| Python source LOC (`headroom/`) | **205,536** | `find headroom -name '*.py' -exec cat {} + \| wc -l` |
| Python source modules (`headroom/`) | **521** | `find headroom -name '*.py' \| wc -l` |
| Python test LOC (`tests/`) | **241,429** | `find tests -name '*.py' -exec cat {} + \| wc -l` |
| Python test modules (`tests/`) | **804** | `find tests -name '*.py' \| wc -l` |
| Rust source LOC (`crates/`) | **79,375** | `find crates -name '*.rs' -exec cat {} + \| wc -l` |
| Rust source modules (`crates/`) | **197** | `find crates -name '*.rs' \| wc -l` |
| Rust proxy surface (`crates/headroom-proxy/src`) | **51 files / 20,539 LOC** | `find crates/headroom-proxy/src -name '*.rs' \| xargs wc -l \| tail -1` |
| Request-path Python transforms (`headroom/transforms/`) | **38 files / 23,299 LOC** | `find headroom/transforms -name '*.py'` |
| `headroom/transforms/cache_aligner.py` | **413 LOC** (detector-only) | `wc -l headroom/transforms/cache_aligner.py` |

---

## 2. Delete-list inventory (Phase 3 source of truth)

### 2.1 `headroom/proxy/` — **104 Python files / 49,446 LOC** (delete whole directory in PR-3.1)

Composition:

| Area | Files | LOC |
|---|---:|---:|
| Top-level `headroom/proxy/*.py` | 93 | 28,531 |
| `headroom/proxy/handlers/` | 8 | 20,212 |
| `headroom/proxy/interceptors/` | 3 | 703 |
| **Total** | **104** | **49,446** |

Largest files (measured):

| File | LOC |
|---|---:|
| `handlers/openai.py` | 9,930 |
| `server.py` | 5,943 |
| `handlers/anthropic.py` | 5,015 |
| `handlers/streaming.py` | 2,145 |
| `handlers/gemini.py` | 1,459 |
| `handlers/batch.py` | 1,289 |
| `handlers/bedrock.py` | 305 |
| `handlers/_debug_dump.py` | 47 |

Memory subsystem: **11 files / 4,461 LOC** (`memory_decision.py`, `memory_decision_policy.py`, `memory_golden_policy.py`, `memory_handler.py`, `memory_injection.py`, `memory_injection_mode_policy.py`, `memory_query.py`, `memory_query_policy.py`, `memory_rank_policy.py`, `memory_ranker.py`, `memory_tool_adapter.py`).
`semantic_cache.py` lives at **`headroom/proxy/semantic_cache.py`** (154 LOC) — not `headroom/semantic_cache.py`.

Full file list (the authoritative Phase 3 delete set for `headroom/proxy/`):

```
headroom/proxy/__init__.py
headroom/proxy/audit.py
headroom/proxy/auth_mode.py
headroom/proxy/auth_policy.py
headroom/proxy/background_compression.py
headroom/proxy/beta_header_merge.py
headroom/proxy/beta_header_policy.py
headroom/proxy/body_forwarding.py
headroom/proxy/budget_basis_policy.py
headroom/proxy/cc_switch_reconciler.py
headroom/proxy/ccr_golden_policy.py
headroom/proxy/ccr_marker_policy.py
headroom/proxy/ccr_session_tracker.py
headroom/proxy/compression_decision.py
headroom/proxy/cost.py
headroom/proxy/debug_introspection.py
headroom/proxy/diagnostic_decode_policy.py
headroom/proxy/extensions.py
headroom/proxy/forwarded_headers.py
headroom/proxy/forwarded_policy.py
headroom/proxy/handlers/__init__.py
headroom/proxy/handlers/_debug_dump.py
headroom/proxy/handlers/anthropic.py
headroom/proxy/handlers/batch.py
headroom/proxy/handlers/bedrock.py
headroom/proxy/handlers/gemini.py
headroom/proxy/handlers/openai.py
headroom/proxy/handlers/streaming.py
headroom/proxy/helpers.py
headroom/proxy/image_compression_decision.py
headroom/proxy/image_compression_policy.py
headroom/proxy/image_isolation.py
headroom/proxy/interceptors/__init__.py
headroom/proxy/interceptors/astgrep.py
headroom/proxy/interceptors/base.py
headroom/proxy/internal_header_policy.py
headroom/proxy/loop_callback_failure_policy.py
headroom/proxy/loopback_guard.py
headroom/proxy/memory_decision.py
headroom/proxy/memory_decision_policy.py
headroom/proxy/memory_golden_policy.py
headroom/proxy/memory_handler.py
headroom/proxy/memory_injection.py
headroom/proxy/memory_injection_mode_policy.py
headroom/proxy/memory_query.py
headroom/proxy/memory_query_policy.py
headroom/proxy/memory_rank_policy.py
headroom/proxy/memory_ranker.py
headroom/proxy/memory_tool_adapter.py
headroom/proxy/model_router.py
headroom/proxy/models.py
headroom/proxy/modes.py
headroom/proxy/outcome.py
headroom/proxy/output_effort_policy.py
headroom/proxy/output_savings.py
headroom/proxy/output_savings_policy.py
headroom/proxy/output_shaper.py
headroom/proxy/output_steering.py
headroom/proxy/output_turn_policy.py
headroom/proxy/output_verbosity_policy.py
headroom/proxy/passthrough.py
headroom/proxy/persistent_metrics.py
headroom/proxy/probe_recorder.py
headroom/proxy/project_context.py
headroom/proxy/project_name_policy.py
headroom/proxy/project_policy.py
headroom/proxy/prometheus_metrics.py
headroom/proxy/proxy_mode_policy.py
headroom/proxy/python_forwarder_mode_policy.py
headroom/proxy/query_log_policy.py
headroom/proxy/rate_limit_policy.py
headroom/proxy/rate_limiter.py
headroom/proxy/request_limit_policy.py
headroom/proxy/request_log_redaction_policy.py
headroom/proxy/request_logger.py
headroom/proxy/request_scope.py
headroom/proxy/route_advice.py
headroom/proxy/runtime_env.py
headroom/proxy/savings_attribution.py
headroom/proxy/savings_tracker.py
headroom/proxy/semantic_cache.py
headroom/proxy/semantic_cache_key.py
headroom/proxy/semantic_cache_key_policy.py
headroom/proxy/server.py
headroom/proxy/sse_byte_buffer_policy.py
headroom/proxy/ssl_context.py
headroom/proxy/stage_timer.py
headroom/proxy/system_compaction.py
headroom/proxy/token_counting.py
headroom/proxy/tool_definition_serialization.py
headroom/proxy/tool_injection_config.py
headroom/proxy/tool_injection_logging.py
headroom/proxy/tool_injection_policy.py
headroom/proxy/tool_injection_tracker.py
headroom/proxy/tool_name_policy.py
headroom/proxy/tool_schema_compaction.py
headroom/proxy/tool_schema_savings_policy.py
headroom/proxy/turn_hooks.py
headroom/proxy/verbosity_controller.py
headroom/proxy/warmup.py
headroom/proxy/wire_debug_format_policy.py
headroom/proxy/wire_debug_redaction_policy.py
headroom/proxy/ws_headers.py
headroom/proxy/ws_session_registry.py
```

### 2.2 Other request-path deletions (PR-3.1 / PR-3.2)

| File | LOC | Phase |
|---|---:|---|
| `headroom/backends/litellm.py` | 1,597 | **Deleted PR-3.2** (kept `base.py` / `anyllm.py` / `__init__.py` — see §3) |
| `headroom/transforms/cache_aligner.py` | 413 | PR-3.1 (Rust port lands in Phase 1 Task 1.4) |
| `headroom/transforms/*.py` shims | see §3 | PR-3.1 (request-path subset only) |

### 2.3 Proxy-only tests

- `tests/test_proxy*.py`: **66 files** (top-level)
- `tests/test_proxy/`: **32 files**

Full list (66 top-level `test_proxy_*.py`):

```
tests/test_proxy_anthropic_cache_stability.py
tests/test_proxy_anthropic_compression_diagnostics.py
tests/test_proxy_anthropic_model_sanitization.py
tests/test_proxy_batch_integration.py
tests/test_proxy_byte_faithful_forwarding.py
tests/test_proxy_cache_telemetry.py
tests/test_proxy_cache_ttl_metrics.py
tests/test_proxy_ccr.py
tests/test_proxy_codex_route_aliases.py
tests/test_proxy_compress_endpoint.py
tests/test_proxy_compression_executor.py
tests/test_proxy_compression_headers.py
tests/test_proxy_config_qdrant_port.py
tests/test_proxy_config_rate_limit.py
tests/test_proxy_copilot_auth_hooks.py
tests/test_proxy_cors.py
tests/test_proxy_count_tokens_integration.py
tests/test_proxy_dashboard_stats_cache.py
tests/test_proxy_debug_endpoints.py
tests/test_proxy_disable_kompress.py
tests/test_proxy_eager_preload_bind.py
tests/test_proxy_extensions.py
tests/test_proxy_favicon_route.py
tests/test_proxy_gemini_integration.py
tests/test_proxy_gemini_native_integration.py
tests/test_proxy_google_cloudcode_route_aliases.py
tests/test_proxy_handler_helpers.py
tests/test_proxy_handlers_batch.py
tests/test_proxy_hardening.py
tests/test_proxy_health.py
tests/test_proxy_healthchecks.py
tests/test_proxy_hooks_regression.py
tests/test_proxy_loop_exception_health.py
tests/test_proxy_loopback_gating.py
tests/test_proxy_memory_integration.py
tests/test_proxy_mode_benchmark.py
tests/test_proxy_mode_policy.py
tests/test_proxy_modes.py
tests/test_proxy_openai.py
tests/test_proxy_openai_cache_key_integration.py
tests/test_proxy_openai_cache_stability.py
tests/test_proxy_openai_responses_bypass.py
tests/test_proxy_openai_responses_integration.py
tests/test_proxy_openai_responses_stream_ccr.py
tests/test_proxy_package_init.py
tests/test_proxy_passthrough.py
tests/test_proxy_passthrough_integration.py
tests/test_proxy_passthrough_transient_retry.py
tests/test_proxy_per_provider_kompress.py
tests/test_proxy_pipeline_lifecycle.py
tests/test_proxy_project_savings.py
tests/test_proxy_request_scope.py
tests/test_proxy_retry_429.py
tests/test_proxy_savings_history.py
tests/test_proxy_scalability.py
tests/test_proxy_semantic_cache_key.py
tests/test_proxy_semantic_cache_key_integration.py
tests/test_proxy_semantic_cache_key_policy.py
tests/test_proxy_settings_endpoints.py
tests/test_proxy_stats_recent_requests.py
tests/test_proxy_streaming_ratelimit_headers.py
tests/test_proxy_streaming_request_logger.py
tests/test_proxy_streaming_resilience.py
tests/test_proxy_system_prompt_immutable.py
tests/test_proxy_telemetry_env.py
tests/test_proxy_warmup.py
```

`tests/test_proxy/` (32 files): `test_anthropic_buffered_timeout.py`, `test_anthropic_ccr_deferred_injection.py`, `test_anthropic_ccr_raise.py`, `test_anthropic_recount_and_reparse_safety.py`, `test_anthropic_streaming_ccr_retrieve.py`, `test_anthropic_upstream_header.py`, `test_background_compression.py`, `test_bedrock_passthrough.py`, `test_bedrock_sse_ping.py`, `test_cc_switch_reconciler.py`, `test_ccr_frozen_prefix_coupling.py`, `test_compression_failure_action.py`, `test_compression_timeout_config.py`, `test_compute_turn_id.py`, `test_gemini_savings_profile.py`, `test_header_safe_transforms.py`, `test_mcp_stats_aggregation.py`, `test_model_router.py`, `test_model_router_wiring.py`, `test_openai_backend_path.py`, `test_openai_chat_ccr_injection.py`, `test_openai_chat_savings_profile.py`, `test_openai_responses_ccr.py`, `test_openai_stream_usage_option.py`, `test_openai_transport_path_prefix.py`, `test_openai_upstream_header.py`, `test_phase3_byte_identity.py`, `test_request_logger.py`, `test_settings_fresh_process_precedence.py`, `test_settings_store.py`, `test_tool_search_repair_after_turn_hooks.py`, `test_transformations_feed.py`.

> PR-3.1 note: the plan's "keep ~40" surviving tests refers to tests that exercise CLI wrappers, RTK, evals, learn, memory writers, tokenizers — confirm each `test_proxy*` file against the §3 consumer map before deleting; some may exercise surviving modules.

---

## 3. `headroom._core` consumer map (Rust dependency surface)

Modules that import `headroom._core` (measured: `grep -rln "headroom\._core\|headroom._core" headroom/` → 13 modules):

| Module | Classification | Off-path importers (must survive / be updated) |
|---|---|---|
| `headroom/__init__.py` | **Survives** | — |
| `headroom/_ort.py` | **Survives** | imported only by `transforms/content_router.py` |
| `headroom/cli/update.py` | **Survives** | CLI |
| `headroom/cli/wrap.py` | **Survives** | CLI (modified in Phases 2/3 — backend switch + spawn) |
| `headroom/proxy/server.py` | **Deleted PR-3.1** | — |
| `headroom/transforms/content_router.py` | **Shared — decision needed** | `headroom/evals/__init__.py`, `evals/adversarial_grid.py`, `evals/batch_compression_eval.py`, `evals/runners/compression_only.py`, `evals/runners/before_after.py` |
| `headroom/transforms/diff_compressor.py` | **Shared — decision needed** | `transforms/__init__.py` (proxy + evals surface) |
| `headroom/transforms/error_detection.py` | Request-path | imported by `transforms/search_compressor.py` |
| `headroom/transforms/log_compressor.py` | **Shared — decision needed** | `transforms/__init__.py` |
| `headroom/transforms/search_compressor.py` | Request-path | `transforms/__init__.py` |
| `headroom/transforms/smart_crusher.py` | **Shared — decision needed** | `headroom/evals/core.py`, `evals/runners/compression_only.py`, `evals/runners/before_after.py`, `integrations/langchain/langgraph.py`, `integrations/mcp/server.py` |
| `headroom/transforms/tag_protector.py` | Request-path | none found outside `headroom/proxy/` + `transforms/` |
| `headroom/transforms/text_crusher.py` | Request-path | none found outside `headroom/proxy/` + `transforms/` |

**Phase 3 implication:** the transform shims are thin delegates to Rust (`headroom._core`); several are imported by **surviving** off-path consumers (`evals/`, `integrations/`). PR-3.1 must either keep those shims or update the off-path consumers — do not delete them blindly. `headroom/_ort.py` survives because `content_router.py` (used by evals) imports it.

Also verified: `headroom/backends/base.py` is imported by surviving `headroom/cache/compression_store.py` and `headroom/telemetry/toin.py` → **keep `backends/base.py`, `anyllm.py`, `__init__.py`** in PR-3.2; delete only `litellm.py`.

---

## 4. Parity baseline (measured `make test-parity`)

Command: `make test-parity` (= `cargo run -p headroom-version-parity -- run --fixtures tests/parity/fixtures`).
**Requires `ORT_DYLIB_PATH` on this machine — see Finding F1.**

| Comparator | total | matched | skipped | diffed |
|---|---:|---:|---:|---:|
| log_compressor | 20 | 20 | 0 | 0 |
| diff_compressor | 27 | 27 | 0 | 0 |
| cache_aligner | 25 | 25 | 0 | 0 |
| tokenizer | 40 | 40 | 0 | 0 |
| ccr | 31 | 31 | 0 | 0 |
| smart_crusher | 17 | 17 | 0 | 0 |
| content_detector | 21 | 21 | 0 | 0 |
| text_crusher | 6 | 6 | 0 | 0 |
| kompress | 21 | 21 | 0 | 0 |
| code_aware_compressor | 30 | 30 | 0 | 0 |
| **Total** | **238** | **238** | **0** | **0** |

**Zero `Skipped` on request-path transforms** — the Phase 1 exit gate. `cache_aligner` (25) and `ccr` (31) were promoted from `stub_comparator!` to real comparators in Tasks 1.3/1.4 and now match, and the previously-real comparators still match. The re-recorded fixture sets grew the totals vs. the Phase 0 baseline (cache_aligner 20→25, ccr 25→31, grand total 227→238). The plan's "111 matched / 65 skipped" CI comment is stale relative to this tree.

### Finding F1 — kompress harness deadlock without an ONNX Runtime dylib

- **Symptom:** with the `kompress-v2-base` ONNX model present in the local HF cache (`~/.cache/huggingface/hub/models--chopratejas--kompress-v2-base`), `parity-run` **hangs indefinitely** on kompress (0% CPU, sleeping in `ort::load_dylib_from_path` → re-entrant `Once` → `semaphore_wait_trap`). The same hang hits `cargo test --workspace` (`tests/kompress_parity.rs`).
- **Why CI is unaffected:** CI (ubuntu-latest) has no cached model → `hf_cache_file` returns `None` → comparator returns `Err` → fixtures `Skipped` fast, before `Session::builder()` is ever reached. Locally the cached model reaches `build_session` → ort dylib load → deadlock.
- **Fix (used for all gate runs below):** `pip install onnxruntime` into the venv, then
  `export ORT_DYLIB_PATH=$PWD/.venv/lib/python3.13/site-packages/onnxruntime/capi/libonnxruntime.1.28.0.dylib`.
- **Confirmed in Task 1.5:** the "slow kompress tail" is this deadlock, not slow inference — without `ORT_DYLIB_PATH` set, `parity-run` and `cargo test --workspace` both stall on kompress indefinitely. With it set, kompress completes (21/21 matched in parity; `kompress_matches_python_fixtures_byte_for_byte` passes in 6.56s under cargo test).
- **Resolved (2026-08-17):** `Kompress::from_files`'s session builder now runs the same ort dylib guard magika uses (`magika_detector::dynamic_ort_loader_ready`) before any ort API is touched, so a missing dylib surfaces as a loud `Err` ("ONNX Runtime unavailable: ... set ORT_DYLIB_PATH") instead of a 0%-CPU hang. `KompressComparator` propagates that error as the Skipped reason. Regression tests: `crates/headroom-core/tests/kompress_ort_fail_loud.rs` and `kompress_comparator_fails_loud_when_dylib_missing` in `crates/headroom-version-parity`. Any dev machine with the model cached still needs the env var (documented in `docs/operations/python-to-rust-migration.md`), but the failure mode is now a clear error, not a hang.

---

## 5. Gate results (Step 4)

| Gate | Command | Result |
|---|---|---|
| Rust format | `cargo fmt --all -- --check` | ✅ clean |
| Rust lint | `cargo clippy --workspace -- -D warnings` | ✅ clean (40.55s) |
| Rust tests | `cargo test --workspace` (with `ORT_DYLIB_PATH`) | ✅ Phase 0: **1,492 passed / 0 failed**; Task 1.5 re-run: **1,512 passed / 0 failed** (kompress parity test now completes in 6.56s with `ORT_DYLIB_PATH`) |
| Python subset (plan list) | `pytest -x test_smart_crusher_bugs.py test_smart_crusher_rust_parity.py test_ccr.py test_acceptance.py` | ✅ Phase 0: **49 passed** (7.94s) |
| Python subset (Task 1.5 list) | `pytest -x test_ccr.py test_smart_crusher_rust_parity.py test_acceptance.py` | ✅ **43 passed** (1.96s) |
| Python subset (full `ci-precheck-python` list) | 11 files incl. relevance/critical_fixes/quality_retention/toin | ✅ Phase 0: **175 passed / 4 skipped** (5.88s) |
| Parity (Task 1.5) | `make test-parity` (with `ORT_DYLIB_PATH`) | ✅ **238 matched / 0 skipped / 0 diffed** — zero `Skipped` on request-path transforms |
| commitlint | `npx @commitlint/cli --from origin/main --to HEAD` | ⏳ fails **only** on the 27 upstream-sync commits in `origin/main..HEAD` (104-char headers on `7de35739`/`d6d121e3`, 15 footer-length violations, 1 empty-type merge). All 5 Task 0.1–1.4 commits pass individually; CI's `commitlint` job (wagoid/commitlint-github-action) lints only PR commits, so the sync commits never enter the check once the branch is based on the synced `origin/main`. Verified at commit time. |

Python env used: `.venv` (Python 3.13.13) + `pip install -e .` (maturin-built `headroom._core` extension) + `pytest pytest-asyncio pytest-cov respx httpx onnxruntime`. The 4 skipped pytest tests are pre-existing optional-dependency skips, not regressions.

---

## 6. Phase 0 exit gate — status

- ✅ `cargo test --workspace` green (1,492 passed)
- ✅ `make test-parity` baseline captured (182 matched / 45 skipped / 0 diffed; skipped = exactly the 2 stubs)
- ✅ Surviving-Python pytest green (49 passed plan list; 175 passed / 4 skipped full ci-precheck list)
- ✅ Delete list locked (this file, §2) — measured against `bbe90131`, superseding the plan's illustrative figures
- ✅ Upstream-sync decision recorded (§0)
- ✅ `make ci-precheck` components: fmt ✅, clippy ✅, rust tests ✅, python tests ✅, commitlint ⏳ (verified at commit)

## 6b. Phase 1 exit gate — status (Task 1.5, verified 2026-08-17)

- ✅ **`make test-parity` has zero `Skipped` on request-path transforms** — 238/238 matched, 0 skipped, 0 diffed (cache_aligner 25/25, ccr 31/31, kompress 21/21 with `ORT_DYLIB_PATH`)
- ✅ Per-PR CI gate confirmed live: `parity` job in `.github/workflows/rust.yml:244`, no `continue-on-error`, runs `make test-parity`, fails on `Diff`
- ✅ `cargo test --workspace` green — 1,512 passed / 0 failed (with `ORT_DYLIB_PATH`; kompress parity test passes in 6.56s)
- ✅ Python subset green — 43 passed (test_ccr.py + smart_crusher_rust_parity + acceptance)
- ✅ `make ci-precheck` components: fmt ✅ (after rustfmt fix for Task 1.3/1.4 code), clippy ✅, rust tests ✅, python tests ✅, commitlint fails only on upstream-sync commits (see §5)
- ⚠️ **Gap found & fixed in Task 1.5:** Tasks 1.3/1.4 shipped un-rustfmt'd code (`ccr/tool_injection.rs`, `cache_aligner.rs`, `headroom-version-parity/src/lib.rs` tests) — `cargo fmt --all` applied; the fix rides in the Task 1.5 commit.

## 6c. Phase 3 PR-3.2 exit gate — status (Task 3.2, verified 2026-08-18)

PR-3.2 retired the LiteLLM **proxy backend** in 4 code commits (backends, providers, cli, tests) + this docs commit:

- ✅ `headroom/backends/litellm.py` deleted (1,597 LOC); `backends/__init__.py` now exports only `base`/`anyllm`
- ✅ `headroom/providers/registry.py` — removed `litellm_backend_cls` param, the litellm branch of `create_proxy_backend` (now warns + returns `None`), `_load_litellm_backend`, and the litellm path of `format_backend_status`
- ✅ CLI surface swept — `--backend` help texts in `headroom/cli/proxy.py` + `headroom/cli/wrap.py` no longer advertise `litellm-*` (10 strings, incl. the aider example)
- ✅ Backend-only tests deleted (8 files: `test_litellm_{caller_key,nonstream_cache_usage,openai_passthrough,upstream_timeout}.py`, `test_bedrock_{region,tool_result_cache_and_streaming_stats}.py`, `test_backends/test_{bedrock_botocore_preflight,litellm_cache_stats}.py`); registry tests trimmed to anyllm-only in `test_provider_registry*.py` (structured-failure coverage of `_log_backend_init_failure` preserved via the anyllm branch)
- ✅ Deleted module unreferenced: `git grep -E "backends\.litellm|LiteLLMBackend|litellm_backend_cls" headroom/ tests/` → nothing
- ✅ Gates: `pytest` on `test_provider_registry*.py` + `test_ccr.py` + `test_litellm_optional.py` → 39 passed; surviving litellm-dependent off-path tests (pricing, vertex provider, savings-tracker) → 26 passed; `cargo test --workspace` → 1,519 passed / 0 failed (with `ORT_DYLIB_PATH`, see F1); `make test-parity` → 238/238 matched, 0 skipped, 0 diffed
- ⚠️ **Amended gate (vs plan text):** the plan's literal exit gate — "no `litellm` in runtime deps or non-test code" — cannot hold on the measured tree: litellm is a **gated core dep** (`python_version < 3.14`, GH #956) consumed by surviving off-path code (`providers/litellm.py` + `providers/__init__.py` re-export, `integrations/litellm_callback.py` + langchain, `pricing/`, `models/registry.py`, `perf/analyzer.py`, `proxy/savings_tracker.py`). **Amended gate:** no litellm in the request path or proxy-backend routing — `git grep -i litellm headroom/` hits are limited to the surviving off-path integrations above. `pyproject.toml` unchanged for PR-3.2 (dropping the dep would break pricing/providers today; a future off-path retirement can revisit it).

## 6d. Phase 3 PR-3.3 exit gate — status (Task 3.3, verified 2026-08-18)

PR-3.3 finished the retirement in 5 code commits + this docs commit:

- ✅ **Q12 — crate rename + contract reframe (2 commits):** `crates/headroom-parity/` → `crates/headroom-version-parity/` (`git mv`; package name, workspace `members`/`default-members`, `Makefile test-parity`, `.github/workflows/rust.yml` comment, `crates/headroom-core` comments, `tests/parity/record_smart_crusher.py` + `tests/test_transforms/test_*_rust_parity.py` docstrings, `examples/diff_fixture.rs`). Comparator contract reframed to version parity: `ComparisonOutcome::Diff { previous, current }` (was `expected/actual`), module doc now frames fixtures as frozen *previous-version* outputs guarding future Rust compressor variants (e.g. a Kompress port); `parity-run` bin about-text and diff labels updated. Lockfile regenerated.
- ✅ **Orphan cleanup (1 commit):** deleted the retired shadow harness `e2e/shadow/` (`runner.py`, `corpus.py`, `test_byte_equality.py`), its sole fixture corpus `tests/parity/fixtures/codex_openai_contracts/`, and the stale `docs/operations/shadow-deploy.md`. Zero remaining references (CI retired the shadow gate in PR-3.1; Python request path is gone so Python-vs-Rust shadowing is impossible).
- ✅ **Docs sweep (1 commit):** `RUST_DEV.md` → `DEV.md` (`git mv` + internal `headroom-parity` refs); code docstring refs in `headroom/transforms/observability.py` + `smart_crusher.py`; `wiki/proxy.md` + `docs/content/docs/proxy.mdx` gunicorn/uvicorn production sections replaced with the Rust binary; `docs/content/docs/litellm.mdx` callout updated (proxy `--backend` LiteLLM routing retired, callback integration survives); CHANGELOG **Breaking** entry added under Unreleased.
- ✅ **Amended gate (vs plan text):** the plan's PR-3.3 "clean sweep — `git grep -i uvicorn|fastapi|litellm headroom/` nothing in non-test code" cannot hold on the measured tree: uvicorn/fastapi remain as comments / `TYPE_CHECKING` imports in surviving off-path modules (`proxy/models.py`, `proxy/request_scope.py`, `proxy/helpers.py`, `memory/traffic_learner.py`, `cli/wrap.py`) and litellm remains in the §6c off-path integrations. `pyproject.toml` unchanged: PR-3.1 already moved fastapi/uvicorn out of runtime deps; the `[proxy]` extra's remaining deps serve surviving MCP/integrations/evals code and the gated litellm dep (§6c) stays. The honest gate is: **no uvicorn/fastapi/litellm in the request path or proxy-backend routing** — grep hits limited to comments/TYPE_CHECKING and §6c integrations.
- ✅ **Gates:** `cargo test --workspace` → 1,519 passed / 0 failed (with `ORT_DYLIB_PATH`); `make test-parity` → 238/238 matched, 0 skipped, 0 diffed via the renamed crate; `make ci-precheck` → fmt ✅, clippy ✅, rust tests ✅, python tests ✅, commitlint ⏳ (only the pre-existing upstream-sync header/footer violations documented in §5; all 6 PR-3.3 commits pass individually).

## 7. Notes for later phases

1. **`/dashboard` deferred to PR-3.1** (plan Global Constraints): the Python server serves an operator web UI (`/dashboard`, `/settings`, `headroom/dashboard/templates/`) with no Rust equivalent. The PR must decide: document the retirement or port a minimal Rust dashboard.
2. **`ORT_DYLIB_PATH` is required on dev machines with the kompress model cached** (Finding F1). Add to Phase 2/3 docs and dev setup; fix the deadlock in a follow-up PR.
3. **Shared transform shims** (`content_router.py`, `smart_crusher.py`, `log_compressor.py`, `diff_compressor.py`) are used by surviving `evals/` + `integrations/` code — PR-3.1 must keep them or update those consumers (§3).
4. **Phase 3 gate command** `git grep -i "uvicorn\|fastapi\|litellm" headroom/` — after PR-3.1/3.2 the remaining hits are comments / TYPE_CHECKING imports in surviving off-path modules (`memory/traffic_learner.py`, `proxy/models.py`, `proxy/request_scope.py`, `proxy/helpers.py`, `cli/wrap.py`) and the off-path litellm integrations listed in §6c. A full litellm sweep is **not** in PR-3.3's scope — it would retire pricing/provider/integration surface that survives.
5. **Rust-side parity baseline for later phases:** **done in PR-3.3 (Q12)** — `crates/headroom-parity/` repurposed to `crates/headroom-version-parity/` (version-parity against the frozen fixture outputs; see §6d).
