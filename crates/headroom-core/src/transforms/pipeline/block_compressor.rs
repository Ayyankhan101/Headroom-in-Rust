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
}

/// Compress a single block's text content through the pipeline.
pub trait BlockCompressor: Send + Sync {
    fn compress(
        &self,
        content: &str,
        content_type: ContentType,
        ctx: &CompressionContext,
        store: &dyn CcrStore,
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
        store: &dyn CcrStore,
    ) -> BlockCompressResult {
        let result = self.pipeline.run(content, content_type, ctx, store);
        BlockCompressResult {
            output: result.output,
            bytes_saved: result.bytes_saved,
            cache_key: result.cache_keys.into_iter().next(),
            steps_applied: result.steps_applied,
        }
    }
}
