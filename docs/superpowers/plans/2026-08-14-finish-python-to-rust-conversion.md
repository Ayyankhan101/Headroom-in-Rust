# Finish the Python → Rust Conversion — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Rust proxy the only request-path implementation — with proof of byte-equality, a backend switch for canary/rollback, deletion of the Python request path, and ops cutover — executed as a sequence of gated PRs.

**Architecture:** Verify → Switch → Retire. Phase 0 locks a measured baseline (exact delete list, `headroom._core` dependency surface, parity report). Phase 1 finishes parity (promote `cache_aligner` + `ccr` comparators; re-record stale fixtures). Phase 2 adds `HEADROOM_PROXY_BACKEND={python|rust}` to the Python CLI (covering both `proxy start` and the `wrap` spawn path) plus a byte-equality shadow test. Phase 3 deletes the Python request path in three revertible PRs (server/handlers, LiteLLM, cleanup). Phase 4 cuts ops over to the Rust binary and rewrites docs.

**Tech Stack:** Rust (headroom-core, headroom-proxy, headroom-parity via `cargo`), Python (Click CLI, pytest, PyO3 `headroom._core`), Docker, GitHub Actions (rust.yml), `make` gates (`test-parity`, `ci-precheck`).

## Global Constraints

- **Baseline is the synced tree.** The 27-commit upstream sync was executed 2026-08-14 (clean fast-forward onto `origin/main`) **before** this plan runs. Every inline file list below is **illustrative, not exhaustive** — the authoritative delete list, LOC counts, and `headroom._core` consumer map are measured fresh into `BASELINE.md` in Phase 0 Task 0.1 against the synced tree. Notable sync deltas the plan text predates: `headroom/proxy/` has **93 Python files / 28,531 LOC** (plan names only ~15); sync added `savings_attribution.py`, `outcome.py` (595 LOC), `turn_hooks.py`; Python handlers total **20,212 LOC** (not ~25.7K) and are `anthropic.py openai.py streaming.py gemini.py batch.py bedrock.py _debug_dump.py` (no `conversations.py`/`chat_completions.py` — those are Rust-side); `semantic_cache.py` lives at `headroom/proxy/semantic_cache.py` (not `headroom/semantic_cache.py`).
- **`/dashboard` fate deferred to Phase 3.** The Python server serves an operator web UI (`/dashboard`, `/settings`, static templates under `headroom/dashboard/templates/`) with no Rust equivalent. The decision to retire it (and document the loss) or port a minimal Rust dashboard is made in the Phase 3 PR, not in this plan.
- **Byte-faithful passthrough invariant** — any non-modifying request must round-trip byte-equal (SHA-256) through the proxy; no silent fallbacks, fail-loud only.
- **Commit convention** — `fix:` prefix for migration code commits (semantic-release); `docs:` allowed for pure documentation; no `Co-Authored-By: Claude` trailer.
- **Gates before push** — `make ci-precheck` must be green (cargo fmt/clippy/test + smart-crusher Python subset + commitlint); `make test-parity` per-PR (Skipped allowed, Diff blocks).
- **Upstream sync decided before Phase 0** — the 26-commit `HEAD..upstream/main` gap is resolved (merged or documented as intentional divergence) before any inventory is locked, so the delete list and docs describe the tree they ship with.
- **Rollback** — every retirement PR is `git revert`-able; pre-cutover images retained 30 days.
- **No regex in Rust for parsing** — realignment build-constraints policy (volatile detection uses explicit byte-position checks).

---

## Phase 0 — Baseline verification (grounding)

The upstream-sync decision (Section 7 of the design doc) has been **resolved and applied** (2026-08-14, 27 commits, clean fast-forward). Everything below is measured against the *synced* tree.

### Task 0.1: Lock the measured baseline

**Files:**
- Produce: `docs/operations/python-to-rust-migration.md` §"Current state" tables (created in Phase 4, filled from these measurements)
- Modify: none yet — this task is measurement-only

**Interfaces:**
- Produces: `BASELINE.md` (or equivalent notes file) with the numbers every later PR references.

- [ ] **Step 1: Capture the exact delete-list inventory**

Run and save output:

```bash
# Python request path — files + LOC
wc -l headroom/proxy/*.py headroom/proxy/handlers/*.py headroom/proxy/interceptors/*.py \
  headroom/backends/litellm.py headroom/semantic_cache.py headroom/proxy/memory_*.py 2>/dev/null

# Rust proxy surface for comparison
find crates/headroom-proxy/src -name '*.rs' | xargs wc -l | tail -1
```

Expected: **note that `headroom/proxy/server.py` is 5,933 lines** — the design doc's "~2,864" figure is stale (verified 2026-08-14). The plan's deletion PRs use the *measured* numbers.

- [ ] **Step 2: Map which Python modules import `headroom._core` (Rust dependency surface)**

```bash
grep -rln "headroom._core\|headroom\._core" headroom/ | sort > /tmp/py_depends_on_rust.txt
cat /tmp/py_depends_on_rust.txt
```

Expected: the surviving-Python list (cli, evals, learn, memory writers, tokenizers) plus any request-path modules that must keep working until Phase 3.

- [ ] **Step 3: Capture the current parity report**

```bash
make test-parity 2>&1 | tee /tmp/parity_baseline.txt
```

Expected: exactly two transforms report `skipped` for *all* their fixtures — `cache_aligner` (20) and `ccr` (25) — and everything else `matched`. Record the counts; the Phase 1 exit gate is "zero `Skipped` on request-path transforms."

- [ ] **Step 4: Run the gates to establish starting truth**

```bash
cargo test --workspace
# venv activated:
pytest -x tests/test_transforms/test_smart_crusher_bugs.py tests/test_transforms/test_smart_crusher_rust_parity.py tests/test_ccr.py tests/test_acceptance.py
make ci-precheck
```

Expected: all green. If any gate is already red, stop and fix before Phase 0 is considered complete (the plan's exit gates assume a green starting tree).

- [ ] **Step 5: Freeze the delete list**

Write `BASELINE.md` containing: the measured file list from Step 1, the `headroom._core` consumers from Step 2, and the parity baseline from Step 3. This is the single source of truth; later PRs diff against it.

- [ ] **Step 6: Commit**

```bash
git add BASELINE.md
git commit -m "docs: lock measured baseline for python-to-rust conversion"
```

**Exit gate:** `cargo test --workspace` green; `make test-parity` baseline captured (know exactly which comparators are `Skipped`); surviving-Python pytest green; delete list locked in `BASELINE.md`; upstream-sync decision recorded (merged commit hash or "intentionally diverged" note).

---

## Phase 1 — Parity completion

The per-PR parity gate **already exists** in `.github/workflows/rust.yml` (job `parity`, `if: needs.rust-changes.outputs.rust == 'true'`, no `continue-on-error`, comment "111 matched / 65 skipped / 0 diffed" measured on main). No CI change is needed for Q6. The remaining work is the two stubbed comparators — and **re-recording the fixtures**, because the recorded ones document stale/deleted Python behavior (verified 2026-08-14):

- `tests/parity/fixtures/cache_aligner/*.json` (20) record the **pre-PR-A2 rewrite path** (`output.messages` shows a `[Dynamic Context]` block inserted and `transforms_applied: ["cache_align"]`), but current `headroom/transforms/cache_aligner.py` is detector-only (apply is a no-op on messages). Matching these fixtures would require porting a deliberately-deleted feature.
- `tests/parity/fixtures/ccr/*.json` (25) record a `headroom_retrieve` tool definition with a `query` input property; current `headroom/ccr/tool_injection.py:create_ccr_tool_definition` emits only `hash`. The fixtures also show injection with an empty marker-free input, i.e. the **sticky-on** path (`session_has_done_ccr=True`).

### Task 1.1: Re-record `cache_aligner` fixtures against the detector-only transform

**Files:**
- Modify: `tests/parity/recorder.py` — extend `run_default_workload()` to drive the detector-only `CacheAligner` (the patch in `record_all()` already wraps `CacheAligner.apply`; the workload just never calls it with a tokenizer, per the comment at recorder.py:230)
- Modify: `tests/parity/fixtures/cache_aligner/*.json` (re-recorded)

**Interfaces:**
- Consumes: `headroom/transforms/cache_aligner.py` `CacheAligner` (current detector-only API — `apply(messages)` returns `TransformResult` with populated `warnings`/`cache_metrics`, messages untouched)
- Produces: fixture `output` whose `messages` equal the input messages (no `[Dynamic Context]` block), `transforms_applied` no longer contains `"cache_align"` rewrite entries.

- [ ] **Step 1: Extend the recorder workload**

`tests/parity/recorder.py` already monkey-patches `CacheAligner.apply` (`record_all`, line ~230). Add a `cache_aligner` block to `run_default_workload()` driving the current detector-only transform: a system prompt containing a UUID, an ISO-8601 timestamp, and a hex hash; a stable control message. `CacheAligner.apply` needs a `Tokenizer` argument — build `EstimatingTokenCounter` the same way `headroom/transforms/cache_aligner.py` does, and pass it.

```python
# inside tests/parity/recorder.py run_default_workload()
# --- cache_aligner (detector-only) ---
from headroom.transforms.cache_aligner import CacheAligner
from headroom.tokenizers import EstimatingTokenCounter

aligner = CacheAligner()
tok = EstimatingTokenCounter()
for content in (
    "You are a helpful assistant. Request id 12. UUID 123e4567-e89b-12d3-a456-426614174000.",
    "Current date: 2026-08-14T09:30:00Z. Trace 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08.",
    "Stable system prompt, no volatile content.",
):
    aligner.apply([{"role": "system", "content": content}], tokenizer=tok)
```

- [ ] **Step 2: Run it and verify the new fixture shape**

```bash
python scripts/record_fixtures.py
python -c "import json,glob; f=glob.glob('tests/parity/fixtures/cache_aligner/*.json')[0]; d=json.load(open(f)); print(d['output']['messages'] == d['input'])"
```

Expected: `output.messages == input` (detector is a no-op on messages), `output.warnings` non-empty for the volatile-content inputs, and **no** `[Dynamic Context]` block anywhere. Note: `scripts/record_fixtures.py` has **no `--only` flag** — it re-records the full fixture set, so run it once for both Tasks 1.1 and 1.2.

- [ ] **Step 3: Delete the stale fixtures**

```bash
rm tests/parity/fixtures/cache_aligner/*.json   # keep only the re-recorded set
```

- [ ] **Step 4: Commit**

```bash
git add tests/parity/ tests/parity/fixtures/cache_aligner/
git commit -m "fix(parity): re-record cache_aligner fixtures against detector-only transform"
```

### Task 1.2: Re-record `ccr` fixtures against the sticky-on injection path

**Files:**
- Modify: `tests/parity/recorder.py` — extend `run_default_workload()` to drive the sticky-on injector
- Modify: `tests/parity/fixtures/ccr/*.json` (re-recorded)

**Interfaces:**
- Consumes: `headroom/ccr/tool_injection.py` `CCRToolInjector(provider="anthropic").inject_tool_definition(tools, session_has_done_ccr=True)` and `create_ccr_tool_definition("anthropic")`
- Produces: fixture `output` = `[tools_with_injected_headroom_retrieve, true]` matching the current definition (hash-only `input_schema`).

- [ ] **Step 1: Extend the recorder workload**

`record_all()` already wraps `CCRToolInjector.inject_tool_definition` (recorder.py:255). Add a `ccr` block to `run_default_workload()` driving it with `session_has_done_ccr=True` over a few tools arrays (empty, single tool, tool already named `headroom_retrieve`, tool present in MCP form with a `function.name`). The sticky-on flag is the correct model: the real proxy registers `headroom_retrieve` on every request once a session has done CCR (Phase B PR-B7).

```python
# inside tests/parity/recorder.py run_default_workload()
# --- ccr (sticky-on) ---
from headroom.ccr.tool_injection import CCRToolInjector

injector = CCRToolInjector(provider="anthropic")
for tools in ([], [{"name": "other_tool"}], [{"name": "headroom_retrieve"}], [{"type": "function", "function": {"name": "headroom_retrieve"}}]):
    injector.inject_tool_definition(tools, session_has_done_ccr=True)
```

- [ ] **Step 2: Run it and verify against current Python**

```bash
python scripts/record_fixtures.py
python -c "
from headroom.ccr.tool_injection import create_ccr_tool_definition
d = create_ccr_tool_definition('anthropic')
assert 'query' not in d['input_schema']['properties'], 'fixture must match current definition'
"
```

Expected: re-recorded fixtures match the current tool definition (hash-only), and the already-present-tool case records `(tools, false)`.

- [ ] **Step 3: Delete stale fixtures and commit**

```bash
rm tests/parity/fixtures/ccr/*.json
git add tests/parity/ tests/parity/fixtures/ccr/
git commit -m "fix(parity): re-record ccr fixtures against sticky-on injection"
```

### Task 1.3: Implement the `ccr` comparator

**Files:**
- Modify: `crates/headroom-parity/src/lib.rs` (replace `stub_comparator!(CcrComparator, "ccr")`)

**Interfaces:**
- Consumes: `headroom-core::ccr` types; fixture `input` = tools array (`serde_json::Value` array), fixture `config` = `{}`
- Produces: `CcrComparator` implementing `TransformComparator` with `name() == "ccr"`, returning a 2-element JSON array `[tools_with_injected, injected_bool]`.

The fixture format is a 2-element array `[tools_list, bool]` (Python tuple serialized as JSON array). The comparator must:

- [ ] **Step 1: Write the failing test** — extend the `tests` module in `lib.rs`:

```rust
#[test]
fn ccr_comparator_matches_recorded_fixture() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/fixtures");
    let report = run_comparator(&dir, &CcrComparator).unwrap();
    assert_eq!(report.matched, report.total(), "all ccr fixtures must match, got {report:?}");
    assert!(report.skipped.is_empty());
    assert!(report.diffed.is_empty());
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p headroom-parity ccr_comparator_matches_recorded_fixture
```

Expected: FAIL — fixture diff (comparator currently bails → `Skipped`).

- [ ] **Step 3: Write the minimal implementation**

In `lib.rs`, replace the stub with a real comparator:

```rust
/// Real comparator for the `ccr` transform. Fixture input is a tools
/// array; output is the 2-element JSON array `[tools_with_injected, bool]`
/// that Python's tuple `(updated_tools, was_injected)` serialized to.
/// Matches the sticky-on path: `session_has_done_ccr=True` so the tool is
/// injected even when the input carries no compression markers.
pub struct CcrComparator;

impl TransformComparator for CcrComparator {
    fn name(&self) -> &str {
        "ccr"
    }

    fn run(
        &self,
        input: &serde_json::Value,
        _config: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        use headroom_core::ccr::tool_injection::inject_retrieve_tool;
        let tools = input
            .as_array()
            .context("ccr fixture input must be a JSON array of tools")?;
        let (updated, injected) = inject_retrieve_tool(tools, true);
        Ok(serde_json::json!([updated, injected]))
    }
}
```

- [ ] **Step 4: Add the Rust tool-injection helper it calls**

If `headroom_core::ccr::tool_injection::inject_retrieve_tool` does not exist yet, add it under `crates/headroom-core/src/ccr/` (port of `create_ccr_tool_definition("anthropic")` + the inject loop from `inject_tool_definition`, sticky-on branch only), with its own unit tests asserting the exact tool-definition bytes match `create_ccr_tool_definition("anthropic")` output (name `headroom_retrieve`, hash-only `input_schema`, description string verbatim from `headroom/ccr/tool_injection.py`).

- [ ] **Step 5: Run the full parity harness**

```bash
make test-parity
```

Expected: `[ccr] total=25 matched=25 skipped=0 diffed=0` (count may differ if re-record produced a different number of fixtures).

- [ ] **Step 6: Commit**

```bash
git add crates/headroom-core/src/ccr/ crates/headroom-parity/src/lib.rs
git commit -m "fix(parity): promote ccr comparator from stub to real"
```

### Task 1.4: Implement the `cache_aligner` comparator

**Files:**
- Modify: `crates/headroom-parity/src/lib.rs` (replace `stub_comparator!(CacheAlignerComparator, "cache_aligner")`)

**Interfaces:**
- Consumes: fixture `input` = messages array, fixture `config` = `CacheAlignerConfig`-shaped object, fixture `output` = `TransformResult` (detector-only shape)
- Produces: `CacheAlignerComparator` returning the same `TransformResult` JSON the Python recorder serialized.

- [ ] **Step 1: Write the failing test** (mirror Task 1.3 Step 1, asserting all `cache_aligner` fixtures match).

- [ ] **Step 2: Run it to verify it fails** — `cargo test -p headroom-parity cache_aligner_...` → FAIL.

- [ ] **Step 3: Port the detector-only transform to `headroom-core`**

Port `headroom/transforms/cache_aligner.py`'s detector-only behavior (UUID / ISO-8601 / JWT-shape / hex-hash detection via structural checks, **no regex**; messages untouched; `warnings` + `cache_metrics` populated) into `crates/headroom-core/src/transforms/cache_aligner.rs`. Reuse or move the existing `crates/headroom-proxy/src/cache_stabilization/volatile_detector.rs` findings if their shapes line up; otherwise keep the two separate (proxy detector is request-scoped, transform result is message-scoped) but do not duplicate logic — extract shared primitives into `headroom-core` and have both call them.

Note: the fixture `config` carries `detection_tiers`, `date_patterns`, `entropy_threshold`, etc. — read each key with `serde_json::Value` accessors exactly like `LogCompressorComparator` does, falling back to defaults.

- [ ] **Step 4: Implement the comparator**

```rust
pub struct CacheAlignerComparator;

impl TransformComparator for CacheAlignerComparator {
    fn name(&self) -> &str {
        "cache_aligner"
    }
    fn run(&self, input: &serde_json::Value, config: &serde_json::Value) -> Result<serde_json::Value> {
        use headroom_core::transforms::cache_aligner::detect_cache_volatility;
        let messages = input
            .as_array()
            .context("cache_aligner fixture input must be a JSON array of messages")?;
        let result = detect_cache_volatility(messages, config);
        Ok(serde_json::json!({
            "cache_metrics": result.cache_metrics,
            "warnings": result.warnings,
            "messages": result.messages,
        }))
    }
}
```

(The exact output keys must match what `record_cache_aligner.py` serializes — read the recorder's `asdict`/`_json_default` handling in `tests/parity/recorder.py` and mirror it, like `ContentDetectorComparator` does.)

- [ ] **Step 5: Run and fix**

```bash
make test-parity
```

Expected: `[cache_aligner] total=20 matched=20 skipped=0 diffed=0`. If diffs appear, they are either serializer-shape mismatches (fix the comparator's emitted keys) or genuine detector divergences (fix the Rust port).

- [ ] **Step 6: Commit**

```bash
git add crates/headroom-core/src/transforms/ crates/headroom-parity/src/lib.rs
git commit -m "fix(parity): promote cache_aligner comparator from stub to real"
```

### Task 1.5: Verify the parity gate and exit criteria

**Files:** none (verification only)

- [ ] **Step 1: Confirm the per-PR gate is live**

```bash
grep -n "parity" .github/workflows/rust.yml | head
```

Expected: a `parity` job triggered on `pull_request` with no `continue-on-error`, failing on `Diff`. (Verified 2026-08-14: it exists. If it has drifted, re-add it per REALIGNMENT PR-I6.)

- [ ] **Step 2: Run every gate**

```bash
make test-parity   # zero Skipped on request-path transforms
cargo test --workspace
pytest -x tests/test_ccr.py tests/test_transforms/test_smart_crusher_rust_parity.py tests/test_acceptance.py
make ci-precheck
```

- [ ] **Step 3: Update BASELINE.md** — mark parity as `matched` across all fixture sets.

- [ ] **Step 4: Commit**

```bash
git commit -am "docs: record full parity in baseline"
```

**Phase 1 exit gate:** `make test-parity` has **no `Skipped`** on request-path transforms (cache_aligner, ccr, and all previously-real comparators match); the per-PR CI gate is confirmed live; `cargo test --workspace` green.

---

## Phase 2 — Backend switch + canary

### Task 2.1: Add `HEADROOM_PROXY_BACKEND` to the `proxy` CLI command

**Files:**
- Modify: `headroom/cli/proxy.py` (the `proxy()` command, ~line 929–1638; switch inserted after the flag-parsing/warning block, before `run_server` is imported/called at ~line 1037)
- Test: `tests/test_cli/test_proxy_backend_switch.py` (new)

**Interfaces:**
- Consumes: click command params already present; env `HEADROOM_PROXY_BACKEND` (values `python` | `rust`, default `python` in this phase)
- Produces: when `rust`, the command spawns `./target/release/headroom-proxy` with mapped flags/env and **does not** import `headroom.proxy.server`; when `python` (default), behavior is byte-identical to today.

- [ ] **Step 1: Write the failing test**

```python
# tests/test_cli/test_proxy_backend_switch.py
from click.testing import CliRunner
from headroom.cli.proxy import proxy

def test_proxy_backend_rust_spawns_binary(monkeypatch, tmp_path):
    fake_bin = tmp_path / "headroom-proxy"
    fake_bin.write_text("#!/bin/sh\necho fake-rust-binary\n")
    fake_bin.chmod(0o755)
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "rust")
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND_BIN", str(fake_bin))
    # Replace subprocess spawn + readiness polling with capture:
    spawned = {}
    import headroom.cli.proxy as p
    monkeypatch.setattr(p, "_spawn_rust_proxy", lambda cmd, **kw: spawned.update(cmd=cmd) or object())
    runner = CliRunner()
    result = runner.invoke(proxy, ["--port", "8787", "--no-optimize"])
    assert result.exit_code == 0
    assert "headroom-proxy" in " ".join(spawned["cmd"])
    assert "--port" not in spawned["cmd"]  # mapped to --listen/HEADROOM_PROXY_LISTEN
```

- [ ] **Step 2: Run it to verify it fails**

```bash
pytest tests/test_cli/test_proxy_backend_switch.py -v
```

Expected: FAIL — `_spawn_rust_proxy` doesn't exist yet.

- [ ] **Step 3: Implement the switch**

In `headroom/cli/proxy.py`, inside `proxy()` after the existing warning/validation block and **before** `from headroom.proxy.server import ...`:

```python
    proxy_backend = os.environ.get("HEADROOM_PROXY_BACKEND", "python").lower()
    if proxy_backend not in ("python", "rust"):
        click.secho(f"error: HEADROOM_PROXY_BACKEND={proxy_backend!r} must be 'python' or 'rust'", fg="red", err=True)
        raise SystemExit(2)

    if proxy_backend == "rust":
        _spawn_rust_proxy(
            host=host, port=port, config_kwargs=_rust_proxy_env_mapping(),
        )
        return
```

Add module-level helpers `_rust_proxy_env_mapping()` and `_spawn_rust_proxy()`:

```python
def _rust_proxy_env_mapping() -> dict[str, str]:
    """Map this CLI's resolved settings onto the Rust proxy's env surface.

    The Rust proxy (crates/headroom-proxy/src/config.rs) reads
    HEADROOM_PROXY_LISTEN, HEADROOM_PROXY_UPSTREAM, HEADROOM_PROXY_COMPRESSION,
    HEADROOM_PROXY_COMPRESSION_MODE, HEADROOM_PROXY_CACHE_CONTROL_AUTO_FROZEN,
    HEADROOM_PROXY_AUTH_MODE_POLICY_ENFORCEMENT, HEADROOM_PROXY_STRIP_INTERNAL_HEADERS,
    HEADROOM_PROXY_BETA_HEADER_STICKY, HEADROOM_PROXY_ENABLE_RESPONSES_STREAMING,
    HEADROOM_PROXY_ENABLE_CONVERSATIONS_PASSTHROUGH, HEADROOM_PROXY_ENABLE_BEDROCK_NATIVE,
    HEADROOM_PROXY_BEDROCK_REGION/ENDPOINT/AWS_PROFILE, HEADROOM_PROXY_VERTEX_REGION/ADC_SCOPE,
    HEADROOM_ROLLOUT_CHANNEL, HEADROOM_FEATURES, HEADROOM_DISABLE_FEATURES.
    Flags that have no Rust equivalent are intentionally NOT mapped (fail-loud
    if the operator set them: warn and continue only for cosmetic ones).
    """
    out = dict(os.environ)
    out["HEADROOM_PROXY_LISTEN"] = f"{host}:{port}"
    out["HEADROOM_PROXY_COMPRESSION"] = "1" if not no_optimize else "0"
    if target_ratio is not None:
        out.setdefault("HEADROOM_TARGET_RATIO", str(target_ratio))  # if Rust reads it
    # ... remaining mappings per config.rs; unmapped flags warn loudly
    return out

def _spawn_rust_proxy(*, host: str, port: int, config_kwargs: dict[str, str]) -> None:
    """Spawn ./target/release/headroom-proxy, forward health checks, wait for /healthz."""
    binary = os.environ.get("HEADROOM_PROXY_BACKEND_BIN") or str(ROOT / "target" / "release" / "headroom-proxy")
    if not os.path.exists(binary):
        click.secho(f"error: Rust proxy binary not found at {binary}. Run `cargo build --release -p headroom-proxy`.", fg="red", err=True)
        raise SystemExit(1)
    env = dict(config_kwargs)
    proc = subprocess.Popen([binary], env=env)
    _wait_for_healthz(port, proc)   # reuse/adapt the poll loop from wrap.py's _check_proxy
    proc.wait()
```

(Exact mapping table must be completed against `config.rs` fields — the plan lists the known env vars; the implementer fills any remaining field-to-env pairs and adds a unit test asserting the mapping dict keys.)

- [ ] **Step 4: Run tests to verify they pass**

```bash
pytest tests/test_cli/test_proxy_backend_switch.py -v
cargo build --release -p headroom-proxy   # ensure the binary exists for manual check
```

- [ ] **Step 5: Commit**

```bash
git add headroom/cli/proxy.py tests/test_cli/test_proxy_backend_switch.py
git commit -m "fix(cli): add HEADROOM_PROXY_BACKEND=rust to proxy start"
```

### Task 2.2: Route the `wrap` spawn path through the switch

**Files:**
- Modify: `headroom/cli/wrap.py` (`_start_proxy_process`, ~line 600–753 — the `cmd = [sys.executable, "-m", "headroom.cli", "proxy", "--port", str(port)]` construction)
- Test: `tests/test_cli/test_wrap_proxy_backend.py` (new) or extend `tests/test_cli/test_wrap.py`

**Interfaces:**
- Consumes: same `HEADROOM_PROXY_BACKEND` env var; wrap inherits `os.environ` when spawning
- Produces: when `HEADROOM_PROXY_BACKEND=rust`, wrap spawns the same `python -m headroom.cli proxy ...` command (which internally switches) — **no change to the spawn command needed if Task 2.1 handles the branch internally**; verify instead that (a) the env var is forwarded and (b) readiness polling still works against the Rust `/healthz`.

- [ ] **Step 1: Write the failing test**

```python
def test_wrap_forwards_backend_env(tmp_path, monkeypatch):
    # Patch _start_proxy_process's Popen to capture env; assert
    # HEADROOM_PROXY_BACKEND=rust reaches the child when set.
```

- [ ] **Step 2: Verify the spawn command already forwards env**

```bash
grep -n "HEADROOM_PROXY_BACKEND\|os.environ" headroom/cli/wrap.py | head
```

Expected: wrap uses `popen_kwargs` with inherited env (`os.name == "posix"` path passes `env` or inherits). If the env var is not forwarded explicitly, add `env={**os.environ, **child_env}` to `popen_kwargs`.

- [ ] **Step 3: Verify readiness polling works against Rust**

Rust exposes `/healthz` (`crates/headroom-proxy/src/health.rs`); wrap's `_check_proxy` polls `http://127.0.0.1:{port}/health` with a 2s timeout (copilot `query_proxy_config`) and/or a simpler GET. Confirm the Rust proxy answers the poll target wrap uses; if wrap only polls `/health` and Rust answers it (`{"ok": true, "service": "headroom-proxy"}`), no change is needed; if wrap requires a Python-only payload key, make the poll tolerant (accept `{"ok": true}` OR the config block).

- [ ] **Step 4: Run wrap tests + commit**

```bash
pytest tests/test_cli/test_wrap_proxy_backend.py tests/test_cli/test_wrap_copilot.py -v
git add headroom/cli/wrap.py tests/test_cli/
git commit -m "fix(cli): forward HEADROOM_PROXY_BACKEND through wrap spawn"
```

### Task 2.3: Keep the `/health` config-block contract stable across backends

**Files:**
- Modify: `crates/headroom-proxy/src/health.rs` (add a `config` block to the loopback `/health` response, mirroring Python's `_health_payload(include_config=True)`)
- Test: `crates/headroom-proxy/src/health.rs` test module (or `crates/headroom-proxy/tests/integration_health.rs`)

**Interfaces:**
- Consumes: `AppState` (running `Config`)
- Produces: `/health` response containing `"config": {"backend": "rust", "optimize": <bool>, "cache": <bool>, "rate_limit": <bool>, ...}` for loopback callers, so `headroom/providers/copilot/wrap.py:detect_running_proxy_backend` and `headroom/cli/wrap.py:_agent_savings_config_mismatches` keep working. Python's contract (server.py ~line 3000): loopback callers get `config`; network callers get status-only. Match that gating.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn health_loopback_exposes_config_backend() {
    // start_proxy_with a test config; GET /health from 127.0.0.1;
    // assert payload["config"]["backend"] == "rust"
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p headroom-proxy health_loopback` → FAIL (`config` key missing).

- [ ] **Step 3: Implement**

In `health.rs`, split the handler: detect loopback (mirror Python's `_request_is_loopback`), include `config` for loopback callers. Populate the block from `state.config`: `backend: "rust"`, `optimize: config.compression`, `cache: <resolved semantic-cache flag>`, `rate_limit: <resolved>`, plus the keys `_agent_savings_config_mismatches` reads (`savings_profile`, `target_ratio`, `compress_user_messages`, `compress_system_messages`, `protect_recent`, `min_tokens`, `max_items`) — read `headroom/cli/wrap.py:_agent_savings_config_mismatches` (lines ~2951–3056) for the exact key list and match Python's semantics so the restart-hint logic doesn't misfire.

- [ ] **Step 4: Verify copilot detection against Rust**

```bash
cargo build --release -p headroom-proxy
./target/release/headroom-proxy --upstream http://127.0.0.1:8788 --listen 127.0.0.1:8787 &
curl -s http://127.0.0.1:8787/health | python3 -c "import json,sys; print(json.load(sys.stdin)['config']['backend'])"
# expected: rust
pytest tests/test_provider_copilot_wrap.py -v   # detection tests still green
```

- [ ] **Step 5: Commit**

```bash
git add crates/headroom-proxy/src/health.rs
git commit -m "fix(proxy): expose config.backend on loopback /health for CLI parity"
```

### Task 2.4: Byte-equality shadow test (both backends)

**Files:**
- Add: `e2e/shadow/runner.py` (adapted from REALIGNMENT PR-I4 spec)
- Add: `e2e/shadow/test_byte_equality.py` or a Rust integration test `crates/headroom-proxy/tests/integration_shadow.rs`
- Add: `docs/operations/shadow-deploy.md`

**Interfaces:**
- Consumes: a recorded traffic corpus (reuse `tests/parity/fixtures/codex_openai_contracts/` + the SSE fixtures in `crates/headroom-proxy/tests/fixtures/` + any `tests/fixtures/anthropic_messages_request_real.json`); both backends booted on different ports
- Produces: per-request SHA-256 of upstream-bound bytes from both backends; report of mismatches; exit non-zero if mismatch rate ≥ 0.1%.

- [ ] **Step 1: Write the shadow runner**

For each recorded request, POST the same bytes to the Python backend (port A) and the Rust backend (port B), both configured with an upstream mock that captures the received bytes (a tiny recording server, or reuse the headroom-simulators crate). Compare `sha256(upstream_bytes_python) == sha256(upstream_bytes_rust)` and the response bytes.

```python
# e2e/shadow/runner.py — core loop
def run_case(client_python, client_rust, mock_upstream, request: dict) -> dict:
    py = client_python.post(request["path"], data=request["body"], headers=request["headers"])
    rs = client_rust.post(request["path"], data=request["body"], headers=request["headers"])
    return {
        "path": request["path"],
        "upstream_match": mock_upstream.sha_python == mock_upstream.sha_rust,
        "response_match": py.content == rs.content,
    }
```

- [ ] **Step 2: Write the failing test** — assert all corpus cases have `upstream_match and response_match`. Run against Python+Python first to validate the harness itself (must pass), then flip one side to Rust (must fail until parity is real).

- [ ] **Step 3: Run against both backends**

```bash
# boot upstream mock + python backend + rust backend, then:
python e2e/shadow/runner.py --fixtures tests/parity/fixtures --expect 0.999
```

Expected: <0.1% mismatch after Phase 1 parity work; investigate every mismatch (they are the "uncovered request path" list for Phase 3).

- [ ] **Step 4: Prove rollback in the canary**

With both backends live: start `HEADROOM_PROXY_BACKEND=python`, verify traffic flows; flip to `rust`, verify; flip back — the rollback path is "set the env var", no code change.

- [ ] **Step 5: Commit**

```bash
git add e2e/shadow/ docs/operations/shadow-deploy.md
git commit -m "fix(e2e): add python-vs-rust byte-equality shadow test"
```

**Phase 2 exit gate:** both backends boot via `proxy start` and via `wrap`; shadow test green (≥99.9% byte-equality on the corpus); `/health` config block present for loopback (copilot detection tests pass against Rust); `HEADROOM_PROXY_BACKEND=python` rollback proven in the canary.

---

## Phase 3 — Retirement (3 PRs)

Delete list source of truth: `BASELINE.md` (Task 0.1). Each PR is split into one `fix:` commit **per module** for git-blame friendliness (the REALIGNMENT H1 note is explicit about this). Every PR gate: `cargo test --workspace` + surviving `pytest -x` + `make test-parity` + `make ci-precheck`.

### Task 3.1: PR-3.1 — delete the Python proxy request path

**Files (delete — EXACT list from BASELINE.md Task 0.1, measured against the synced tree; the list below is illustrative and incomplete by design):**
- `headroom/proxy/` — the **entire directory** (server.py 5,933 lines; **93 Python files / 28,531 LOC total**, measured post-sync). This includes `server.py`, `handlers/` (anthropic.py 2,423, openai.py 2,742, streaming.py 1,131, gemini.py 839, batch.py 1,010, **bedrock.py, _debug_dump.py** — note: there is no `conversations.py`/`chat_completions.py` in Python; those are Rust-side), `interceptors/`, and all support modules: `cost, helpers, rate_limiter, request_logger, prometheus_metrics, extensions, models, modes, stage_timer, warmup, debug_introspection, savings_tracker, savings_attribution, outcome, turn_hooks, loopback_guard, ws_session_registry, memory_handler, memory_tool_adapter, semantic_cache` (at `headroom/proxy/semantic_cache.py`), plus the ~50 `*_policy.py`, `*_decision.py`, `body_forwarding.py`, `route_advice.py`, `ssl_context.py`, `model_router.py`, `runtime_env.py`, `persistent_metrics.py`, `probe_recorder.py`, `passthrough.py`, `audit.py`, `auth_mode.py` etc. **Do not hand-enumerate — delete the whole directory minus any survivors BASELINE.md's consumer map proves are imported by off-path Python, and confirm with `git grep` that nothing outside `headroom/proxy/` imports the deleted modules.**
- `/dashboard` web UI: **decision deferred to this PR** (see Global Constraints) — either document the retirement of the Python-served dashboard/static routes (`headroom/dashboard/templates/`) or port a minimal Rust equivalent, and update the exit gate accordingly.
- `headroom/transforms/cache_aligner.py` (413 lines, detector-only — deleted here; its Rust port lives in headroom-core from Task 1.4)
- Proxy-only test files: all `tests/test_proxy_*.py` (66 files measured) + handler/transform tests that exercise the Python request path (from BASELINE.md's consumer map; keep ~40 that test surviving Python)

**Modify:**
- `headroom/cli/proxy.py` — remove the `HEADROOM_PROXY_BACKEND=python` branch entirely; `proxy()` always spawns the Rust binary (keep `_spawn_rust_proxy`; delete the `run_server` import path)
- `headroom/cli/wrap.py` — `_start_proxy_process` now spawns the Rust binary directly (`[binary, "--listen", f"{host}:{port}", ...]`) instead of `python -m headroom.cli proxy`
- `pyproject.toml` — move `fastapi`, `uvicorn`, `pydantic`, and other proxy runtime deps to dev/test extras only (parity harness still needs them until Phase 3.3/Q12 lands)
- `Dockerfile` / `docker-compose.yml` — proxy stage now runs the Rust binary (final form in Phase 4; do the minimal wiring here)

- [ ] **Step 1: Delete in per-module commits**

```bash
git rm headroom/proxy/server.py
git commit -m "fix(proxy): retire python server.py, rust proxy is the request path"
# repeat per module: handlers/, interceptors/, then support modules
```

- [ ] **Step 2: Flip the CLI to always-Rust**

```bash
# remove the python branch from proxy(); wrap() spawns the binary directly
# update tests that asserted the python branch
pytest tests/test_cli/test_proxy_backend_switch.py -v   # update: no python branch to test
```

- [ ] **Step 3: Delete proxy-only tests**

```bash
# from BASELINE.md's measured list
git rm $(cat /tmp/proxy_test_files.txt)   # the 66 test_proxy_*.py + mapped handler tests
pytest -x tests/test_cli/ tests/test_ccr.py tests/test_transforms/test_smart_crusher_rust_parity.py tests/test_acceptance.py
```

- [ ] **Step 4: Run all gates**

```bash
cargo test --workspace && make test-parity && make ci-precheck
```

- [ ] **Step 5: Manual boot check**

```bash
cargo build --release -p headroom-proxy
headroom proxy start --no-optimize &   # must boot the rust binary
curl -s http://127.0.0.1:8787/healthz   # {"ok": true, "service": "headroom-proxy"}
```

**PR-3.1 exit gate:** `git grep -i "uvicorn\|fastapi" headroom/` returns nothing in non-test code; `headroom proxy start` boots the Rust binary; e2e canary passes; all gates green.

### Task 3.2: PR-3.2 — retire the LiteLLM Bedrock/Vertex backend

**Files:**
- Delete: `headroom/backends/litellm.py` (~1,500 lines) — **only this file**
- **Keep:** `headroom/backends/base.py` — imported by `headroom/cache/compression_store.py` and `headroom/telemetry/toin.py`, **both of which survive**; and `headroom/backends/__init__.py` + `anyllm.py` — `__init__.py` re-exports more than litellm (verified: `anyllm`). Do NOT delete the directory wholesale.
- Modify: `headroom/providers/registry.py` — remove `litellm-bedrock`, `litellm-vertex` entries (verified: `create_proxy_backend` at registry.py:188 has a `litellm_backend_cls` parameter); `pyproject.toml` — drop `litellm` dependency
- Delete tests: `tests/test_backends_litellm*.py`, `tests/test_vertex_claude_compression.py` (it calls `registry.create_proxy_backend` — verify against BASELINE.md's consumer map first)

- [ ] **Step 1: Delete litellm + registry entries + dep** (keep `base.py`, `anyllm.py`, `__init__.py`)
- [ ] **Step 2: Run gates** — `pytest -x tests/test_provider_registry.py` (still green without litellm entries), `pytest -x tests/test_ccr.py` (exercises toin/compression_store paths that import backends.base), `cargo test --workspace`
- [ ] **Step 3: Commit per module**

```bash
git rm headroom/backends/litellm.py && git commit -m "fix(backends): retire python litellm converter"
# then registry + deps commits
```

**PR-3.2 exit gate:** no `litellm` in runtime deps or non-test code (`git grep -i litellm headroom/` clean except tests); `headroom/backends/base.py` still importable by `cache/compression_store.py` and `telemetry/toin.py`; native Bedrock/Vertex Rust routes still covered by `cargo test --workspace`.

### Task 3.3: PR-3.3 — final cleanup

**Files:**
- Delete: orphaned modules and dead fixtures from BASELINE.md's orphan list (e.g. `tests/parity/fixtures/` entries for comparators retired by Q12 decision), stale docs
- Modify: `headroom/__init__.py` (drop unused imports), `pyproject.toml` (minimal runtime deps), `Cargo.toml` (final workspace cleanup), `RUST_DEV.md` → `DEV.md`, `CHANGELOG.md` breaking entry

**Decision Q12 (greenlit in the design doc):** repurpose `crates/headroom-parity/` → `crates/headroom-version-parity/`, comparing current Rust vs previous Rust on the recorded fixtures (guards future ML compressor variants, e.g. a Kompress port). If greenlit:

- [ ] **Step 1: Rename the crate and reframe the comparator contract**

```bash
git mv crates/headroom-parity crates/headroom-version-parity
# update Cargo.toml package name + workspace member + Makefile test-parity target path
# comparator trait now drives (previous_version_output, current_version_output) instead of (fixture.output, rust_output)
```

- [ ] **Step 2: Wire the version-parity gate**

`Makefile` `test-parity` runs the version-parity harness; keep the per-PR CI job (it now catches Rust-vs-Rust regressions on future compressor changes).

- [ ] **Step 3: Docs sweep**

```bash
git mv RUST_DEV.md DEV.md
# update README.md, wiki/, docs/ to reference the Rust-only proxy
# add CHANGELOG breaking entry:
#   **Breaking**: Python proxy retired. Operators must use the Rust binary `headroom-proxy`.
#   See migration guide at `docs/operations/python-to-rust-migration.md`.
```

- [ ] **Step 4: Final gate**

```bash
git grep -i "uvicorn\|fastapi\|litellm" headroom/   # nothing in non-test code
make ci-precheck && cargo test --workspace && make test-parity
```

**PR-3.3 exit gate:** minimal `pyproject.toml` runtime deps; `DEV.md` in place; CHANGELOG breaking entry; version-parity harness green.

**Phase 3 exit gate:** `git grep -i "uvicorn\|fastapi\|litellm" headroom/` returns nothing in non-test code; `headroom proxy start` boots the Rust binary; e2e canary passes; `make ci-precheck` green.

---

## Phase 4 — Ops cutover + docs

### Task 4.1: Dockerfile + compose → Rust binary

**Files:**
- Modify: `Dockerfile` (single-stage `FROM gcr.io/distroless/static` or `scratch`, COPY `target/release/headroom-proxy`, ENTRYPOINT `["/headroom-proxy"]`)
- Modify: `docker-compose.yml` — `command: ["--listen", "0.0.0.0:8787", "--upstream", "..."]` (replaces `["--host", "0.0.0.0"]`), remove Python-specific env
- Modify: `.github/workflows/docker.yml` if it builds via the Python path

- [ ] **Step 1: Write the single-stage Dockerfile**

```dockerfile
FROM rust:1.95.0 AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p headroom-proxy

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /build/target/release/headroom-proxy /headroom-proxy
EXPOSE 8787
ENTRYPOINT ["/headroom-proxy"]
```

- [ ] **Step 2: Verify image size and boot**

```bash
docker build -t headroom-rust .
docker run --rm -p 8787:8787 headroom-rust --listen 0.0.0.0:8787 --upstream http://host.docker.internal:8788 &
curl -s http://127.0.0.1:8787/healthz
```

Expected: image ~50 MB (vs ~500 MB Python image); `/healthz` OK.

- [ ] **Step 3: Update compose + CI + commit**

### Task 4.2: Operator migration guide + docs refresh

**Files:**
- Add: `docs/operations/python-to-rust-migration.md` (the operator guide promised by the design doc)
- Modify: `README.md`, `wiki/`, `docs/configuration.md` (Rust env-var surface replaces Python flags), `CHANGELOG.md`

- [ ] **Step 1: Write the migration guide** — sections: what changed (Python proxy → `headroom-proxy` binary), config surface (`HEADROOM_PROXY_*` env vars from `crates/headroom-proxy/src/config.rs`), the 30-day image-retention rollback, how to verify (`/healthz`, `/health` config block), FAQ (memory/eval CLIs unchanged, they are off-path).

- [ ] **Step 2: Refresh README/wiki** — the proxy is the Rust binary; `headroom proxy start` wraps it.

- [ ] **Step 3: Verify docs build + staging deploy**

```bash
make ci-precheck
# docs build per repo convention (mkdocs/sphinx if configured — check pyproject/docs dir)
# operator runbook verified end-to-end on a staging deploy: boot, health, a real conversation through the proxy
```

- [ ] **Step 4: Commit**

```bash
git add docs/operations/python-to-rust-migration.md README.md wiki/ docs/ CHANGELOG.md
git commit -m "docs: add operator migration guide and refresh docs for rust-only proxy"
```

**Phase 4 exit gate:** `make ci-precheck` green; docs build; operator runbook verified on staging; image retained for 30-day rollback.

---

## Self-review against the design doc

- **§1 scope item 1 (parity proof)** → Phase 1 Tasks 1.1–1.5 ✓
- **§1 scope item 2 (backend switch)** → Phase 2 Tasks 2.1–2.3; wrap-path coverage explicitly added (Task 2.2) per the spec review ✓
- **§1 scope item 3 (retirement)** → Phase 3 Tasks 3.1–3.3 ✓
- **§1 scope item 4 (ops cutover)** → Phase 4 Tasks 4.1–4.2 ✓
- **§1 scope item 5 (docs + PR)** → the design doc + this plan are the Phase 5 docs PR; Phase 4 Task 4.2 produces the operator guide ✓
- **§4 Phase 0 "upstream sync resolved before Phase 0"** → Task 0.1 preamble ✓
- **§4 Phase 2 "wrap spawn path + /health schema"** → Tasks 2.2, 2.3 ✓
- **§7 Q6** → verified already wired in CI (Task 1.5); **Q7 + deviation** → Task 2.1 (default stays `python` through canary; rollback = env var + retained images, not in-tree Python); **Q12** → Task 3.3 Step 1; **sync before Phase 0** → Task 0.1 ✓
- **§6 risks** — "deleting Python before parity" gated by Phase 1/2 exit gates; "uncovered request path" caught by the Phase 2 shadow test; "surviving-Python regression" gated per-PR by the smart-crusher pytest list ✓
- **Placeholder scan** — every task lists concrete files, commands, and expected outputs; the only intentional "fill in" is the exact env-mapping table in Task 2.1 Step 3, which names the source file (`config.rs`) and requires a unit test on the mapping dict — implementer completes against the named source, not from memory.
- **Pre-flight review (2026-08-14, against the synced tree)** — the following gaps found during the plan review were fixed in-place: (1) Phase 3.1 now deletes the entire `headroom/proxy/` directory (93 files / 28,531 LOC) instead of an incomplete ~15-file enumeration, and carries the deferred `/dashboard` decision; (2) Phase 3.2 keeps `backends/base.py`/`anyllm.py`/`__init__.py` (consumed by surviving `cache/compression_store.py` + `telemetry/toin.py`); (3) Tasks 1.1/1.2 now extend the existing `tests/parity/recorder.py` workload instead of writing new recorder scripts, and use `python scripts/record_fixtures.py` (no `--only` flag exists); (4) stale paths corrected (`semantic_cache.py` → `headroom/proxy/`; no `conversations.py`/`chat_completions.py` in Python handlers); (5) Global Constraints note the sync delta (27 commits, savings_attribution/outcome/turn_hooks added upstream).
- **Type consistency** — `inject_retrieve_tool(tools, sticky: bool) -> (Vec<Value>, bool)` defined in Task 1.3 and consumed only there; `detect_cache_volatility(messages, config) -> CacheAlignResult` defined and consumed in Task 1.4; `_spawn_rust_proxy`/`_rust_proxy_env_mapping` defined in Task 2.1 and consumed by Task 2.2 via env forwarding.
