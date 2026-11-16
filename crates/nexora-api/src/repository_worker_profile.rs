use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryWorkerProfile {
    pub directory: PathBuf,
    pub language_id: String,
}
