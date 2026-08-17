# Shadow test: Python vs Rust byte-equality

The shadow test replays a recorded traffic corpus through **both proxy
backends** at the same time — the Python proxy (`headroom.proxy.server`) and
the Rust proxy (`headroom-proxy`) — each forwarding to its own recording mock
upstream. For every case it compares:

1. **Upstream-bound request bytes** — `sha256` of the raw body each backend
   forwarded to its upstream. The two backends must forward identical bytes
   for the same client request.
2. **Response bytes** — `sha256` of the body each backend returned to the
   client (status codes must match too).

A case matches only when both agree. This is the **Phase 2 exit gate**
(plan Task 2.4, REALIGNMENT PR-I4): byte-equality must hold on the corpus
before the Rust proxy can become the default request path.

## Topology

```
client (runner)  ──POST──▶ python backend ──▶ recording upstream A
       │
       └──────────POST──▶ rust backend   ──▶ recording upstream B

runner compares: sha256(upstream A body) == sha256(upstream B body)
                 sha256(python response) == sha256(rust response)
```

The recording upstreams are tiny stdlib HTTP servers that capture the raw
request bytes per request and answer with a deterministic canned payload
(JSON, or SSE for `"stream": true` requests), so any byte difference the
comparison sees is proxy-introduced.

## Prerequisites

```bash
cargo build --release -p headroom-proxy      # the Rust backend
# Python backend deps (already in pyproject):
pip install fastapi uvicorn                  # + httpx, headroom._core (make build-wheel)
```

The pytest gate skips with a clear message when the release binary is missing
or fastapi is unavailable.

## Running

As a pytest gate (the Phase 2 exit gate):

```bash
python -m pytest e2e/shadow/test_byte_equality.py -v
```

As the operator CLI (boots both backends and both upstreams itself):

```bash
python e2e/shadow/runner.py --boot --rust-bin target/release/headroom-proxy
```

Against already-running backends (e.g. the canary):

```bash
python e2e/shadow/runner.py --python-url http://127.0.0.1:9001 \
    --rust-url http://127.0.0.1:9002
```

Extra recorded traffic can be added with `--fixtures <dir>` (repeatable);
JSON files that look like forwardable requests are routed to
`/v1/messages` (Anthropic) or `/v1/chat/completions` (OpenAI) by content,
and anything else (e.g. JSON schemas) is skipped with a note in the report.

Exit code is non-zero when the match rate falls below `--expect` (default
`0.999`, i.e. a mismatch rate ≥ 0.1% fails the gate).

## Reading the report

Each case line has one of three markers:

- **OK** — both upstream-bound bytes and response bytes were byte-identical.
- **EXP** — a *documented* divergence: the normalized Rust bytes equal the
  Python bytes, and the reason is printed. Counts toward the match rate.
- **FAIL** — a real mismatch: the gate is red.

Example:

```
shadow: 6/6 cases matched (100.0%)

OK  anthropic_real     /v1/messages           upstream=match response=match (status 200/200)
EXP openai_basic       /v1/chat/completions   upstream=match response=match (status 200/200)
    - EXPECTED divergence: python normalizes OpenAI max_tokens -> max_completion_tokens; rust is byte-faithful
```

`--report-json path.json` writes the per-case shas and notes for archival /
alerting.

## Known divergences (Phase 3 uncovered request paths)

The shadow test found the Python proxy (the reference) applies semantic
normalizations to OpenAI request bodies that the Rust proxy — byte-faithful
by Phase 2 contract — does not replicate. These are real behavior
differences a client would observe, so they are tracked as uncovered request
paths for Phase 3, not hidden:

| Case | Python behavior | Rust behavior | Impact |
|------|-----------------|---------------|--------|
| `openai_basic` | renames `max_tokens` → `max_completion_tokens` | forwards the client body unchanged | upstream sees a different (equivalent) field name |
| `openai_stream` | injects `stream_options.include_usage: true` | forwards the client body unchanged | streamed responses lack the final usage chunk |

The runner encodes these in `EXPECTED_DIVERGENCES` (in
`e2e/shadow/runner.py`): each entry pairs a human-readable reason with a
normalizer that applies the Python transform to the Rust side. A case is
`EXP` only when the *normalized* Rust bytes equal the Python bytes — any
divergence outside these two documented transforms fails the gate
immediately, so a future Rust change that starts altering request bodies
turns the gate red.

## Mismatch triage

A `FAIL` means one backend forwarded different bytes (or returned a
different response) for the same client request:

1. Read the per-case shas and notes (`--report-json` for the machine form).
2. Reproduce with the case in isolation and diff the upstream-received body
   against the client body (the dump helpers under `e2e/shadow/` make this
   easy) to decide which side diverged and how.
3. If the Rust side altered the bytes: that is a parity bug — fix the Rust
   proxy and re-run.
4. If the Python side transformed the bytes semantically and the Rust side
   is byte-faithful: decide whether the transform must be ported to Rust
   (Phase 3 work) or documented as an expected divergence. Do **not** add it
   to `EXPECTED_DIVERGENCES` without a written reason and a normalizer that
   actually captures the transform.

## Canary rollback

The rollback path is the env switch introduced by Phase H Task 2.1 — no code
change:

```bash
HEADROOM_PROXY_BACKEND=python wrap claude     # roll back to Python
HEADROOM_PROXY_BACKEND=rust   wrap claude     # Rust is the canary
```

Both backends boot through `wrap` and answer the readiness probe (a TCP
connect, backend-agnostic); the Rust proxy additionally serves `/healthz`
and the loopback `/health` config block. The shadow test proves the two
backends forward byte-equivalent traffic on the covered corpus, so flipping
the env var back and forth is safe.
