//! CCR tool-injection port (`headroom/ccr/tool_injection.py`).
//!
//! Mirrors the deterministic synchronous entry points the parity fixtures
//! record: `create_ccr_tool_definition("anthropic")` and the inject loop
//! of `CCRToolInjector.inject_tool_definition`, sticky-on branch only.
//! The parity harness drives this with `session_has_done_ccr=True` for
//! every fixture (see `CcrComparator`), so only the sticky-on gate is
//! modeled here — the per-request marker-scan path
//! (`scan_for_markers` / `verify_ownership`) has no Rust port and is not
//! exercised by fixtures.

use serde_json::{json, Value};

/// Tool name injected for CCR retrieval — must stay in lockstep with
/// `headroom/ccr/tool_injection.py::CCR_TOOL_NAME`.
pub const CCR_TOOL_NAME: &str = "headroom_retrieve";

/// Build the Anthropic-format `headroom_retrieve` tool definition.
///
/// Byte-for-byte match with `create_ccr_tool_definition("anthropic")`
/// (hash-only `input_schema`; the `query` property was removed
/// upstream). The description string is load-bearing: the parity
/// harness compares fixture output against this verbatim.
pub fn create_ccr_tool_definition() -> Value {
    json!({
        "name": CCR_TOOL_NAME,
        "description": "Retrieve original uncompressed content that was compressed to save tokens. Use this when you need more data than what's shown in compressed tool results. The hash is provided in compression markers like [N items compressed... hash=abc123].",
        "input_schema": {
            "type": "object",
            "properties": {
                "hash": {
                    "type": "string",
                    "description": "Hash key from the compression marker (e.g., 'abc123' from hash=abc123)",
                },
            },
            "required": ["hash"],
        },
    })
}

/// Whether `tool` is already the CCR retrieve tool, in either the
/// Anthropic form (`name`) or the OpenAI/MCP form (`function.name`).
/// Mirrors the Python loop in `inject_tool_definition`:
/// `tool.get("name") or tool.get("function", {}).get("name")`.
fn is_retrieve_tool(tool: &Value) -> bool {
    tool.get("name").and_then(Value::as_str) == Some(CCR_TOOL_NAME)
        || tool
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)
            == Some(CCR_TOOL_NAME)
}

/// Sticky-on injection: register the CCR retrieve tool unless a tool of
/// the same name is already present (e.g. from an MCP server).
///
/// Mirrors `CCRToolInjector.inject_tool_definition(tools,
/// session_has_done_ccr=True)`. When `session_has_done_ccr` is false and
/// no marker hashes are available (this port doesn't track them), the
/// tools are returned unchanged with `false` — the same observable
/// result as Python's no-marker, non-sticky path.
///
/// Returns `(updated_tools, was_injected)`.
pub fn inject_retrieve_tool(tools: &[Value], session_has_done_ccr: bool) -> (Vec<Value>, bool) {
    if !session_has_done_ccr {
        return (tools.to_vec(), false);
    }
    if tools.iter().any(is_retrieve_tool) {
        return (tools.to_vec(), false);
    }
    let mut updated = tools.to_vec();
    updated.push(create_ccr_tool_definition());
    (updated, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_is_hash_only_anthropic_format() {
        let def = create_ccr_tool_definition();
        assert_eq!(def["name"], CCR_TOOL_NAME);
        // Anthropic format: top-level `input_schema`, not `parameters`.
        assert!(def.get("input_schema").is_some());
        assert!(def.get("parameters").is_none());
        let props = def["input_schema"]["properties"].as_object().unwrap();
        assert!(props.contains_key("hash"));
        // The removed `query` property must not come back.
        assert!(!props.contains_key("query"));
        assert_eq!(
            def["input_schema"]["required"],
            json!(["hash"]),
            "hash must be the only required property"
        );
        // Description must match Python verbatim (parity contract).
        assert_eq!(
            def["description"],
            "Retrieve original uncompressed content that was compressed to save tokens. Use this when you need more data than what's shown in compressed tool results. The hash is provided in compression markers like [N items compressed... hash=abc123]."
        );
    }

    #[test]
    fn injects_when_sticky_on() {
        let tools = vec![json!({"name": "other_tool"})];
        let (updated, injected) = inject_retrieve_tool(&tools, true);
        assert!(injected);
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[1]["name"], CCR_TOOL_NAME);
    }

    #[test]
    fn injects_into_empty_and_null_inputs() {
        let (updated, injected) = inject_retrieve_tool(&[], true);
        assert!(injected);
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0]["name"], CCR_TOOL_NAME);
    }

    #[test]
    fn skips_when_tool_already_present_anthropic_form() {
        let tools = vec![json!({"name": CCR_TOOL_NAME})];
        let (updated, injected) = inject_retrieve_tool(&tools, true);
        assert!(!injected);
        assert_eq!(
            updated, tools,
            "already-present tool must not be duplicated"
        );
    }

    #[test]
    fn skips_when_tool_already_present_mcp_form() {
        let tools = vec![json!({"type": "function", "function": {"name": CCR_TOOL_NAME}})];
        let (updated, injected) = inject_retrieve_tool(&tools, true);
        assert!(!injected);
        assert_eq!(updated, tools);
    }

    #[test]
    fn non_sticky_returns_unchanged() {
        let tools = vec![json!({"name": "other_tool"})];
        let (updated, injected) = inject_retrieve_tool(&tools, false);
        assert!(!injected);
        assert_eq!(updated, tools);
    }

    #[test]
    fn mcp_other_tool_still_injects() {
        let tools = vec![json!({"type": "function", "function": {"name": "other_tool"}})];
        let (updated, injected) = inject_retrieve_tool(&tools, true);
        assert!(injected);
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[1]["name"], CCR_TOOL_NAME);
    }
}
