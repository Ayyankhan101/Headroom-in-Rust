"""Tests for the `HEADROOM_PROXY_BACKEND` switch in `headroom proxy`.

Phase 2 Task 2.1: when `HEADROOM_PROXY_BACKEND=rust`, `headroom proxy`
spawns the Rust `headroom-proxy` binary (with flags mapped onto its env
surface) and does NOT import the Python `headroom.proxy.server` request
path. When `python` (default) or unset, behavior is unchanged.
"""

import builtins

import pytest
from click.testing import CliRunner

from headroom.cli.proxy import proxy


@pytest.fixture
def fake_rust_binary(tmp_path):
    """A fake `headroom-proxy` executable the test can point at."""
    fake_bin = tmp_path / "headroom-proxy"
    fake_bin.write_text("#!/bin/sh\necho fake-rust-binary\n")
    fake_bin.chmod(0o755)
    return str(fake_bin)


def test_proxy_backend_rust_spawns_binary(monkeypatch, fake_rust_binary):
    """`HEADROOM_PROXY_BACKEND=rust` spawns the Rust binary, not run_server."""
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "rust")
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND_BIN", fake_rust_binary)

    # Replace spawn + readiness polling with capture.
    spawned = {}
    import headroom.cli.proxy as p

    monkeypatch.setattr(
        p, "_spawn_rust_proxy", lambda cmd, **kw: spawned.update(cmd=cmd) or object()
    )

    runner = CliRunner()
    result = runner.invoke(proxy, ["--port", "8787", "--no-optimize"])
    assert result.exit_code == 0, result.output
    assert "headroom-proxy" in " ".join(spawned["cmd"])
    # CLI flags are mapped onto the Rust env surface, not passed as args.
    assert "--port" not in spawned["cmd"]


def test_proxy_backend_rust_does_not_import_python_server(monkeypatch, fake_rust_binary):
    """The rust branch must not import `headroom.proxy.server`."""
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "rust")
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND_BIN", fake_rust_binary)

    import headroom.cli.proxy as p

    calls = []

    def fake_spawn(cmd, **kw):
        calls.append(cmd)
        return object()

    monkeypatch.setattr(p, "_spawn_rust_proxy", fake_spawn)
    # Poison the Python server import: if the rust branch ever imports it,
    # the invoke raises instead of spawning.
    real_import = builtins.__import__

    def guarded_import(name, *args, **kwargs):
        if name == "headroom.proxy.server" or name.startswith("headroom.proxy.server."):
            raise AssertionError("rust backend must not import headroom.proxy.server")
        return real_import(name, *args, **kwargs)

    monkeypatch.setattr(builtins, "__import__", guarded_import)

    runner = CliRunner()
    result = runner.invoke(proxy, ["--port", "8787", "--no-optimize"])
    assert result.exit_code == 0, result.output
    assert calls, "expected _spawn_rust_proxy to be called"


def test_rust_proxy_env_mapping_maps_cli_flags(monkeypatch, fake_rust_binary):
    """CLI flags map onto the Rust env surface (listen + compression)."""
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "rust")
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND_BIN", fake_rust_binary)
    # A pre-existing env var must survive the mapping (pass-through base).
    monkeypatch.setenv("HEADROOM_PROXY_UPSTREAM", "http://127.0.0.1:8788")

    import headroom.cli.proxy as p

    env = p._rust_proxy_env_mapping(host="127.0.0.1", port=8787, no_optimize=False)
    assert env["HEADROOM_PROXY_LISTEN"] == "127.0.0.1:8787"
    assert env["HEADROOM_PROXY_COMPRESSION"] == "1"
    assert env["HEADROOM_PROXY_UPSTREAM"] == "http://127.0.0.1:8788"

    env_noopt = p._rust_proxy_env_mapping(host="0.0.0.0", port=9000, no_optimize=True)
    assert env_noopt["HEADROOM_PROXY_LISTEN"] == "0.0.0.0:9000"
    assert env_noopt["HEADROOM_PROXY_COMPRESSION"] == "0"


def test_proxy_backend_invalid_value_exits(monkeypatch):
    """A value other than python/rust is a hard error (exit 2)."""
    monkeypatch.setenv("HEADROOM_PROXY_BACKEND", "swift")

    import headroom.cli.proxy as p

    monkeypatch.setattr(p, "_spawn_rust_proxy", lambda cmd, **kw: object())

    runner = CliRunner()
    result = runner.invoke(proxy, ["--port", "8787", "--no-optimize"])
    assert result.exit_code == 2, result.output
