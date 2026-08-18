"""Headroom Backends - API translation layers for different LLM providers.

Backends handle the translation between the proxy's canonical format
(Anthropic Messages API) and provider-specific APIs.

Supported backend libraries:
- any-llm: 38+ providers (openai, anthropic, mistral, groq, ollama, etc.)

Usage:
    # any-llm backend
    headroom proxy --backend anyllm --anyllm-provider openai
"""

from .anyllm import AnyLLMBackend
from .base import Backend, BackendResponse, StreamEvent

__all__ = ["Backend", "BackendResponse", "StreamEvent", "AnyLLMBackend"]
