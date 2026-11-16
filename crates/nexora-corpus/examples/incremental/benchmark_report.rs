use serde::Serialize;
#[derive(Serialize)]
pub struct BenchmarkReport {
    pub schema_version: u32,
    pub case: String,
    pub corpus_fingerprint: String,
    pub case_fingerprint: String,
    pub analysis_engine: String,
    pub binary_sha256: String,
    pub build: String,
    pub os: String,
    pub architecture: String,
    pub source_files: usize,
    pub source_bytes: usize,
    pub edit: String,
    pub samples: Vec<crate::benchmark_sample::BenchmarkSample>,
}
