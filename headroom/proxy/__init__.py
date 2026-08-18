"""Headroom proxy package.

The Python FastAPI proxy server was retired in PR-3.1; ``headroom proxy``
and ``wrap`` now run the Rust ``headroom-proxy`` binary. A small set of
modules survives here because off-path Python code (providers, CLI, evals,
integrations) still imports them; PR-3.3 relocates or retires them.
"""
