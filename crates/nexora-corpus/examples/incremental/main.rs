mod benchmark_comparison;
mod benchmark_report;
mod benchmark_sample;
mod benchmark_workspace;
#[cfg(test)]
mod tests;
use benchmark_sample::BenchmarkSample;
use benchmark_workspace::BenchmarkWorkspace;
use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore, RepositoryWalker, SourceReader};
use sha2::{Digest, Sha256};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: incremental MANIFEST CASE ITERATIONS(1..100)".into());
    }
    let iterations: usize = args[3].parse()?;
    if !(1..=100).contains(&iterations) {
        return Err("iterations 1..100 required".into());
    }
    let suite = nexora_corpus::CorpusSuite::load(Path::new(&args[1]), Path::new("."))?;
    let case = suite
        .cases
        .iter()
        .find(|c| c.id == args[2])
        .ok_or("unknown case")?;
    if !case.benchmark {
        return Err("select a benchmark case".into());
    }
    let conformance = nexora_corpus::CorpusRunner::run(&suite, Some(&case.id), false)?;
    if !conformance.passed {
        return Err("annotated conformance failed".into());
    }
    let workspace = BenchmarkWorkspace::new(&suite, case)?;
    let files = RepositoryWalker::collect(&workspace.path)?;
    let edited = files.first().ok_or("empty source copy")?;
    let original = SourceReader::read(edited)?;
    if original.len() + 1 > 1_048_576 {
        return Err("edit exceeds source limit".into());
    }
    let source_bytes = files
        .iter()
        .map(|p| SourceReader::read(p).map(|s| s.len()))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .sum();
    let mut samples = Vec::new();
    for iteration in 1..=iterations {
        std::fs::write(edited, &original)?;
        let indexer = Indexer::new(suite.languages[&case.pack].clone());
        let mut cache = IndexCache::default();
        let mut store = MemoryStore::default();
        let mut reference_store = MemoryStore::default();
        let mut previous = None;
        for phase in ["cold", "warm", "edited"] {
            if phase == "edited" {
                std::fs::write(edited, format!("{original}\n"))?;
            }
            let (result, full) = benchmark_comparison::BenchmarkComparison::measure(
                &indexer,
                &workspace.path,
                TenantId("benchmark".into()),
                &mut cache,
                &mut store,
                &mut reference_store,
            )?;
            if !result.snapshot.diagnostics.is_empty() {
                return Err("benchmark requires clean source analysis".into());
            }
            if serde_json::to_value(&result.snapshot)? != serde_json::to_value(&full.snapshot)? {
                return Err("incremental differs from full snapshot".into());
            }
            if phase == "cold"
                && (result.reuse.analyzed_files != files.len() || result.reuse.reused_files != 0)
            {
                return Err("cold counters invalid".into());
            }
            if phase == "warm"
                && (result.reuse.analyzed_files != 0
                    || result.reuse.reused_files != files.len()
                    || previous.as_ref() != Some(&result.snapshot.id))
            {
                return Err("warm identity/reuse invalid".into());
            }
            if phase == "edited"
                && (result.reuse.analyzed_files != 1
                    || result.reuse.reused_files != files.len() - 1
                    || previous.as_ref() == Some(&result.snapshot.id))
            {
                return Err("edit identity/reuse invalid".into());
            }
            previous = Some(result.snapshot.id);
            samples.push(BenchmarkSample {
                iteration,
                phase: phase.into(),
                timings: result.timings,
                reuse: result.reuse,
                full_scan_timings: full.timings,
                equivalent: true,
            });
        }
    }
    let report = benchmark_report::BenchmarkReport {
        schema_version: 1,
        case: case.id.clone(),
        corpus_fingerprint: suite.fingerprint.clone(),
        case_fingerprint: suite.fingerprints[&case.id].clone(),
        analysis_engine: nexora_indexer::ANALYSIS_ENGINE_VERSION.into(),
        binary_sha256: format!(
            "{:x}",
            Sha256::digest(std::fs::read(std::env::current_exe()?)?)
        ),
        build: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
        .into(),
        os: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        source_files: files.len(),
        source_bytes,
        edit: "append one newline to first sorted source in an owned copy".into(),
        samples,
    };
    serde_json::to_writer_pretty(std::io::stdout(), &report)?;
    println!();
    Ok(())
}
