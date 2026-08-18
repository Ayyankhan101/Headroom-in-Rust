//! Block-level compressor trait — wraps [`CompressionPipeline`] for
//! the live-zone dispatcher's per-block dispatch needs.
//!
//! The live-zone dispatcher identifies compressible blocks within a
//! request body and needs a way to compress each block's text content
//! independently. This trait provides that interface, and
//! [`PipelineBlockCompressor`] implements it by delegating to the
//! formal [`CompressionPipeline`].

use crate::ccr::CcrStore;
use crate::transforms::pipeline::config::PipelineConfig;
use crate::transforms::pipeline::orchestrator::CompressionPipeline;
use crate::transforms::pipeline::traits::CompressionContext;
use crate::transforms::ContentType;

/// Result of compressing a single block through the pipeline.
#[derive(Debug, Clone)]
pub struct BlockCompressResult {
    /// Compressed output text. Equal to input if nothing fired.
    pub output: String,
    /// Bytes saved (input.len() - output.len()), clamped to 0.
    pub bytes_saved: usize,
    /// CCR cache key if an offload stored the original. None for reformat-only.
    pub cache_key: Option<String>,
    /// Transform names that ran, in execution order.
    pub steps_applied: Vec<String>,
    /// Strategy tag for telemetry (the last step that fired).
    pub strategy: &'static str,
}

/// Compress a single block's text content through the pipeline.
pub trait BlockCompressor: Send + Sync {
    fn compress(
        &self,
        content: &str,
        content_type: ContentType,
        ctx: &CompressionContext,
        store: Option<&dyn CcrStore>,
    ) -> BlockCompressResult;
}

/// Implementation that delegates to [`CompressionPipeline`].
///
/// Constructed via [`PipelineBlockCompressor::from_config`] which
/// registers the standard set of reformat and offload transforms.
pub struct PipelineBlockCompressor {
    pipeline: CompressionPipeline,
}

impl PipelineBlockCompressor {
    pub fn from_config(config: &PipelineConfig) -> Self {
        use crate::transforms::pipeline::offloads::{
            DiffNoise, DiffOffload, JsonOffload, LogOffload, ProseFieldOffload,
        };
        use crate::transforms::pipeline::reformats::{JsonMinifier, LogTemplate};

        let pipeline = CompressionPipeline::builder()
            .with_reformat(JsonMinifier)
            .with_reformat(LogTemplate::new(config.reformat.log_template.clone()))
            .with_offload(JsonOffload::from_pipeline(config))
            .with_offload(LogOffload::new(config.bloat.log.clone()))
            .with_offload(DiffOffload::new(config.bloat.diff.clone()))
            .with_offload(DiffNoise::new(config.offload.diff_noise.clone()))
            .with_offload(ProseFieldOffload::new(config.offload.prose_field.clone()))
            .with_config(config.clone())
            .build();

        Self { pipeline }
    }
}

impl BlockCompressor for PipelineBlockCompressor {
    fn compress(
        &self,
        content: &str,
        content_type: ContentType,
        ctx: &CompressionContext,
        store: Option<&dyn CcrStore>,
    ) -> BlockCompressResult {
        // The pipeline requires a concrete store reference. When the
        // caller passes None, we skip offload transforms entirely
        // by using an in-memory store that we discard afterward.
        // The pipeline's reformats (JsonMinifier, LogTemplate) don't
        // need a store, so they still run.
        let empty_store = crate::ccr::InMemoryCcrStore::new();
        let store_ref: &dyn CcrStore = store.unwrap_or(&empty_store);
        let result = self.pipeline.run(content, content_type, ctx, store_ref);
        let strategy = match result.steps_applied.last().map(String::as_str) {
            Some("json_minifier") => "json_minifier",
            Some("log_template") => "log_template",
            Some("json_offload") => "smart_crusher",
            Some("log_offload") => "log_compressor",
            Some("diff_offload") => "diff_compressor",
            Some("diff_noise") => "diff_noise",
            Some("prose_field_offload") => "text_crusher",
            Some("search_offload") => "search_compressor",
            _ => "pipeline",
        };
        BlockCompressResult {
            output: result.output,
            bytes_saved: result.bytes_saved,
            cache_key: result.cache_keys.into_iter().next(),
            steps_applied: result.steps_applied,
            strategy,
        }
    }
}
