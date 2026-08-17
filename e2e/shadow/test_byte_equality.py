"""Byte-equality shadow gate: Python proxy vs Rust proxy.

Boots the two recording mock upstreams, the Python backend, and the Rust
backend, replays the shadow corpus through both, and asserts every case
matches — both the sha256 of the upstream-bound request bytes and the
response bytes returned to the client.

This is the Phase 2 exit gate (plan Task 2.4). It is not part of the default
``pytest`` run (``testpaths = ["tests"]``); run it explicitly:

    python -m pytest e2e/shadow/test_byte_equality.py -v

Skips (with a clear message) when the release Rust binary is missing or the
Python proxy dependencies (fastapi) are unavailable.
"""

from __future__ import annotations

import subprocess
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

_REPO_ROOT = Path(__file__).resolve().parents[2]
_RUST_BIN = _REPO_ROOT / "target" / "release" / "headroom-proxy"

pytest.importorskip(
    "fastapi", reason="Python proxy backend needs fastapi (pip install fastapi uvicorn)"
)

pytestmark = pytest.mark.skipif(
    not _RUST_BIN.exists(),
    reason="release Rust proxy binary missing — build it with: cargo build --release -p headroom-proxy",
)

from .corpus import build_corpus  # noqa: E402
from .runner import (  # noqa: E402
    RecordingUpstream,
    boot_python_backend,
    boot_rust_backend,
    run_shadow,
)


@pytest.fixture(scope="module")
def shadow_env() -> Iterator[dict[str, Any]]:
    """Boot upstreams + both backends; yield URLs; tear everything down."""
    upstream_py = RecordingUpstream()
    upstream_rs = RecordingUpstream()
    procs: list[subprocess.Popen] = []
    try:
        py_proc, py_port = boot_python_backend(upstream_py.url)
        rs_proc, rs_port = boot_rust_backend(upstream_rs.url, str(_RUST_BIN))
        procs = [py_proc, rs_proc]
        yield {
            "python_url": f"http://127.0.0.1:{py_port}",
            "rust_url": f"http://127.0.0.1:{rs_port}",
            "upstream_py": upstream_py,
            "upstream_rs": upstream_rs,
        }
    finally:
        for proc in procs:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.kill()
        upstream_py.stop()
        upstream_rs.stop()


def test_shadow_byte_equality(shadow_env: dict[str, Any]) -> None:
    """Every corpus case must forward identical bytes and return an identical response."""
    cases, notes = build_corpus(_REPO_ROOT)
    assert cases, "corpus unexpectedly empty"
    report = run_shadow(
        cases,
        shadow_env["python_url"],
        shadow_env["rust_url"],
        shadow_env["upstream_py"],
        shadow_env["upstream_rs"],
    )
    report.notes.extend(notes)

    print("\n" + report.detail())

    assert report.all_match, (
        f"byte-equality shadow gate failed: {report.matched}/{report.total} cases matched. "
        "Each mismatch is an uncovered request path — investigate before declaring parity."
    )


def test_shadow_corpus_nonempty() -> None:
    """The corpus must contain the real recorded Anthropic request."""
    cases, _ = build_corpus(_REPO_ROOT)
    names = {c.name for c in cases}
    assert "anthropic_real" in names, "real Anthropic fixture missing from corpus"
    assert "anthropic_stream" in names, "derived Anthropic streaming case missing from corpus"
    assert any(c.path == "/v1/chat/completions" for c in cases), "no OpenAI chat cases in corpus"
