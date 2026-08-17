"""Byte-equality shadow gate (retired with PR-3.1).

Phase 2 (plan Task 2.4) proved the Rust proxy forwards byte-identical
requests/responses vs the Python proxy on the shadow corpus. PR-3.1 deletes
``headroom.proxy.server`` (the Python backend this gate booted), so the
comparison no longer has a Python side — the gate now skips with a clear
message. The runner/``boot_python_backend`` path is kept for archaeology and
for the git history of the Phase 2 exit gate; it errors if invoked against a
tree without the Python proxy.

Not part of the default ``pytest`` run (``testpaths = ["tests"]``).
"""

from __future__ import annotations

import pytest

_REPO_ROOT = Path(__file__).resolve().parents[2]

pytestmark = pytest.mark.skip(
    reason="Python proxy retired in PR-3.1 — the python-vs-rust byte-equality "
    "gate was proven in Phase 2 and has no Python side to compare anymore"
)


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
