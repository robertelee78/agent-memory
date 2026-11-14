//! Optional validator-cache generation, bound to the complete Rust artifact.
use crate::artifact;
use crate::pack_validation;
use std::path::Path;

/// Replacement for Python's executing-package/bytecode generation. Callers
/// treat failure as a cache miss; canonical Markdown operations stay available.
/// inspect_runtime freshly observes both pack files and the installed binary.
pub fn engine_generation(pack: &Path, binary: &Path) -> pack_validation::Result<String> {
    let inspection = artifact::inspect_runtime(pack, binary)?;
    inspection
        .get("artifact_digest")
        .and_then(crate::codec::json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            pack_validation::Invalid::new(
                "runtime-binary",
                artifact::BINARY_ENTRY,
                "Artifact generation is unavailable",
            )
        })
}
