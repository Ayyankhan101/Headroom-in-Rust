"""Tests for memory-injection request tags."""

from headroom.proxy.helpers import log_memory_injection


def test_log_memory_injection_marks_only_successful_injection() -> None:
    tags: dict[str, str] = {}

    log_memory_injection(
        request_id="hr_test_memory",
        session_id=None,
        decision="no_eligible_user_turn",
        bytes_injected=0,
        tags=tags,
    )
    assert "memory_injected" not in tags

    log_memory_injection(
        request_id="hr_test_memory",
        session_id=None,
        decision="injected_live_zone_tail",
        bytes_injected=42,
        tags=tags,
    )
    assert tags["memory_injected"] == "true"


