use headroom_core::ccr::InMemoryCcrStore;
use headroom_core::transforms::pipeline::block_compressor::{BlockCompressor, PipelineBlockCompressor};
use headroom_core::transforms::pipeline::config::PipelineConfig;
use headroom_core::transforms::pipeline::traits::CompressionContext;
use headroom_core::transforms::ContentType;

#[test]
fn pipeline_block_compressor_compresses_json_array() {
    let config = PipelineConfig::default();
    let compressor = PipelineBlockCompressor::from_config(&config);
    let store = InMemoryCcrStore::new();
    let ctx = CompressionContext::default();

    // A pretty-printed JSON object — JsonMinifier should strip whitespace.
    let input = "{\n  \"a\": 1,\n  \"b\": 2\n}";
    let result = compressor.compress(input, ContentType::JsonArray, &ctx, Some(&store));
    assert!(result.bytes_saved > 0, "expected bytes saved on pretty JSON");
    assert!(!result.steps_applied.is_empty(), "expected at least one step");
}

#[test]
fn pipeline_block_compressor_returns_zero_for_empty_input() {
    let config = PipelineConfig::default();
    let compressor = PipelineBlockCompressor::from_config(&config);
    let store = InMemoryCcrStore::new();
    let ctx = CompressionContext::default();

    let result = compressor.compress("", ContentType::PlainText, &ctx, Some(&store));
    assert_eq!(result.bytes_saved, 0);
    assert!(result.steps_applied.is_empty());
}

#[test]
fn pipeline_block_compressor_noop_for_unrelated_content_type() {
    let config = PipelineConfig::default();
    let compressor = PipelineBlockCompressor::from_config(&config);
    let store = InMemoryCcrStore::new();
    let ctx = CompressionContext::default();

    let result = compressor.compress("not json", ContentType::PlainText, &ctx, Some(&store));
    assert_eq!(result.bytes_saved, 0);
    assert!(result.steps_applied.is_empty());
}
