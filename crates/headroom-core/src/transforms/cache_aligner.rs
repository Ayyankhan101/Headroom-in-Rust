//! Detector-only `cache_aligner` transform port
//! (`headroom/transforms/cache_aligner.py`).
//!
//! PR-A2 / P2-23: the Python transform is **detector-only** — it never
//! mutates messages and never rewrites the system prompt (the old
//! `[Dynamic Context]` rewrite path was deliberately deleted). `apply`
//! returns a `TransformResult` whose `messages` equal the input, with
//! `warnings` and `cache_metrics` populated for observability. This
//! module ports that detector — structural checks only, **no regex**
//! (realignment build-constraints policy) — plus the observability
//! scope / metrics computation, so the parity harness can compare
//! fixture outputs byte-for-byte.
//!
//! # Relationship to the proxy's `volatile_detector`
//!
//! `headroom-proxy` has its own request-scoped volatile detector
//! (`cache_stabilization/volatile_detector.rs`) that scans JSON request
//! bodies for substring windows (ISO-8601 prefix, UUID v4 only,
//! ID-named keys). This transform detector is different by design: it
//! splits *message text* into whitespace tokens and classifies each
//! whole token (any-version UUID, full ISO-8601 parse, JWT shape,
//! MD5/SHA1/SHA256 hex hashes). The shapes do not line up, so the two
//! stay separate (per the phase-1 plan: proxy detector is request-scoped,
//! transform result is message-scoped).

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Canonical UUID (RFC 4122) with dashes is 36 chars. The 32-char
/// dashless form is structurally identical to an MD5 hex digest and is
/// deliberately NOT accepted as a UUID (it classifies as a hex hash),
/// mirroring Python.
const UUID_CANONICAL_LEN: usize = 36;

/// JWT shape constraints: exactly three base64url segments joined by
/// `.`, no signature verification (we never have the key).
const JWT_SEGMENT_COUNT: usize = 3;
const JWT_MIN_SEGMENT_BYTES: usize = 4;

/// Hex hash length profile: MD5=32, SHA1=40, SHA256=64.
const HEX_HASH_LENGTHS: [usize; 3] = [32, 40, 64];

/// Stable labels — log consumers filter on these; keep in lockstep with
/// Python's `_LABEL_*` constants.
const LABEL_UUID: &str = "uuid";
const LABEL_ISO8601: &str = "iso8601";
const LABEL_JWT: &str = "jwt";
const LABEL_HEX_HASH: &str = "hex_hash";

/// One detected piece of volatile content. `sample` is truncated — never
/// the full content, so secrets aren't logged verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub label: String,
    pub sample: String,
}

/// Cache prefix metrics — mirrors Python `CachePrefixMetrics`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePrefixMetrics {
    pub stable_prefix_bytes: usize,
    pub stable_prefix_tokens_est: usize,
    pub stable_prefix_hash: String,
    /// Always `false` for a stateless detection: `prefix_changed` /
    /// `previous_hash` are per-instance session state in Python, and the
    /// parity fixtures are recorded against a fresh instance per call.
    pub prefix_changed: bool,
    pub previous_hash: Option<String>,
}

/// Detector-only result — mirrors the serialized `TransformResult`
/// fields the Python recorder writes (the empty/null fields are emitted
/// by the comparator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheAlignResult {
    pub messages: Vec<Value>,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub warnings: Vec<String>,
    pub cache_metrics: CachePrefixMetrics,
    pub markers_inserted: Vec<String>,
}

/// Split content into whitespace-delimited tokens, stripping surrounding
/// punctuation that commonly wraps an inline token. Mirrors Python
/// `_split_tokens` (`str.split` + `str.strip`).
fn split_tokens(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    const STRIP: &[char] = &[
        '.', ',', ';', ':', '!', '?', '"', '\'', '(', ')', '[', ']', '{', '}', '<', '>',
    ];
    content
        .split_whitespace()
        .filter_map(|raw| {
            let cleaned = raw.trim_matches(STRIP);
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned.to_string())
            }
        })
        .collect()
}

/// Canonical dashed UUID: 36 chars, hyphens at 8/13/18/23, hex elsewhere.
/// Any version nibble is accepted — Python's `uuid.UUID` does not
/// validate the version either.
fn is_uuid(token: &str) -> bool {
    let b = token.as_bytes();
    if b.len() != UUID_CANONICAL_LEN {
        return false;
    }
    if b.iter().filter(|&&c| c == b'-').count() != 4 {
        return false;
    }
    for (i, &c) in b.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if c != b'-' {
                    return false;
                }
            }
            _ => {
                if !c.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// ISO 8601 datetime/date, structural parse approximating Python's
/// `datetime.fromisoformat` for the forms the fixtures (and real system
/// prompts) use: `YYYY-MM-DD` date-only and
/// `YYYY-MM-DD[Tt ]HH:MM[:SS[.fff]][Z|±HH[:MM]|±HHMM]`. Week dates and
/// ordinal dates that `fromisoformat` also accepts are out of scope —
/// the parity fixtures never exercise them.
fn is_iso8601(token: &str) -> bool {
    if token.len() < 8 {
        return false;
    }
    let bytes = token.as_bytes();
    if !bytes.contains(&b'T') && !bytes.contains(&b't') && !bytes.contains(&b'-') {
        return false;
    }
    let digit = |r: std::ops::Range<usize>| {
        bytes.get(r).map(|s| s.iter().all(u8::is_ascii_digit)) == Some(true)
    };
    // date part: YYYY-MM-DD (ranges validated like `fromisoformat`).
    if !(digit(0..4)
        && bytes.get(4) == Some(&b'-')
        && digit(5..7)
        && bytes.get(7) == Some(&b'-')
        && digit(8..10))
    {
        return false;
    }
    let two = |r: std::ops::Range<usize>| -> Option<u8> {
        bytes
            .get(r)
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| s.parse::<u8>().ok())
    };
    let month = two(5..7).unwrap_or(0);
    let day = two(8..10).unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    let mut i = 10;
    if i == bytes.len() {
        return true; // date-only
    }
    match bytes[i] {
        b'T' | b't' | b' ' => i += 1,
        _ => return false,
    }
    // time part: HH:MM[:SS[.fraction]] (ranges validated like
    // `fromisoformat`).
    if !(digit(i..i + 2) && bytes.get(i + 2) == Some(&b':') && digit(i + 3..i + 5)) {
        return false;
    }
    let hour = two(i..i + 2).unwrap_or(0);
    let minute = two(i + 3..i + 5).unwrap_or(0);
    if hour > 23 || minute > 59 {
        return false;
    }
    i += 5;
    if bytes.get(i) == Some(&b':') && digit(i + 1..i + 3) {
        let sec = two(i + 1..i + 3).unwrap_or(0);
        if sec > 59 {
            return false;
        }
        i += 3;
    }
    if bytes.get(i) == Some(&b':') && digit(i + 1..i + 3) {
        i += 3;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    // optional timezone
    if i < bytes.len() {
        match bytes[i] {
            b'Z' | b'z' => i += 1,
            b'+' | b'-' => {
                i += 1;
                if !digit(i..i + 2) {
                    return false;
                }
                i += 2;
                if bytes.get(i) == Some(&b':') {
                    if !digit(i + 1..i + 3) {
                        return false;
                    }
                    i += 3;
                } else if digit(i..i + 2) {
                    i += 2;
                }
            }
            _ => return false,
        }
    }
    i == bytes.len()
}

/// JWT shape: exactly three base64url segments. Mirrors Python: raw
/// segment length >= 4, ASCII-only (`.encode("ascii")` raises
/// otherwise), and after discarding non-alphabet chars the base64
/// payload length must not be 1 mod 4 (that is a hard
/// `binascii.Error` in `a2b_base64`).
fn is_jwt_shape(token: &str) -> bool {
    let segments: Vec<&str> = token.split('.').collect();
    if segments.len() != JWT_SEGMENT_COUNT {
        return false;
    }
    segments.iter().all(|seg| {
        if seg.len() < JWT_MIN_SEGMENT_BYTES || !seg.is_ascii() {
            return false;
        }
        let data = seg
            .bytes()
            .filter(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
            .count();
        data % 4 != 1
    })
}

/// MD5/SHA1/SHA256 hex digest: fixed length, all hex digits.
fn is_hex_hash(token: &str) -> bool {
    if !HEX_HASH_LENGTHS.contains(&token.len()) {
        return false;
    }
    token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Classify a token; `None` when it matches no volatile pattern. Order
/// matters (mirrors Python): more specific / longer checks first so a
/// dashless UUID doesn't get misclassified as a hex hash.
fn classify_token(token: &str) -> Option<&'static str> {
    if is_uuid(token) {
        return Some(LABEL_UUID);
    }
    if token.contains('.') && is_jwt_shape(token) {
        return Some(LABEL_JWT);
    }
    if is_iso8601(token) {
        return Some(LABEL_ISO8601);
    }
    if is_hex_hash(token) {
        return Some(LABEL_HEX_HASH);
    }
    None
}

/// Truncate a finding sample: keep short tokens whole; otherwise
/// `first8...last4` (Python `token[:8] + "..." + token[-4:]`).
fn truncate_sample(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 16 {
        return token.to_string();
    }
    let head: String = chars.iter().take(8).collect();
    let tail: String = chars
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}...{tail}")
}

/// Detect volatile content in arbitrary text (pure, no mutation).
/// Mirrors Python `detect_volatile_content`.
pub fn detect_volatile_content(content: &str) -> Vec<Finding> {
    if content.is_empty() {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for token in split_tokens(content) {
        if let Some(label) = classify_token(&token) {
            findings.push(Finding {
                label: label.to_string(),
                sample: truncate_sample(&token),
            });
        }
    }
    findings
}

/// One warning string per apply call when any finding exists, with the
/// per-label counts sorted by label. Byte-for-byte match with Python's
/// `warnings` construction in `CacheAligner.apply`.
fn detect_warnings(messages: &[Value]) -> Vec<String> {
    let mut all: Vec<Finding> = Vec::new();
    for msg in messages {
        if msg.get("role").and_then(Value::as_str) != Some("system") {
            continue;
        }
        let content = match msg.get("content") {
            Some(Value::String(s)) => s.as_str(),
            _ => continue, // non-string / missing content is skipped
        };
        all.extend(detect_volatile_content(content));
    }
    if all.is_empty() {
        return Vec::new();
    }
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for f in &all {
        *counts.entry(&f.label).or_insert(0) += 1;
    }
    let counts_str = counts
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ");
    vec![format!(
        "CacheAligner: detected volatile content in system prompt ({counts_str}); cache prefix unstable. Move dynamic values out of the system prompt to recover cache hits."
    )]
}

/// Stable-prefix observability scope: join all system-message string
/// contents with `\n---\n` (mirrors `_stable_prefix_observability_scope`
/// with `frozen_message_count=0`, which is what the recorder drives).
/// Returns `(scope_text, utf8_byte_len)`.
fn stable_prefix_scope(messages: &[Value]) -> (String, usize) {
    let parts: Vec<&str> = messages
        .iter()
        .filter(|m| m.get("role").and_then(Value::as_str) == Some("system"))
        .filter_map(|m| match m.get("content") {
            Some(Value::String(s)) => Some(s.as_str()),
            _ => None, // non-string content is excluded entirely (Python)
        })
        .collect();
    let scope = parts.join("\n---\n");
    let bytes = scope.len(); // == Python len(scope_text.encode("utf-8")) for UTF-8
    (scope, bytes)
}

/// `sha256(utf8)[:16]` — Python `compute_short_hash(scope_text)`.
fn short_hash(scope_text: &str) -> String {
    let digest = Sha256::digest(scope_text.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..16].to_string()
}

/// Count tokens for one message, mirroring Python
/// `OpenAITokenCounter.count_message` for the message shapes the parity
/// fixtures record (`role` + string `content`): 3 base + role + content.
/// Other fields (name, tool_calls, structured content blocks beyond text)
/// are not exercised by fixtures and contribute 0, matching Python's
/// behavior for absent fields.
fn count_message(count_text: &dyn Fn(&str) -> usize, msg: &Value) -> usize {
    let mut tokens = 3usize;
    tokens += count_text(msg.get("role").and_then(Value::as_str).unwrap_or(""));
    match msg.get("content") {
        Some(Value::String(s)) => tokens += count_text(s),
        Some(Value::Array(blocks)) => {
            for block in blocks {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    tokens += count_text(text);
                }
            }
        }
        _ => {}
    }
    tokens
}

/// Count tokens for a message list, mirroring Python
/// `OpenAITokenCounter.count_messages` (sum of per-message counts plus 3
/// priming tokens).
fn count_messages(count_text: &dyn Fn(&str) -> usize, messages: &[Value]) -> usize {
    messages
        .iter()
        .map(|m| count_message(count_text, m))
        .sum::<usize>()
        + 3
}

/// Run the detector-only transform. `count_text` is injected so the
/// harness can use its own tokenizer (the fixtures were recorded with
/// o200k via `OpenAITokenCounter("gpt-4o-mini")`).
///
/// Messages are returned unchanged (a clone of the input — the transform
/// never mutates), `warnings` carry the detection summary, and
/// `cache_metrics` are computed from the stable-prefix scope.
pub fn detect_cache_volatility<F>(messages: &[Value], count_text: F) -> CacheAlignResult
where
    F: Fn(&str) -> usize,
{
    let warnings = detect_warnings(messages);
    let (scope_text, scope_bytes) = stable_prefix_scope(messages);
    let stable_hash = short_hash(&scope_text);
    let cache_metrics = CachePrefixMetrics {
        stable_prefix_bytes: scope_bytes,
        stable_prefix_tokens_est: count_text(&scope_text),
        stable_prefix_hash: stable_hash.clone(),
        prefix_changed: false,
        previous_hash: None,
    };
    let tokens = count_messages(&count_text, messages);
    CacheAlignResult {
        messages: messages.to_vec(),
        tokens_before: tokens,
        tokens_after: tokens,
        warnings,
        cache_metrics,
        markers_inserted: vec![format!("stable_prefix_hash:{stable_hash}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(s: &str) -> Option<&'static str> {
        classify_token(s)
    }

    #[test]
    fn classifies_uuid_any_version() {
        assert_eq!(
            classify("123e4567-e89b-12d3-a456-426614174000"),
            Some(LABEL_UUID)
        );
        assert_eq!(
            classify("550e8400-e29b-41d4-a716-446655440000"),
            Some(LABEL_UUID)
        );
        // Dashless 32-hex form is a hex hash, not a UUID (Python parity).
        assert_eq!(
            classify("550e8400e29b41d4a716446655440000"),
            Some(LABEL_HEX_HASH)
        );
        // Bad hyphen position is not a UUID.
        assert_eq!(classify("123e4567e89b-12d3-a456-426614174000"), None);
    }

    #[test]
    fn classifies_iso8601() {
        assert_eq!(classify("2026-08-17"), Some(LABEL_ISO8601));
        assert_eq!(classify("2026-08-14T09:30:00Z"), Some(LABEL_ISO8601));
        assert_eq!(classify("2026-08-14T09:30:00"), Some(LABEL_ISO8601));
        assert_eq!(classify("2026-08-14T09:30:00+05:30"), Some(LABEL_ISO8601));
        assert_eq!(classify("2026-08-14 09:30"), Some(LABEL_ISO8601));
        // No T and no dash → rejected before parsing.
        assert_eq!(classify("09:30:00"), None);
        // Out-of-range dates are rejected like fromisoformat.
        assert_eq!(classify("2026-13-45"), None);
        assert_eq!(classify("2026-08-32"), None);
    }

    #[test]
    fn classifies_jwt_shape() {
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjMifQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        assert_eq!(classify(jwt), Some(LABEL_JWT));
        // Two segments is not a JWT.
        assert_eq!(classify("abc.def"), None);
    }

    #[test]
    fn classifies_hex_hashes_by_length() {
        assert_eq!(
            classify("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"),
            Some(LABEL_HEX_HASH)
        );
        assert_eq!(
            classify("0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33"),
            Some(LABEL_HEX_HASH)
        );
        assert_eq!(
            classify("d41d8cd98f00b204e9800998ecf8427e"),
            Some(LABEL_HEX_HASH)
        );
        // Non-hex char breaks the hash classification.
        assert_eq!(classify("d41d8cd98f00b204e9800998ecf8427g"), None);
    }

    #[test]
    fn split_tokens_strips_punctuation() {
        assert_eq!(
            split_tokens("The date is 2026-08-17. Request id 0."),
            vec!["The", "date", "is", "2026-08-17", "Request", "id", "0"]
        );
        assert_eq!(split_tokens(""), Vec::<String>::new());
    }

    #[test]
    fn truncate_sample_matches_python() {
        let long = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        assert_eq!(truncate_sample(long), "9f86d081...0a08");
        assert_eq!(truncate_sample("short"), "short");
    }

    #[test]
    fn warnings_sorted_by_label() {
        let messages = vec![
            serde_json::json!({"role": "system", "content":
                "Current date: 2026-08-14T09:30:00Z. Trace 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08."}),
            serde_json::json!({"role": "user", "content": "hi"}),
        ];
        let warnings = detect_warnings(&messages);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("(hex_hash=1, iso8601=1)"),
            "counts must be label-sorted: {warnings:?}"
        );
    }

    #[test]
    fn stable_content_yields_no_warnings() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "Stable system prompt, no volatile content at all."}),
        ];
        assert!(detect_warnings(&messages).is_empty());
    }

    #[test]
    fn scope_joins_system_strings_only() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "a"}),
            serde_json::json!({"role": "user", "content": "b"}),
            serde_json::json!({"role": "system", "content": "c"}),
        ];
        let (scope, bytes) = stable_prefix_scope(&messages);
        assert_eq!(scope, "a\n---\nc");
        assert_eq!(bytes, scope.len());
    }

    #[test]
    fn short_hash_is_16_hex_chars() {
        let h = short_hash("hello");
        assert_eq!(h.len(), 16);
        assert!(h.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn detection_never_mutates_messages() {
        let messages = vec![
            serde_json::json!({"role": "system", "content": "UUID 123e4567-e89b-12d3-a456-426614174000."}),
        ];
        let count = |_: &str| 0usize;
        let result = detect_cache_volatility(&messages, count);
        assert_eq!(
            result.messages, messages,
            "messages must round-trip unchanged"
        );
        assert_eq!(
            result.markers_inserted,
            vec![format!(
                "stable_prefix_hash:{}",
                result.cache_metrics.stable_prefix_hash
            )]
        );
        assert_eq!(result.cache_metrics.previous_hash, None);
        assert!(!result.cache_metrics.prefix_changed);
    }
}
