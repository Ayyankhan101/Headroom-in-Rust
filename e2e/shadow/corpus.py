"""Shadow-test traffic corpus.

The corpus mixes the single real recorded Anthropic request checked into the
repo (``crates/headroom-proxy/tests/fixtures/anthropic_messages_request_real.json``)
with derived streaming variants and representative OpenAI chat-completions
bodies matching the codex contract (``tests/parity/fixtures/codex_openai_contracts/``).

Every case is posted *byte-for-byte* to both backends by ``runner.py``; the
two backends are only equivalent if the upstream receives identical bytes and
the client receives an identical response.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

#: Repo-relative path of the single real recorded Anthropic request.
ANTHROPIC_FIXTURE = "crates/headroom-proxy/tests/fixtures/anthropic_messages_request_real.json"

_ANTHROPIC_HEADERS = {
    "content-type": "application/json",
    "anthropic-version": "2023-06-01",
}
_OPENAI_HEADERS = {"content-type": "application/json"}


@dataclass(frozen=True)
class Case:
    """One recorded request to replay through both backends."""

    name: str
    path: str
    headers: dict[str, str]
    body: bytes

    def __post_init__(self) -> None:
        if not self.name or not self.path:
            raise ValueError("case name and path are required")


def default_corpus(repo_root: Path) -> list[Case]:
    """The built-in corpus: real fixture + derived streaming + OpenAI cases."""
    repo_root = Path(repo_root)
    cases: list[Case] = []

    # --- Anthropic /v1/messages -------------------------------------------------
    anthropic_body = (repo_root / ANTHROPIC_FIXTURE).read_bytes()
    cases.append(Case("anthropic_real", "/v1/messages", _ANTHROPIC_HEADERS, anthropic_body))

    # Derived streaming variant of the same real request.
    anthropic_obj = json.loads(anthropic_body)
    anthropic_obj["stream"] = True
    cases.append(
        Case(
            "anthropic_stream",
            "/v1/messages",
            _ANTHROPIC_HEADERS,
            _compact_json(anthropic_obj),
        )
    )

    # --- OpenAI /v1/chat/completions (codex contract) ---------------------------
    cases.append(
        Case(
            "openai_basic",
            "/v1/chat/completions",
            _OPENAI_HEADERS,
            _compact_json(
                {
                    "model": "gpt-4o-mini",
                    "messages": [
                        {"role": "system", "content": "You are a helpful coding assistant."},
                        {"role": "user", "content": "Explain closures in Python."},
                        {"role": "assistant", "content": "A closure captures its enclosing scope."},
                        {"role": "user", "content": "Show a short example."},
                    ],
                    "temperature": 0.2,
                    "max_tokens": 256,
                }
            ),
        )
    )
    cases.append(
        Case(
            "openai_tools",
            "/v1/chat/completions",
            _OPENAI_HEADERS,
            _compact_json(
                {
                    "model": "gpt-4o-mini",
                    "messages": [
                        {"role": "user", "content": "List the files in src/ and summarize them."},
                        {
                            "role": "assistant",
                            "content": None,
                            "tool_calls": [
                                {
                                    "id": "call_1",
                                    "type": "function",
                                    "function": {
                                        "name": "list_dir",
                                        "arguments": '{"path": "src"}',
                                    },
                                }
                            ],
                        },
                        {
                            "role": "tool",
                            "tool_call_id": "call_1",
                            "content": '["main.py", "util.py"]',
                        },
                    ],
                    "tools": [
                        {
                            "type": "function",
                            "function": {
                                "name": "list_dir",
                                "description": "List directory entries",
                                "parameters": {
                                    "type": "object",
                                    "properties": {"path": {"type": "string"}},
                                    "required": ["path"],
                                },
                            },
                        }
                    ],
                    "tool_choice": "auto",
                }
            ),
        )
    )
    cases.append(
        Case(
            "openai_stream",
            "/v1/chat/completions",
            _OPENAI_HEADERS,
            _compact_json(
                {
                    "model": "gpt-4o-mini",
                    "messages": [{"role": "user", "content": "Count to five."}],
                    "stream": True,
                }
            ),
        )
    )
    cases.append(
        Case(
            "openai_minimal",
            "/v1/chat/completions",
            _OPENAI_HEADERS,
            _compact_json(
                {"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}]}
            ),
        )
    )
    return cases


def _compact_json(obj: object) -> bytes:
    """Canonical compact JSON, matching the form real clients and the Python
    proxy's OpenAI path emit (no whitespace after separators). Byte-equality
    between the two backends is only meaningful for canonical bodies: the
    Python proxy re-serializes OpenAI requests compactly, so a spaced input
    would report a whitespace-only mismatch that no upstream would care about.
    """
    return json.dumps(obj, separators=(",", ":")).encode()


def _sniff_path(body: bytes) -> str | None:
    """Route an arbitrary JSON request body to its endpoint, or None if it is
    not a forwardable request (e.g. a JSON *schema* rather than a request)."""
    try:
        obj = json.loads(body)
    except (ValueError, TypeError):
        return None
    if not isinstance(obj, dict):
        return None
    if "messages" not in obj or "model" not in obj:
        return None
    model = str(obj.get("model", ""))
    if model.startswith("claude") or "anthropic_version" in obj:
        return "/v1/messages"
    return "/v1/chat/completions"


def load_fixture_dir(repo_root: Path, fixture_dir: Path) -> tuple[list[Case], list[str]]:
    """Load recorded JSON requests from a fixture directory.

    Returns ``(cases, skipped)`` where ``skipped`` lists files that were not
    forwardable request bodies (schemas, non-JSON, etc.) so operators can see
    exactly which corpus inputs the runner did *not* replay.
    """
    repo_root = Path(repo_root)
    fixture_dir = Path(fixture_dir)
    if not fixture_dir.is_absolute():
        fixture_dir = repo_root / fixture_dir
    cases: list[Case] = []
    skipped: list[str] = []
    if not fixture_dir.is_dir():
        return cases, [f"{fixture_dir}: not a directory"]
    for path in sorted(fixture_dir.rglob("*.json")):
        body = path.read_bytes()
        endpoint = _sniff_path(body)
        if endpoint is None:
            skipped.append(f"{path.relative_to(repo_root)}: not a forwardable request body")
            continue
        cases.append(
            Case(
                name=f"fixture:{path.relative_to(repo_root)}",
                path=endpoint,
                headers=_ANTHROPIC_HEADERS if endpoint == "/v1/messages" else _OPENAI_HEADERS,
                body=body,
            )
        )
    return cases, skipped


def build_corpus(
    repo_root: Path, extra_dirs: list[str] | None = None
) -> tuple[list[Case], list[str]]:
    """Default corpus plus any explicitly requested fixture directories.

    Returns ``(cases, notes)`` where ``notes`` are informational (skipped
    files, missing dirs) surfaced in the shadow report.
    """
    cases = default_corpus(repo_root)
    notes: list[str] = []
    for extra in extra_dirs or []:
        extra_cases, skipped = load_fixture_dir(repo_root, Path(extra))
        cases.extend(extra_cases)
        notes.extend(skipped)
        if extra_cases:
            notes.append(f"{extra}: {len(extra_cases)} request(s) loaded")
        else:
            notes.append(f"{extra}: no forwardable requests found")
    return cases, notes


__all__ = ["ANTHROPIC_FIXTURE", "Case", "build_corpus", "default_corpus", "load_fixture_dir"]
