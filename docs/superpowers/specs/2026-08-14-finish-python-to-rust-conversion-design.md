# Finish the Python → Rust Conversion — Design

**Status:** Approved (2026-08-14)
**Owner:** fork maintainer (`Ayyankhan101/Headroom-in-Rust`)
**Upstream:** `headroomlabs-ai/headroom`
**Deliverable:** a fully documented PR to the fork's public `main` branch containing the analysis, this design, and the detailed implementation plan — making the fork's "Rust-first version" of Headroom visible to upstream and the public.

---

## 1. Context and current-state analysis

Headroom is an LLM context-optimization proxy. The codebase is mid-migration from Python to Rust. Measured state on 2026-08-14:

| Metric | Value |
|---|---|
| Python source LOC (`headroom/`) | ~203,362 LOC / 520 modules |
| Rust source LOC (`crates/`) | ~79,375 LOC / 197 modules |
| Python test LOC (`tests/`) | ~237,307 LOC |
| Rust test LOC (in-crate) | ~14,449 LOC |

### What is already in Rust (largely complete)

- **`crates/headroom-core`** — shared types + transform surface: SmartCrusher, Diff/Log/Code/Kompress (with `onnx_cpu.rs`) /TextCrusher compressors, tokenizer, CCR with persistent backends (in-memory / sqlite / redis), `signals/` trait module, `auth_mode`, `rollout`, `cache_control`, `compression_policy`.
- **`crates/headroom-proxy`** — the request-path proxy: Anthropic/OpenAI/Responses/streaming handlers, byte-faithful SSE parser, WebSocket, native Bedrock (SigV4) and Vertex (ADC) routes, cache stabilization, Prometheus/OTel observability, `responses_items.rs`.
- **`crates/headroom-py`** — PyO3 cdylib exposing `headroom._core`; Python transform modules (e.g. `smart_crusher.py`) are thin shims that delegate to Rust.
- **`crates/headroom-parity`** — Rust-vs-Python oracle harness with 11 fixture sets; 8 comparators real, 2 stubs (`cache_aligner`, `ccr`), 1 set (`codex_openai_contracts`) reviewed.
- **`crates/headroom-simulators`** — offline upstream simulators for tests.

The REALIGNMENT plan (phases A–G, drafted 2026-05-01) is largely implemented: upstream branches `realign-C1`…`realign-G3`, `realign-I6`, `realign-phase-AB` exist; the Rust proxy now has native Bedrock/Vertex, SSE, WebSocket, and cache-stabilization code in-tree.

### What remains (the scope of this plan)

1. **Parity proof** — promote the last stubbed comparators (`cache_aligner`, `ccr`) and confirm the Rust proxy is byte-equivalent on the request paths it claims to own.
2. **Backend switch** — `headroom proxy start` (Python CLI) currently boots the Python FastAPI server (`headroom/proxy/server.py`, ~2,864 LOC; handlers total ~25.7K LOC). It must gain a `HEADROOM_PROXY_BACKEND={python|rust}` switch and spawn the Rust binary.
3. **Retirement** — delete the Python request path once parity is proven: `headroom/proxy/*`, request-path `headroom/transforms/*`, `headroom/backends/litellm.py`, semantic cache, memory request-path, and ~150 proxy-only test files.
4. **Ops cutover** — Dockerfile/docker-compose entrypoint → Rust binary; README/wiki/docs refresh; operator migration guide; CHANGELOG breaking entry.
5. **Documentation + PR** — everything above documented and packaged as a PR to the fork's public `main`.

### Constraints and conventions

- Single contributor, sequential phases, each phase gated by verification.
- Project conventions: `fix:` commit prefix for migration commits (semantic-release), no `Co-Authored-By: Claude` trailer, `make ci-precheck` before push, byte-faithful passthrough invariant, no silent fallbacks (fail-loud).
- The fork is currently **26 commits behind upstream `main`** — syncing is a prerequisite decision before Phase 4 (docs describe the current tree).
- Verification gates must be runnable: `cargo test --workspace`, `make test-parity`, `pytest -x` (surviving Python), `make ci-precheck`.

---

## 2. Goals and non-goals

### Goals

- Make the Rust proxy the **only** request-path implementation, with proof it is byte-equivalent to the Python proxy on supported traffic before Python is deleted.
- Retain Python only where it is the right tool: CLI wrappers, evals, learn, memory writers, tokenizers (parity backstop), TOIN, subscription tracking, Copilot auth — all off-path.
- Produce a complete, professional, reviewable documentation set and package it as a PR to `main`.
- Keep every deletion revertible; keep a rollback path (backend switch + retained images) through a canary period.

**PR scope (confirmed with owner):** this PR is **documentation-only** — it ships the analysis, design, and implementation plan so the fork's "Rust-first version" is visible for review. The migration *code* work (Phases 0–4) is executed in subsequent PRs that follow this plan.

### Non-goals

- Porting the surviving off-path Python (CLI, evals, memory writers, TOIN) to Rust in this effort — explicitly out of scope per the approved end state.
- Rewriting or re-architecting Rust code that already works; this plan only ports what is missing, deletes what is proven redundant, and cuts over.
- The 26-commit upstream sync itself (a prerequisite decision, executed before Phase 4 if greenlit).

---

## 3. End-state architecture

```
Request path (100% Rust):
  headroom-proxy (binary) ──> headroom-core (library)
        │                        ├─ transforms (SmartCrusher, Diff, Log, Code, Kompress, TextCrusher)
        │                        ├─ tokenizer
        │                        ├─ ccr (in-memory / sqlite / redis backends)
        │                        ├─ signals (KeywordDetector + trait tiers)
        │                        ├─ auth_mode / rollout / cache_control / compression_policy
        └──> headroom-py (PyO3) ── only for off-path Python that needs transforms

Off-path (Python survives):
  headroom/cli/*          wrap launchers, evals, init, install, learn, memory, perf, tools
  headroom/rtk/installer  RTK binary downloader
  headroom/providers/*    codex/claude client config install
  headroom/evals, learn, memory writers, tokenizers, telemetry/toin, subscription, copilot_auth
```

**Deleted from the repo** (once parity is proven): `headroom/proxy/*` (server, handlers, interceptors, policies, cost, rate limiter, request logger, prometheus metrics), request-path `headroom/transforms/*` Python, `headroom/backends/litellm.py`, `semantic_cache.py`, memory request-path modules, ~150 proxy-only test files, proxy-only runtime deps (`fastapi`, `uvicorn`, etc. — kept in dev/test deps for the parity harness).

**Parity harness fate (decision Q12):** repurposed, not deleted — `headroom-parity` becomes a Rust-vs-Rust version-parity harness over the recorded fixtures, guarding future ML compressor changes (e.g. a Kompress port).

---

## 4. Phase structure

All phases sequential; each has a verification exit gate. Phases 3–5 map to REALIGNMENT Phase H (H1/H2/H3) refreshed against the current tree.

### Phase 0 — Baseline verification (grounding)

Inventory the *current* tree: exact delete list (files + LOC + import consumers), which Python modules the Rust proxy still depends on via `headroom._core`, and the current parity report. Run the gates once to establish the starting truth.

**Exit gate:** `cargo test --workspace` green, `make test-parity` report captured (know exactly which comparators are `Skipped`), `pytest -x` green for surviving-Python smoke list, delete list locked and reviewed.

### Phase 1 — Parity completion

Promote the last stubbed comparators: record `cache_aligner` and `ccr` fixtures (unblock `cache_aligner`'s tokenizer dependency first — see REALIGNMENT Phase 0 blockers), implement real comparators, and enable `make test-parity` as a per-PR gate (REALIGNMENT Q6, PR-I6).

**Exit gate:** `make test-parity` has no `Skipped` on request-path transforms; the gate is wired into CI.

### Phase 2 — Backend switch + canary

Add `HEADROOM_PROXY_BACKEND={python|rust}` (REALIGNMENT Q7) to `headroom/cli/proxy.py`. When `rust`, `headroom proxy start` spawns `./target/release/headroom-proxy` with the equivalent flags/env and forwards health checks. Add a byte-equality shadow test: the same recorded/real traffic through both backends must produce SHA-256-identical upstream bytes and responses.

**Exit gate:** both backends boot; shadow test green; rollback path (`HEADROOM_PROXY_BACKEND=python`) proven in the e2e canary.

### Phase 3 — Retirement (3 PRs, refreshed REALIGNMENT H1/H2/H3)

- **PR 3.1 (H1)** — delete the Python proxy request path (server, handlers, interceptors, policies, savings tracker, loopback guard, ws session registry, prometheus metrics, extensions, models, modes, stage timer, warmup, debug introspection, remaining `cache_aligner.py` stub). `headroom/cli/proxy.py` now always boots the Rust binary. Update Dockerfile/compose.
- **PR 3.2 (H2)** — delete `headroom/backends/litellm.py` + provider registry entries + `litellm` dependency (native Bedrock/Vertex Rust paths from Phase D are the only Bedrock/Vertex route).
- **PR 3.3 (H3)** — cleanup: orphaned modules, dead fixtures, stale docs, minimal `pyproject.toml`, final `Cargo.toml` workspace cleanup, CHANGELOG breaking entry, `RUST_DEV.md` → `DEV.md`.

Each PR: `fix:` commits split per-module for git-blame friendliness; gate = `cargo test --workspace` + `pytest -x` + `make test-parity` + `make ci-precheck` green.

**Exit gate:** `git grep -i "uvicorn\|fastapi" headroom/` returns nothing in non-test code; `headroom proxy start` boots the Rust binary; e2e canary passes.

### Phase 4 — Ops cutover + docs

Dockerfile entrypoint → Rust binary (single-stage, ~50 MB image target); docker-compose refresh; README/wiki/docs refresh; `docs/operations/python-to-rust-migration.md` operator guide; keep prior images for 30-day rollback.

**Exit gate:** `make ci-precheck` green; docs build; operator runbook verified end-to-end on a staging deploy.

### Phase 5 — Documentation + PR packaging

Produce the full documentation set (analysis, this design, the implementation plan, operator migration guide, decisions log), prepare the branch, and open the PR to `main`.

**Exit gate:** PR ready for review with all documentation attached.

---

## 5. PR & documentation strategy

- **Target:** the fork's public `main` (`origin/main`). The branch is cut from current `origin/main` (after the upstream-sync decision is resolved).
- **Branch naming (project convention):** `realign-<phase><num>-<slug>`, e.g. `realign-H0-doc-finish-python-to-rust-conversion`.
- **PR contents:**
  1. `docs/superpowers/specs/2026-08-14-finish-python-to-rust-conversion-design.md` (this document)
  2. The detailed implementation plan (writing-plans output) — phases 0–4, PR-by-PR with file lists, acceptance criteria, rollback
  3. The current-state analysis (Section 1 expanded: LOC tables, module inventory, what's already in Rust)
  4. `REALIGNMENT/` refresh — mark A–G done, supersede Phase H with the new Phase 3 PRs
  5. Operator-facing migration notes preview (final version lands in Phase 4)
- **Commits:** one logical commit per document; `docs:` prefix is acceptable for pure documentation (project convention keeps `fix:` for migration *code* commits — the migration code commits themselves land in later PRs, not this one).

---

## 6. Risks and rollback

| Risk | Mitigation |
|---|---|
| Deleting Python before Rust proves parity | Phase 0 proof + Phase 1 parity gate + Phase 2 shadow test are prerequisites for any deletion |
| Uncovered request path in Rust (policy, handler edge case, WS frame type) | Phase 2 canary with both backends live; `HEADROOM_PROXY_BACKEND=python` remains as emergency fallback |
| 26-commit fork drift makes docs stale | Upstream sync decided before Phase 4; docs describe the tree they ship with |
| Regression in surviving Python | Per-PR `pytest -x` gate over the surviving-module smoke list |
| Parity harness loses its oracle after Python is deleted | Repurposed to Rust-vs-Rust version parity (decision Q12) |
| Rollback of a retirement PR | Each PR is `git revert`-able; images retained 30 days post-cutover (REALIGNMENT H1) |

---

## 7. Decisions and open questions

**Decided (this session):**
- End state: full Rust request path; Python survives only off-path (Phase H list).
- Approach: Verify → Switch → Retire (approach A), single contributor, sequential.
- Deliverable: documented PR to the fork's public `main`.

**Carried from REALIGNMENT `12-decisions-needed.md` (need greenlight before the corresponding PR):**
- Q6 — enable `make test-parity` per-PR gate now (recommended: yes, Phase 1).
- Q7 — `HEADROOM_PROXY_BACKEND` env var with default flip to `rust` after canary (Phase 2).
- Q12 — repurpose parity harness to Rust-vs-Rust (Phase 3.3).
- New — upstream 26-commit sync: sync before Phase 4, or document the fork as intentionally diverged?
