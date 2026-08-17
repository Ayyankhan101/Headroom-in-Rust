"""Tests for the `wrap` spawn path forwarding `HEADROOM_PROXY_BACKEND`.

Phase 2 Task 2.2: `headroom wrap` starts the proxy by spawning
``python -m headroom.cli proxy`` — the same command as always — and the
`HEADROOM_PROXY_BACKEND` switch inside the child CLI (Task 2.1) picks the
backend. This file verifies that:

(a) the env var (and `HEADROOM_PROXY_BACKEND_BIN`) reach the child process,
(b) the spawn command is unchanged — the switch happens inside the child,
(c) readiness polling (`_check_proxy`) is a plain TCP connect that works
    against any backend that binds the port, including the Rust binary, and
(d) config polling (`_query_proxy_config`) tolerates a config-less health
    payload (the Rust `/healthz` shape) without crashing.
"""

import socket
from unittest.mock import patch

from headroom.cli import wrap as wrap_mod


class _FakeProxyProcess:
    returncode = None

    def __init__(self):
        self.killed = False

    def poll(self):
        return None

    def kill(self):
        self.killed = True


def _capture_start_proxy_popen(monkeypatch, tmp_path):
    """Monkeypatch `_start_proxy`'s dependencies and capture the Popen call.

    Returns ``captured`` with the ``(args, kwargs)`` of the Popen spawn so
    tests can assert on both the child command and the child environment.
    """
    captured: dict = {}

    def fake_popen(*args, **kwargs):
        captured["args"] = args
        captured["kwargs"] = kwargs
        return _FakeProxyProcess()

    monkeypatch.delenv(wrap_mod._WRAP_PROXY_TIMEOUT_ENV, raising=False)
    monkeypatch.setattr(wrap_mod, "_ml_wrap_extras_detected", lambda: False)
    monkeypatch.setattr(wrap_mod, "_get_log_path", lambda: tmp_path / "proxy.log")
    monkeypatch.setattr(wrap_mod, "_check_proxy", lambda _port: True)
    monkeypatch.setattr(wrap_mod.time, "sleep", lambda _seconds: None)
    monkeypatch.setattr(wrap_mod.subprocess, "Popen", fake_popen)
    return captured


def test_wrap_forwards_backend_env_to_child(monkeypatch, tmp_path):
    """HEADROOM_PROXY_BACKEND=rust must reach the spawned proxy child."""
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "rust")
    captured = _capture_start_proxy_popen(monkeypatch, tmp_path)

    wrap_mod._start_proxy(8787, agent_type="codex")

    env = captured["kwargs"]["env"]
    assert env["HEADROOM_PROXY_BACKEND"] == "rust"


def test_wrap_forwards_backend_bin_env_to_child(monkeypatch, tmp_path):
    """HEADROOM_PROXY_BACKEND_BIN (custom binary path) must pass through."""
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "rust")
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND_BIN", "/opt/headroom/headroom-proxy")
    captured = _capture_start_proxy_popen(monkeypatch, tmp_path)

    wrap_mod._start_proxy(8787, agent_type="codex")

    env = captured["kwargs"]["env"]
    assert env["HEADROOM_PROXY_BACKEND"] == "rust"
    assert env["HEADROOM_PROXY_BACKEND_BIN"] == "/opt/headroom/headroom-proxy"


def test_wrap_spawn_command_unchanged(monkeypatch, tmp_path):
    """wrap still spawns `python -m headroom.cli proxy`; the switch is internal.

    Task 2.1 handles the branch inside the proxy CLI, so the wrap spawn
    command must NOT change: no direct binary spawn, no --backend flag.
    """
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "rust")
    captured = _capture_start_proxy_popen(monkeypatch, tmp_path)

    wrap_mod._start_proxy(8787, agent_type="codex")

    cmd = captured["args"][0]
    assert cmd[0] == wrap_mod.sys.executable
    assert cmd[1:3] == ["-m", "headroom.cli"]
    assert cmd[3] == "proxy"
    assert "--port" in cmd
    assert "--backend" not in cmd
    assert "headroom-proxy" not in cmd  # the binary name never appears in the command


def test_check_proxy_is_tcp_connect_backend_agnostic():
    """Readiness polling must work against any backend that binds the port.

    The Rust binary serves `/healthz` (no Python config payload), so the
    wrap readiness poll must be a plain TCP connect, not an HTTP GET that
    requires a Python-only payload key.
    """
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    port = listener.getsockname()[1]
    try:
        assert wrap_mod._check_proxy(port) is True
    finally:
        listener.close()

    # Once nothing is listening, the same probe reports not-ready.
    assert wrap_mod._check_proxy(port) is False


def test_query_proxy_config_tolerates_configless_health(monkeypatch):
    """A health payload without a `config` block must not crash the poll.

    The Rust proxy currently serves only `/healthz` (``{"ok": true,
    "service": "headroom-proxy"}``); the `/health` config block arrives in
    Task 2.3. Until then `_query_proxy_config` must return None gracefully
    instead of raising.
    """

    class _FakeResponse:
        def __enter__(self):
            return self

        def __exit__(self, *exc):  # noqa: ANN002, ANN003
            return False

        def read(self):
            return b'{"ok": true, "service": "headroom-proxy"}'

    def fake_urlopen(_url, timeout=2):  # noqa: ANN001, ANN202
        return _FakeResponse()

    with patch("urllib.request.urlopen", fake_urlopen):
        assert wrap_mod._query_proxy_config(8787) is None
