//! Fail-loud regression test for the Kompress ONNX deadlock (BASELINE.md
//! Finding F1).
//!
//! With the `kompress-v2-base` model cached but the ONNX Runtime dylib
//! unpinned/unavailable, `Kompress::from_files` used to hang indefinitely
//! inside `ort::load_dylib_from_path` (re-entrant `Once` →
//! `semaphore_wait_trap`, 0% CPU). The fix runs the same ort dylib guard the
//! magika detector uses (`dynamic_ort_loader_ready`) before touching any
//! `ort` API, so a missing dylib surfaces as a loud `Err` instead of a hang.
//!
//! This lives in its own integration-test binary so the process-global ort
//! loader guard starts uninitialized: pointing `ORT_DYLIB_PATH` at a
//! nonexistent library then deterministically exercises the failure path
//! (a shared-process unit test could already have the guard cached `Ok`).

use std::path::{Path, PathBuf};

use headroom_core::transforms::kompress::{Kompress, KompressConfig};

fn hf_cache_file(repo_dir: &str, rel: &[&str]) -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let snapshots = Path::new(&home)
        .join(".cache/huggingface/hub")
        .join(repo_dir)
        .join("snapshots");
    for snap in fs::read_dir(snapshots).ok()?.filter_map(|e| e.ok()) {
        let mut cand = snap.path();
        for part in rel {
            cand = cand.join(part);
        }
        if cand.exists() {
            return Some(cand);
        }
    }
    None
}

use std::fs;

#[test]
fn from_files_fails_loud_when_onnx_dylib_missing() {
    // Need a real tokenizer to get past `load_tokenizer` and reach the ONNX
    // guard; skip on machines without the HF cache (e.g. CI), matching
    // `tests/kompress_parity.rs`.
    let Some(tok) = hf_cache_file("models--answerdotai--ModernBERT-base", &["tokenizer.json"])
    else {
        eprintln!("SKIP: ModernBERT tokenizer not in HF cache");
        return;
    };

    // Fresh process → the ort loader guard has not run yet, so this bogus
    // path is honored and the guard returns a loud `Err` before any ort API
    // is touched. (In a shared-process test the guard could already be
    // cached `Ok` from another test, which is why this lives in its own
    // binary.)
    std::env::set_var("ORT_DYLIB_PATH", "/nonexistent/onnxruntime.dylib");

    let bogus_onnx = Path::new("/nonexistent/kompress-int8-wo.onnx");
    let err = Kompress::from_files(&tok, bogus_onnx, KompressConfig::default()).unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("ORT_DYLIB_PATH") || msg.contains("ONNX Runtime"),
        "expected a loud dylib error naming ORT_DYLIB_PATH / ONNX Runtime, got: {msg}"
    );
    // The hint must be actionable: point the operator at `set ORT_DYLIB_PATH`
    // (or `pip install onnxruntime`) so a dev with the cached model knows how
    // to unblock kompress instead of chasing a deadlock.
    assert!(
        msg.contains("set ORT_DYLIB_PATH")
            && (msg.contains("pip install onnxruntime") || msg.contains("onnxruntime")),
        "expected an actionable hint naming 'set ORT_DYLIB_PATH', got: {msg}"
    );
    eprintln!("fail-loud error as expected: {msg}");
}
