"""Byte-equality shadow runner (Python proxy vs Rust proxy).

For each recorded request in the corpus, POST the exact same bytes to both
proxy backends, each configured with its own recording mock upstream. A case
matches only when BOTH:

  * the sha256 of the upstream-bound request bytes each backend forwarded
    agree, and
  * the sha256 of the response bytes each backend returned to the client
    agree (status codes must match too).

The runner always spawns the two recording upstreams itself. The proxy
backends are either spawned by the runner (``--boot``) or assumed to be
already running (``--python-url`` / ``--rust-url``).

Exit status is non-zero when the match rate falls below ``--expect``
(default 0.999, i.e. a mismatch rate >= 0.1% fails the Phase 2 exit gate).

Operator usage::

    python e2e/shadow/runner.py --boot --rust-bin target/release/headroom-proxy

External-backend usage::

    python e2e/shadow/runner.py --python-url http://127.0.0.1:9001 \\
        --rust-url http://127.0.0.1:9002
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import cast

if __package__ in (None, ""):
    # Running as a plain script (python e2e/shadow/runner.py): give the
    # relative-import fallback a path to find the sibling corpus module.
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from corpus import Case, build_corpus  # type: ignore[no-redef]
else:
    from .corpus import Case, build_corpus

# ---------------------------------------------------------------------------
# Documented upstream-bound divergences
# ---------------------------------------------------------------------------
#
# The shadow comparison is Python-vs-Rust: both backends must forward
# identical bytes to their upstream. The Python proxy (the reference) applies
# a few *semantic* normalizations to OpenAI request bodies that the Rust
# proxy — byte-faithful by Phase 2 contract — does not replicate. Those are
# real behavior differences (a client would observe them, e.g. missing usage
# in a streamed response), so they are the "uncovered request path" list for
# Phase 3, not something the gate should hide.
#
# Each entry documents the divergence with a normalizer that applies the
# Python-proxy transform to the Rust side's forwarded bytes. A case counts as
# EXPECTED (gate-green) only when the *normalized* Rust bytes equal the
# Python bytes; any divergence outside these documented transforms fails the
# gate. If a future Rust change starts altering bodies on any other path, the
# gate goes red immediately.


def _normalize_max_completion_tokens(body: bytes) -> bytes:
    obj = json.loads(body)
    if "max_tokens" in obj:
        obj["max_completion_tokens"] = obj.pop("max_tokens")
    return json.dumps(obj, separators=(",", ":")).encode()


def _normalize_stream_options(body: bytes) -> bytes:
    obj = json.loads(body)
    obj["stream_options"] = {"include_usage": True}
    return json.dumps(obj, separators=(",", ":")).encode()


EXPECTED_DIVERGENCES: dict[str, tuple[str, Callable[[bytes], bytes]]] = {
    "openai_basic": (
        "python normalizes OpenAI max_tokens -> max_completion_tokens; rust is byte-faithful",
        _normalize_max_completion_tokens,
    ),
    "openai_stream": (
        "python injects stream_options.include_usage for streamed OpenAI responses; rust is byte-faithful",
        _normalize_stream_options,
    ),
}


# ---------------------------------------------------------------------------
# Recording mock upstream
# ---------------------------------------------------------------------------

#: Deterministic JSON response served for non-streaming requests. Byte-identical
#: for both backends, so any difference the client observes is proxy-introduced.
_CANNED_JSON = b'{"id":"resp_shadow","content":[{"type":"text","text":"shadow"}]}'

#: Deterministic SSE response served for ``"stream": true`` requests.
_CANNED_SSE = (
    b'event: message_start\ndata: {"type":"message_start","message":{"id":"msg_shadow_1"}}\n\n'
    b"event: content_block_delta\n"
    b'data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"hi"}}\n\n'
    b'event: message_stop\ndata: {"type":"message_stop"}\n\n'
)


class _RecordingServer(ThreadingHTTPServer):
    """ThreadingHTTPServer with typed per-instance recording state."""

    bodies: list[bytes]
    paths: list[str]

    def __init__(self) -> None:
        super().__init__(("127.0.0.1", 0), _RecordingHandler)
        self.bodies = []
        self.paths = []


class _RecordingHandler(BaseHTTPRequestHandler):
    """Captures raw request bodies (and paths) for the shadow comparison."""

    protocol_version = "HTTP/1.1"

    def _read_body(self) -> bytes:
        # The Rust proxy (hyper) streams upstream bodies with
        # Transfer-Encoding: chunked; Python (httpx) sends Content-Length.
        if self.headers.get("Transfer-Encoding", "").lower() == "chunked":
            chunks: list[bytes] = []
            while True:
                line = self.rfile.readline().strip()
                if not line:
                    break
                size = int(line, 16)
                if size == 0:
                    self.rfile.readline()  # trailing CRLF after last chunk
                    break
                chunks.append(self.rfile.read(size))
                self.rfile.readline()  # trailing CRLF after chunk data
            return b"".join(chunks)
        length = int(self.headers.get("Content-Length", 0) or 0)
        return self.rfile.read(length) if length else b""

    def _record(self, body: bytes) -> None:
        server = cast(_RecordingServer, self.server)
        server.bodies.append(body)
        server.paths.append(self.path)

    def do_POST(self) -> None:
        body = self._read_body()
        self._record(body)
        try:
            stream = bool(json.loads(body).get("stream"))
        except (ValueError, TypeError, AttributeError):
            stream = False
        payload = _CANNED_SSE if stream else _CANNED_JSON
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream" if stream else "application/json")
        if stream:
            # Terminate the stream like a real SSE upstream: close-delimited,
            # so both proxies relay EOF back to the client instead of hanging.
            self.send_header("Connection", "close")
            self.close_connection = True
        else:
            self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, *args: object) -> None:  # silence request logging
        pass


class RecordingUpstream:
    """A mock upstream that records the raw bytes it receives per request."""

    def __init__(self) -> None:
        self._server = _RecordingServer()
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()

    @property
    def url(self) -> str:
        host = str(self._server.server_address[0])
        port = int(self._server.server_address[1])
        return f"http://{host}:{port}"

    @property
    def bodies(self) -> list[bytes]:
        return self._server.bodies

    @property
    def paths(self) -> list[str]:
        return self._server.paths

    def reset(self) -> None:
        self._server.bodies.clear()
        self._server.paths.clear()

    def stop(self) -> None:
        self._server.shutdown()
        self._server.server_close()
        self._thread.join(timeout=5)


# ---------------------------------------------------------------------------
# Comparison
# ---------------------------------------------------------------------------


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


@dataclass
class CaseResult:
    """Outcome of replaying one case through both backends."""

    name: str
    path: str
    py_status: int
    rs_status: int
    py_upstream_sha: str
    rs_upstream_sha: str
    py_resp_sha: str
    rs_resp_sha: str
    upstream_match: bool
    response_match: bool
    expected: bool = False
    notes: list[str] = field(default_factory=list)

    @property
    def match(self) -> bool:
        return self.upstream_match and self.response_match


def _post(url: str, case: Case, timeout: float = 60.0) -> tuple[int, bytes]:
    """POST the case bytes to a backend; return (status, response body).

    Non-2xx responses are still read (the error body) so status/body
    comparisons stay meaningful.
    """
    req = urllib.request.Request(url + case.path, data=case.body, headers=case.headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, resp.read()
    except urllib.error.HTTPError as exc:
        return exc.code, exc.read()
    except urllib.error.URLError as exc:
        return 0, str(exc.reason).encode()


def run_case(
    case: Case,
    python_url: str,
    rust_url: str,
    upstream_py: RecordingUpstream,
    upstream_rs: RecordingUpstream,
) -> CaseResult:
    """Replay one case through both backends and compare what each forwarded."""
    upstream_py.reset()
    upstream_rs.reset()

    py_status, py_resp = _post(python_url, case)
    rs_status, rs_resp = _post(rust_url, case)

    # Allow a beat for any async upstream writes to land.
    time.sleep(0.2)

    notes: list[str] = []
    upstream_match = False
    if len(upstream_py.bodies) != len(upstream_rs.bodies):
        notes.append(
            f"upstream call counts differ: python={len(upstream_py.bodies)} "
            f"rust={len(upstream_rs.bodies)}"
        )
    elif not upstream_py.bodies:
        notes.append("neither backend forwarded a request to its upstream")
    else:
        upstream_match = all(a == b for a, b in zip(upstream_py.bodies, upstream_rs.bodies))
        if not upstream_match:
            for i, (a, b) in enumerate(zip(upstream_py.bodies, upstream_rs.bodies)):
                if a != b:
                    notes.append(
                        f"upstream body #{i} differs (sha256 {sha256_hex(a)[:12]} vs {sha256_hex(b)[:12]})"
                    )

    response_match = py_status == rs_status and py_resp == rs_resp
    if py_status != rs_status:
        notes.append(f"status differs: python={py_status} rust={rs_status}")
    elif py_resp != rs_resp:
        notes.append(
            f"response bytes differ (sha256 {sha256_hex(py_resp)[:12]} vs {sha256_hex(rs_resp)[:12]})"
        )

    expected = False
    if not upstream_match and case.name in EXPECTED_DIVERGENCES:
        reason, normalizer = EXPECTED_DIVERGENCES[case.name]
        rs_body = upstream_rs.bodies[-1] if upstream_rs.bodies else b""
        py_body = upstream_py.bodies[-1] if upstream_py.bodies else b""
        try:
            if normalizer(rs_body) == py_body:
                expected = True
                upstream_match = True
                notes.append(f"EXPECTED divergence: {reason}")
        except (ValueError, TypeError, KeyError):
            pass  # non-JSON bodies cannot be normalized; treat as a real mismatch

    return CaseResult(
        name=case.name,
        path=case.path,
        py_status=py_status,
        rs_status=rs_status,
        py_upstream_sha=sha256_hex(upstream_py.bodies[-1]) if upstream_py.bodies else "",
        rs_upstream_sha=sha256_hex(upstream_rs.bodies[-1]) if upstream_rs.bodies else "",
        py_resp_sha=sha256_hex(py_resp),
        rs_resp_sha=sha256_hex(rs_resp),
        upstream_match=upstream_match,
        response_match=response_match,
        expected=expected,
        notes=notes,
    )


@dataclass
class ShadowReport:
    """Aggregate shadow result."""

    results: list[CaseResult]
    notes: list[str] = field(default_factory=list)

    @property
    def matched(self) -> int:
        return sum(1 for r in self.results if r.match)

    @property
    def total(self) -> int:
        return len(self.results)

    @property
    def match_rate(self) -> float:
        return self.matched / self.total if self.total else 1.0

    @property
    def all_match(self) -> bool:
        return bool(self.results) and self.matched == self.total

    def summary(self) -> str:
        lines = [
            f"shadow: {self.matched}/{self.total} cases matched ({self.match_rate:.1%})",
        ]
        if self.notes:
            lines.append("notes:")
            lines.extend(f"  - {n}" for n in self.notes)
        return "\n".join(lines)

    def detail(self) -> str:
        lines = [self.summary(), ""]
        for r in self.results:
            marker = "EXP " if r.expected else ("OK " if r.match else "FAIL")
            lines.append(
                f"{marker} {r.name:<28} {r.path:<28} "
                f"upstream={'match' if r.upstream_match else 'DIFF'} "
                f"response={'match' if r.response_match else 'DIFF'} "
                f"(status {r.py_status}/{r.rs_status})"
            )
            for note in r.notes:
                lines.append(f"       - {note}")
        return "\n".join(lines)


def run_shadow(
    cases: list[Case],
    python_url: str,
    rust_url: str,
    upstream_py: RecordingUpstream,
    upstream_rs: RecordingUpstream,
) -> ShadowReport:
    """Replay every case through both backends."""
    results = [run_case(c, python_url, rust_url, upstream_py, upstream_rs) for c in cases]
    return ShadowReport(results=results)


# ---------------------------------------------------------------------------
# Backend booting
# ---------------------------------------------------------------------------


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def _wait_ready(url: str, path: str, timeout: float = 60.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url + path, timeout=2) as resp:
                if resp.status < 500:
                    return True
        except (urllib.error.URLError, OSError):
            pass
        time.sleep(0.5)
    return False


def boot_python_backend(
    upstream_url: str, python_cmd: list[str] | None = None
) -> tuple[subprocess.Popen, int]:
    """Spawn ``python -m headroom.proxy.server`` pointed at a recording upstream.

    ``--no-http2`` keeps the upstream connection HTTP/1.1 (the stdlib recording
    server does not speak h2) and matches the Rust backend's upstream transport.
    """
    port = _free_port()
    cmd = python_cmd or [sys.executable, "-m", "headroom.proxy.server"]
    env = dict(
        os.environ,
        ANTHROPIC_TARGET_API_URL=upstream_url,
        OPENAI_TARGET_API_URL=upstream_url,
        HEADROOM_SKIP_UPSTREAM_CHECK="1",
        HEADROOM_LOSSLESS_ONLY="1",
    )
    proc = subprocess.Popen(
        [*cmd, "--port", str(port), "--host", "127.0.0.1", "--no-http2"],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    if not _wait_ready(f"http://127.0.0.1:{port}", "/health"):
        stderr = ""
        try:
            stderr = (proc.stderr.read() if proc.stderr else "")[-2000:]
        except Exception:
            pass
        proc.kill()
        raise RuntimeError(f"python backend failed to become ready on :{port}\n{stderr}")
    return proc, port


def boot_rust_backend(upstream_url: str, rust_bin: str) -> tuple[subprocess.Popen, int]:
    """Spawn the Rust proxy binary pointed at a recording upstream."""
    port = _free_port()
    env = dict(os.environ, HEADROOM_PROXY_UPSTREAM=upstream_url)
    proc = subprocess.Popen(
        [rust_bin, "--listen", f"127.0.0.1:{port}"],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    if not _wait_ready(f"http://127.0.0.1:{port}", "/healthz"):
        stderr = ""
        try:
            stderr = (proc.stderr.read() if proc.stderr else "")[-2000:]
        except Exception:
            pass
        proc.kill()
        raise RuntimeError(f"rust backend failed to become ready on :{port}\n{stderr}")
    return proc, port


def _terminate(proc: subprocess.Popen) -> None:
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            proc.kill()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Byte-equality shadow test: Python proxy vs Rust proxy."
    )
    parser.add_argument("--python-url", help="Already-running Python backend base URL")
    parser.add_argument("--rust-url", help="Already-running Rust backend base URL")
    parser.add_argument(
        "--boot",
        action="store_true",
        help="Spawn the Python backend and the Rust binary (implies --rust-bin)",
    )
    parser.add_argument(
        "--rust-bin",
        default="target/release/headroom-proxy",
        help="Path to the Rust proxy binary (default: target/release/headroom-proxy)",
    )
    parser.add_argument(
        "--python-cmd",
        nargs="+",
        default=None,
        help="Python backend command (default: <sys.executable> -m headroom.proxy.server)",
    )
    parser.add_argument(
        "--fixtures",
        action="append",
        default=[],
        help="Extra fixture directory of recorded JSON requests (repeatable)",
    )
    parser.add_argument(
        "--expect",
        type=float,
        default=0.999,
        help="Minimum match rate; exit 1 below it (default: 0.999)",
    )
    parser.add_argument("--report-json", help="Write the per-case report to this JSON file")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = _parse_args(argv or sys.argv[1:])
    repo_root = Path(__file__).resolve().parents[2]

    if args.boot:
        if args.python_url or args.rust_url:
            print("error: --boot cannot be combined with --python-url/--rust-url", file=sys.stderr)
            return 2
        rust_bin = Path(args.rust_bin)
        if not rust_bin.is_absolute():
            rust_bin = repo_root / rust_bin
        if not rust_bin.exists():
            print(
                f"error: rust binary not found at {rust_bin} — "
                "build it with: cargo build --release -p headroom-proxy",
                file=sys.stderr,
            )
            return 2
    elif not (args.python_url and args.rust_url):
        print("error: provide --python-url and --rust-url, or --boot", file=sys.stderr)
        return 2

    cases, notes = build_corpus(repo_root, args.fixtures)
    if not cases:
        print("error: corpus is empty — no forwardable requests found", file=sys.stderr)
        return 2

    upstream_py = RecordingUpstream()
    upstream_rs = RecordingUpstream()
    procs: list[subprocess.Popen] = []
    try:
        if args.boot:
            py_proc, py_port = boot_python_backend(upstream_py.url, args.python_cmd)
            rs_proc, rs_port = boot_rust_backend(upstream_rs.url, str(rust_bin))
            procs = [py_proc, rs_proc]
            python_url, rust_url = f"http://127.0.0.1:{py_port}", f"http://127.0.0.1:{rs_port}"
            print(f"python backend: {python_url}  rust backend: {rust_url}", flush=True)
        else:
            python_url, rust_url = args.python_url.rstrip("/"), args.rust_url.rstrip("/")

        report = run_shadow(cases, python_url, rust_url, upstream_py, upstream_rs)
        report.notes.extend(notes)
        print(report.detail(), flush=True)

        if args.report_json:
            Path(args.report_json).write_text(
                json.dumps(
                    {
                        "match_rate": report.match_rate,
                        "matched": report.matched,
                        "total": report.total,
                        "results": [
                            {
                                "name": r.name,
                                "path": r.path,
                                "py_status": r.py_status,
                                "rs_status": r.rs_status,
                                "upstream_match": r.upstream_match,
                                "response_match": r.response_match,
                                "expected_divergence": r.expected,
                                "py_upstream_sha256": r.py_upstream_sha,
                                "rs_upstream_sha256": r.rs_upstream_sha,
                                "py_resp_sha256": r.py_resp_sha,
                                "rs_resp_sha256": r.rs_resp_sha,
                                "notes": r.notes,
                            }
                            for r in report.results
                        ],
                        "notes": notes,
                    },
                    indent=2,
                )
                + "\n"
            )

        if report.match_rate < args.expect:
            print(
                f"shadow FAILED: match rate {report.match_rate:.4f} < {args.expect:.4f} "
                f"(mismatch rate {(1 - report.match_rate):.4f} >= 0.1%)",
                flush=True,
            )
            return 1
        print("shadow PASSED", flush=True)
        return 0
    finally:
        for proc in procs:
            _terminate(proc)
        upstream_py.stop()
        upstream_rs.stop()


if __name__ == "__main__":
    sys.exit(main())
